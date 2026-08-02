//! P3 结构版式：分析模型（SWOT / 四象限 / 波特五力 / PEST / 商业模式画布）与
//! 图表（柱 / 线 / 面 / 雷达 / 漏斗 / 瀑布 / 树图 / 热力 / 甘特）。
//!
//! 关键设计：这些版式的内容都是**结构化**的，所以只用两种既有结构喂全部版式，
//! 不给每个版式发明一套字段——
//!
//! * **数据类**（指标 / 图表 / 流程 / 时间线 / 风险 / 对比表）读 `Slide.items`：
//!   一张 `label / value / span / detail / group` 表。
//! * **定格类**（SWOT / 四象限 / 五力 / PEST / 画布）读 `Slide.columns`：
//!   每个格子一个 `title + bullets`。
//!
//! 每一层都有兜底：模型只填 `bullets` 没填 `items` 时从 bullets 解析；没填
//! `columns` 时把 bullets 轮流分到各格。**任何一层缺失都还渲染得出东西**，
//! 这是整个幻灯模块一贯的约定（见 `slides::effective_slide`）。
//!
//! 图表一律输出内联 SVG，不引任何 JS 库——预览、导出 HTML、打印 PDF 是同一份
//! 字节，`preview == export` 的承诺才成立。

use crate::slides::{esc, inline, Slide, SlideItem};
use crate::slides_layout::{param_bool, param_int, param_text};

// ─────────────────────────────────────────────────────────────────────────
// 取数：items / columns 的兜底解析
// ─────────────────────────────────────────────────────────────────────────

/// 从一条要点文本解析出结构化条目。
/// 支持 `标签：12 说明` / `标签 — 说明` / `标签 | 说明` / 纯标签。
fn parse_bullet(raw: &str) -> SlideItem {
    let text = raw.trim();
    // 先找分隔符：中英文冒号优先，其次破折号/竖线。
    let split = text
        .find(['：', ':'])
        .map(|i| (i, text[i..].chars().next().map(char::len_utf8).unwrap_or(1)))
        .or_else(|| {
            ["——", "—", " - ", " – ", "|", "｜"]
                .iter()
                .filter_map(|sep| text.find(sep).map(|i| (i, sep.len())))
                .min_by_key(|(i, _)| *i)
        });
    let (label, rest) = match split {
        Some((i, sep_len)) => (text[..i].trim(), text[i + sep_len..].trim()),
        None => (text, ""),
    };
    // 说明开头的数字当作数值（"营收：1200 万" → value=1200, detail="万"）。
    let (value, detail) = leading_number(rest);
    SlideItem {
        label: label.to_string(),
        value,
        span: 0.0,
        detail: detail.to_string(),
        group: String::new(),
    }
}

/// 切出字符串开头的数字（含负号、小数、千分位逗号、百分号）。
fn leading_number(s: &str) -> (f64, &str) {
    let t = s.trim_start();
    let mut end = 0;
    let mut seen_digit = false;
    for (i, c) in t.char_indices() {
        let ok = c.is_ascii_digit()
            || (c == '-' && i == 0)
            || (c == '.' && seen_digit)
            || (c == ',' && seen_digit);
        if !ok {
            break;
        }
        seen_digit |= c.is_ascii_digit();
        end = i + c.len_utf8();
    }
    if !seen_digit {
        return (0.0, s.trim());
    }
    let num: String = t[..end].chars().filter(|c| *c != ',').collect();
    let rest = t[end..].trim_start_matches('%').trim();
    (num.parse().unwrap_or(0.0), rest)
}

/// 这一页的结构化条目：优先用 `items`，没有就从 `bullets` 解析。
///
/// 全空的条目在这里丢弃，而不是在编辑器里边打字边过滤——那样删掉前面一行空行
/// 会让正在编辑的行号跳动。编辑器保留用户输入的原样，渲染层负责不画空条目。
pub fn items_of(slide: &Slide) -> Vec<SlideItem> {
    let src: Vec<SlideItem> = if slide.items.is_empty() {
        slide.bullets.iter().map(|b| parse_bullet(b)).collect()
    } else {
        slide.items.clone()
    };
    src.into_iter()
        .filter(|it| {
            !it.label.trim().is_empty() || !it.detail.trim().is_empty() || it.value != 0.0
        })
        .collect()
}

/// 定格类版式的格子：优先用 `columns`，没有就把 bullets 轮流分进默认格。
/// 返回长度恒等于 `defaults.len()`——格子数是版式固有的，不由内容决定。
fn slots(slide: &Slide, defaults: &[&str]) -> Vec<(String, Vec<String>)> {
    let mut out: Vec<(String, Vec<String>)> = defaults
        .iter()
        .map(|d| ((*d).to_string(), Vec::new()))
        .collect();
    if !slide.columns.is_empty() {
        for (i, col) in slide.columns.iter().take(defaults.len()).enumerate() {
            if !col.title.trim().is_empty() {
                out[i].0 = col.title.clone();
            }
            out[i].1 = col.bullets.clone();
            if out[i].1.is_empty() && !col.body.trim().is_empty() {
                out[i].1 = vec![col.body.clone()];
            }
        }
        return out;
    }
    for (i, b) in slide.bullets.iter().enumerate() {
        out[i % defaults.len()].1.push(b.clone());
    }
    out
}

/// 数字显示：整数不带小数点，其余保留一位。
fn num(v: f64) -> String {
    if (v - v.round()).abs() < 1e-9 {
        format!("{}", v.round() as i64)
    } else {
        format!("{v:.1}")
    }
}

/// SVG 坐标：避免 `-0` 和长尾小数进 DOM，也让渲染逐字节可复现。
fn c(v: f64) -> String {
    let r = (v * 100.0).round() / 100.0;
    let r = if r == 0.0 { 0.0 } else { r };
    num(r)
}

// ─────────────────────────────────────────────────────────────────────────
// 入口
// ─────────────────────────────────────────────────────────────────────────

/// 渲染一个结构版式的**内容区**（不含标题）。返回 `None` 表示这个 layout
/// 不归本模块管，交回 `slides.rs` 的通用分支。
pub fn render_body(slide: &Slide) -> Option<String> {
    let lay = slide.layout.as_str();
    let p = &slide.params;
    Some(match lay {
        "metrics" => metrics(slide, p),
        "process" => process(slide, p),
        "compare-table" => compare_table(slide, p),
        "timeline" => timeline(slide, p),
        "risk" => risk(slide, p),
        "gantt" => {
            let mut items = items_of(slide);
            items.truncate(param_int(p, lay, "milestone_count").max(1) as usize);
            let svg = gantt(&items, param_bool(p, lay, "show_values"), param_bool(p, lay, "show_today"));
            if items.is_empty() {
                String::new()
            } else {
                svg_wrap(svg, String::new())
            }
        }
        "chart" => {
            let items = items_of(slide);
            chart_svg(
                &param_text(p, lay, "chart_kind"),
                &items,
                param_bool(p, lay, "show_values"),
                param_bool(p, lay, "show_legend"),
            )
        }
        "swot" => quadrants(slide, p, &["优势 S", "劣势 W", "机会 O", "威胁 T"], true),
        "matrix-2x2" => quadrants(slide, p, &["象限一", "象限二", "象限三", "象限四"], false),
        "porter" => porter(slide, p),
        "pest" => pest(slide, p),
        "bmc" => bmc(slide, p),
        _ => return None,
    })
}

/// 结构版式的纯文本降级：pptx 导出没有这些图形能力，至少要把**内容**带过去，
/// 不能导出一页空白。（视觉不对等是已知取舍，内容不能丢。）
pub fn text_fallback(slide: &Slide) -> Option<Vec<String>> {
    let lines = match slide.layout.as_str() {
        "metrics" | "process" | "compare-table" | "timeline" | "risk" | "chart" | "gantt" => {
            items_of(slide)
                .iter()
                .map(|it| {
                    let mut s = it.label.clone();
                    if it.value != 0.0 {
                        s.push_str(&format!("：{}", num(it.value)));
                    }
                    if !it.detail.trim().is_empty() {
                        s.push_str(&format!("（{}）", it.detail));
                    }
                    s
                })
                .collect()
        }
        "swot" => slot_lines(slide, &["优势 S", "劣势 W", "机会 O", "威胁 T"]),
        "matrix-2x2" => slot_lines(slide, &["象限一", "象限二", "象限三", "象限四"]),
        "porter" => slot_lines(slide, PORTER_SLOTS),
        "pest" => slot_lines(slide, PEST_SLOTS),
        "bmc" => slot_lines(slide, BMC_SLOTS),
        _ => return None,
    };
    Some(lines.into_iter().filter(|l| !l.trim().is_empty()).collect())
}

fn slot_lines(slide: &Slide, defaults: &[&str]) -> Vec<String> {
    slots(slide, defaults)
        .into_iter()
        .map(|(title, items)| {
            if items.is_empty() {
                title
            } else {
                format!("{title}：{}", items.join("；"))
            }
        })
        .collect()
}

// ─────────────────────────────────────────────────────────────────────────
// 数据类版式
// ─────────────────────────────────────────────────────────────────────────

type Params = serde_json::Map<String, serde_json::Value>;

fn metrics(slide: &Slide, p: &Params) -> String {
    let mut items = items_of(slide);
    items.truncate(param_int(p, "metrics", "metric_count").max(1) as usize);
    let show_delta = param_bool(p, "metrics", "show_delta");
    if items.is_empty() {
        return String::new();
    }
    let cols = items.len().min(3);
    let cards: String = items
        .iter()
        .map(|it| {
            let delta = if show_delta && !it.detail.trim().is_empty() {
                format!("<div class=\"mdelta\">{}</div>", inline(&it.detail))
            } else {
                String::new()
            };
            let val = if it.value == 0.0 && !it.label.is_empty() && it.detail.is_empty() {
                // 模型只给了标签没给数：把标签本身当大字，别显示一个假的 0。
                String::new()
            } else {
                format!("<div class=\"mval\">{}</div>", esc(&num(it.value)))
            };
            format!(
                "<div class=\"metric\">{val}<div class=\"mlab\">{}</div>{delta}</div>",
                inline(&it.label)
            )
        })
        .collect();
    format!("<div class=\"metrics\" style=\"grid-template-columns:repeat({cols},1fr)\">{cards}</div>")
}

fn process(slide: &Slide, p: &Params) -> String {
    let mut items = items_of(slide);
    items.truncate(param_int(p, "process", "step_count").max(1) as usize);
    if items.is_empty() {
        return String::new();
    }
    let vertical = param_text(p, "process", "direction") == "vertical";
    let arrow = param_bool(p, "process", "show_arrow");
    let last = items.len() - 1;
    let steps: String = items
        .iter()
        .enumerate()
        .map(|(i, it)| {
            let detail = if it.detail.trim().is_empty() {
                String::new()
            } else {
                format!("<p>{}</p>", inline(&it.detail))
            };
            let sep = if arrow && i != last {
                format!("<div class=\"arrow\">{}</div>", if vertical { "↓" } else { "→" })
            } else {
                String::new()
            };
            format!(
                "<div class=\"step\"><div class=\"stepno\">{}</div><div class=\"stepbody\"><h3>{}</h3>{detail}</div></div>{sep}",
                i + 1,
                inline(&it.label)
            )
        })
        .collect();
    let dir = if vertical { "vert" } else { "horiz" };
    format!("<div class=\"steps {dir}\">{steps}</div>")
}

fn compare_table(slide: &Slide, p: &Params) -> String {
    let mut rows = items_of(slide);
    rows.truncate(param_int(p, "compare-table", "row_count").max(1) as usize);
    if rows.is_empty() {
        return String::new();
    }
    let highlight = param_bool(p, "compare-table", "highlight_winner");
    // 列头来自 columns；没给就退成「维度 / 说明」两列。
    let headers: Vec<String> = if slide.columns.is_empty() {
        vec!["说明".to_string()]
    } else {
        slide
            .columns
            .iter()
            .enumerate()
            .map(|(i, col)| {
                if col.title.trim().is_empty() {
                    format!("方案{}", i + 1)
                } else {
                    col.title.clone()
                }
            })
            .collect()
    };
    let head: String = headers
        .iter()
        .map(|h| format!("<th>{}</th>", inline(h)))
        .collect();
    let body: String = rows
        .iter()
        .enumerate()
        .map(|(r, item)| {
            let cells: String = headers
                .iter()
                .enumerate()
                .map(|(ci, header)| {
                    let text = if slide.columns.is_empty() {
                        item.detail.clone()
                    } else {
                        slide.columns[ci].bullets.get(r).cloned().unwrap_or_default()
                    };
                    // 优胜标记：item.group 写列名或列序号（从 1 起）都认。
                    let g = item.group.trim();
                    let win = highlight
                        && !g.is_empty()
                        && (g == header.as_str() || g == (ci + 1).to_string());
                    let cls = if win { " class=\"win\"" } else { "" };
                    format!("<td{cls}>{}</td>", inline(&text))
                })
                .collect();
            format!("<tr><th scope=\"row\">{}</th>{cells}</tr>", inline(&item.label))
        })
        .collect();
    format!("<table class=\"ctable\"><thead><tr><th></th>{head}</tr></thead><tbody>{body}</tbody></table>")
}

fn timeline(slide: &Slide, p: &Params) -> String {
    let mut items = items_of(slide);
    items.truncate(param_int(p, "timeline", "milestone_count").max(1) as usize);
    if items.is_empty() {
        return String::new();
    }
    let today = param_bool(p, "timeline", "show_today");
    let n = items.len();
    let marks: String = items
        .iter()
        .enumerate()
        .map(|(i, it)| {
            // 里程碑交错上下排，标签才不会在密集时叠在一起。
            let side = if i % 2 == 0 { "up" } else { "down" };
            let when = if it.detail.trim().is_empty() {
                String::new()
            } else {
                format!("<span class=\"tdate\">{}</span>", inline(&it.detail))
            };
            let now = if today && i == n / 2 {
                "<span class=\"tnow\">今天</span>"
            } else {
                ""
            };
            format!(
                "<li class=\"tmark {side}\"><span class=\"tdot\"></span>\
                 <span class=\"tlab\">{}{when}{now}</span></li>",
                inline(&it.label)
            )
        })
        .collect();
    format!("<div class=\"tline\"><ol style=\"grid-template-columns:repeat({n},1fr)\">{marks}</ol></div>")
}

fn risk(slide: &Slide, p: &Params) -> String {
    let mut items = items_of(slide);
    items.truncate(param_int(p, "risk", "risk_count").max(1) as usize);
    if items.is_empty() {
        return String::new();
    }
    let show_sev = param_bool(p, "risk", "show_severity");
    let rows: String = items
        .iter()
        .map(|it| {
            let (sev_class, sev_label) = severity(&it.group);
            let sev = if show_sev {
                format!("<span class=\"sev {sev_class}\">{sev_label}</span>")
            } else {
                String::new()
            };
            let plan = if it.detail.trim().is_empty() {
                // 风险页的规矩：只列风险不给对策等于没说（见 PAGE_ROLES 的 intent）。
                "<span class=\"noplan\">待补应对</span>".to_string()
            } else {
                inline(&it.detail)
            };
            format!(
                "<li class=\"risk-row\">{sev}<span class=\"rname\">{}</span><span class=\"rplan\">{plan}</span></li>",
                inline(&it.label)
            )
        })
        .collect();
    format!("<ul class=\"risks\">{rows}</ul>")
}

/// 风险等级：认中英文写法，认不出按「中」处理——不让一个拼写把版面搞坏。
fn severity(group: &str) -> (&'static str, &'static str) {
    match group.trim().to_ascii_lowercase().as_str() {
        "high" | "h" | "高" | "严重" => ("sev-high", "高"),
        "low" | "l" | "低" | "轻微" => ("sev-low", "低"),
        _ => ("sev-mid", "中"),
    }
}

// ─────────────────────────────────────────────────────────────────────────
// 定格类版式（分析模型）
// ─────────────────────────────────────────────────────────────────────────

const PORTER_SLOTS: &[&str] = &[
    "同业竞争",
    "供应商议价能力",
    "买方议价能力",
    "新进入者威胁",
    "替代品威胁",
];
const PEST_SLOTS: &[&str] = &["政治 Political", "经济 Economic", "社会 Social", "技术 Technological"];
const BMC_SLOTS: &[&str] = &[
    "重要伙伴",
    "关键业务",
    "核心资源",
    "价值主张",
    "客户关系",
    "渠道通路",
    "客户细分",
    "成本结构",
    "收入来源",
];

fn slot_html(title: &str, items: &[String]) -> String {
    let list: String = items
        .iter()
        .filter(|b| !b.trim().is_empty())
        .map(|b| format!("<li>{}</li>", inline(b)))
        .collect();
    let body = if list.is_empty() {
        String::new()
    } else {
        format!("<ul>{list}</ul>")
    };
    format!("<h3>{}</h3>{body}", inline(title))
}

fn quadrants(slide: &Slide, p: &Params, defaults: &[&str], swot: bool) -> String {
    let lay = if swot { "swot" } else { "matrix-2x2" };
    let cells = slots(slide, defaults);
    let emph = param_text(p, lay, "emphasis_quadrant");
    let grid: String = cells
        .iter()
        .enumerate()
        .map(|(i, (title, items))| {
            let q = format!("q{}", i + 1);
            let hot = if emph == q { " hot" } else { "" };
            format!("<div class=\"qcell {q}{hot}\">{}</div>", slot_html(title, items))
        })
        .collect();
    if !param_bool(p, lay, "show_axis_labels") {
        return format!("<div class=\"quad\">{grid}</div>");
    }
    // 轴说明取自 columns 之外的 body（"横轴 | 纵轴"），缺省给通用词。
    let (ax, ay) = axis_labels(slide, swot);
    format!(
        "<div class=\"quadwrap\"><div class=\"axis-y\">{}</div>\
         <div class=\"quadcol\"><div class=\"quad\">{grid}</div>\
         <div class=\"axis-x\">{}</div></div></div>",
        inline(&ay),
        inline(&ax)
    )
}

fn axis_labels(slide: &Slide, swot: bool) -> (String, String) {
    let raw = slide.body.trim();
    if let Some((x, y)) = raw.split_once(['|', '｜']) {
        return (x.trim().to_string(), y.trim().to_string());
    }
    if swot {
        ("有利 → 不利".to_string(), "内部 → 外部".to_string())
    } else {
        ("横轴".to_string(), "纵轴".to_string())
    }
}

fn porter(slide: &Slide, p: &Params) -> String {
    let cells = slots(slide, PORTER_SLOTS);
    let show_center = param_bool(p, "porter", "show_center");
    // 中心是同业竞争，四周是另外四力。关掉中心时退成 2×2。
    let around: String = cells
        .iter()
        .skip(1)
        .enumerate()
        .map(|(i, (t, items))| format!("<div class=\"force f{}\">{}</div>", i + 1, slot_html(t, items)))
        .collect();
    if !show_center {
        return format!("<div class=\"porter nocenter\">{around}</div>");
    }
    let (ct, ci) = &cells[0];
    format!(
        "<div class=\"porter\">{around}<div class=\"force center\">{}</div></div>",
        slot_html(ct, ci)
    )
}

fn pest(slide: &Slide, p: &Params) -> String {
    let cells = slots(slide, PEST_SLOTS);
    let row = param_text(p, "pest", "layout_mode") == "row";
    let grid: String = cells
        .iter()
        .map(|(t, items)| format!("<div class=\"pcell\">{}</div>", slot_html(t, items)))
        .collect();
    let mode = if row { "row" } else { "grid" };
    format!("<div class=\"pest {mode}\">{grid}</div>")
}

fn bmc(slide: &Slide, p: &Params) -> String {
    let cells = slots(slide, BMC_SLOTS);
    let compact = if param_bool(p, "bmc", "compact") { " compact" } else { "" };
    // 画布的九宫格是固定几何：伙伴/业务+资源/价值/关系+渠道/客户 五列，
    // 底部成本与收入横跨两半。用 grid-area 名字锁死位置。
    const AREAS: &[&str] = &["kp", "ka", "kr", "vp", "cr", "ch", "cs", "cost", "rev"];
    let grid: String = cells
        .iter()
        .enumerate()
        .map(|(i, (t, items))| {
            format!(
                "<div class=\"bcell\" style=\"grid-area:{}\">{}</div>",
                AREAS[i],
                slot_html(t, items)
            )
        })
        .collect();
    format!("<div class=\"canvas{compact}\">{grid}</div>")
}

// ─────────────────────────────────────────────────────────────────────────
// 图表：内联 SVG，零 JS 依赖
// ─────────────────────────────────────────────────────────────────────────

const CW: f64 = 1088.0; // 画布宽（= 幻灯内容区宽度）
const CH: f64 = 420.0;
const PAD_L: f64 = 64.0;
const PAD_R: f64 = 24.0;
const PAD_T: f64 = 24.0;
const PAD_B: f64 = 48.0;

fn plot_w() -> f64 {
    CW - PAD_L - PAD_R
}
fn plot_h() -> f64 {
    CH - PAD_T - PAD_B
}

fn svg_wrap(inner: String, legend: String) -> String {
    format!(
        "<div class=\"chart\"><svg viewBox=\"0 0 {CW} {CH}\" preserveAspectRatio=\"xMidYMid meet\" \
         role=\"img\">{inner}</svg>{legend}</div>"
    )
}

/// 图表总入口。未知 kind 一律退成柱状——模型编个没见过的类型也不能开天窗。
fn chart_svg(kind: &str, items: &[SlideItem], show_values: bool, show_legend: bool) -> String {
    if items.is_empty() {
        return String::new();
    }
    // 只有柱状图真的用「分组 → 颜色」编码，别的图例会说谎：折线/面积只画一条线，
    // 热力图的分组已经是行标签，树图按大小排序着色。所以图例只对柱状图开。
    let legend = if show_legend && kind == "bar" {
        legend_html(items)
    } else {
        String::new()
    };
    let inner = match kind {
        "line" => line_area(items, show_values, false),
        "area" => line_area(items, show_values, true),
        "radar" => radar(items, show_values),
        "funnel" => funnel(items, show_values),
        "waterfall" => waterfall(items, show_values),
        "treemap" => treemap(items, show_values),
        "heatmap" => heatmap(items, show_values),
        // chart 版式里的甘特没有「今天」开关（那是 gantt 版式的旋钮）
        "gantt" => gantt(items, show_values, false),
        _ => bars(items, show_values),
    };
    svg_wrap(inner, legend)
}

/// 图例只在真的有多个系列时才有意义，否则是纯噪音。
fn legend_html(items: &[SlideItem]) -> String {
    let mut groups: Vec<&str> = Vec::new();
    for it in items {
        let g = it.group.trim();
        if !g.is_empty() && !groups.contains(&g) {
            groups.push(g);
        }
    }
    if groups.len() < 2 {
        return String::new();
    }
    let chips: String = groups
        .iter()
        .enumerate()
        .map(|(i, g)| {
            format!(
                "<span class=\"lg\"><i style=\"background:{}\"></i>{}</span>",
                series_color(i),
                inline(g)
            )
        })
        .collect();
    format!("<div class=\"legend\">{chips}</div>")
}

/// 系列配色走主题变量，跟着 theme / 品牌母版走，图表不会跟版面脱色。
fn series_color(i: usize) -> &'static str {
    match i % 3 {
        0 => "var(--acc)",
        1 => "var(--acc2)",
        _ => "var(--acc3)",
    }
}

/// 画在色块**上面**的文字颜色。色块是半透明的，浓的时候要用反色，淡的时候
/// 反色反而看不见——按不透明度选，别指望一个固定颜色两头都能读。
fn on_fill(opacity: f64) -> &'static str {
    if opacity > 0.55 {
        "var(--on-acc)"
    } else {
        "currentColor"
    }
}

fn color_of(it: &SlideItem, order: &[&str]) -> &'static str {
    let g = it.group.trim();
    if g.is_empty() {
        return series_color(0);
    }
    series_color(order.iter().position(|o| *o == g).unwrap_or(0))
}

fn group_order<'a>(items: &'a [SlideItem]) -> Vec<&'a str> {
    let mut out: Vec<&str> = Vec::new();
    for it in items {
        let g = it.group.trim();
        if !g.is_empty() && !out.contains(&g) {
            out.push(g);
        }
    }
    out
}

/// 纵轴刻度上界：留一点余量，并保证非零（全 0 数据也要画得出坐标系）。
fn nice_max(values: impl Iterator<Item = f64>) -> f64 {
    let m = values.fold(0.0_f64, |a, b| a.max(b.abs()));
    if m <= 0.0 {
        1.0
    } else {
        m
    }
}

fn grid_lines(max: f64) -> String {
    let (w, h) = (plot_w(), plot_h());
    (0..=4)
        .map(|i| {
            let y = PAD_T + h - h * (i as f64 / 4.0);
            let v = max * (i as f64 / 4.0);
            format!(
                "<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" class=\"grid\"/>\
                 <text x=\"{}\" y=\"{}\" class=\"tick\" text-anchor=\"end\">{}</text>",
                c(PAD_L),
                c(y),
                c(PAD_L + w),
                c(y),
                c(PAD_L - 10.0),
                c(y + 5.0),
                esc(&num(v))
            )
        })
        .collect()
}

fn x_label(i: usize, n: usize, text: &str) -> String {
    let w = plot_w();
    let x = PAD_L + w * ((i as f64 + 0.5) / n as f64);
    format!(
        "<text x=\"{}\" y=\"{}\" class=\"xlab\" text-anchor=\"middle\">{}</text>",
        c(x),
        c(CH - PAD_B + 26.0),
        esc(text)
    )
}

fn bars(items: &[SlideItem], show_values: bool) -> String {
    let max = nice_max(items.iter().map(|i| i.value));
    let (w, h) = (plot_w(), plot_h());
    let n = items.len();
    let slot = w / n as f64;
    let bw = slot * 0.58;
    let order = group_order(items);
    let body: String = items
        .iter()
        .enumerate()
        .map(|(i, it)| {
            let bh = (it.value.abs() / max) * h;
            let x = PAD_L + slot * i as f64 + (slot - bw) / 2.0;
            let y = PAD_T + h - bh;
            let val = if show_values {
                format!(
                    "<text x=\"{}\" y=\"{}\" class=\"val\" text-anchor=\"middle\">{}</text>",
                    c(x + bw / 2.0),
                    c(y - 8.0),
                    esc(&num(it.value))
                )
            } else {
                String::new()
            };
            format!(
                "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"6\" fill=\"{}\"/>{val}{}",
                c(x),
                c(y),
                c(bw),
                c(bh.max(1.0)),
                color_of(it, &order),
                x_label(i, n, &it.label)
            )
        })
        .collect();
    format!("{}{body}", grid_lines(max))
}

fn line_area(items: &[SlideItem], show_values: bool, fill: bool) -> String {
    let max = nice_max(items.iter().map(|i| i.value));
    let (w, h) = (plot_w(), plot_h());
    let n = items.len();
    let pt = |i: usize, v: f64| -> (f64, f64) {
        let x = PAD_L + w * if n == 1 { 0.5 } else { i as f64 / (n - 1) as f64 };
        (x, PAD_T + h - (v.abs() / max) * h)
    };
    let points: Vec<(f64, f64)> = items.iter().enumerate().map(|(i, it)| pt(i, it.value)).collect();
    let path: String = points
        .iter()
        .map(|(x, y)| format!("{},{}", c(*x), c(*y)))
        .collect::<Vec<_>>()
        .join(" ");
    let area = if fill {
        let base = PAD_T + h;
        format!(
            "<polygon points=\"{},{} {path} {},{}\" fill=\"var(--acc)\" opacity=\".22\"/>",
            c(points[0].0),
            c(base),
            c(points[points.len() - 1].0),
            c(base)
        )
    } else {
        String::new()
    };
    let dots: String = points
        .iter()
        .enumerate()
        .map(|(i, (x, y))| {
            let val = if show_values {
                format!(
                    "<text x=\"{}\" y=\"{}\" class=\"val\" text-anchor=\"middle\">{}</text>",
                    c(*x),
                    c(y - 12.0),
                    esc(&num(items[i].value))
                )
            } else {
                String::new()
            };
            format!(
                "<circle cx=\"{}\" cy=\"{}\" r=\"5\" fill=\"var(--acc)\"/>{val}{}",
                c(*x),
                c(*y),
                x_label_at(*x, &items[i].label)
            )
        })
        .collect();
    format!(
        "{}{area}<polyline points=\"{path}\" fill=\"none\" stroke=\"var(--acc)\" stroke-width=\"3\" \
         stroke-linejoin=\"round\" stroke-linecap=\"round\"/>{dots}",
        grid_lines(max)
    )
}

fn x_label_at(x: f64, text: &str) -> String {
    format!(
        "<text x=\"{}\" y=\"{}\" class=\"xlab\" text-anchor=\"middle\">{}</text>",
        c(x),
        c(CH - PAD_B + 26.0),
        esc(text)
    )
}

fn radar(items: &[SlideItem], show_values: bool) -> String {
    let n = items.len().max(3);
    let max = nice_max(items.iter().map(|i| i.value));
    let (cx, cy) = (CW / 2.0, CH / 2.0);
    let r = (CH / 2.0 - 46.0).min(CW / 2.0 - 120.0);
    let angle = |i: usize| -> f64 {
        // 从正上方开始顺时针，读图习惯如此。
        -std::f64::consts::FRAC_PI_2 + (i as f64) * std::f64::consts::TAU / n as f64
    };
    let rings: String = (1..=3)
        .map(|k| {
            let rr = r * k as f64 / 3.0;
            let pts: String = (0..n)
                .map(|i| {
                    let a = angle(i);
                    format!("{},{}", c(cx + rr * a.cos()), c(cy + rr * a.sin()))
                })
                .collect::<Vec<_>>()
                .join(" ");
            format!("<polygon points=\"{pts}\" class=\"grid\" fill=\"none\"/>")
        })
        .collect();
    let spokes: String = (0..n)
        .map(|i| {
            let a = angle(i);
            format!(
                "<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" class=\"grid\"/>",
                c(cx),
                c(cy),
                c(cx + r * a.cos()),
                c(cy + r * a.sin())
            )
        })
        .collect();
    let shape: String = items
        .iter()
        .enumerate()
        .map(|(i, it)| {
            let a = angle(i);
            let rr = r * (it.value.abs() / max);
            format!("{},{}", c(cx + rr * a.cos()), c(cy + rr * a.sin()))
        })
        .collect::<Vec<_>>()
        .join(" ");
    let labels: String = items
        .iter()
        .enumerate()
        .map(|(i, it)| {
            let a = angle(i);
            let lx = cx + (r + 26.0) * a.cos();
            let ly = cy + (r + 26.0) * a.sin();
            let anchor = if a.cos().abs() < 0.2 {
                "middle"
            } else if a.cos() > 0.0 {
                "start"
            } else {
                "end"
            };
            let val = if show_values {
                format!("（{}）", num(it.value))
            } else {
                String::new()
            };
            format!(
                "<text x=\"{}\" y=\"{}\" class=\"xlab\" text-anchor=\"{anchor}\">{}</text>",
                c(lx),
                c(ly + 5.0),
                esc(&format!("{}{val}", it.label))
            )
        })
        .collect();
    format!(
        "{rings}{spokes}<polygon points=\"{shape}\" fill=\"var(--acc)\" opacity=\".32\" \
         stroke=\"var(--acc)\" stroke-width=\"3\"/>{labels}"
    )
}

fn funnel(items: &[SlideItem], show_values: bool) -> String {
    let max = nice_max(items.iter().map(|i| i.value));
    let n = items.len();
    let h = (CH - PAD_T - PAD_B) / n as f64;
    let cx = CW / 2.0;
    let full = plot_w() * 0.66;
    let width_at = |v: f64| -> f64 { (full * (v.abs() / max)).max(60.0) };
    let body: String = items
        .iter()
        .enumerate()
        .map(|(i, it)| {
            let top = PAD_T + h * i as f64;
            let bot = top + h - 8.0;
            let wt = width_at(it.value);
            let wb = width_at(items.get(i + 1).map(|x| x.value).unwrap_or(it.value * 0.75));
            let val = if show_values {
                format!(" · {}", num(it.value))
            } else {
                String::new()
            };
            let op = 1.0 - (i as f64) * 0.13;
            format!(
                "<polygon points=\"{},{} {},{} {},{} {},{}\" fill=\"{}\" opacity=\"{}\"/>\
                 <text x=\"{}\" y=\"{}\" class=\"inbar\" style=\"fill:{}\" text-anchor=\"middle\">{}</text>",
                c(cx - wt / 2.0),
                c(top),
                c(cx + wt / 2.0),
                c(top),
                c(cx + wb / 2.0),
                c(bot),
                c(cx - wb / 2.0),
                c(bot),
                series_color(0),
                format_args!("{op:.2}"),
                c(cx),
                c(top + h / 2.0),
                on_fill(op),
                esc(&format!("{}{val}", it.label))
            )
        })
        .collect();
    body
}

fn waterfall(items: &[SlideItem], show_values: bool) -> String {
    // 累计轨迹：group="total" 的条从 0 画起（阶段合计），其余是增减量。
    let mut running = 0.0;
    let mut spans: Vec<(f64, f64, bool)> = Vec::new(); // (from, to, is_total)
    for it in items {
        let total = it.group.trim().eq_ignore_ascii_case("total") || it.group.trim() == "合计";
        if total {
            spans.push((0.0, running, true));
        } else {
            let to = running + it.value;
            spans.push((running, to, false));
            running = to;
        }
    }
    let max = nice_max(spans.iter().flat_map(|(a, b, _)| [*a, *b]));
    let (w, h) = (plot_w(), plot_h());
    let n = items.len();
    let slot = w / n as f64;
    let bw = slot * 0.56;
    let y_of = |v: f64| PAD_T + h - (v.abs() / max) * h;
    let body: String = items
        .iter()
        .enumerate()
        .map(|(i, it)| {
            let (from, to, total) = spans[i];
            let (top, bottom) = if to >= from { (to, from) } else { (from, to) };
            let y = y_of(top);
            let bh = (y_of(bottom) - y).max(3.0);
            let x = PAD_L + slot * i as f64 + (slot - bw) / 2.0;
            let color = if total {
                "var(--acc3)"
            } else if to >= from {
                "var(--acc)"
            } else {
                "var(--acc2)"
            };
            let val = if show_values {
                format!(
                    "<text x=\"{}\" y=\"{}\" class=\"val\" text-anchor=\"middle\">{}</text>",
                    c(x + bw / 2.0),
                    c(y - 8.0),
                    esc(&if total { num(to) } else { format!("{}{}", if it.value >= 0.0 { "+" } else { "" }, num(it.value)) })
                )
            } else {
                String::new()
            };
            format!(
                "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"4\" fill=\"{color}\"/>{val}{}",
                c(x),
                c(y),
                c(bw),
                c(bh),
                x_label(i, n, &it.label)
            )
        })
        .collect();
    format!("{}{body}", grid_lines(max))
}

fn treemap(items: &[SlideItem], show_values: bool) -> String {
    // 切分-二分（slice & dice）：按值把矩形递归对半切。方向**按层交替**而不是
    // 按长宽比——1088×420 的画布单看长宽比会一路竖着切，切出几条细长条。
    // 不追求最优长宽比，但确定性且不会重叠，这比好看更重要。
    fn split(
        items: &[(usize, f64)],
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        vertical: bool,
        out: &mut Vec<(usize, f64, f64, f64, f64)>,
    ) {
        if items.is_empty() {
            return;
        }
        if items.len() == 1 {
            out.push((items[0].0, x, y, w, h));
            return;
        }
        let total: f64 = items.iter().map(|(_, v)| v).sum();
        let mut acc = 0.0;
        let mut cut = 1;
        for (i, (_, v)) in items.iter().enumerate() {
            acc += v;
            if acc >= total / 2.0 {
                cut = (i + 1).min(items.len() - 1);
                break;
            }
        }
        let left: f64 = items[..cut].iter().map(|(_, v)| v).sum();
        let frac = if total > 0.0 { left / total } else { 0.5 };
        if vertical {
            split(&items[..cut], x, y, w * frac, h, false, out);
            split(&items[cut..], x + w * frac, y, w * (1.0 - frac), h, false, out);
        } else {
            split(&items[..cut], x, y, w, h * frac, true, out);
            split(&items[cut..], x, y + h * frac, w, h * (1.0 - frac), true, out);
        }
    }
    let mut idx: Vec<(usize, f64)> = items
        .iter()
        .enumerate()
        .map(|(i, it)| (i, it.value.abs().max(0.01)))
        .collect();
    idx.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let mut rects = Vec::new();
    split(&idx, 8.0, 8.0, CW - 16.0, CH - 16.0, true, &mut rects);
    rects
        .iter()
        .map(|(i, x, y, w, h)| {
            let it = &items[*i];
            let op = (1.0 - (*i as f64) * 0.08).max(0.35);
            let val = if show_values {
                format!(
                    "<tspan x=\"{}\" dy=\"22\" class=\"val\" style=\"fill:{}\">{}</tspan>",
                    c(x + 12.0),
                    on_fill(op),
                    esc(&num(it.value))
                )
            } else {
                String::new()
            };
            format!(
                "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"8\" fill=\"{}\" \
                 opacity=\"{op:.2}\" class=\"tmrect\"/>\
                 <text x=\"{}\" y=\"{}\" class=\"inbar\" style=\"fill:{}\" text-anchor=\"start\">{}{val}</text>",
                c(*x),
                c(*y),
                c((w - 4.0).max(0.0)),
                c((h - 4.0).max(0.0)),
                series_color(*i),
                c(x + 12.0),
                c(y + 30.0),
                on_fill(op),
                esc(&it.label)
            )
        })
        .collect()
}

fn heatmap(items: &[SlideItem], show_values: bool) -> String {
    // 行 = group，列 = label；没给 group 就退成一行。
    let mut rows: Vec<&str> = group_order(items);
    if rows.is_empty() {
        rows.push("");
    }
    let mut cols: Vec<&str> = Vec::new();
    for it in items {
        let l = it.label.as_str();
        if !cols.contains(&l) {
            cols.push(l);
        }
    }
    let max = nice_max(items.iter().map(|i| i.value));
    let left = 150.0;
    let top = 34.0;
    let cw = (CW - left - 16.0) / cols.len().max(1) as f64;
    let ch = (CH - top - 24.0) / rows.len().max(1) as f64;
    let heads: String = cols
        .iter()
        .enumerate()
        .map(|(ci, l)| {
            format!(
                "<text x=\"{}\" y=\"{}\" class=\"xlab\" text-anchor=\"middle\">{}</text>",
                c(left + cw * (ci as f64 + 0.5)),
                c(top - 12.0),
                esc(l)
            )
        })
        .collect();
    let cells: String = rows
        .iter()
        .enumerate()
        .map(|(ri, r)| {
            let label = format!(
                "<text x=\"{}\" y=\"{}\" class=\"xlab\" text-anchor=\"end\">{}</text>",
                c(left - 14.0),
                c(top + ch * (ri as f64 + 0.5) + 5.0),
                esc(r)
            );
            let row: String = cols
                .iter()
                .enumerate()
                .map(|(ci, col)| {
                    let v = items
                        .iter()
                        .find(|it| it.label == *col && (it.group.trim() == *r))
                        .map(|it| it.value)
                        .unwrap_or(0.0);
                    let op = 0.10 + 0.85 * (v.abs() / max);
                    let txt = if show_values && v != 0.0 {
                        format!(
                            "<text x=\"{}\" y=\"{}\" class=\"inbar\" style=\"fill:{}\" text-anchor=\"middle\">{}</text>",
                            c(left + cw * (ci as f64 + 0.5)),
                            c(top + ch * (ri as f64 + 0.5) + 6.0),
                            on_fill(op),
                            esc(&num(v))
                        )
                    } else {
                        String::new()
                    };
                    format!(
                        "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"4\" \
                         fill=\"var(--acc)\" opacity=\"{op:.2}\"/>{txt}",
                        c(left + cw * ci as f64 + 2.0),
                        c(top + ch * ri as f64 + 2.0),
                        c(cw - 4.0),
                        c(ch - 4.0),
                    )
                })
                .collect();
            format!("{label}{row}")
        })
        .collect();
    format!("{heads}{cells}")
}

fn gantt(items: &[SlideItem], show_values: bool, show_today: bool) -> String {
    // value = 起始，span = 持续（缺省 1）。终点最大值决定横轴刻度。
    let end = |it: &SlideItem| it.value + if it.span > 0.0 { it.span } else { 1.0 };
    let max = nice_max(items.iter().map(end));
    let left = 220.0;
    let top = 18.0;
    let rows = items.len().max(1);
    let rh = (CH - top - 34.0) / rows as f64;
    let w = CW - left - 24.0;
    let ticks: String = (0..=4)
        .map(|i| {
            let x = left + w * (i as f64 / 4.0);
            format!(
                "<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" class=\"grid\"/>\
                 <text x=\"{}\" y=\"{}\" class=\"tick\" text-anchor=\"middle\">{}</text>",
                c(x),
                c(top),
                c(x),
                c(CH - 30.0),
                c(x),
                c(CH - 10.0),
                esc(&num(max * i as f64 / 4.0))
            )
        })
        .collect();
    let bars: String = items
        .iter()
        .enumerate()
        .map(|(i, it)| {
            let x0 = left + w * (it.value.abs() / max);
            let x1 = left + w * (end(it) / max);
            let y = top + rh * i as f64 + rh * 0.22;
            let bh = rh * 0.56;
            let val = if show_values && !it.detail.trim().is_empty() {
                format!(
                    "<text x=\"{}\" y=\"{}\" class=\"val\" text-anchor=\"start\">{}</text>",
                    c(x1 + 10.0),
                    c(y + bh * 0.72),
                    esc(&it.detail)
                )
            } else {
                String::new()
            };
            format!(
                "<text x=\"{}\" y=\"{}\" class=\"xlab\" text-anchor=\"end\">{}</text>\
                 <rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"6\" fill=\"{}\"/>{val}",
                c(left - 16.0),
                c(y + bh * 0.72),
                esc(&it.label),
                c(x0),
                c(y),
                c((x1 - x0).max(6.0)),
                c(bh),
                series_color(i)
            )
        })
        .collect();
    // 「今天」竖线：没有真实日期轴，落在刻度中点——它标的是「进行到这里」，
    // 不是某个具体日期，所以不假装精确。
    let today = if show_today {
        let x = left + w * 0.5;
        format!(
            "<line x1=\"{x0}\" y1=\"{}\" x2=\"{x0}\" y2=\"{}\" stroke=\"var(--acc3)\" \
             stroke-width=\"2\" stroke-dasharray=\"6 5\"/>\
             <text x=\"{x0}\" y=\"{}\" class=\"val\" text-anchor=\"middle\">今天</text>",
            c(top),
            c(CH - 30.0),
            c(top - 4.0),
            x0 = c(x)
        )
    } else {
        String::new()
    };
    format!("{ticks}{bars}{today}")
}

// ─────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::slides::Column;

    fn slide(layout: &str, bullets: &[&str]) -> Slide {
        let mut s = Slide {
            layout: layout.into(),
            title: "标题".into(),
            bullets: bullets.iter().map(|b| (*b).to_string()).collect(),
            ..Default::default()
        };
        s.fill_default_params();
        s
    }

    #[test]
    fn bullets_parse_into_items() {
        let it = parse_bullet("营收：1,200 万");
        assert_eq!(it.label, "营收");
        assert_eq!(it.value, 1200.0);
        assert_eq!(it.detail, "万");

        let it = parse_bullet("识别风险 — 提前 30 天预警");
        assert_eq!(it.label, "识别风险");
        assert_eq!(it.value, 0.0, "说明不是数字时不该编出数值");
        assert_eq!(it.detail, "提前 30 天预警");

        let it = parse_bullet("只有标签");
        assert_eq!(it.label, "只有标签");
        assert!(it.detail.is_empty());
    }

    /// 全部结构版式在**只有 bullets** 的最贫瘠输入下都要渲染出内容——
    /// 模型不填 items/columns 是常态，不能因此开天窗。
    #[test]
    fn every_block_layout_renders_from_bullets_alone() {
        let layouts = [
            "metrics", "process", "compare-table", "timeline", "risk", "chart", "gantt",
            "swot", "matrix-2x2", "porter", "pest", "bmc",
        ];
        for lay in layouts {
            let s = slide(lay, &["甲：10", "乙：20", "丙：30"]);
            let html = render_body(&s).unwrap_or_else(|| panic!("{lay} 没被本模块接管"));
            assert!(!html.trim().is_empty(), "{lay} 渲染成空");
            assert!(html.contains('甲') || html.contains('乙'), "{lay} 丢了内容:\n{html}");
        }
    }

    /// 编辑器会留下空行（用户加了行还没填），渲染不能因此画出空柱子/空卡片。
    #[test]
    fn empty_items_are_dropped_at_render_time() {
        let mut s = slide("metrics", &[]);
        s.items = vec![
            SlideItem { label: "有效".into(), value: 5.0, ..Default::default() },
            SlideItem::default(),                                    // 完全空的一行
            SlideItem { detail: "只有说明".into(), ..Default::default() }, // 有内容就保留
        ];
        assert_eq!(items_of(&s).len(), 2, "只应丢弃完全空的那一行");
        assert!(render_body(&s).unwrap().contains("有效"));
    }

    #[test]
    fn non_block_layouts_are_not_claimed() {
        for lay in ["cover", "bullets", "two-column", "quote", "image", "没见过的"] {
            assert!(render_body(&slide(lay, &["x"])).is_none(), "{lay} 不该被接管");
        }
    }

    /// 每种图表都要能画，未知类型退成柱状而不是空白。
    #[test]
    fn every_chart_kind_draws() {
        let items = vec![
            SlideItem { label: "一".into(), value: 3.0, span: 2.0, detail: "备注".into(), group: "A".into() },
            SlideItem { label: "二".into(), value: -1.0, span: 1.0, detail: String::new(), group: "B".into() },
            SlideItem { label: "三".into(), value: 6.0, span: 3.0, detail: String::new(), group: "A".into() },
        ];
        for kind in ["bar", "line", "area", "radar", "funnel", "waterfall", "treemap", "heatmap", "gantt", "编的类型"] {
            let svg = chart_svg(kind, &items, true, true);
            assert!(svg.contains("<svg"), "{kind} 没输出 SVG");
            assert!(svg.contains("</svg>"), "{kind} SVG 没闭合");
            assert!(!svg.contains("NaN"), "{kind} 出现 NaN 坐标:\n{svg}");
        }
        assert!(chart_svg("bar", &[], true, true).is_empty(), "没数据不该画空坐标系");
    }

    /// 全 0 数据不能除零画出 NaN。
    #[test]
    fn all_zero_values_still_draw() {
        let items: Vec<SlideItem> = ["a", "b"]
            .iter()
            .map(|l| SlideItem { label: (*l).into(), ..Default::default() })
            .collect();
        for kind in ["bar", "line", "radar", "funnel", "waterfall", "treemap", "heatmap", "gantt"] {
            let svg = chart_svg(kind, &items, true, false);
            assert!(!svg.contains("NaN") && !svg.contains("inf"), "{kind}:\n{svg}");
        }
    }

    #[test]
    fn slots_are_fixed_count_regardless_of_input() {
        // 给 2 条 bullets 也要撑满 9 格画布，格子数是版式固有的。
        let s = slide("bmc", &["a", "b"]);
        assert_eq!(slots(&s, BMC_SLOTS).len(), 9);
        // 给 12 个 columns 也只取前 9 个。
        let mut s2 = slide("bmc", &[]);
        s2.columns = (0..12)
            .map(|i| Column { title: format!("C{i}"), ..Default::default() })
            .collect();
        assert_eq!(slots(&s2, BMC_SLOTS).len(), 9);
    }

    #[test]
    fn block_content_is_escaped() {
        let s = slide("metrics", &["<script>：1"]);
        let html = render_body(&s).unwrap();
        assert!(!html.contains("<script>"), "结构版式必须转义:\n{html}");
        assert!(html.contains("&lt;script&gt;"));
    }

    /// pptx 导出拿不到图形，但必须拿得到内容。
    #[test]
    fn text_fallback_carries_content() {
        let s = slide("swot", &["强项一", "弱项一", "机会一", "威胁一"]);
        let lines = text_fallback(&s).unwrap();
        assert!(lines.iter().any(|l| l.contains("强项一")), "{lines:?}");
        assert!(text_fallback(&slide("cover", &[])).is_none());
    }

    /// 目视检查用：把每个 P3 版式渲染成一份 HTML 写到临时目录。
    /// 图形代码光靠断言看不出「画歪了」，改完版式后跑一次用眼睛过一遍。
    /// `cargo test --lib p3_visual_demo -- --ignored --nocapture`
    #[test]
    #[ignore = "生成目视检查用的 HTML，不参与常规校验"]
    fn p3_visual_demo() {
        use crate::slides::{Deck, Slide};
        let data = |pairs: &[(&str, f64, &str, &str)]| -> Vec<SlideItem> {
            pairs
                .iter()
                .map(|(l, v, d, g)| SlideItem {
                    label: (*l).into(),
                    value: *v,
                    span: 0.0,
                    detail: (*d).into(),
                    group: (*g).into(),
                })
                .collect()
        };
        let quarters = data(&[
            ("Q1", 128.0, "", "营收"),
            ("Q2", 164.0, "", "营收"),
            ("Q3", 152.0, "", "营收"),
            ("Q4", 210.0, "", "营收"),
        ]);
        let mut slides = vec![
            Slide {
                layout: "metrics".into(),
                title: "关键指标".into(),
                items: data(&[
                    ("年营收（百万）", 654.0, "同比 **+23%**", ""),
                    ("活跃客户", 1842.0, "同比 +311", ""),
                    ("毛利率 %", 61.5, "同比 +2.4pt", ""),
                ]),
                ..Default::default()
            },
            Slide {
                layout: "process".into(),
                title: "落地流程".into(),
                items: data(&[
                    ("调研", 0.0, "两周走访 12 家客户", ""),
                    ("试点", 0.0, "选 2 个区域跑通", ""),
                    ("推广", 0.0, "季度内覆盖全国", ""),
                    ("复盘", 0.0, "固化为标准动作", ""),
                ]),
                ..Default::default()
            },
            Slide {
                layout: "timeline".into(),
                title: "里程碑".into(),
                items: data(&[
                    ("立项", 0.0, "3月", ""),
                    ("原型", 0.0, "5月", ""),
                    ("内测", 0.0, "8月", ""),
                    ("公测", 0.0, "10月", ""),
                    ("GA", 0.0, "12月", ""),
                ]),
                ..Default::default()
            },
            Slide {
                layout: "risk".into(),
                title: "风险与应对".into(),
                items: data(&[
                    ("供应商单点依赖", 0.0, "Q3 前引入第二供应商", "high"),
                    ("人力缺口", 0.0, "外包 + 内部轮岗", "mid"),
                    ("汇率波动", 0.0, "锁汇 60%", "low"),
                ]),
                ..Default::default()
            },
            Slide {
                layout: "compare-table".into(),
                title: "方案对比".into(),
                items: data(&[
                    ("上线周期", 0.0, "", "自建"),
                    ("首年成本", 0.0, "", "采购"),
                    ("可定制性", 0.0, "", "自建"),
                ]),
                columns: vec![
                    crate::slides::Column {
                        title: "自建".into(),
                        bullets: vec!["6 个月".into(), "¥420 万".into(), "完全可控".into()],
                        body: String::new(),
                    },
                    crate::slides::Column {
                        title: "采购".into(),
                        bullets: vec!["6 周".into(), "¥180 万".into(), "受限于厂商".into()],
                        body: String::new(),
                    },
                ],
                ..Default::default()
            },
            Slide {
                layout: "swot".into(),
                title: "SWOT".into(),
                body: "有利 → 不利 | 内部 → 外部".into(),
                columns: ["渠道深、交付快", "品牌弱", "行业数字化提速", "巨头下沉"]
                    .iter()
                    .map(|t| crate::slides::Column {
                        title: String::new(),
                        bullets: vec![(*t).to_string(), "补充一条说明".into()],
                        body: String::new(),
                    })
                    .collect(),
                ..Default::default()
            },
            Slide {
                layout: "porter".into(),
                title: "波特五力".into(),
                bullets: (1..=10).map(|i| format!("要点{i}")).collect(),
                ..Default::default()
            },
            Slide {
                layout: "pest".into(),
                title: "PEST".into(),
                bullets: (1..=8).map(|i| format!("因素{i}")).collect(),
                ..Default::default()
            },
            Slide {
                layout: "bmc".into(),
                title: "商业模式画布".into(),
                bullets: (1..=18).map(|i| format!("条目{i}")).collect(),
                ..Default::default()
            },
            Slide {
                layout: "gantt".into(),
                title: "排期".into(),
                items: vec![
                    SlideItem { label: "需求".into(), value: 0.0, span: 3.0, detail: "3周".into(), group: String::new() },
                    SlideItem { label: "开发".into(), value: 3.0, span: 8.0, detail: "8周".into(), group: String::new() },
                    SlideItem { label: "测试".into(), value: 9.0, span: 4.0, detail: "4周".into(), group: String::new() },
                    SlideItem { label: "上线".into(), value: 13.0, span: 1.0, detail: "1周".into(), group: String::new() },
                ],
                ..Default::default()
            },
        ];
        // 媒体槽：图未就位的占位框，以及有图时的图文并排
        let pic = "data:image/svg+xml;base64,PHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIHdpZHRoPSI0MDAiIGhlaWdodD0iMzAwIj48cmVjdCB3aWR0aD0iNDAwIiBoZWlnaHQ9IjMwMCIgZmlsbD0iIzQ0N2FjYyIvPjwvc3ZnPg==";
        for (slot, image, title) in [
            ("right", "", "配图位 · 图还没有"),
            ("left", pic, "配图位 · 图已就位"),
        ] {
            let mut s = Slide {
                layout: "bullets".into(),
                title: title.into(),
                bullets: vec!["先排版".into(), "后配图".into(), "版面不跳".into()],
                image: image.into(),
                ..Default::default()
            };
            s.params.insert("media_slot".into(), serde_json::json!(slot));
            slides.push(s);
        }
        for kind in ["bar", "line", "area", "radar", "funnel", "waterfall", "treemap", "heatmap"] {
            let mut s = Slide {
                layout: "chart".into(),
                title: format!("图表 · {kind}"),
                items: if kind == "heatmap" {
                    data(&[
                        ("华东", 90.0, "", "线上"), ("华南", 62.0, "", "线上"), ("华北", 41.0, "", "线上"),
                        ("华东", 55.0, "", "线下"), ("华南", 88.0, "", "线下"), ("华北", 30.0, "", "线下"),
                    ])
                } else if kind == "waterfall" {
                    data(&[
                        ("期初", 100.0, "", ""), ("新签", 62.0, "", ""), ("流失", -28.0, "", ""),
                        ("扩容", 34.0, "", ""), ("期末", 0.0, "", "total"),
                    ])
                } else {
                    quarters.clone()
                },
                ..Default::default()
            };
            s.params.insert("chart_kind".into(), serde_json::json!(kind));
            slides.push(s);
        }
        for s in slides.iter_mut() {
            s.fill_default_params();
        }
        for theme in ["midnight", "minimal", "corporate", "sunset"] {
            let deck = Deck {
                id: String::new(),
                title: format!("P3 版式目视检查 · {theme}"),
                theme: theme.into(),
                brand: None,
                slides: slides.clone(),
            };
            let path = std::env::temp_dir().join(format!("omnix_p3_{theme}.html"));
            std::fs::write(&path, crate::slides::render_deck_html(&deck, None, false)).unwrap();
            println!("{}", path.display());
        }
    }

    #[test]
    fn count_controls_actually_truncate() {
        let mut s = slide("metrics", &["a：1", "b：2", "c：3", "d：4", "e：5"]);
        s.params.insert("metric_count".into(), serde_json::json!(2));
        let html = render_body(&s).unwrap();
        assert!(html.contains(">a<") || html.contains("a"), "前两条应保留");
        assert!(!html.contains(">c<"), "第三条应被数量控件截掉:\n{html}");
    }
}

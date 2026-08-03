//! P0 体检：把渲染时会**静默咽下去**的问题一次报出来。
//!
//! 生成出来的演示，出错的地方几乎都是无声的：要点太多，超出的部分被
//! `.slide{overflow:hidden}` 直接裁掉；SWOT 只填了两格，另外两格空着照样出图；
//! 图表数据全是 0，画出一个空坐标系；数量控件把后面的条目截断了，而 JSON 里
//! 那些条目还在。**JSON 看起来完美无缺。** 写这份演示的人（或模型）没有任何
//! 办法发现，除非把每一页渲染出来挨个看——而那正是没人会做的事。
//!
//! ## 只报预算，不假装测量
//!
//! 这里报的全是**结构预算**（条数、字数、格子数），不是像素。按字数估算行高
//! 会跟渲染器漂移，而漂移的测量比没有测量更糟：它看起来权威，却悄悄是错的。
//! 真实溢出要真渲染才知道，那是另一件事，不混在这里说。
//!
//! 所以每条结论的措辞都是「超过预算」而不是「一定会溢出」——留白密度、分栏数、
//! 主题字号都会影响真实结果，预算只负责把**值得看一眼的页**挑出来。

use serde::Serialize;

use crate::slides::{Deck, Slide};
use crate::slides_layout::{param_int, param_text, ALL_LAYOUTS};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// 这一页现在就是坏的（空白页、数据版式没有数据）
    Error,
    /// 大概率不是你想要的（超预算、空格子、数据被截断）
    Warning,
    /// 提一句，不一定要改
    Info,
}

#[derive(Debug, Clone, Serialize)]
pub struct Finding {
    /// 稳定的机器可读码，前端据此分组、测试据此断言
    pub code: &'static str,
    pub severity: Severity,
    /// 0 起的页码；`None` = 整份演示层面的问题
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slide: Option<usize>,
    /// 一句话，读完就知道该怎么改
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct LintReport {
    /// 没有 error 且没有 warning
    pub ok: bool,
    pub errors: usize,
    pub warnings: usize,
    pub infos: usize,
    pub findings: Vec<Finding>,
}

/// 内容预算。数字是按 1280×720 画布、96px 边距、默认字号折出来的保守值——
/// 宁可放过几页，也不要天天报一堆没事的页把人练到不看。
struct Budget {
    /// 单栏能放几条要点
    bullets: usize,
    /// 一条要点多少字算长（超了通常要折行）
    bullet_chars: usize,
    /// 标题多少字算长
    title_chars: usize,
}

fn budget_for(layout: &str) -> Budget {
    match layout {
        // 封面/章节页的标题是大字号，能放的字少得多
        "cover" | "section" => Budget { bullets: 0, bullet_chars: 40, title_chars: 20 },
        "quote" => Budget { bullets: 0, bullet_chars: 40, title_chars: 40 },
        // 定格类与数据类的正文不走 bullets，只管标题
        "swot" | "matrix-2x2" | "porter" | "pest" | "bmc" => {
            Budget { bullets: 0, bullet_chars: 22, title_chars: 26 }
        }
        _ => Budget { bullets: 6, bullet_chars: 48, title_chars: 30 },
    }
}

/// 数据类版式的「数量控件」键——超过它的条目会被 `truncate` 悄悄丢掉。
fn count_control(layout: &str) -> Option<&'static str> {
    match layout {
        "metrics" => Some("metric_count"),
        "process" => Some("step_count"),
        "timeline" | "gantt" => Some("milestone_count"),
        "risk" => Some("risk_count"),
        "compare-table" => Some("row_count"),
        _ => None,
    }
}

/// 定格类版式的格子数。格子空着照样占版面，是很显眼的半成品。
fn slot_count(layout: &str) -> Option<usize> {
    match layout {
        "swot" | "matrix-2x2" | "pest" => Some(4),
        "porter" => Some(5),
        "bmc" => Some(9),
        _ => None,
    }
}

/// 字数按**字符**算。中文一字一格、英文一词多格，用字符数会高估英文的占位，
/// 但预算本就取保守值，宁可早提醒。
fn chars(s: &str) -> usize {
    s.chars().count()
}

pub fn lint_deck(deck: &Deck) -> LintReport {
    let mut f: Vec<Finding> = Vec::new();
    for (i, slide) in deck.slides.iter().enumerate() {
        lint_slide(i, slide, &mut f);
    }
    lint_whole(deck, &mut f);

    let errors = f.iter().filter(|x| x.severity == Severity::Error).count();
    let warnings = f.iter().filter(|x| x.severity == Severity::Warning).count();
    let infos = f.len() - errors - warnings;
    // 严重的排前面；同级按页码，读起来跟翻页顺序一致。
    f.sort_by_key(|x| (x.severity as u8, x.slide.unwrap_or(usize::MAX)));
    LintReport { ok: errors == 0 && warnings == 0, errors, warnings, infos, findings: f }
}

fn add(f: &mut Vec<Finding>, i: usize, code: &'static str, sev: Severity, msg: String) {
    f.push(Finding { code, severity: sev, slide: Some(i), message: msg });
}

fn lint_slide(i: usize, slide: &Slide, f: &mut Vec<Finding>) {
    let lay = slide.layout.as_str();
    let n = i + 1;

    if !ALL_LAYOUTS.contains(&lay) {
        add(f, i, "unknown-layout", Severity::Warning,
            format!("第 {n} 页的版式「{lay}」不在版式清单里，会退回通用的标题+要点样式渲染。"));
    }

    // ── 整页空白 ─────────────────────────────────────────────────────────
    let has_text = !slide.title.trim().is_empty()
        || !slide.subtitle.trim().is_empty()
        || !slide.body.trim().is_empty();
    let has_content = has_text
        || !slide.bullets.is_empty()
        || !slide.items.is_empty()
        || !slide.columns.is_empty()
        || !slide.image.trim().is_empty();
    if !has_content {
        add(f, i, "empty-slide", Severity::Error,
            format!("第 {n} 页什么内容都没有，会渲染成一张空白页。"));
        return; // 空页后面的检查都没意义
    }

    let b = budget_for(lay);

    // ── 标题/要点的字数与条数 ────────────────────────────────────────────
    if chars(&slide.title) > b.title_chars {
        add(f, i, "title-too-long", Severity::Warning,
            format!("第 {n} 页标题 {} 字，超过 {} 字的预算，可能折行挤掉正文空间。",
                    chars(&slide.title), b.title_chars));
    }
    if b.bullets > 0 && !slide.bullets.is_empty() {
        // 分栏能装更多；预算跟着「分栏数」控件走，跟渲染用的是同一个值。
        let columns = param_int(&slide.params, lay, "columns").max(1) as usize;
        let allowed = b.bullets * columns;
        if slide.bullets.len() > allowed {
            let extra = slide.bullets.len() - allowed;
            let hint = if columns == 1 {
                "——删几条，或把「分栏数」调到 2".to_string()
            } else {
                "——删几条，或拆成两页".to_string()
            };
            add(f, i, "bullets-over-budget", Severity::Warning,
                format!("第 {n} 页有 {} 条要点，{columns} 栏排布的预算是 {allowed} 条，多出 {extra} 条{hint}。",
                        slide.bullets.len()));
        }
        if let Some((k, long)) = slide
            .bullets
            .iter()
            .enumerate()
            .find(|(_, x)| chars(x) > b.bullet_chars)
        {
            add(f, i, "bullet-too-long", Severity::Warning,
                format!("第 {n} 页第 {} 条要点 {} 字（预算 {}）：「{}」——要点写成一行才是要点。",
                        k + 1, chars(long), b.bullet_chars, truncate_quote(long)));
        }
    }

    // ── 数量控件把条目悄悄截断 ───────────────────────────────────────────
    if let Some(key) = count_control(lay) {
        let shown = param_int(&slide.params, lay, key).max(0) as usize;
        let have = crate::slides_blocks::items_of(slide).len();
        if have > shown {
            add(f, i, "items-truncated", Severity::Warning,
                format!("第 {n} 页有 {have} 条数据，但「{}」设为 {shown}，后 {} 条不会显示——\
                         调大控件，或把多的删掉。", control_label(lay, key), have - shown));
        }
    }

    // ── 定格类版式的空格子 ───────────────────────────────────────────────
    if let Some(total) = slot_count(lay) {
        let filled = (0..total)
            .filter(|k| {
                slide.columns.get(*k).is_some_and(|c| {
                    !c.bullets.is_empty() || !c.body.trim().is_empty()
                })
            })
            .count();
        // 没给 columns 时会把 bullets 轮流分格，那种情况按 bullets 数估
        let filled = if slide.columns.is_empty() {
            slide.bullets.len().min(total)
        } else {
            filled
        };
        if filled < total {
            add(f, i, "empty-slot", Severity::Warning,
                format!("第 {n} 页的「{}」有 {} 格是空的（共 {total} 格）——空格子照样占版面，\
                         很显眼。", crate::slides_layout::layout_label(lay), total - filled));
        }
    }

    // ── 数据类版式没有数据 ───────────────────────────────────────────────
    let data_layout = count_control(lay).is_some() || lay == "chart";
    if data_layout {
        let items = crate::slides_blocks::items_of(slide);
        if items.is_empty() {
            add(f, i, "no-data", Severity::Error,
                format!("第 {n} 页是「{}」，但没有任何数据条目，正文区会是空的。",
                        crate::slides_layout::layout_label(lay)));
        } else if (lay == "chart" || lay == "metrics") && items.iter().all(|x| x.value == 0.0) {
            add(f, i, "all-zero-values", Severity::Warning,
                format!("第 {n} 页的数值全是 0，会画出一个没有信息的空图——\
                         要么补上真实数字，要么换成不需要数值的版式。"));
        }
    }

    // ── 风险页只列风险不给对策 ───────────────────────────────────────────
    if lay == "risk" {
        let items = crate::slides_blocks::items_of(slide);
        let naked = items.iter().filter(|x| x.detail.trim().is_empty()).count();
        if naked > 0 {
            add(f, i, "risk-without-plan", Severity::Warning,
                format!("第 {n} 页有 {naked} 条风险没写应对措施，页面上会显示「待补应对」——\
                         只列风险不给对策等于没说。"));
        }
    }

    // ── 开了配图位却没有图 ───────────────────────────────────────────────
    let slot = param_text(&slide.params, lay, "media_slot");
    if (slot == "left" || slot == "right") && slide.image.trim().is_empty() {
        add(f, i, "media-slot-empty", Severity::Info,
            format!("第 {n} 页留了配图位但还没有图，现在显示的是虚线占位框。"));
    }
    if (lay == "image" || lay == "image-left") && slide.image.trim().is_empty() {
        add(f, i, "media-slot-empty", Severity::Info,
            format!("第 {n} 页是图片版式却没有图，整块视觉区是空的。"));
    }

    // ── 演讲备注 ─────────────────────────────────────────────────────────
    if slide.notes.trim().is_empty() && lay != "cover" && lay != "section" {
        add(f, i, "notes-missing", Severity::Info,
            format!("第 {n} 页没有演讲备注，演讲者视图里这一页会是空的。"));
    }
}

/// 控件的中文名，报告里要跟界面上那个滑杆叫一样的名字。
fn control_label(layout: &str, key: &str) -> String {
    crate::slides_layout::controls_for(layout)
        .into_iter()
        .find(|c| c.key == key)
        .map(|c| c.label.to_string())
        .unwrap_or_else(|| key.to_string())
}

fn truncate_quote(s: &str) -> String {
    let clean = s.replace("**", "");
    if chars(&clean) <= 24 {
        return clean;
    }
    format!("{}…", clean.chars().take(24).collect::<String>())
}

fn lint_whole(deck: &Deck, f: &mut Vec<Finding>) {
    if deck.slides.is_empty() {
        f.push(Finding {
            code: "empty-deck",
            severity: Severity::Error,
            slide: None,
            message: "这份演示一页都没有。".to_string(),
        });
        return;
    }
    let first = &deck.slides[0];
    if first.layout != "cover" && first.role != "cover" {
        f.push(Finding {
            code: "no-cover",
            severity: Severity::Info,
            slide: Some(0),
            message: "第 1 页不是封面，观众开场看到的是正文。".to_string(),
        });
    }
    if deck.slides.len() > 25 {
        f.push(Finding {
            code: "deck-too-long",
            severity: Severity::Info,
            slide: None,
            message: format!("共 {} 页，偏长了——8~14 页通常更讲得完。", deck.slides.len()),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::slides::{Column, SlideItem};

    fn deck_of(slides: Vec<Slide>) -> Deck {
        let mut d = Deck {
            id: String::new(),
            title: "T".into(),
            theme: "midnight".into(),
            brand: None,
            slides,
        };
        for s in d.slides.iter_mut() {
            s.fill_default_params();
        }
        d
    }

    fn codes(deck: &Deck) -> Vec<&'static str> {
        lint_deck(deck).findings.into_iter().map(|x| x.code).collect()
    }

    /// 一份写得好的演示不该被报 error/warning，否则报告会被练到没人看。
    #[test]
    fn a_good_deck_is_clean() {
        let d = deck_of(vec![
            Slide {
                layout: "cover".into(),
                title: "季度复盘".into(),
                subtitle: "2026 Q2".into(),
                ..Default::default()
            },
            Slide {
                layout: "bullets".into(),
                title: "三件事".into(),
                bullets: vec!["渠道跑通了".into(), "成本降了两成".into(), "人手还差".into()],
                notes: "先讲结论".into(),
                ..Default::default()
            },
            Slide {
                layout: "metrics".into(),
                title: "关键指标".into(),
                items: vec![
                    SlideItem { label: "营收".into(), value: 654.0, detail: "+23%".into(), ..Default::default() },
                    SlideItem { label: "客户".into(), value: 1842.0, ..Default::default() },
                ],
                notes: "数字念一遍就好".into(),
                ..Default::default()
            },
        ]);
        let r = lint_deck(&d);
        assert!(r.ok, "干净的演示不该有问题：{:#?}", r.findings);
        assert_eq!(r.errors, 0);
        assert_eq!(r.warnings, 0);
    }

    #[test]
    fn catches_silently_clipped_bullets() {
        let d = deck_of(vec![Slide {
            layout: "bullets".into(),
            title: "太多了".into(),
            bullets: (1..=9).map(|i| format!("第 {i} 条")).collect(),
            ..Default::default()
        }]);
        assert!(codes(&d).contains(&"bullets-over-budget"));

        // 分栏能装更多——预算跟着渲染用的同一个控件走
        let mut d2 = d.clone();
        d2.slides[0].params.insert("columns".into(), serde_json::json!(2));
        assert!(!codes(&d2).contains(&"bullets-over-budget"), "两栏 12 条预算内");
    }

    /// 数量控件会 truncate，多出来的条目在 JSON 里还在、页面上没了。
    #[test]
    fn catches_items_truncated_by_the_count_control() {
        let mut s = Slide {
            layout: "metrics".into(),
            title: "指标".into(),
            items: (1..=6)
                .map(|i| SlideItem { label: format!("指标{i}"), value: i as f64, ..Default::default() })
                .collect(),
            notes: "n".into(),
            ..Default::default()
        };
        s.fill_default_params(); // metric_count 默认 3
        let d = deck_of(vec![s]);
        let msg = lint_deck(&d)
            .findings
            .into_iter()
            .find(|x| x.code == "items-truncated")
            .expect("应报数据被截断");
        assert!(msg.message.contains("指标数"), "要用界面上那个控件的名字: {}", msg.message);
        assert!(msg.message.contains("后 3 条"), "要说清丢了几条: {}", msg.message);
    }

    #[test]
    fn catches_half_finished_analysis_models() {
        let d = deck_of(vec![Slide {
            layout: "swot".into(),
            title: "SWOT".into(),
            columns: vec![
                Column { title: "优势".into(), bullets: vec!["渠道".into()], ..Default::default() },
                Column { title: "劣势".into(), bullets: vec!["品牌".into()], ..Default::default() },
            ],
            notes: "n".into(),
            ..Default::default()
        }]);
        let r = lint_deck(&d);
        let m = r.findings.iter().find(|x| x.code == "empty-slot").expect("应报空格子");
        assert!(m.message.contains("2 格是空的"), "{}", m.message);
    }

    #[test]
    fn catches_charts_that_say_nothing() {
        // 没数据 = error
        let d = deck_of(vec![Slide {
            layout: "chart".into(), title: "趋势".into(), notes: "n".into(), ..Default::default()
        }]);
        assert!(codes(&d).contains(&"no-data"));

        // 全 0 = warning（画得出来，但没信息）
        let d2 = deck_of(vec![Slide {
            layout: "chart".into(),
            title: "趋势".into(),
            items: (1..=3).map(|i| SlideItem { label: format!("Q{i}"), ..Default::default() }).collect(),
            notes: "n".into(),
            ..Default::default()
        }]);
        assert!(codes(&d2).contains(&"all-zero-values"));
    }

    #[test]
    fn catches_risk_without_a_plan() {
        let d = deck_of(vec![Slide {
            layout: "risk".into(),
            title: "风险".into(),
            items: vec![
                SlideItem { label: "供应商单点".into(), detail: "引入第二家".into(), ..Default::default() },
                SlideItem { label: "汇率".into(), ..Default::default() },
            ],
            notes: "n".into(),
            ..Default::default()
        }]);
        assert!(codes(&d).contains(&"risk-without-plan"));
    }

    #[test]
    fn catches_blank_pages_and_stops_there() {
        let d = deck_of(vec![Slide { layout: "bullets".into(), ..Default::default() }]);
        let c = codes(&d);
        assert!(c.contains(&"empty-slide"));
        // 空页不该再顺带报一串「没有备注」之类的噪音
        assert!(!c.contains(&"notes-missing"), "空页只报一条就够了: {c:?}");
    }

    /// 严重的排前面——报告是给人从上往下读的。
    #[test]
    fn findings_are_sorted_by_severity_then_page() {
        let d = deck_of(vec![
            Slide { layout: "bullets".into(), title: "有内容".into(), bullets: vec!["a".into()], ..Default::default() },
            Slide { layout: "chart".into(), title: "空图".into(), notes: "n".into(), ..Default::default() },
        ]);
        let sev: Vec<Severity> = lint_deck(&d).findings.iter().map(|x| x.severity).collect();
        let mut sorted = sev.clone();
        sorted.sort_by_key(|s| *s as u8);
        assert_eq!(sev, sorted, "结论必须按严重程度排序");
    }

    /// 每条结论都要能单独读懂：带页码、给得出下一步。
    #[test]
    fn every_message_names_the_page_and_is_actionable() {
        let d = deck_of(vec![
            Slide { layout: "swot".into(), title: "半成品".into(), ..Default::default() },
            Slide { layout: "编的版式".into(), title: "x".into(), bullets: vec!["y".into()], ..Default::default() },
        ]);
        for x in lint_deck(&d).findings {
            if x.slide.is_some() {
                assert!(x.message.contains('页'), "结论要说清是哪一页: {}", x.message);
            }
            assert!(x.message.chars().count() > 10, "结论太短，读不出该做什么: {}", x.message);
        }
    }
}

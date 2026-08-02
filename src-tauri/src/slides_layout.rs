//! 版式目录：页面角色（P1）与参数化控件契约（P2）。
//!
//! 从 slides.rs 分出来，因为这两件事会持续长大：角色决定「这一页在讲什么」，
//! 控件决定「这一页有哪些可以不调模型就改的旋钮」。
//!
//! 设计取舍：控件值存在 `Slide.params`（键值对），渲染时读取；模型只负责填
//! 文案和挑一个初始值。用户拖滑杆改的是 params，本地立即重渲染——不再为
//! 「把三栏改成四栏」这种事跑一次模型。

use serde::Serialize;

// ─────────────────────────────────────────────────────────────────────────
// P1 · 页面角色
// ─────────────────────────────────────────────────────────────────────────

/// 一页在叙事里承担的功能。大纲阶段就定下来，模型据此挑版式，
/// 而不是事后把什么内容都套进通用 bullets。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct PageRole {
    /// 稳定标识（写进 deck JSON）
    pub key: &'static str,
    /// 中文名（界面显示）
    pub label: &'static str,
    /// 这个角色适合的版式，第一个是默认
    pub layouts: &'static [&'static str],
    /// 给模型看的用途说明
    pub intent: &'static str,
}

pub const PAGE_ROLES: &[PageRole] = &[
    PageRole { key: "cover", label: "封面", layouts: &["cover"], intent: "开场：主题、副标题、场合" },
    PageRole { key: "agenda", label: "目录", layouts: &["bullets", "two-column"], intent: "全篇脉络，让听众知道要走哪几步" },
    PageRole { key: "section", label: "章节页", layouts: &["section"], intent: "换章过渡，只有章节名和一句引子" },
    PageRole { key: "background", label: "背景", layouts: &["content", "bullets"], intent: "问题从哪来、为什么现在谈" },
    PageRole { key: "metric", label: "关键指标", layouts: &["metrics", "bullets"], intent: "少数几个数字说明现状或成果" },
    PageRole { key: "trend", label: "趋势", layouts: &["chart", "content"], intent: "随时间变化，强调方向而非精确值" },
    PageRole { key: "compare", label: "对比", layouts: &["two-column", "compare-table"], intent: "两个及以上方案/时期的并列比较" },
    PageRole { key: "process", label: "流程", layouts: &["process", "bullets"], intent: "按顺序推进的步骤或阶段" },
    PageRole { key: "matrix", label: "分析模型", layouts: &["swot", "matrix-2x2", "porter", "pest", "bmc"], intent: "结构化框架：SWOT、四象限、五力、PEST、商业模式画布" },
    PageRole { key: "timeline", label: "时间线", layouts: &["timeline", "gantt"], intent: "里程碑与排期" },
    PageRole { key: "risk", label: "风险", layouts: &["risk", "two-column"], intent: "风险项 + 影响 + 应对，不要只列风险不给对策" },
    PageRole { key: "case", label: "案例", layouts: &["image-left", "content"], intent: "具体例子佐证观点" },
    PageRole { key: "quote", label: "引述", layouts: &["quote"], intent: "一句有分量的话，配出处" },
    PageRole { key: "image", label: "图片页", layouts: &["image", "image-left"], intent: "以视觉为主，文字只作注解" },
    PageRole { key: "summary", label: "小结", layouts: &["bullets", "metrics"], intent: "收敛已讲内容，回扣主线" },
    PageRole { key: "action", label: "行动项", layouts: &["process", "bullets"], intent: "谁、做什么、什么时候之前" },
    PageRole { key: "closing", label: "结尾", layouts: &["section", "cover"], intent: "致谢/联系方式/下一步" },
];

pub fn role(key: &str) -> Option<&'static PageRole> {
    PAGE_ROLES.iter().find(|r| r.key == key)
}

/// 角色的默认版式；角色未知时回落到 content，保证永远渲染得出东西。
pub fn default_layout_for_role(role_key: &str) -> &'static str {
    role(role_key)
        .and_then(|r| r.layouts.first().copied())
        .unwrap_or("content")
}

/// 给大纲提示词用的角色清单。
pub fn roles_for_prompt() -> String {
    PAGE_ROLES
        .iter()
        .map(|r| format!("- {}（{}）: {}", r.key, r.label, r.intent))
        .collect::<Vec<_>>()
        .join("\n")
}

// ─────────────────────────────────────────────────────────────────────────
// P2 · 控件契约
// ─────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ControlKind {
    /// 整数滑杆（模块数量、列数、字号档位）
    Range,
    /// 开关（显示/隐藏某个元素）
    Toggle,
    /// 有限选项下拉（强调位置、图表类型、密度）
    Select,
}

#[derive(Debug, Clone, Serialize)]
pub struct Control {
    pub key: &'static str,
    pub label: &'static str,
    pub kind: ControlKind,
    /// Range 用，Select/Toggle 忽略
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<i64>,
    /// 默认值：Range 是整数、Toggle 是 true/false、Select 是选项 key
    pub default: ControlValue,
    /// Select 的可选项
    #[serde(skip_serializing_if = "<[_]>::is_empty")]
    pub options: &'static [(&'static str, &'static str)],
    /// 一句话说明，界面 tooltip 和模型都会看到
    pub desc: &'static str,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum ControlValue {
    Int(i64),
    Bool(bool),
    Text(String),
}

impl ControlValue {
    pub fn as_int(&self) -> Option<i64> {
        match self {
            ControlValue::Int(v) => Some(*v),
            _ => None,
        }
    }
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            ControlValue::Bool(v) => Some(*v),
            _ => None,
        }
    }
    pub fn as_text(&self) -> Option<&str> {
        match self {
            ControlValue::Text(v) => Some(v),
            _ => None,
        }
    }
}

const fn range(
    key: &'static str,
    label: &'static str,
    min: i64,
    max: i64,
    default: i64,
    desc: &'static str,
) -> Control {
    Control {
        key,
        label,
        kind: ControlKind::Range,
        min: Some(min),
        max: Some(max),
        default: ControlValue::Int(default),
        options: &[],
        desc,
    }
}

const fn toggle(key: &'static str, label: &'static str, default: bool, desc: &'static str) -> Control {
    Control {
        key,
        label,
        kind: ControlKind::Toggle,
        min: None,
        max: None,
        default: ControlValue::Bool(default),
        options: &[],
        desc,
    }
}

fn select(
    key: &'static str,
    label: &'static str,
    options: &'static [(&'static str, &'static str)],
    default: &str,
    desc: &'static str,
) -> Control {
    Control {
        key,
        label,
        kind: ControlKind::Select,
        min: None,
        max: None,
        default: ControlValue::Text(default.to_string()),
        options,
        desc,
    }
}

const DENSITY: &[(&str, &str)] = &[("loose", "疏"), ("normal", "适中"), ("tight", "密")];
const EMPHASIS: &[(&str, &str)] = &[("none", "不强调"), ("first", "首项"), ("last", "末项")];
const CHART_KINDS: &[(&str, &str)] = &[
    ("bar", "柱状"),
    ("line", "折线"),
    ("area", "面积"),
    ("radar", "雷达"),
    ("funnel", "漏斗"),
    ("waterfall", "瀑布"),
    ("treemap", "矩形树图"),
    ("heatmap", "热力图"),
    ("gantt", "甘特"),
];

/// 所有版式共有的控件。
fn common_controls() -> Vec<Control> {
    vec![
        select("density", "留白密度", DENSITY, "normal", "内容与留白的比例"),
        toggle("show_page_number", "显示页码", true, "右下角页码"),
    ]
}

/// 某个版式暴露的控件（含通用控件）。未知版式只给通用控件——
/// 模型编出没见过的 layout 时也不会炸。
pub fn controls_for(layout: &str) -> Vec<Control> {
    let mut out = match layout {
        "cover" => vec![
            select("align", "对齐", &[("left", "左对齐"), ("center", "居中")], "left", "标题区对齐方式"),
            toggle("show_accent", "显示装饰条", true, "标题上方的品牌色装饰"),
        ],
        "bullets" | "content" => vec![
            range("columns", "分栏数", 1, 3, 1, "要点分几栏排布"),
            select("emphasis", "强调项", EMPHASIS, "none", "把某一条要点放大强调"),
            toggle("show_index", "显示序号", false, "给每条要点加编号"),
        ],
        "two-column" => vec![
            range("column_count", "栏数", 2, 4, 2, "并列几栏"),
            toggle("show_divider", "显示分隔线", true, "栏与栏之间的竖线"),
            select("balance", "栏宽", &[("equal", "等宽"), ("left-heavy", "左宽"), ("right-heavy", "右宽")], "equal", "各栏宽度分配"),
        ],
        "metrics" => vec![
            range("metric_count", "指标数", 2, 6, 3, "展示几个关键数字"),
            toggle("show_delta", "显示同比/环比", true, "数字下方的变化幅度"),
        ],
        "process" => vec![
            range("step_count", "步骤数", 3, 7, 4, "流程分几步"),
            select("direction", "方向", &[("horizontal", "横向"), ("vertical", "纵向")], "horizontal", "流程排列方向"),
            toggle("show_arrow", "显示箭头", true, "步骤之间的连接箭头"),
        ],
        "chart" => vec![
            select("chart_kind", "图表类型", CHART_KINDS, "bar", "用哪种图表呈现"),
            toggle("show_values", "显示数值", true, "在图元上标注数值"),
            toggle("show_legend", "显示图例", true, "图表下方的系列说明"),
        ],
        "compare-table" => vec![
            range("row_count", "对比项数", 2, 8, 4, "表格行数"),
            toggle("highlight_winner", "高亮优胜", false, "标出每行更优的一侧"),
        ],
        "swot" | "matrix-2x2" => vec![
            toggle("show_axis_labels", "显示坐标轴标签", true, "四象限的两条轴说明"),
            select("emphasis_quadrant", "强调象限", &[("none", "不强调"), ("q1", "左上"), ("q2", "右上"), ("q3", "左下"), ("q4", "右下")], "none", "突出其中一个象限"),
        ],
        "porter" => vec![toggle("show_center", "显示中心力", true, "中心的「同业竞争」块")],
        "pest" => vec![select("layout_mode", "排布", &[("grid", "田字格"), ("row", "横排")], "grid", "四个维度怎么摆")],
        "bmc" => vec![toggle("compact", "紧凑模式", false, "九宫格压缩到一屏")],
        "timeline" | "gantt" => vec![
            range("milestone_count", "节点数", 3, 8, 5, "时间线上的里程碑数量"),
            toggle("show_today", "显示今天", false, "标出当前时间位置"),
        ],
        "risk" => vec![
            range("risk_count", "风险项数", 2, 6, 3, "列出几条风险"),
            toggle("show_severity", "显示等级", true, "高/中/低标记"),
        ],
        "image" | "image-left" => vec![
            select("fit", "填充方式", &[("cover", "裁切填满"), ("contain", "完整显示")], "cover", "图片如何适配画面"),
            range("caption_size", "注解字号", 0, 2, 1, "0=不显示注解"),
        ],
        _ => vec![],
    };
    out.extend(common_controls());
    out
}

/// 版式的控件默认值——新建页面时写进 params，用户没调过也有确定的渲染结果。
pub fn default_params(layout: &str) -> serde_json::Map<String, serde_json::Value> {
    let mut map = serde_json::Map::new();
    for c in controls_for(layout) {
        let v = match &c.default {
            ControlValue::Int(i) => serde_json::json!(i),
            ControlValue::Bool(b) => serde_json::json!(b),
            ControlValue::Text(t) => serde_json::json!(t),
        };
        map.insert(c.key.to_string(), v);
    }
    map
}

/// 读一个整数参数，越界或类型不对时回落到该版式的默认值。
/// 渲染层永远不该因为脏参数而崩或画歪。
pub fn param_int(params: &serde_json::Map<String, serde_json::Value>, layout: &str, key: &str) -> i64 {
    let controls = controls_for(layout);
    let ctrl = controls.iter().find(|c| c.key == key);
    let fallback = ctrl.and_then(|c| c.default.as_int()).unwrap_or(0);
    let raw = params.get(key).and_then(|v| v.as_i64()).unwrap_or(fallback);
    match ctrl {
        Some(c) => raw.clamp(c.min.unwrap_or(i64::MIN), c.max.unwrap_or(i64::MAX)),
        None => raw,
    }
}

pub fn param_bool(params: &serde_json::Map<String, serde_json::Value>, layout: &str, key: &str) -> bool {
    let controls = controls_for(layout);
    let fallback = controls
        .iter()
        .find(|c| c.key == key)
        .and_then(|c| c.default.as_bool())
        .unwrap_or(false);
    params.get(key).and_then(|v| v.as_bool()).unwrap_or(fallback)
}

pub fn param_text(params: &serde_json::Map<String, serde_json::Value>, layout: &str, key: &str) -> String {
    let controls = controls_for(layout);
    let ctrl = controls.iter().find(|c| c.key == key);
    let fallback = ctrl
        .and_then(|c| c.default.as_text())
        .unwrap_or("")
        .to_string();
    let raw = params
        .get(key)
        .and_then(|v| v.as_str())
        .unwrap_or(&fallback)
        .to_string();
    // 脏值回落：不在选项里的一律用默认，避免渲染出未定义分支。
    match ctrl {
        Some(c) if !c.options.is_empty() => {
            if c.options.iter().any(|(k, _)| *k == raw) {
                raw
            } else {
                fallback
            }
        }
        _ => raw,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_role_has_a_renderable_layout() {
        for r in PAGE_ROLES {
            assert!(!r.layouts.is_empty(), "角色 {} 没有版式", r.key);
            assert_eq!(default_layout_for_role(r.key), r.layouts[0]);
        }
        // 未知角色也要有确定回落，模型编造角色名时不能开天窗。
        assert_eq!(default_layout_for_role("nonsense"), "content");
    }

    #[test]
    fn controls_always_include_common_ones() {
        for layout in ["cover", "bullets", "chart", "swot", "完全没见过的版式"] {
            let keys: Vec<&str> = controls_for(layout).iter().map(|c| c.key).collect();
            assert!(keys.contains(&"density"), "{layout} 缺通用控件");
            assert!(keys.contains(&"show_page_number"), "{layout} 缺通用控件");
        }
    }

    #[test]
    fn default_params_cover_every_control() {
        let params = default_params("process");
        for c in controls_for("process") {
            assert!(params.contains_key(c.key), "默认值漏了 {}", c.key);
        }
    }

    #[test]
    fn dirty_params_fall_back_instead_of_breaking_render() {
        let mut p = serde_json::Map::new();
        p.insert("step_count".into(), serde_json::json!(999)); // 越界
        p.insert("direction".into(), serde_json::json!("斜着")); // 不在选项里
        p.insert("show_arrow".into(), serde_json::json!("不是布尔"));

        assert_eq!(param_int(&p, "process", "step_count"), 7, "越界应被夹到 max");
        assert_eq!(param_text(&p, "process", "direction"), "horizontal", "非法选项回落默认");
        assert!(param_bool(&p, "process", "show_arrow"), "类型不对回落默认");
        // 缺失的键也要给默认
        assert_eq!(param_int(&serde_json::Map::new(), "metrics", "metric_count"), 3);
    }
}

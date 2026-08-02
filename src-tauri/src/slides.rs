//! Presentation / PPT model + renderer.
//!
//! Design goal (user request #4): make good-looking, *editable* slide decks
//! where small tweaks are deterministic — not "hope the agent understands".
//!
//! The **single source of truth is a structured JSON `Deck`** (layout + typed
//! fields per slide), never an image. Both the live preview and the export use
//! the SAME canonical renderer here (`render_deck_html`), so what you see is
//! exactly what you export, and an AI edit is a surgical change to one field of
//! the model followed by a deterministic re-render.

use serde::{Deserialize, Serialize};

// ─────────────────────────────────────────────────────────────────────────
// Model
// ─────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Deck {
    #[serde(default)]
    pub id: String,
    #[serde(default = "default_title")]
    pub title: String,
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default)]
    pub slides: Vec<Slide>,
    /// 母版/品牌覆盖（D）：在 theme 之上覆盖主色/字体/Logo/页脚。
    /// `None` = 纯用内置主题。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub brand: Option<Brand>,
}

/// 品牌母版（D）：一份可复用的视觉覆盖。空字段表示"用主题默认值"。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Brand {
    #[serde(default)]
    pub name: String,
    /// 标题/强调色（#rrggbb）
    #[serde(default)]
    pub primary: String,
    /// 项目符号/装饰条颜色
    #[serde(default)]
    pub accent: String,
    /// 幻灯背景（单色或 CSS 渐变值）
    #[serde(default)]
    pub background: String,
    /// 正文颜色
    #[serde(default)]
    pub text: String,
    /// CSS font-family
    #[serde(default)]
    pub font: String,
    /// Logo 图片（本地路径或 http URL），显示在右上角
    #[serde(default)]
    pub logo: String,
    /// 页脚文字（左下角）
    #[serde(default)]
    pub footer: String,
}

// ── 大纲（A：两阶段生成）────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutlineItem {
    /// P1 页面角色。模型先挑角色（这页干什么），版式由角色推导——
    /// 模型漏填 layout 或编造一个没见过的版式时，角色是可靠的兜底。
    #[serde(default)]
    pub role: String,
    /// 缺省是**空**而不是 content——空才能让 `normalize` 用角色推导出版式。
    /// 默认成 content 的话，模型只填角色不填版式时角色的推荐就白给了。
    #[serde(default)]
    pub layout: String,
    #[serde(default)]
    pub title: String,
    /// 这一页要讲的要点提纲（展开阶段据此生成正式内容）
    #[serde(default)]
    pub points: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Outline {
    #[serde(default = "default_title")]
    pub title: String,
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default)]
    pub items: Vec<OutlineItem>,
}

impl Outline {
    /// 补齐每一项的版式：模型漏填、或编了一个不存在的版式时，用角色的推荐兜底
    /// （角色也不认识就退到 content）。生成大纲和展开大纲都要过这一道，
    /// 因为大纲既可能来自模型，也可能被前端改过。
    pub fn normalize(&mut self) {
        for item in self.items.iter_mut() {
            let known = crate::slides_layout::ALL_LAYOUTS.contains(&item.layout.as_str());
            if !known {
                item.layout = crate::slides_layout::default_layout_for_role(&item.role).to_string();
            }
        }
        if !THEMES.contains(&self.theme.as_str()) {
            self.theme = default_theme();
        }
    }
}

/// One slide. `layout` selects how the typed fields are arranged; unknown
/// layouts fall back to a generic title+content render so a model that invents
/// a layout name never produces a blank slide.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Slide {
    #[serde(default = "default_layout")]
    pub layout: String,
    #[serde(default)]
    pub title: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub subtitle: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bullets: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub body: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub columns: Vec<Column>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub image: String,
    /// Speaker notes — shown in the editor, not on the slide face.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub notes: String,
    /// P1 页面角色（cover/metric/risk…）：这一页在叙事里干什么。大纲阶段定下，
    /// 决定默认版式，也让模型知道该往里放什么内容。空 = 未指定。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub role: String,
    /// P2 控件值：模块数量、强调项、图表类型这类可以不调模型就改的参数。
    /// 键取自 `slides_layout::controls_for(layout)`；脏值在读取时回落默认。
    #[serde(default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub params: serde_json::Map<String, serde_json::Value>,
    /// P3 结构化数据：指标/图表/流程/时间线/风险/对比表共用的一张表。
    /// 空着也没关系——`slides_blocks` 会从 `bullets` 解析出来。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<SlideItem>,
}

/// 一条结构化数据。一张表喂全部数据类版式，不给每个版式发明字段：
/// 柱状图的 `label/value`、流程的 `label/detail`、风险的 `label/detail/group`
/// （group=等级）、甘特的 `value/span`（起始/时长）、热力图的 `group/label/value`
/// （行/列/强度）都落在这五个字段上。
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct SlideItem {
    #[serde(default)]
    pub label: String,
    /// 主数值：柱高、指标数字、甘特起点、热力强度
    #[serde(default)]
    pub value: f64,
    /// 第二个数值：目前只有甘特用（条长）
    #[serde(default, skip_serializing_if = "is_zero")]
    pub span: f64,
    /// 说明文本：同比、步骤描述、应对措施、日期
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub detail: String,
    /// 分组：图表系列、风险等级、热力图行、对比表优胜列
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub group: String,
}

fn is_zero(v: &f64) -> bool {
    *v == 0.0
}

impl Slide {
    /// 补齐该版式缺失的控件默认值——旧 deck 或模型没给 params 时，
    /// 渲染和控件面板都要有确定的值可用。
    pub fn fill_default_params(&mut self) {
        for (k, v) in crate::slides_layout::default_params(&self.layout) {
            self.params.entry(k).or_insert(v);
        }
    }

    /// 只保留文案的字段清单之外的一切（版式、参数、图片槽）都算「模板属性」。
    /// 模板锁就是把这些复原。
    pub fn restore_template_from(&mut self, original: &Slide) {
        self.layout = original.layout.clone();
        self.role = original.role.clone();
        self.params = original.params.clone();
        self.image = original.image.clone();
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Column {
    #[serde(default)]
    pub title: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bullets: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub body: String,
}

fn default_title() -> String {
    "未命名演示".to_string()
}
fn default_theme() -> String {
    "midnight".to_string()
}
fn default_layout() -> String {
    "content".to_string()
}

pub const THEMES: &[&str] = &["midnight", "minimal", "corporate", "sunset"];

// ─────────────────────────────────────────────────────────────────────────
// Parsing model output → Deck
// ─────────────────────────────────────────────────────────────────────────

/// Extract a JSON object from a model reply that may be wrapped in prose or a
/// ```json fence. Returns the substring from the first `{` to its matching `}`.
pub fn extract_json(raw: &str) -> Option<String> {
    let s = raw.trim();
    // Strip a leading ```json / ``` fence if present.
    let s = s
        .trim_start_matches("```json")
        .trim_start_matches("```JSON")
        .trim_start_matches("```")
        .trim();
    let start = s.find('{')?;
    let bytes = s.as_bytes();
    let mut depth = 0i32;
    let mut in_str = false;
    let mut escaped = false;
    for (i, &b) in bytes.iter().enumerate().skip(start) {
        if in_str {
            if escaped {
                escaped = false;
            } else if b == b'\\' {
                escaped = true;
            } else if b == b'"' {
                in_str = false;
            }
            continue;
        }
        match b {
            b'"' => in_str = true,
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(s[start..=i].to_string());
                }
            }
            _ => {}
        }
    }
    None
}

/// Parse a (possibly fenced/prose-wrapped) model reply into a `Deck`.
pub fn parse_deck(raw: &str) -> Result<Deck, String> {
    let json = extract_json(raw).ok_or_else(|| "回复里找不到 JSON 幻灯模型".to_string())?;
    let mut deck: Deck =
        serde_json::from_str(&json).map_err(|e| format!("幻灯 JSON 解析失败: {e}"))?;
    if deck.slides.is_empty() {
        return Err("生成的演示没有任何幻灯页".to_string());
    }
    if !THEMES.contains(&deck.theme.as_str()) {
        deck.theme = default_theme();
    }
    Ok(deck)
}

// ─────────────────────────────────────────────────────────────────────────
// Rendering — the ONE canonical renderer (preview == export)
// ─────────────────────────────────────────────────────────────────────────

pub(crate) fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Minimal inline formatting: `**bold**` → <strong>, plus HTML-escape. Keeps
/// slide text safe (content comes from a model) while allowing light emphasis.
pub(crate) fn inline(s: &str) -> String {
    let escaped = esc(s);
    let mut out = String::with_capacity(escaped.len());
    let mut rest = escaped.as_str();
    while let Some(open) = rest.find("**") {
        out.push_str(&rest[..open]);
        let after = &rest[open + 2..];
        if let Some(close) = after.find("**") {
            out.push_str("<strong>");
            out.push_str(&after[..close]);
            out.push_str("</strong>");
            rest = &after[close + 2..];
        } else {
            out.push_str("**");
            rest = after;
        }
    }
    out.push_str(rest);
    out
}

fn bullets_html(bullets: &[String]) -> String {
    bullets_html_with(bullets, 1, "none", false)
}

/// 受控件驱动的要点渲染（P2）：分栏数、强调项、序号都是参数，
/// 用户拖控件即时生效，不需要再跑一次模型。
fn bullets_html_with(bullets: &[String], columns: i64, emphasis: &str, show_index: bool) -> String {
    if bullets.is_empty() {
        return String::new();
    }
    let last = bullets.len().saturating_sub(1);
    let items: String = bullets
        .iter()
        .enumerate()
        .map(|(i, b)| {
            let strong = (emphasis == "first" && i == 0) || (emphasis == "last" && i == last);
            let cls = if strong { " class=\"em\"" } else { "" };
            let idx = if show_index {
                format!("<span class=\"idx\">{}</span>", i + 1)
            } else {
                String::new()
            };
            format!("<li{cls}>{idx}{}</li>", inline(b))
        })
        .collect();
    let col_style = if columns > 1 {
        format!(" style=\"column-count:{columns}\"")
    } else {
        String::new()
    };
    format!("<ul class=\"bullets\"{col_style}>{items}</ul>")
}

/// Render one slide as an inner HTML fragment (without the outer `<section>`).
/// Resolve an image reference for embedding (C). `http(s)` URLs pass through;
/// a local path is read and inlined as a `data:` URI so the preview iframe and
/// the exported HTML/PDF are all self-contained (no asset-protocol needed).
/// Unreadable paths yield an empty string — a missing image never breaks a slide.
pub(crate) fn image_src(reference: &str) -> String {
    let r = reference.trim();
    if r.is_empty() || r.starts_with("http://") || r.starts_with("https://") || r.starts_with("data:")
    {
        return r.to_string();
    }
    let path = std::path::Path::new(r);
    let mime = match path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        _ => "image/png",
    };
    match std::fs::read(path) {
        Ok(bytes) => {
            use base64::Engine as _;
            format!(
                "data:{mime};base64,{}",
                base64::engine::general_purpose::STANDARD.encode(bytes)
            )
        }
        Err(_) => String::new(),
    }
}

/// Brand overrides → CSS custom properties + rules layered after the theme (D).
fn brand_css(brand: &Brand) -> String {
    let mut css = String::new();
    if !brand.font.trim().is_empty() {
        css.push_str(&format!("body{{font-family:{};}}", brand.font));
    }
    if !brand.background.trim().is_empty() {
        css.push_str(&format!(".slide{{background:{};}}", brand.background));
    }
    if !brand.text.trim().is_empty() {
        css.push_str(&format!(".slide{{color:{};}}", brand.text));
    }
    if !brand.primary.trim().is_empty() {
        // Override the theme's gradient title with a flat brand color.
        css.push_str(&format!(
            ".s-title,.quote blockquote{{background:none;-webkit-text-fill-color:{c};color:{c};}}.col h2{{color:{c};}}",
            c = brand.primary
        ));
    }
    if !brand.accent.trim().is_empty() {
        // --acc 同时驱动 P3 的图表与分析模型配色，母版换色时图表跟着换，
        // 不会出现「版面是品牌色、图表还是主题色」的脱节。
        css.push_str(&format!(
            ".bullets li:before,.accent{{background:{c};}}.slide{{--acc:{c};}}",
            c = brand.accent
        ));
    }
    if !brand.logo.trim().is_empty() {
        css.push_str(".brand-logo{position:absolute;top:32px;right:40px;max-height:44px;max-width:180px;object-fit:contain;}");
    }
    if !brand.footer.trim().is_empty() {
        css.push_str(".brand-footer{position:absolute;bottom:28px;left:96px;font-size:16px;opacity:.55;}");
    }
    css
}

fn render_slide_inner(slide: &Slide) -> String {
    use crate::slides_layout::{param_bool, param_int, param_text};
    let lay = slide.layout.as_str();
    let p = &slide.params;
    let title = if slide.title.is_empty() {
        String::new()
    } else {
        format!("<h1 class=\"s-title\">{}</h1>", inline(&slide.title))
    };
    // 图片版式的副标题就是图注，字号档位 0 = 不显示。
    let caption_size = param_int(p, lay, "caption_size");
    let hide_caption = (lay == "image" || lay == "image-left") && caption_size == 0;
    let subtitle = if slide.subtitle.is_empty() || hide_caption {
        String::new()
    } else {
        let cls = if lay == "image" || lay == "image-left" {
            format!(" cap-{caption_size}")
        } else {
            String::new()
        };
        format!("<p class=\"s-sub{cls}\">{}</p>", inline(&slide.subtitle))
    };
    let body = if slide.body.is_empty() {
        String::new()
    } else {
        let paras: String = slide
            .body
            .split('\n')
            .filter(|l| !l.trim().is_empty())
            .map(|l| format!("<p>{}</p>", inline(l)))
            .collect();
        format!("<div class=\"s-body\">{paras}</div>")
    };
    // 媒体槽：图还没配好也把位置**留出来**（虚线占位框）。这样先排版后配图，
    // 图片落位时版面不会整个跳一次——空 image 直接不渲染才是版面会跳的原因。
    let media = |reserve: bool| -> String {
        let src = image_src(&slide.image);
        if !src.is_empty() {
            // fit 决定图片是裁切填满还是完整显示（构图重要时选后者）。
            let fit = param_text(p, lay, "fit");
            let style = if fit == "contain" || fit == "cover" {
                format!(" style=\"object-fit:{fit}\"")
            } else {
                String::new()
            };
            format!(
                "<div class=\"s-image\"><img src=\"{}\"{style} alt=\"\"/></div>",
                esc(&src)
            )
        } else if reserve {
            "<div class=\"s-image ph\"><span>图片位 · 点「配图」生成</span></div>".to_string()
        } else {
            String::new()
        }
    };

    // P3 结构版式（分析模型 / 图表）自带内容区，标题行仍由这里统一出，
    // 保证所有版式的标题排版一致。
    if let Some(block) = crate::slides_blocks::render_body(slide) {
        return format!("<div class=\"box content block\">{title}{subtitle}{block}</div>");
    }

    match slide.layout.as_str() {
        "cover" => {
            let accent = if param_bool(p, lay, "show_accent") {
                "<div class=\"accent\"></div>"
            } else {
                ""
            };
            let align = param_text(p, lay, "align");
            format!("<div class=\"box cover align-{align}\">{accent}{title}{subtitle}</div>")
        }
        "section" => format!("<div class=\"box section\">{title}{subtitle}</div>"),
        "quote" => format!(
            "<div class=\"box quote\"><blockquote>{}</blockquote>{}</div>",
            inline(&slide.body),
            if slide.subtitle.is_empty() {
                String::new()
            } else {
                format!("<cite>— {}</cite>", inline(&slide.subtitle))
            }
        ),
        // bullets 和 content 共用同一套要点控件，所以要点渲染也必须共用——
        // 各写各的就会出现「content 上拖分栏没反应」。
        "bullets" => {
            let list = tuned_bullets(&slide.bullets, p, lay);
            with_media_slot(p, lay, &media, format!("{title}{subtitle}{list}"))
        }
        "two-column" => {
            let cols: String = slide
                .columns
                .iter()
                .map(|c| {
                    format!(
                        "<div class=\"col\"><h2>{}</h2>{}{}</div>",
                        inline(&c.title),
                        if c.body.is_empty() {
                            String::new()
                        } else {
                            format!("<p>{}</p>", inline(&c.body))
                        },
                        bullets_html(&c.bullets)
                    )
                })
                .collect();
            let divider = if param_bool(p, lay, "show_divider") { " divided" } else { "" };
            let balance = param_text(p, lay, "balance");
            // 栏数控件必须真的改网格，否则第 3、4 栏会掉到下一行——
            // 栏宽偏置只有两栏时说得通，多栏一律等宽。
            let n = param_int(p, lay, "column_count").max(1);
            let grid = if n == 2 {
                String::new()
            } else {
                format!(" style=\"grid-template-columns:repeat({n},1fr)\"")
            };
            let bal = if n == 2 { balance } else { "equal".to_string() };
            format!(
                "<div class=\"box content\">{title}{subtitle}<div class=\"cols{divider} bal-{bal}\"{grid}>{cols}</div></div>"
            )
        }
        // 图片版式的槽位是版式自带的，永远预留。
        "image" => format!(
            "<div class=\"box image-layout\">{title}{subtitle}{}</div>",
            media(true)
        ),
        "image-left" => format!(
            "<div class=\"box split\">{}<div class=\"split-text\">{title}{subtitle}{}{body}</div></div>",
            media(true),
            bullets_html(&slide.bullets)
        ),
        // "content" and any unknown layout: generic title + subtitle + bullets + body.
        _ => {
            let list = tuned_bullets(&slide.bullets, p, lay);
            with_media_slot(p, lay, &media, format!("{title}{subtitle}{list}{body}"))
        }
    }
}

/// 受控件驱动的要点渲染。没有这些控件的版式（param_* 会回落到 0/""/false）
/// 得到的就是朴素列表，所以对所有版式都能安全调用。
fn tuned_bullets(
    bullets: &[String],
    p: &serde_json::Map<String, serde_json::Value>,
    layout: &str,
) -> String {
    use crate::slides_layout::{param_bool, param_int, param_text};
    bullets_html_with(
        bullets,
        param_int(p, layout, "columns").max(1),
        &param_text(p, layout, "emphasis"),
        param_bool(p, layout, "show_index"),
    )
}

/// 把内容和媒体槽拼成一页。图片**只由这里出**——`content` 不该自带图片，
/// 否则开了槽位就会画两张。
///
/// `media_slot` = none（或该版式没有这个控件）时退回旧行为：有图放在内容下方，
/// 没图什么也不加。
fn with_media_slot(
    p: &serde_json::Map<String, serde_json::Value>,
    layout: &str,
    media: &dyn Fn(bool) -> String,
    content: String,
) -> String {
    let slot = crate::slides_layout::param_text(p, layout, "media_slot");
    if slot != "left" && slot != "right" {
        let pic = media(false);
        return format!("<div class=\"box content\">{content}{pic}</div>");
    }
    let pic = media(true);
    let text = format!("<div class=\"slot-text\">{content}</div>");
    let inner = if slot == "left" {
        format!("{pic}{text}")
    } else {
        format!("{text}{pic}")
    };
    format!("<div class=\"box content slotted\">{inner}</div>")
}

/// Full self-contained HTML document. If `only` is `Some(i)`, render just that
/// slide (focused editor preview); otherwise render the whole deck (export /
/// scrollable preview). `print` adds page-break rules for PDF export.
/// Salvage a slide whose content landed in the wrong field for its layout, so an
/// AI field slip degrades to a sensible slide instead of a silently blank one
/// (SCHEMA_SPEC documents the right fields, but models drift):
/// - quote with empty `body` → promote `title` to the quotation
/// - quote `subtitle` starting with a dash → strip it (both renderers prepend "— ")
/// - two-column with no `columns` but `bullets` → split bullets into two columns
///
/// Both renderers (HTML preview and pptx export) MUST apply this, and nothing
/// else may mutate content, or preview and export drift apart.
pub(crate) fn effective_slide(slide: &Slide) -> std::borrow::Cow<'_, Slide> {
    use std::borrow::Cow;
    match slide.layout.as_str() {
        "quote" => {
            let promote = slide.body.trim().is_empty() && !slide.title.trim().is_empty();
            let dashed = slide.subtitle.trim_start().starts_with(['—', '-', '–']);
            if !promote && !dashed {
                return Cow::Borrowed(slide);
            }
            let mut s = slide.clone();
            if promote {
                s.body = std::mem::take(&mut s.title);
            }
            if dashed {
                s.subtitle = s
                    .subtitle
                    .trim_start()
                    .trim_start_matches(['—', '-', '–'])
                    .trim_start()
                    .to_string();
            }
            Cow::Owned(s)
        }
        "two-column" if slide.columns.is_empty() && !slide.bullets.is_empty() => {
            let mut s = slide.clone();
            let mid = s.bullets.len().div_ceil(2);
            let right = s.bullets.split_off(mid);
            s.columns = vec![
                Column { bullets: std::mem::take(&mut s.bullets), ..Default::default() },
                Column { bullets: right, ..Default::default() },
            ];
            Cow::Owned(s)
        }
        _ => Cow::Borrowed(slide),
    }
}

pub fn render_deck_html(deck: &Deck, only: Option<usize>, print: bool) -> String {
    let theme = if THEMES.contains(&deck.theme.as_str()) {
        deck.theme.as_str()
    } else {
        "midnight"
    };
    // Brand furniture (D): logo + footer are painted on every slide.
    let (logo_el, footer_el) = match &deck.brand {
        Some(b) => {
            let logo = image_src(&b.logo);
            (
                if logo.is_empty() {
                    String::new()
                } else {
                    format!("<img class=\"brand-logo\" src=\"{}\" alt=\"\"/>", esc(&logo))
                },
                if b.footer.trim().is_empty() {
                    String::new()
                } else {
                    format!("<div class=\"brand-footer\">{}</div>", inline(&b.footer))
                },
            )
        }
        None => (String::new(), String::new()),
    };
    let sections: String = deck
        .slides
        .iter()
        .enumerate()
        .filter(|(i, _)| only.map(|o| o == *i).unwrap_or(true))
        .map(|(i, s)| {
            let s = effective_slide(s);
            // 密度与页码开关是通用控件，作用在整页上（P2）。
            let density = crate::slides_layout::param_text(&s.params, &s.layout, "density");
            let pagenum = if crate::slides_layout::param_bool(&s.params, &s.layout, "show_page_number")
            {
                format!("<div class=\"pagenum\">{}</div>", i + 1)
            } else {
                String::new()
            };
            format!(
                "<section class=\"slide layout-{} dense-{}\" data-index=\"{}\">{}{logo_el}{footer_el}{pagenum}</section>",
                esc(&s.layout),
                esc(&density),
                i,
                render_slide_inner(&s),
            )
        })
        .collect();
    let print_css = if print {
        "@media print{body{background:#000;}.slide{page-break-after:always;box-shadow:none;margin:0;}}"
    } else {
        ""
    };
    // Brand CSS comes after the theme so it wins.
    let brand_style = deck.brand.as_ref().map(brand_css).unwrap_or_default();
    format!(
        "<!doctype html><html lang=\"zh\"><head><meta charset=\"utf-8\"/>\
<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"/>\
<title>{}</title><style>{}{}{}</style></head>\
<body class=\"theme-{}\">{}</body></html>",
        esc(&deck.title),
        BASE_CSS,
        print_css,
        brand_style,
        theme,
        sections
    )
}

/// Shared slide CSS + all theme palettes. Slides are a fixed 1280×720 canvas so
/// preview and PDF export are pixel-consistent; the preview iframe scales it.
const BASE_CSS: &str = r#"
*{box-sizing:border-box;margin:0;padding:0}
body{font-family:'Inter','PingFang SC','Microsoft YaHei',system-ui,sans-serif;background:#0b1020;display:flex;flex-direction:column;align-items:center;gap:24px;padding:24px}
.slide{position:relative;width:1280px;height:720px;border-radius:18px;overflow:hidden;display:flex;box-shadow:0 24px 60px rgba(0,0,0,.45)}
.slide .box{width:100%;height:100%;padding:80px 96px;display:flex;flex-direction:column;justify-content:center;gap:24px}
.slide .box.cover{justify-content:center;align-items:flex-start}
.slide .box.section{justify-content:center}
.s-title{font-size:52px;line-height:1.15;font-weight:800;letter-spacing:-.02em}
.box.cover .s-title{font-size:72px}
.box.section .s-title{font-size:60px}
.s-sub{font-size:26px;font-weight:500;opacity:.82}
.box.cover .s-sub{font-size:30px;margin-top:8px}
.s-body{font-size:26px;line-height:1.55;display:flex;flex-direction:column;gap:12px}
.bullets{list-style:none;display:flex;flex-direction:column;gap:18px;margin-top:8px}
.bullets li{font-size:27px;line-height:1.4;padding-left:38px;position:relative}
.bullets li:before{content:'';position:absolute;left:0;top:14px;width:16px;height:16px;border-radius:4px;transform:rotate(45deg)}
.accent{width:120px;height:10px;border-radius:6px;margin-bottom:12px}
/* P2 控件驱动的样式：分栏/序号/强调/栏宽/密度都由 params 决定 */
.bullets[style*="column-count"]{display:block}
.bullets[style*="column-count"] li{break-inside:avoid;margin-bottom:18px}
.bullets li.em{font-size:32px;font-weight:700}
.bullets li .idx{position:absolute;left:0;top:0;width:26px;text-align:center;opacity:.65;font-weight:700}
.bullets li .idx+*{margin-left:0}
.cover.align-center{align-items:center;text-align:center}
.cover.align-center .accent{margin-left:auto;margin-right:auto}
.cols{display:grid;grid-template-columns:1fr 1fr;gap:56px;margin-top:12px}
.cols.bal-left-heavy{grid-template-columns:1.6fr 1fr}
.cols.bal-right-heavy{grid-template-columns:1fr 1.6fr}
.cols.divided .col+.col{border-left:1px solid currentColor;padding-left:32px;opacity:1}
.cols.divided .col+.col{border-left-color:rgba(128,128,128,.35)}
.slide.dense-loose .box{gap:14px}
.slide.dense-tight .bullets{gap:10px}
.slide.dense-tight .bullets li{font-size:24px}
.col h2{font-size:30px;margin-bottom:16px;font-weight:700}
.col p{font-size:24px;line-height:1.5;opacity:.9}
.quote blockquote{font-size:44px;line-height:1.4;font-weight:700}
.quote cite{display:block;margin-top:28px;font-size:26px;font-style:normal;opacity:.75}
.image-layout .s-image{flex:1;display:flex;align-items:center;justify-content:center;margin-top:12px;min-height:0}
.image-layout .s-image img{max-width:100%;max-height:100%;width:100%;height:100%;border-radius:12px;object-fit:contain}
/* 图注字号档位（caption_size 控件） */
.s-sub.cap-1{font-size:22px}
.s-sub.cap-2{font-size:30px;font-weight:600}
.split{padding:0}
.split .s-image{width:46%;height:100%}
.split .s-image img{width:100%;height:100%;object-fit:cover}
.split .split-text{flex:1;padding:80px;display:flex;flex-direction:column;justify-content:center;gap:20px}
.pagenum{position:absolute;bottom:28px;right:40px;font-size:18px;opacity:.5}
/* 媒体槽：图未就位时的占位框，保证「先排版后配图」不改变版面 */
.s-image.ph{border:2px dashed var(--grid);border-radius:14px;display:flex;align-items:center;justify-content:center;font-size:20px;opacity:.55;min-height:180px}
.split .s-image.ph{border-radius:0;min-height:100%}
.box.slotted{flex-direction:row;align-items:center;gap:56px}
.box.slotted .slot-text{flex:1;min-width:0;display:flex;flex-direction:column;gap:20px}
/* 占位框与真图必须占**同一个盒子**，否则配好图那一刻图区仍会变大变小——
   「版面不跳」的承诺就落空了 */
.box.slotted .s-image{flex:0 0 38%;align-self:center;height:66%;display:flex;align-items:center;justify-content:center}
.box.slotted .s-image img{width:100%;height:100%;border-radius:14px;object-fit:cover}
.box.slotted .s-image.ph{min-height:0}
/* ── P3 结构版式：分析模型 + 图表 ── */
.box.block{justify-content:flex-start;gap:18px}
.box.block>.s-title{font-size:44px}
.box.block .metrics,.box.block .steps,.box.block .quad,.box.block .porter,
.box.block .pest,.box.block .canvas,.box.block .chart,.box.block .quadwrap,
.box.block .risks,.box.block .tline{flex:1;min-height:0}
/* table 不吃 flex:1，用 auto 外边距把它在纵向居中 */
.box.block .ctable{margin:auto 0}
.metrics{display:grid;gap:28px;align-content:center}
.metric{background:var(--surface);border-radius:16px;padding:28px 32px;display:flex;flex-direction:column;gap:8px;justify-content:center}
.mval{font-size:64px;font-weight:800;line-height:1;color:var(--acc)}
.mlab{font-size:24px;font-weight:600}
.mdelta{font-size:19px;opacity:.7}
/* 步骤卡按内容高度收缩再整体居中——拉满高度会得到一排空荡荡的长条 */
.steps{display:flex;align-items:center;gap:12px}
.steps.horiz{flex-direction:row}
.steps.vert{flex-direction:column;justify-content:center}
.step{flex:1;background:var(--surface);border-radius:14px;padding:22px;display:flex;gap:14px;align-items:flex-start}
.steps.vert .step{flex:0 0 auto}
.stepno{flex:0 0 auto;width:38px;height:38px;border-radius:50%;background:var(--acc);color:var(--on-acc);display:flex;align-items:center;justify-content:center;font-weight:800;font-size:20px}
.stepbody h3{font-size:24px;font-weight:700;line-height:1.25}
.stepbody p{font-size:19px;line-height:1.45;opacity:.8;margin-top:6px}
.arrow{flex:0 0 auto;display:flex;align-items:center;font-size:30px;opacity:.5}
.ctable{width:100%;border-collapse:collapse;font-size:22px}
.ctable th,.ctable td{padding:14px 18px;text-align:left;border-bottom:1px solid var(--grid)}
.ctable thead th{font-size:24px;color:var(--acc);font-weight:700}
.ctable tbody th{font-weight:600;opacity:.85}
.ctable td.win{background:var(--surface2);font-weight:700}
.tline{display:flex;align-items:center;position:relative;padding:40px 0}
.tline:before{content:'';position:absolute;left:0;right:0;top:50%;height:3px;background:var(--grid)}
.tline ol{list-style:none;display:grid;width:100%;position:relative}
.tmark{position:relative;display:flex;flex-direction:column;align-items:center}
.tdot{width:18px;height:18px;border-radius:50%;background:var(--acc);z-index:1}
.tlab{position:absolute;width:92%;text-align:center;font-size:20px;line-height:1.3;display:flex;flex-direction:column;gap:4px}
.tmark.up .tlab{bottom:28px}
.tmark.down .tlab{top:28px}
.tdate{font-size:17px;opacity:.65}
.tnow{font-size:15px;color:var(--acc);font-weight:700}
.risks{list-style:none;display:flex;flex-direction:column;gap:14px;justify-content:center}
.risk-row{display:grid;grid-template-columns:64px 1fr 1.3fr;gap:18px;align-items:center;background:var(--surface);border-radius:12px;padding:16px 20px;font-size:21px}
.sev{font-size:17px;font-weight:800;text-align:center;border-radius:999px;padding:4px 0;color:#0b1020}
/* 等级色是语义色（红/琥珀/蓝），不跟主题变——换主题不该把"高危"变成品牌色 */
.sev-high{background:#ff7a6b}.sev-mid{background:#f6c453}.sev-low{background:#8fb6ff}
.rname{font-weight:700}
.rplan{opacity:.82}
.noplan{opacity:.45;font-style:italic}
.quadwrap{display:flex;gap:14px;align-items:stretch}
.quadcol{flex:1;display:flex;flex-direction:column;gap:10px;min-width:0}
.axis-y{writing-mode:vertical-rl;transform:rotate(180deg);font-size:18px;opacity:.6;text-align:center}
.axis-x{font-size:18px;opacity:.6;text-align:center}
.quad{flex:1;display:grid;grid-template-columns:1fr 1fr;grid-template-rows:1fr 1fr;gap:14px}
.qcell,.force,.pcell,.bcell{background:var(--surface);border-radius:14px;padding:18px 22px;overflow:hidden}
.qcell.hot{background:var(--surface2);outline:2px solid var(--acc)}
.qcell h3,.force h3,.pcell h3,.bcell h3{font-size:22px;font-weight:700;color:var(--acc);margin-bottom:8px}
.qcell ul,.force ul,.pcell ul,.bcell ul{list-style:none;display:flex;flex-direction:column;gap:6px}
.qcell li,.force li,.pcell li,.bcell li{font-size:19px;line-height:1.35;padding-left:14px;position:relative}
.qcell li:before,.force li:before,.pcell li:before,.bcell li:before{content:'·';position:absolute;left:2px;opacity:.6}
.porter{display:grid;grid-template-columns:1fr 1fr 1fr;grid-template-rows:1fr 1fr 1fr;gap:12px}
.porter .f1{grid-area:2/1}.porter .f2{grid-area:2/3}.porter .f3{grid-area:1/2}.porter .f4{grid-area:3/2}
.porter .center{grid-area:2/2;background:var(--surface2);outline:2px solid var(--acc)}
.porter.nocenter{grid-template-columns:1fr 1fr;grid-template-rows:1fr 1fr}
.porter.nocenter .f1,.porter.nocenter .f2,.porter.nocenter .f3,.porter.nocenter .f4{grid-area:auto}
.pest{display:grid;gap:14px}
.pest.grid{grid-template-columns:1fr 1fr;grid-template-rows:1fr 1fr}
.pest.row{grid-template-columns:repeat(4,1fr)}
.canvas{display:grid;gap:10px;grid-template-columns:repeat(5,1fr);grid-template-rows:1fr 1fr .62fr;
  grid-template-areas:"kp ka vp cr cs" "kp kr vp ch cs" "cost cost cost rev rev"}
.canvas .bcell h3{font-size:19px}
.canvas .bcell li{font-size:16px}
.canvas.compact .bcell{padding:12px 14px}
.canvas.compact .bcell h3{font-size:17px;margin-bottom:4px}
.canvas.compact .bcell li{font-size:14px;line-height:1.25}
.chart{display:flex;flex-direction:column;gap:10px;justify-content:center}
.chart svg{width:100%;height:auto;max-height:100%;overflow:visible}
.chart .grid{stroke:var(--grid);stroke-width:1}
.chart .tick,.chart .xlab{fill:currentColor;opacity:.62;font-size:17px}
.chart .val{fill:currentColor;font-size:18px;font-weight:700}
.chart .inbar{fill:currentColor;font-size:18px;font-weight:600}
.chart .tmrect{stroke:var(--grid)}
.legend{display:flex;gap:20px;justify-content:center;font-size:18px;opacity:.8}
.legend .lg{display:inline-flex;align-items:center;gap:7px}
.legend i{width:14px;height:14px;border-radius:4px;display:inline-block}
.slide.dense-tight .box.block{gap:10px}
.slide.dense-tight .metric{padding:18px 22px}
.slide.dense-tight .mval{font-size:52px}
/* ── theme: midnight ── */
.theme-midnight .slide{background:linear-gradient(135deg,#111a33 0%,#0b1020 100%);color:#eaf0ff;
  --acc:#4dd0e1;--acc2:#8fb6ff;--acc3:#ffb86b;--on-acc:#0b1020;--grid:rgba(234,240,255,.16);
  --surface:rgba(255,255,255,.07);--surface2:rgba(255,255,255,.15)}
.theme-midnight .s-sub,.theme-midnight .col p{color:#aab8dd}
.theme-midnight .bullets li:before,.theme-midnight .accent{background:#4dd0e1}
.theme-midnight .s-title,.theme-midnight .quote blockquote{background:linear-gradient(90deg,#eaf0ff,#8fb6ff);-webkit-background-clip:text;background-clip:text;-webkit-text-fill-color:transparent}
.theme-midnight .col h2{color:#4dd0e1}
/* ── theme: minimal ── */
.theme-minimal .slide{background:#ffffff;color:#1a1a2e;
  --acc:#111827;--acc2:#4b5563;--acc3:#b45309;--on-acc:#ffffff;--grid:rgba(0,0,0,.14);
  --surface:rgba(0,0,0,.045);--surface2:rgba(0,0,0,.10)}
.theme-minimal .s-sub,.theme-minimal .col p{color:#5a5a72}
.theme-minimal .bullets li:before,.theme-minimal .accent{background:#111}
.theme-minimal .col h2{color:#111}
.theme-minimal .quote blockquote{color:#111}
/* ── theme: corporate ── */
.theme-corporate .slide{background:#f4f7fb;color:#0f2540;
  --acc:#2f6fed;--acc2:#1b4fb5;--acc3:#b45309;--on-acc:#ffffff;--grid:rgba(15,37,64,.16);
  --surface:rgba(47,111,237,.09);--surface2:rgba(47,111,237,.19)}
.theme-corporate .s-sub,.theme-corporate .col p{color:#3d5a80}
.theme-corporate .bullets li:before,.theme-corporate .accent{background:#2f6fed}
.theme-corporate .col h2{color:#2f6fed}
.theme-corporate .box.cover,.theme-corporate .box.section{background:linear-gradient(135deg,#0f2540,#1b3a5c);color:#fff;margin:0}
.theme-corporate .box.cover .s-sub{color:#bcd3f5}
/* ── theme: sunset ── */
.theme-sunset .slide{background:linear-gradient(135deg,#2b1055 0%,#7597de 100%);color:#fff;
  --acc:#ff8f6b;--acc2:#ffcf8f;--acc3:#c9a7ff;--on-acc:#2b1055;--grid:rgba(255,255,255,.22);
  --surface:rgba(255,255,255,.12);--surface2:rgba(255,255,255,.22)}
.theme-sunset .s-sub,.theme-sunset .col p{color:#ffe0c7}
.theme-sunset .bullets li:before,.theme-sunset .accent{background:#ff8f6b}
.theme-sunset .col h2{color:#ffcf8f}
.theme-sunset .quote blockquote{color:#fff}
"#;

// ─────────────────────────────────────────────────────────────────────────
// Prompts for gateway generation / editing
// ─────────────────────────────────────────────────────────────────────────

/// The strict schema contract we hand the model so its output parses every time.
pub const SCHEMA_SPEC: &str = r#"你是专业的演示文稿设计师。只输出一个 JSON 对象，不要任何解释文字、不要 markdown 代码围栏。
JSON 结构：
{
  "title": "演示标题",
  "theme": "midnight | minimal | corporate | sunset 之一",
  "slides": [
    {
      "layout": "见下方版式清单",
      "title": "标题（cover/section 用大标题）",
      "subtitle": "副标题/署名（quote 里作为出处）",
      "bullets": ["要点1", "要点2"],
      "body": "正文段落（quote 里作为引文正文，可用 \n 分段）",
      "columns": [{"title":"列标题","bullets":["..."]}],
      "items": [{"label":"名称","value":0,"detail":"说明","group":"分组"}],
      "image": "图片URL（可留空）"
    }
  ]
}
版式清单：
- 文字类：cover / section / bullets / content / two-column / quote / image / image-left
- 数据类（内容放 items）：metrics 指标卡 · chart 图表 · process 流程 · timeline 时间线 ·
  gantt 甘特 · risk 风险（detail 写应对措施） · compare-table 对比表
- 分析模型（内容放 columns，每格 title + bullets）：swot · matrix-2x2 四象限 ·
  porter 波特五力 · pest · bmc 商业模式画布

规则：首页用 cover；每页只放该 layout 需要的字段；bullets 每条精炼不超过一行；要点用 **加粗** 强调关键词；一份演示 8-14 页为宜，宁少而精；数字只用有依据的，**不要编造精确数值**。用与用户需求相同的语言撰写。"#;

pub fn build_generate_prompt(topic: &str, slide_count: u32) -> String {
    format!(
        "{SCHEMA_SPEC}\n\n请就以下主题制作大约 {slide_count} 页的演示：\n{topic}"
    )
}

pub fn build_edit_prompt(current_json: &str, instruction: &str) -> String {
    format!(
        "{SCHEMA_SPEC}\n\n下面是当前演示的 JSON。请根据修改指令，**只改动需要改的部分**，其余保持不变，然后输出完整的新 JSON。\n\n当前 JSON：\n{current_json}\n\n修改指令：\n{instruction}"
    )
}

// ── A：两阶段（大纲 → 展开）────────────────────────────────────────────────

/// 大纲规格。P1 起先选**角色**（这页干什么）再选版式——角色让模型在规划阶段
/// 就把「风险页」和「指标页」区分开，而不是事后把什么内容都套进通用 bullets。
pub fn outline_spec() -> String {
    format!(
        r#"只输出一个 JSON 对象，不要解释文字、不要代码围栏。
{{"title":"演示标题","theme":"midnight | minimal | corporate | sunset 之一","items":[{{"role":"页面角色 key","layout":"该角色的推荐版式","title":"这一页的标题","points":["这页要讲的要点提纲1","要点2"]}}]}}

可选页面角色（先按叙事需要挑角色，再用它的推荐版式）：
{roles}

规则：首页角色用 cover；points 是提纲级要点（短语即可，正式措辞留到展开阶段）；
8-14 页为宜，宁少而精；同一角色不要连着出现三页以上；用与用户需求相同的语言。"#,
        roles = crate::slides_layout::roles_for_prompt()
    )
}

pub fn build_outline_prompt(topic: &str, slide_count: u32) -> String {
    format!(
        "你是专业的演示文稿设计师。先为下面的主题规划**大纲**（还不写正式内容）。\n{}\n\n主题（约 {slide_count} 页）：\n{topic}",
        outline_spec()
    )
}

/// Expand one outline item into a full slide. Kept per-slide so pages can be
/// generated in parallel and a single failure never sinks the whole deck.
pub fn build_expand_slide_prompt(
    deck_title: &str,
    index: usize,
    total: usize,
    item: &OutlineItem,
) -> String {
    let points = if item.points.is_empty() {
        "（无提纲，请据标题自行发挥）".to_string()
    } else {
        item.points
            .iter()
            .map(|p| format!("- {p}"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    format!(
        r#"你是专业的演示文稿设计师。把下面这一页的提纲展开成正式的幻灯内容。只输出**一个幻灯页的 JSON 对象**，不要数组、不要解释、不要代码围栏。

单页 JSON 结构（只放该 layout 需要的字段）：
{{"layout":"{layout}","title":"...","subtitle":"...","bullets":["..."],"body":"...","columns":[{{"title":"...","bullets":["..."]}}],"items":[{{"label":"...","value":0,"span":0,"detail":"...","group":"..."}}],"notes":"演讲备注"}}

**本页版式的填法**：{hint}

规则：bullets 每条精炼不超过一行、用 **加粗** 强调关键词；notes 写 1-3 句口播提示；用与提纲相同的语言。
数值必须来自提纲或常识，**不要编造精确数字**；没有可靠数字时宁可不用 value。

演示标题：{deck_title}（第 {n}/{total} 页）
本页 layout：{layout}
本页标题：{title}
本页提纲：
{points}"#,
        layout = item.layout,
        title = item.title,
        n = index + 1,
        hint = crate::slides_layout::fields_hint_for(&item.layout),
    )
}

// ── B：单页精修（差分编辑）──────────────────────────────────────────────────

/// Send ONLY the target slide (plus a one-line deck context) — 5-10x faster and
/// cheaper than round-tripping the whole deck, and it cannot corrupt other pages.
pub fn build_slide_edit_prompt(
    deck_title: &str,
    index: usize,
    total: usize,
    slide_json: &str,
    instruction: &str,
) -> String {
    format!(
        r#"你是专业的演示文稿设计师。修改下面这**一页**幻灯。只输出修改后的**单个幻灯页 JSON 对象**，不要数组、不要解释、不要代码围栏。

保持 JSON 结构不变（可增删字段以匹配 layout）；只改动指令要求的部分；用与原内容相同的语言。

**模板锁**：除非指令明确要求换版式或改结构，否则不得改动 layout、role、params、image
（版式、页面角色、控件参数、图片槽都属于模板属性）——只替换可见文案。这些字段
即使你改了系统也会还原，改了等于白费。

所属演示：{deck_title}（第 {n}/{total} 页）

当前这一页的 JSON：
{slide_json}

修改指令：
{instruction}"#,
        n = index + 1,
    )
}

// ── C：自动配图 ─────────────────────────────────────────────────────────────

/// Turn a slide's content into an image-generation prompt. Local + deterministic
/// (no model call) so "配图" is one click, and the caller can still edit it.
pub fn build_image_prompt(slide: &Slide, deck_title: &str) -> String {
    let mut topic = slide.title.clone();
    if topic.trim().is_empty() {
        topic = slide.bullets.first().cloned().unwrap_or_default();
    }
    if topic.trim().is_empty() {
        topic = slide.body.chars().take(60).collect();
    }
    if topic.trim().is_empty() {
        topic = deck_title.to_string();
    }
    let extra: String = slide
        .bullets
        .iter()
        .take(3)
        .map(|b| b.replace("**", ""))
        .collect::<Vec<_>>()
        .join("; ");
    format!(
        "Professional presentation illustration for a slide titled \"{topic}\". \
         Context: {extra}. Clean modern editorial style, ample negative space, \
         no text, no words, no letters, no watermark, 16:9 composition."
    )
}

// ─────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_json_strips_fence_and_prose() {
        let raw = "好的，这是你的演示：\n```json\n{\"title\":\"T\",\"slides\":[]}\n```\n完成";
        let j = extract_json(raw).unwrap();
        assert_eq!(j, "{\"title\":\"T\",\"slides\":[]}");
    }

    #[test]
    fn extract_json_handles_braces_in_strings() {
        let raw = r#"{"title":"a } b","slides":[{"title":"x{y}"}]}"#;
        let j = extract_json(raw).unwrap();
        assert_eq!(j, raw);
    }

    /// 模型只挑角色不填版式是常态（提示词就是这么引导的）。
    /// 那时角色的推荐必须真的生效，而不是所有页都退成 content。
    #[test]
    fn outline_derives_layout_from_role() {
        let mut o: Outline = serde_json::from_str(
            r#"{"items":[{"role":"matrix","title":"a"},
                         {"role":"metric","title":"b","layout":"编的版式"},
                         {"role":"不存在","title":"c"},
                         {"role":"trend","title":"d","layout":"bullets"}]}"#,
        )
        .unwrap();
        o.normalize();
        assert_eq!(o.items[0].layout, "swot", "漏填版式应取角色首选");
        assert_eq!(o.items[1].layout, "metrics", "编造的版式应被角色首选顶掉");
        assert_eq!(o.items[2].layout, "content", "角色也不认识才退到 content");
        assert_eq!(o.items[3].layout, "bullets", "模型给了合法版式就尊重它");
        assert_eq!(o.theme, "midnight", "缺省主题");
    }

    #[test]
    fn parse_deck_rejects_empty_and_bad_theme() {
        assert!(parse_deck("{\"slides\":[]}").is_err());
        let d = parse_deck("{\"theme\":\"nope\",\"slides\":[{\"title\":\"a\"}]}").unwrap();
        assert_eq!(d.theme, "midnight"); // unknown theme falls back
    }

    #[test]
    fn render_is_deterministic_and_escapes() {
        let deck = Deck {
            id: "d1".into(),
            title: "My <deck>".into(),
            theme: "minimal".into(),
            brand: None,
            slides: vec![Slide {
                layout: "bullets".into(),
                title: "Hi & <b>".into(),
                bullets: vec!["one **key**".into(), "two".into()],
                ..Default::default()
            }],
        };
        let a = render_deck_html(&deck, None, false);
        let b = render_deck_html(&deck, None, false);
        assert_eq!(a, b, "render must be deterministic");
        assert!(a.contains("&lt;deck&gt;"), "title escaped");
        assert!(a.contains("Hi &amp; &lt;b&gt;"), "slide title escaped");
        assert!(a.contains("<strong>key</strong>"), "bold applied");
        assert!(a.contains("theme-minimal"));
    }

    #[test]
    fn unknown_layout_falls_back_not_blank() {
        let deck = Deck {
            id: String::new(),
            title: "T".into(),
            theme: "midnight".into(),
            brand: None,
            slides: vec![Slide {
                layout: "totally-made-up".into(),
                title: "Still shows".into(),
                ..Default::default()
            }],
        };
        let html = render_deck_html(&deck, None, false);
        assert!(html.contains("Still shows"));
    }

    #[test]
    fn only_renders_single_slide() {
        let deck = Deck {
            id: String::new(),
            title: "T".into(),
            theme: "midnight".into(),
            brand: None,
            slides: vec![
                Slide { title: "AAA".into(), ..Default::default() },
                Slide { title: "BBB".into(), ..Default::default() },
            ],
        };
        let html = render_deck_html(&deck, Some(1), false);
        assert!(html.contains("BBB"));
        assert!(!html.contains("AAA"));
    }
}

#[cfg(test)]
mod param_render_tests {
    use super::*;

    fn slide_with(layout: &str, params: serde_json::Value) -> Slide {
        let mut s = Slide {
            layout: layout.into(),
            title: "标题".into(),
            bullets: vec!["第一条".into(), "第二条".into(), "第三条".into()],
            ..Default::default()
        };
        if let serde_json::Value::Object(m) = params {
            s.params = m;
        }
        s.fill_default_params();
        s
    }

    /// P2 的核心承诺：改参数就改渲染，不需要跑模型。
    #[test]
    fn controls_change_the_rendered_html() {
        let plain = render_slide_inner(&slide_with("bullets", serde_json::json!({})));
        assert!(!plain.contains("column-count"), "默认单栏");
        assert!(!plain.contains("class=\"em\""), "默认不强调");
        assert!(!plain.contains("class=\"idx\""), "默认无序号");

        let tuned = render_slide_inner(&slide_with(
            "bullets",
            serde_json::json!({"columns": 2, "emphasis": "first", "show_index": true}),
        ));
        assert!(tuned.contains("column-count:2"), "分栏参数应生效:\n{tuned}");
        assert!(tuned.contains("class=\"em\""), "强调参数应生效");
        assert!(tuned.contains("class=\"idx\""), "序号参数应生效");
    }

    #[test]
    fn emphasis_last_marks_only_the_last_bullet() {
        let html = render_slide_inner(&slide_with("bullets", serde_json::json!({"emphasis": "last"})));
        assert_eq!(html.matches("class=\"em\"").count(), 1, "只强调一条");
        // 强调的应该是最后一条
        let em_pos = html.find("class=\"em\"").unwrap();
        assert!(html[em_pos..].contains("第三条"), "应强调最后一条:\n{html}");
    }

    /// 脏参数不能让渲染崩或画歪——模型可能填出任何东西。
    #[test]
    fn dirty_params_still_render() {
        let html = render_slide_inner(&slide_with(
            "bullets",
            serde_json::json!({"columns": 99, "emphasis": "斜着", "show_index": "yes"}),
        ));
        assert!(html.contains("column-count:3"), "越界夹到 max=3:\n{html}");
        assert!(!html.contains("class=\"em\""), "非法强调值回落 none");
    }

    #[test]
    fn page_number_toggle_controls_the_footer() {
        let mut deck = Deck {
            id: String::new(),
            title: "T".into(),
            theme: "midnight".into(),
            brand: None,
            slides: vec![slide_with("bullets", serde_json::json!({}))],
        };
        // 只看元素，不看 CSS——样式表里始终有 .pagenum 规则。
        let element = "<div class=\"pagenum\">";
        assert!(render_deck_html(&deck, None, false).contains(element));

        deck.slides[0]
            .params
            .insert("show_page_number".into(), serde_json::json!(false));
        assert!(
            !render_deck_html(&deck, None, false).contains(element),
            "关掉页码后不应再渲染页码元素"
        );
    }

    /// 面板上能拖的**每个**旋钮都必须改渲染。这条用例逐版式核对：
    /// 控件契约里声明的键，要么出现在渲染出的 HTML 里、要么改变了输出。
    /// 加了控件却忘了接线，是这个设计最容易犯的错。
    #[test]
    fn every_declared_control_changes_the_render() {
        use crate::slides_layout::{controls_for, variant_params, ALL_LAYOUTS};
        for layout in ALL_LAYOUTS {
            for c in controls_for(layout) {
                if c.key == "show_page_number" {
                    continue; // 整页控件，作用在 render_deck_html 而非单页片段
                }
                let mut base = slide_with(layout, serde_json::json!({}));
                base.subtitle = "副标题".into();
                base.image = "https://example.com/a.png".into();
                base.columns = (1..=4)
                    .map(|i| Column { title: format!("栏{i}"), bullets: vec!["条目".into()], ..Default::default() })
                    .collect();
                base.items = (1..=6)
                    .map(|i| SlideItem {
                        label: format!("项{i}"),
                        value: i as f64,
                        span: 1.0,
                        detail: "说明".into(),
                        // 分组用栏名：对比表的「优胜列」认列名，图表认系列名，
                        // 一个值同时满足两种读法，用例才不用给每个版式定制夹具。
                        group: format!("栏{}", i % 2 + 1),
                    })
                    .collect();
                let mut tweaked = base.clone();
                // 只改这一个控件，其余保持默认——差异必定来自它。
                tweaked.params = {
                    let one = variant_params(layout, &base.params);
                    let mut p = base.params.clone();
                    if let Some(v) = one.get(c.key) {
                        p.insert(c.key.to_string(), v.clone());
                    }
                    p
                };
                if tweaked.params == base.params {
                    continue; // variant 没能给出不同值（如单选项下拉），跳过
                }
                assert_ne!(
                    render_slide_inner(&base),
                    render_slide_inner(&tweaked),
                    "{layout} 的控件「{}」({}) 拖了没反应——声明了但没接线",
                    c.label,
                    c.key
                );
            }
        }
    }

    /// 控件必须真的改渲染。栏数曾经是个摆设（网格写死两栏），
    /// 这条用例守住「面板上能拖的每个旋钮都有效果」。
    #[test]
    fn two_column_count_changes_the_grid() {
        let mut s = slide_with("two-column", serde_json::json!({}));
        s.columns = (1..=4)
            .map(|i| Column { title: format!("栏{i}"), ..Default::default() })
            .collect();
        let two = render_slide_inner(&s);
        assert!(!two.contains("grid-template-columns:repeat"), "两栏用默认网格");
        assert!(two.contains("bal-equal"));

        s.params.insert("column_count".into(), serde_json::json!(4));
        let four = render_slide_inner(&s);
        assert!(four.contains("grid-template-columns:repeat(4,1fr)"), "栏数应改网格:\n{four}");

        // 多栏时栏宽偏置无意义，不该带着一个只对两栏成立的类名
        s.params.insert("balance".into(), serde_json::json!("left-heavy"));
        assert!(render_slide_inner(&s).contains("bal-equal"), "多栏应强制等宽");
    }

    /// 媒体槽：图还没配好也要把位置留出来，否则配好图那一刻整页版面会跳。
    #[test]
    fn media_slot_reserves_space_before_the_image_exists() {
        let plain = render_slide_inner(&slide_with("bullets", serde_json::json!({})));
        assert!(!plain.contains("s-image"), "没开配图位就不该有图片区");

        let reserved =
            render_slide_inner(&slide_with("bullets", serde_json::json!({"media_slot": "right"})));
        assert!(reserved.contains("s-image ph"), "开了配图位应出现占位框:\n{reserved}");
        assert!(reserved.contains("slotted"), "应切成图文并排");

        // 图片版式即使没有图也永远留位
        let empty_img = render_slide_inner(&slide_with("image", serde_json::json!({})));
        assert!(empty_img.contains("s-image ph"), "图片版式必须留位:\n{empty_img}");

        // 真有图时占位框让位给图片
        let mut with_pic = slide_with("bullets", serde_json::json!({"media_slot": "left"}));
        with_pic.image = "https://example.com/a.png".into();
        let html = render_slide_inner(&with_pic);
        assert!(html.contains("<img src=\"https://example.com/a.png\""), "{html}");
        assert!(!html.contains("ph"), "有图就不该再显示占位框");
    }

    /// 图片只能由媒体槽出一次。content 分支曾经把图片拼进内容里，
    /// 开了槽位就会画两张——同一张图出现两次是很难在断言里看出来的那种 bug。
    #[test]
    fn image_is_never_rendered_twice() {
        for layout in ["content", "bullets", "image", "image-left"] {
            for slot in ["none", "left", "right"] {
                let mut s = slide_with(layout, serde_json::json!({ "media_slot": slot }));
                s.image = "https://example.com/a.png".into();
                let html = render_slide_inner(&s);
                assert_eq!(
                    html.matches("<img src=").count(),
                    1,
                    "{layout} + slot={slot} 画了不止一张图:\n{html}"
                );
            }
        }
    }

    /// P0 模板锁：模型改了版式/参数也要被还原，只留文案改动。
    #[test]
    fn template_lock_restores_structure_but_keeps_text() {
        let original = slide_with("bullets", serde_json::json!({"columns": 2}));
        let mut edited = Slide {
            layout: "quote".into(),          // 模型擅自换版式
            role: "closing".into(),          // 擅自改角色
            title: "新标题".into(),          // 文案改动——应保留
            bullets: vec!["改写后的要点".into()],
            image: "evil.png".into(),        // 擅自塞图
            ..Default::default()
        };
        edited.params.insert("columns".into(), serde_json::json!(3));

        edited.restore_template_from(&original);

        assert_eq!(edited.layout, "bullets", "版式应还原");
        assert_eq!(edited.role, original.role, "角色应还原");
        assert_eq!(edited.image, "", "图片槽应还原");
        assert_eq!(edited.params.get("columns"), Some(&serde_json::json!(2)), "参数应还原");
        assert_eq!(edited.title, "新标题", "文案改动必须保留");
        assert_eq!(edited.bullets, vec!["改写后的要点".to_string()], "文案改动必须保留");
    }
}

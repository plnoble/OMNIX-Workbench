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

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Minimal inline formatting: `**bold**` → <strong>, plus HTML-escape. Keeps
/// slide text safe (content comes from a model) while allowing light emphasis.
fn inline(s: &str) -> String {
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
    if bullets.is_empty() {
        return String::new();
    }
    let items: String = bullets
        .iter()
        .map(|b| format!("<li>{}</li>", inline(b)))
        .collect();
    format!("<ul class=\"bullets\">{items}</ul>")
}

/// Render one slide as an inner HTML fragment (without the outer `<section>`).
fn render_slide_inner(slide: &Slide) -> String {
    let title = if slide.title.is_empty() {
        String::new()
    } else {
        format!("<h1 class=\"s-title\">{}</h1>", inline(&slide.title))
    };
    let subtitle = if slide.subtitle.is_empty() {
        String::new()
    } else {
        format!("<p class=\"s-sub\">{}</p>", inline(&slide.subtitle))
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
    let image = if slide.image.is_empty() {
        String::new()
    } else {
        format!(
            "<div class=\"s-image\"><img src=\"{}\" alt=\"\"/></div>",
            esc(&slide.image)
        )
    };

    match slide.layout.as_str() {
        "cover" => format!(
            "<div class=\"box cover\"><div class=\"accent\"></div>{title}{subtitle}</div>"
        ),
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
        "bullets" => format!(
            "<div class=\"box content\">{title}{subtitle}{}</div>",
            bullets_html(&slide.bullets)
        ),
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
            format!("<div class=\"box content\">{title}{subtitle}<div class=\"cols\">{cols}</div></div>")
        }
        "image" => format!(
            "<div class=\"box image-layout\">{title}{subtitle}{image}</div>"
        ),
        "image-left" => format!(
            "<div class=\"box split\">{image}<div class=\"split-text\">{title}{subtitle}{}{body}</div></div>",
            bullets_html(&slide.bullets)
        ),
        // "content" and any unknown layout: generic title + subtitle + bullets + body.
        _ => format!(
            "<div class=\"box content\">{title}{subtitle}{}{body}{image}</div>",
            bullets_html(&slide.bullets)
        ),
    }
}

/// Full self-contained HTML document. If `only` is `Some(i)`, render just that
/// slide (focused editor preview); otherwise render the whole deck (export /
/// scrollable preview). `print` adds page-break rules for PDF export.
pub fn render_deck_html(deck: &Deck, only: Option<usize>, print: bool) -> String {
    let theme = if THEMES.contains(&deck.theme.as_str()) {
        deck.theme.as_str()
    } else {
        "midnight"
    };
    let sections: String = deck
        .slides
        .iter()
        .enumerate()
        .filter(|(i, _)| only.map(|o| o == *i).unwrap_or(true))
        .map(|(i, s)| {
            format!(
                "<section class=\"slide layout-{}\" data-index=\"{}\">{}<div class=\"pagenum\">{}</div></section>",
                esc(&s.layout),
                i,
                render_slide_inner(s),
                i + 1
            )
        })
        .collect();
    let print_css = if print {
        "@media print{body{background:#000;}.slide{page-break-after:always;box-shadow:none;margin:0;}}"
    } else {
        ""
    };
    format!(
        "<!doctype html><html lang=\"zh\"><head><meta charset=\"utf-8\"/>\
<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"/>\
<title>{}</title><style>{}{}</style></head>\
<body class=\"theme-{}\">{}</body></html>",
        esc(&deck.title),
        BASE_CSS,
        print_css,
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
.cols{display:grid;grid-template-columns:1fr 1fr;gap:56px;margin-top:12px}
.col h2{font-size:30px;margin-bottom:16px;font-weight:700}
.col p{font-size:24px;line-height:1.5;opacity:.9}
.quote blockquote{font-size:44px;line-height:1.4;font-weight:700}
.quote cite{display:block;margin-top:28px;font-size:26px;font-style:normal;opacity:.75}
.image-layout .s-image{flex:1;display:flex;align-items:center;justify-content:center;margin-top:12px}
.image-layout .s-image img{max-width:100%;max-height:100%;border-radius:12px}
.split{padding:0}
.split .s-image{width:46%;height:100%}
.split .s-image img{width:100%;height:100%;object-fit:cover}
.split .split-text{flex:1;padding:80px;display:flex;flex-direction:column;justify-content:center;gap:20px}
.pagenum{position:absolute;bottom:28px;right:40px;font-size:18px;opacity:.5}
/* ── theme: midnight ── */
.theme-midnight .slide{background:linear-gradient(135deg,#111a33 0%,#0b1020 100%);color:#eaf0ff}
.theme-midnight .s-sub,.theme-midnight .col p{color:#aab8dd}
.theme-midnight .bullets li:before,.theme-midnight .accent{background:#4dd0e1}
.theme-midnight .s-title,.theme-midnight .quote blockquote{background:linear-gradient(90deg,#eaf0ff,#8fb6ff);-webkit-background-clip:text;background-clip:text;-webkit-text-fill-color:transparent}
.theme-midnight .col h2{color:#4dd0e1}
/* ── theme: minimal ── */
.theme-minimal .slide{background:#ffffff;color:#1a1a2e}
.theme-minimal .s-sub,.theme-minimal .col p{color:#5a5a72}
.theme-minimal .bullets li:before,.theme-minimal .accent{background:#111}
.theme-minimal .col h2{color:#111}
.theme-minimal .quote blockquote{color:#111}
/* ── theme: corporate ── */
.theme-corporate .slide{background:#f4f7fb;color:#0f2540}
.theme-corporate .s-sub,.theme-corporate .col p{color:#3d5a80}
.theme-corporate .bullets li:before,.theme-corporate .accent{background:#2f6fed}
.theme-corporate .col h2{color:#2f6fed}
.theme-corporate .box.cover,.theme-corporate .box.section{background:linear-gradient(135deg,#0f2540,#1b3a5c);color:#fff;margin:0}
.theme-corporate .box.cover .s-sub{color:#bcd3f5}
/* ── theme: sunset ── */
.theme-sunset .slide{background:linear-gradient(135deg,#2b1055 0%,#7597de 100%);color:#fff}
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
      "layout": "cover | section | bullets | content | two-column | quote | image | image-left",
      "title": "标题（cover/section 用大标题）",
      "subtitle": "副标题/署名（quote 里作为出处）",
      "bullets": ["要点1", "要点2"],
      "body": "正文段落（quote 里作为引文正文，可用 \n 分段）",
      "columns": [{"title":"列标题","bullets":["..."]}],
      "image": "图片URL（可留空）"
    }
  ]
}
规则：首页用 cover；每页只放该 layout 需要的字段；bullets 每条精炼不超过一行；要点用 **加粗** 强调关键词；一份演示 8-14 页为宜，宁少而精。用与用户需求相同的语言撰写。"#;

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

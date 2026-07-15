//! Tauri commands for the PPT / presentation panel (user request #4).
//!
//! Deck persistence (SQLite `decks`), the canonical HTML render (preview ==
//! export), and AI generate/edit via the gateway model. The structured JSON
//! `Deck` is the single source of truth, so AI edits are surgical + the render
//! is deterministic.

use std::sync::Arc;

use chrono::Utc;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::db::DbManager;
use crate::knowledge;
use crate::slides::{self, Deck};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeckMeta {
    pub id: String,
    pub title: String,
    pub theme: String,
    pub slide_count: i64,
    pub updated_at: String,
}

/// A full deck as sent to the frontend. `model_json` is the serialized `Deck`
/// the editor mutates and passes back on save/render.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeckRecord {
    pub id: String,
    pub title: String,
    pub theme: String,
    pub model_json: String,
}

fn make_id() -> String {
    let nanos = Utc::now()
        .timestamp_nanos_opt()
        .unwrap_or_else(|| Utc::now().timestamp_millis() * 1_000_000);
    format!("deck_{nanos}_{}", std::process::id())
}

/// Serialize a `Deck` back to a JSON string, persist it, and return the record.
fn persist_deck(db: &DbManager, mut deck: Deck) -> Result<DeckRecord, String> {
    if deck.id.is_empty() {
        deck.id = make_id();
    }
    let model_json = serde_json::to_string(&deck).map_err(|e| e.to_string())?;
    let count = deck.slides.len() as i64;
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO decks (id, title, theme, model_json, slide_count, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, CURRENT_TIMESTAMP)
         ON CONFLICT(id) DO UPDATE SET
            title=excluded.title, theme=excluded.theme,
            model_json=excluded.model_json, slide_count=excluded.slide_count,
            updated_at=CURRENT_TIMESTAMP",
        params![deck.id, deck.title, deck.theme, model_json, count],
    )
    .map_err(|e| e.to_string())?;
    Ok(DeckRecord {
        id: deck.id,
        title: deck.title,
        theme: deck.theme,
        model_json,
    })
}

/// Snapshot the deck's CURRENT stored model before an AI mutation overwrites it,
/// so any AI edit is undoable. Keeps the newest 20 versions per deck.
/// Best-effort: a snapshot failure must never block the edit itself.
fn snapshot(db: &DbManager, deck_id: &str, label: &str) {
    let Ok(conn) = db.get_connection() else { return };
    let current: Option<String> = conn
        .query_row(
            "SELECT model_json FROM decks WHERE id = ?1",
            params![deck_id],
            |r| r.get(0),
        )
        .ok();
    let Some(json) = current else { return };
    let _ = conn.execute(
        "INSERT INTO deck_versions (deck_id, model_json, label) VALUES (?1, ?2, ?3)",
        params![deck_id, json, label],
    );
    let _ = conn.execute(
        "DELETE FROM deck_versions WHERE deck_id = ?1 AND id NOT IN
           (SELECT id FROM deck_versions WHERE deck_id = ?1 ORDER BY id DESC LIMIT 20)",
        params![deck_id],
    );
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeckVersion {
    pub id: i64,
    pub label: String,
    pub created_at: String,
}

#[tauri::command]
pub fn list_deck_versions(
    id: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<Vec<DeckVersion>, String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare("SELECT id, label, created_at FROM deck_versions WHERE deck_id = ?1 ORDER BY id DESC")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![id], |r| {
            Ok(DeckVersion { id: r.get(0)?, label: r.get(1)?, created_at: r.get(2)? })
        })
        .map_err(|e| e.to_string())?;
    Ok(rows.flatten().collect())
}

/// Restore a snapshot (or the latest one when `version_id` is None = plain undo).
/// The pre-restore state is itself snapshotted, so undo is undoable.
#[tauri::command]
pub fn restore_deck_version(
    id: String,
    version_id: Option<i64>,
    db: State<'_, Arc<DbManager>>,
) -> Result<DeckRecord, String> {
    let (vid, json): (i64, String) = {
        let conn = db.get_connection().map_err(|e| e.to_string())?;
        match version_id {
            Some(v) => conn.query_row(
                "SELECT id, model_json FROM deck_versions WHERE deck_id = ?1 AND id = ?2",
                params![id, v],
                |r| Ok((r.get(0)?, r.get(1)?)),
            ),
            None => conn.query_row(
                "SELECT id, model_json FROM deck_versions WHERE deck_id = ?1 ORDER BY id DESC LIMIT 1",
                params![id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            ),
        }
        .map_err(|_| "没有可回退的版本".to_string())?
    };
    snapshot(&db, &id, "回退前");
    let mut deck: Deck = serde_json::from_str(&json).map_err(|e| e.to_string())?;
    deck.id = id.clone();
    let rec = persist_deck(&db, deck)?;
    // The restored snapshot is now the live state — drop it from history.
    if let Ok(conn) = db.get_connection() {
        let _ = conn.execute("DELETE FROM deck_versions WHERE id = ?1", params![vid]);
    }
    Ok(rec)
}

#[tauri::command]
pub fn list_decks(db: State<'_, Arc<DbManager>>) -> Result<Vec<DeckMeta>, String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT id, title, theme, slide_count, updated_at
             FROM decks ORDER BY updated_at DESC",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok(DeckMeta {
                id: r.get(0)?,
                title: r.get(1)?,
                theme: r.get(2)?,
                slide_count: r.get(3)?,
                updated_at: r.get(4)?,
            })
        })
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r.map_err(|e| e.to_string())?);
    }
    Ok(out)
}

#[tauri::command]
pub fn get_deck(id: String, db: State<'_, Arc<DbManager>>) -> Result<DeckRecord, String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.query_row(
        "SELECT id, title, theme, model_json FROM decks WHERE id = ?1",
        params![id],
        |r| {
            Ok(DeckRecord {
                id: r.get(0)?,
                title: r.get(1)?,
                theme: r.get(2)?,
                model_json: r.get(3)?,
            })
        },
    )
    .map_err(|_| "演示不存在".to_string())
}

#[tauri::command]
pub fn create_deck(
    title: String,
    theme: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<DeckRecord, String> {
    let title = if title.trim().is_empty() {
        "未命名演示".to_string()
    } else {
        title
    };
    let theme = if slides::THEMES.contains(&theme.as_str()) {
        theme
    } else {
        "midnight".to_string()
    };
    let deck = Deck {
        id: make_id(),
        title: title.clone(),
        theme,
        brand: None,
        slides: vec![slides::Slide {
            layout: "cover".to_string(),
            title,
            subtitle: "用下方指令让 AI 生成，或直接编辑".to_string(),
            ..Default::default()
        }],
    };
    persist_deck(&db, deck)
}

/// Save an edited deck. The incoming `model_json` is parsed (validated) before
/// persisting so a malformed edit can never corrupt the stored deck.
#[tauri::command]
pub fn save_deck(
    id: String,
    model_json: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<DeckRecord, String> {
    let mut deck: Deck =
        serde_json::from_str(&model_json).map_err(|e| format!("演示 JSON 无效: {e}"))?;
    deck.id = id;
    if deck.slides.is_empty() {
        return Err("演示至少需要一页".to_string());
    }
    persist_deck(&db, deck)
}

#[tauri::command]
pub fn delete_deck(id: String, db: State<'_, Arc<DbManager>>) -> Result<(), String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM decks WHERE id = ?1", params![id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Render a deck (or one slide) to a self-contained HTML document. Used both for
/// the live preview (iframe srcdoc) and for export. No DB access — pure.
#[tauri::command]
pub fn render_deck(
    model_json: String,
    slide_index: Option<usize>,
    print: bool,
) -> Result<String, String> {
    let deck: Deck =
        serde_json::from_str(&model_json).map_err(|e| format!("演示 JSON 无效: {e}"))?;
    Ok(slides::render_deck_html(&deck, slide_index, print))
}

fn exports_dir() -> Result<std::path::PathBuf, String> {
    let dir = crate::storage::exports_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

/// Keep deck titles usable as file names.
fn sanitize_filename(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c => c,
        })
        .collect();
    let trimmed = cleaned.trim();
    if trimmed.is_empty() {
        "presentation".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Reveal the exported file in Explorer / file manager (GUI launcher — no console).
fn reveal_in_folder(path: &std::path::Path) {
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("explorer")
            .arg("/select,")
            .arg(path)
            .spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg("-R").arg(path).spawn();
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if let Some(parent) = path.parent() {
            let _ = std::process::Command::new("xdg-open").arg(parent).spawn();
        }
    }
}

/// Export the deck as a self-contained HTML file into ~/.omnix/exports.
#[tauri::command]
pub fn export_deck_html(model_json: String) -> Result<String, String> {
    let deck: Deck =
        serde_json::from_str(&model_json).map_err(|e| format!("演示 JSON 无效: {e}"))?;
    let html = crate::slides::render_deck_html(&deck, None, true);
    let path = exports_dir()?.join(format!("{}.html", sanitize_filename(&deck.title)));
    std::fs::write(&path, html).map_err(|e| format!("写出 HTML 失败: {e}"))?;
    reveal_in_folder(&path);
    Ok(path.to_string_lossy().to_string())
}

#[cfg(windows)]
fn find_edge() -> Option<std::path::PathBuf> {
    for base in [
        std::env::var("ProgramFiles(x86)").ok(),
        std::env::var("ProgramFiles").ok(),
    ]
    .into_iter()
    .flatten()
    {
        let p = std::path::PathBuf::from(base).join("Microsoft/Edge/Application/msedge.exe");
        if p.exists() {
            return Some(p);
        }
    }
    which::which("msedge").ok()
}

/// Export the deck as a real PDF using Edge/Chrome headless printing (Windows
/// ships Edge, so no extra dependency). Falls back with a clear error telling
/// the user to export HTML and print manually if no browser is found.
#[tauri::command]
pub async fn export_deck_pdf(model_json: String) -> Result<String, String> {
    let deck: Deck =
        serde_json::from_str(&model_json).map_err(|e| format!("演示 JSON 无效: {e}"))?;
    let html = crate::slides::render_deck_html(&deck, None, true);
    let dir = exports_dir()?;
    let stem = sanitize_filename(&deck.title);
    let html_path = dir.join(format!(".{stem}.print.html"));
    let pdf_path = dir.join(format!("{stem}.pdf"));
    std::fs::write(&html_path, html).map_err(|e| format!("写出临时 HTML 失败: {e}"))?;

    #[cfg(windows)]
    let browser = find_edge();
    #[cfg(not(windows))]
    let browser = which::which("google-chrome")
        .or_else(|_| which::which("chromium"))
        .ok();

    let Some(browser) = browser else {
        let _ = std::fs::remove_file(&html_path);
        return Err("没找到 Edge/Chrome，无法直接导出 PDF——请改用「导出 HTML」后在浏览器里打印为 PDF".into());
    };

    let file_url = format!("file:///{}", html_path.to_string_lossy().replace('\\', "/"));
    let mut cmd = tokio::process::Command::new(&browser);
    cmd.arg("--headless=new")
        .arg("--disable-gpu")
        .arg("--no-pdf-header-footer")
        // 1280×720 slide → landscape page matching the 16:9 canvas
        .arg(format!("--print-to-pdf={}", pdf_path.to_string_lossy()))
        .arg(&file_url);
    crate::proc::NoWindow::no_window(&mut cmd);
    let status = cmd
        .status()
        .await
        .map_err(|e| format!("启动浏览器打印失败: {e}"))?;
    let _ = std::fs::remove_file(&html_path);
    if !status.success() || !pdf_path.exists() {
        return Err("浏览器打印 PDF 失败——请改用「导出 HTML」后手动打印".into());
    }
    reveal_in_folder(&pdf_path);
    Ok(pdf_path.to_string_lossy().to_string())
}

/// Generate a brand-new deck from a topic via the gateway model, persist it.
#[tauri::command]
pub async fn generate_deck(
    topic: String,
    chat_model: String,
    slide_count: Option<u32>,
    db: State<'_, Arc<DbManager>>,
) -> Result<DeckRecord, String> {
    if topic.trim().is_empty() {
        return Err("请先描述要做什么演示".to_string());
    }
    let prompt = slides::build_generate_prompt(topic.trim(), slide_count.unwrap_or(10));
    let reply = knowledge::chat_once(&db, &chat_model, &prompt).await?;
    let deck = slides::parse_deck(&reply)?;
    persist_deck(&db, deck)
}

// ── A：两阶段生成（大纲 → 展开）─────────────────────────────────────────────

/// Stage 1: plan the deck as an outline the user can fix in seconds before any
/// expensive full generation happens.
#[tauri::command]
pub async fn generate_outline(
    topic: String,
    chat_model: String,
    slide_count: Option<u32>,
    db: State<'_, Arc<DbManager>>,
) -> Result<slides::Outline, String> {
    if topic.trim().is_empty() {
        return Err("请先描述要做什么演示".to_string());
    }
    let prompt = slides::build_outline_prompt(topic.trim(), slide_count.unwrap_or(10));
    let reply = knowledge::chat_once(&db, &chat_model, &prompt).await?;
    let json = slides::extract_json(&reply).ok_or("回复里找不到大纲 JSON")?;
    let mut outline: slides::Outline =
        serde_json::from_str(&json).map_err(|e| format!("大纲 JSON 解析失败: {e}"))?;
    if outline.items.is_empty() {
        return Err("生成的大纲是空的".to_string());
    }
    if !slides::THEMES.contains(&outline.theme.as_str()) {
        outline.theme = "midnight".to_string();
    }
    Ok(outline)
}

/// Stage 2: expand each outline item into a full slide, **in parallel**, then
/// persist as a new deck. A page that fails to parse degrades to its outline
/// content instead of sinking the whole deck.
#[tauri::command]
pub async fn expand_outline(
    outline: slides::Outline,
    chat_model: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<DeckRecord, String> {
    if outline.items.is_empty() {
        return Err("大纲是空的".to_string());
    }
    let total = outline.items.len();
    let title = outline.title.clone();

    let mut tasks = Vec::new();
    for (i, item) in outline.items.iter().enumerate() {
        let prompt = slides::build_expand_slide_prompt(&title, i, total, item);
        let model = chat_model.clone();
        let db2 = Arc::clone(&db);
        let fallback = item.clone();
        tasks.push(async move {
            match knowledge::chat_once(&db2, &model, &prompt).await {
                Ok(reply) => slides::extract_json(&reply)
                    .and_then(|j| serde_json::from_str::<slides::Slide>(&j).ok())
                    .unwrap_or_else(|| fallback_slide(&fallback)),
                Err(_) => fallback_slide(&fallback),
            }
        });
    }
    let expanded: Vec<slides::Slide> = futures::future::join_all(tasks).await;

    let deck = Deck {
        id: make_id(),
        title: outline.title,
        theme: outline.theme,
        brand: None,
        slides: expanded,
    };
    persist_deck(&db, deck)
}

/// Outline item → a usable slide when the model call/parse fails.
fn fallback_slide(item: &slides::OutlineItem) -> slides::Slide {
    slides::Slide {
        layout: item.layout.clone(),
        title: item.title.clone(),
        bullets: item.points.clone(),
        ..Default::default()
    }
}

// ── B：单页精修（差分编辑）──────────────────────────────────────────────────

/// Edit ONE slide: only that slide's JSON goes to the model and only that slide
/// comes back. Much faster/cheaper than a whole-deck round trip, and it
/// physically cannot corrupt other pages.
#[tauri::command]
pub async fn edit_slide_ai(
    id: String,
    slide_index: usize,
    instruction: String,
    chat_model: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<DeckRecord, String> {
    if instruction.trim().is_empty() {
        return Err("请先输入修改指令".to_string());
    }
    let current = {
        let conn = db.get_connection().map_err(|e| e.to_string())?;
        conn.query_row(
            "SELECT model_json FROM decks WHERE id = ?1",
            params![id],
            |r| r.get::<_, String>(0),
        )
        .map_err(|_| "演示不存在".to_string())?
    };
    let mut deck: Deck = serde_json::from_str(&current).map_err(|e| e.to_string())?;
    let slide = deck
        .slides
        .get(slide_index)
        .ok_or_else(|| "页码超出范围".to_string())?;
    let slide_json = serde_json::to_string_pretty(slide).map_err(|e| e.to_string())?;
    let prompt = slides::build_slide_edit_prompt(
        &deck.title,
        slide_index,
        deck.slides.len(),
        &slide_json,
        instruction.trim(),
    );
    let reply = knowledge::chat_once(&db, &chat_model, &prompt).await?;
    snapshot(&db, &id, &format!("AI 改第 {} 页前", slide_index + 1));
    let json = slides::extract_json(&reply).ok_or("回复里找不到这一页的 JSON")?;
    let new_slide: slides::Slide =
        serde_json::from_str(&json).map_err(|e| format!("单页 JSON 解析失败: {e}"))?;
    deck.slides[slide_index] = new_slide;
    deck.id = id;
    persist_deck(&db, deck)
}

// ── C：自动配图 ─────────────────────────────────────────────────────────────

/// Suggested image prompt for a slide (local, no model call).
#[tauri::command]
pub fn suggest_slide_image_prompt(
    model_json: String,
    slide_index: usize,
) -> Result<String, String> {
    let deck: Deck = serde_json::from_str(&model_json).map_err(|e| e.to_string())?;
    let slide = deck.slides.get(slide_index).ok_or("页码超出范围")?;
    Ok(slides::build_image_prompt(slide, &deck.title))
}

/// Generate an illustration through the existing media pipeline and write the
/// resulting local path into `slide.image` (renderer inlines it as a data URI,
/// so preview/HTML/PDF/pptx all stay self-contained).
#[tauri::command]
pub async fn generate_slide_image(
    id: String,
    slide_index: usize,
    platform_id: String,
    model: String,
    prompt: String,
    size: Option<String>,
    db: State<'_, Arc<DbManager>>,
) -> Result<DeckRecord, String> {
    let task = crate::commands::media_generate_image_core(
        &db,
        &platform_id,
        &model,
        prompt.trim(),
        size.as_deref().unwrap_or("1280x720"),
    )
    .await?;
    let path = task
        .result_path
        .ok_or_else(|| task.error.unwrap_or_else(|| "生图未返回结果".to_string()))?;

    let current = {
        let conn = db.get_connection().map_err(|e| e.to_string())?;
        conn.query_row(
            "SELECT model_json FROM decks WHERE id = ?1",
            params![id],
            |r| r.get::<_, String>(0),
        )
        .map_err(|_| "演示不存在".to_string())?
    };
    let mut deck: Deck = serde_json::from_str(&current).map_err(|e| e.to_string())?;
    snapshot(&db, &id, &format!("配图第 {} 页前", slide_index + 1));
    let slide = deck
        .slides
        .get_mut(slide_index)
        .ok_or_else(|| "页码超出范围".to_string())?;
    slide.image = path;
    // A text-only layout won't show the picture — promote it so the image lands.
    if slide.layout == "bullets" || slide.layout == "cover" || slide.layout == "section" {
        slide.layout = "image-left".to_string();
    }
    deck.id = id;
    persist_deck(&db, deck)
}

// ── D：母版 / 品牌 ──────────────────────────────────────────────────────────

/// Reusable brand masters (saved separately from any one deck).
#[tauri::command]
pub fn list_brands(db: State<'_, Arc<DbManager>>) -> Result<Vec<slides::Brand>, String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare("SELECT brand_json FROM deck_brands ORDER BY updated_at DESC")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(|e| e.to_string())?;
    Ok(rows
        .flatten()
        .filter_map(|j| serde_json::from_str::<slides::Brand>(&j).ok())
        .collect())
}

#[tauri::command]
pub fn save_brand(brand: slides::Brand, db: State<'_, Arc<DbManager>>) -> Result<(), String> {
    if brand.name.trim().is_empty() {
        return Err("母版需要一个名字".to_string());
    }
    let json = serde_json::to_string(&brand).map_err(|e| e.to_string())?;
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO deck_brands (name, brand_json, updated_at) VALUES (?1, ?2, CURRENT_TIMESTAMP)
         ON CONFLICT(name) DO UPDATE SET brand_json=excluded.brand_json, updated_at=CURRENT_TIMESTAMP",
        params![brand.name, json],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn delete_brand(name: String, db: State<'_, Arc<DbManager>>) -> Result<(), String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM deck_brands WHERE name = ?1", params![name])
        .map_err(|e| e.to_string())?;
    Ok(())
}

// ── E：导出 .pptx ───────────────────────────────────────────────────────────

/// Export a real PowerPoint file from the same JSON model.
#[tauri::command]
pub fn export_deck_pptx(model_json: String) -> Result<String, String> {
    let deck: Deck =
        serde_json::from_str(&model_json).map_err(|e| format!("演示 JSON 无效: {e}"))?;
    let bytes = crate::pptx::build_pptx(&deck)?;
    let path = exports_dir()?.join(format!("{}.pptx", sanitize_filename(&deck.title)));
    std::fs::write(&path, bytes).map_err(|e| format!("写出 pptx 失败: {e}"))?;
    reveal_in_folder(&path);
    Ok(path.to_string_lossy().to_string())
}

/// AI-edit an existing deck with a natural-language instruction. Loads the
/// current model, asks the model to change only what's needed, re-persists.
#[tauri::command]
pub async fn edit_deck_ai(
    id: String,
    instruction: String,
    chat_model: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<DeckRecord, String> {
    if instruction.trim().is_empty() {
        return Err("请先输入修改指令".to_string());
    }
    let current = {
        let conn = db.get_connection().map_err(|e| e.to_string())?;
        conn.query_row(
            "SELECT model_json FROM decks WHERE id = ?1",
            params![id],
            |r| r.get::<_, String>(0),
        )
        .map_err(|_| "演示不存在".to_string())?
    };
    let prompt = slides::build_edit_prompt(&current, instruction.trim());
    let reply = knowledge::chat_once(&db, &chat_model, &prompt).await?;
    let mut deck = slides::parse_deck(&reply)?;
    snapshot(&db, &id, "AI 改整份前");
    deck.id = id;
    persist_deck(&db, deck)
}

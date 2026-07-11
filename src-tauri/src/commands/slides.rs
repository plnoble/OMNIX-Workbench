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
    let dir = dirs::home_dir()
        .ok_or_else(|| "找不到用户目录".to_string())?
        .join(".omnix")
        .join("exports");
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
    deck.id = id;
    persist_deck(&db, deck)
}

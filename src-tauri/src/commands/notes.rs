//! Notes (笔记) — lightweight local Markdown notes. Notes can be created anywhere, and the Quick
//! Assistant can save a result/selection straight into one (`source` records
//! where it came from). Stored locally in SQLite; content is plain Markdown.

use std::path::PathBuf;
use std::sync::Arc;

use rusqlite::params;
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::db::DbManager;

/// `~/.omnix/notes/` — notes are mirrored here as real Markdown files so they
/// exist on disk (portable, and a file-based agent can read them).
fn notes_dir() -> Option<PathBuf> {
    let mut dir = dirs::home_dir()?;
    dir.push(".omnix");
    dir.push("notes");
    let _ = std::fs::create_dir_all(&dir);
    Some(dir)
}

fn write_note_file(note: &Note) {
    if let Some(dir) = notes_dir() {
        let body = format!("# {}\n\n{}\n", note.title, note.content);
        let _ = std::fs::write(dir.join(format!("{}.md", note.id)), body);
    }
}

fn delete_note_file(id: &str) {
    if let Some(dir) = notes_dir() {
        let _ = std::fs::remove_file(dir.join(format!("{}.md", id)));
    }
}

/// Absolute path of the on-disk notes folder (for an "open folder" action).
#[tauri::command]
pub fn get_notes_dir() -> Result<String, String> {
    notes_dir()
        .map(|p| p.to_string_lossy().into_owned())
        .ok_or_else(|| "无法定位笔记目录".into())
}

/// Open the notes folder in the OS file manager. Uses the native command
/// directly so it never depends on the opener plugin's JS permission grant.
#[tauri::command]
pub fn open_notes_folder() -> Result<(), String> {
    let dir = notes_dir().ok_or_else(|| "无法定位笔记目录".to_string())?;
    let path = dir.to_string_lossy().to_string();
    #[cfg(windows)]
    {
        std::process::Command::new("explorer").arg(&path).spawn().map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open").arg(&path).spawn().map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open").arg(&path).spawn().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Note {
    pub id: String,
    pub title: String,
    pub content: String,
    pub tags: String,
    pub source: String,
    pub created_at: String,
    pub updated_at: String,
}

fn ensure_table(db: &DbManager) -> Result<(), String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS notes (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL DEFAULT '',
            content TEXT NOT NULL DEFAULT '',
            tags TEXT NOT NULL DEFAULT '',
            source TEXT NOT NULL DEFAULT '',
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now'))
        )",
        [],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

fn row_to_note(row: &rusqlite::Row) -> rusqlite::Result<Note> {
    Ok(Note {
        id: row.get(0)?,
        title: row.get(1)?,
        content: row.get(2)?,
        tags: row.get(3)?,
        source: row.get(4)?,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
    })
}

const COLS: &str = "id, title, content, tags, source, created_at, updated_at";

#[tauri::command]
pub fn list_notes(query: Option<String>, db: State<'_, Arc<DbManager>>) -> Result<Vec<Note>, String> {
    ensure_table(&db)?;
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let term = query.unwrap_or_default();
    let mut stmt = conn
        .prepare(&format!(
            "SELECT {COLS} FROM notes
             WHERE (?1 = '' OR title LIKE '%' || ?1 || '%' OR content LIKE '%' || ?1 || '%' OR tags LIKE '%' || ?1 || '%')
             ORDER BY updated_at DESC"
        ))
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![term], row_to_note)
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_note(
    id: Option<String>,
    title: String,
    content: String,
    tags: Option<String>,
    source: Option<String>,
    db: State<'_, Arc<DbManager>>,
) -> Result<Note, String> {
    ensure_table(&db)?;
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let id = id.unwrap_or_else(|| format!("note_{}", chrono::Utc::now().timestamp_micros()));
    let title = if title.trim().is_empty() { "无标题笔记".to_string() } else { title };
    conn.execute(
        "INSERT INTO notes (id, title, content, tags, source)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(id) DO UPDATE SET
            title = excluded.title, content = excluded.content,
            tags = excluded.tags, updated_at = datetime('now')",
        params![id, title.trim(), content, tags.unwrap_or_default(), source.unwrap_or_default()],
    )
    .map_err(|e| e.to_string())?;
    let note = conn
        .query_row(&format!("SELECT {COLS} FROM notes WHERE id = ?1"), params![id], row_to_note)
        .map_err(|e| e.to_string())?;
    // Mirror to ~/.omnix/notes/<id>.md so the note exists as a real file.
    write_note_file(&note);
    Ok(note)
}

#[tauri::command]
pub fn delete_note(id: String, db: State<'_, Arc<DbManager>>) -> Result<(), String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM notes WHERE id = ?1", params![id])
        .map_err(|e| e.to_string())?;
    delete_note_file(&id);
    Ok(())
}

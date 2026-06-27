//! Custom assistants (助手库深化: 自定义 + 分享). The built-in assistant
//! templates (`agent_templates.rs`) are read-only presets; this lets the user
//! create their own and share them by exporting/importing JSON. Stored locally
//! in SQLite; shape mirrors `AgentTemplate` so the UI can render both uniformly.

use std::sync::Arc;

use rusqlite::params;
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::db::DbManager;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomAssistant {
    pub slug: String,
    pub name: String,
    pub description: String,
    pub category: String,
    pub instructions: String,
    pub created_at: String,
}

fn ensure_table(db: &DbManager) -> Result<(), String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS custom_assistants (
            slug TEXT PRIMARY KEY,
            name TEXT NOT NULL DEFAULT '',
            description TEXT NOT NULL DEFAULT '',
            category TEXT NOT NULL DEFAULT '自定义',
            instructions TEXT NOT NULL DEFAULT '',
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        )",
        [],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn list_custom_assistants(db: State<'_, Arc<DbManager>>) -> Result<Vec<CustomAssistant>, String> {
    ensure_table(&db)?;
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare("SELECT slug, name, description, category, instructions, created_at FROM custom_assistants ORDER BY created_at DESC")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok(CustomAssistant {
                slug: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                category: row.get(3)?,
                instructions: row.get(4)?,
                created_at: row.get(5)?,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

/// Create or overwrite a custom assistant. Used both by the "新建助手" form and
/// by importing a shared assistant JSON. A blank `slug` is generated.
#[tauri::command]
pub fn save_custom_assistant(
    slug: Option<String>,
    name: String,
    description: String,
    category: Option<String>,
    instructions: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<CustomAssistant, String> {
    ensure_table(&db)?;
    if name.trim().is_empty() {
        return Err("请填写助手名称".into());
    }
    if instructions.trim().is_empty() {
        return Err("请填写助手提示词".into());
    }
    let slug = slug
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| format!("custom-{}", chrono::Utc::now().timestamp_micros()));
    let category = category.filter(|c| !c.trim().is_empty()).unwrap_or_else(|| "自定义".into());
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO custom_assistants (slug, name, description, category, instructions)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(slug) DO UPDATE SET
            name = excluded.name, description = excluded.description,
            category = excluded.category, instructions = excluded.instructions",
        params![slug, name.trim(), description, category, instructions],
    )
    .map_err(|e| e.to_string())?;
    conn.query_row(
        "SELECT slug, name, description, category, instructions, created_at FROM custom_assistants WHERE slug = ?1",
        params![slug],
        |row| {
            Ok(CustomAssistant {
                slug: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                category: row.get(3)?,
                instructions: row.get(4)?,
                created_at: row.get(5)?,
            })
        },
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_custom_assistant(slug: String, db: State<'_, Arc<DbManager>>) -> Result<(), String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM custom_assistants WHERE slug = ?1", params![slug])
        .map_err(|e| e.to_string())?;
    Ok(())
}

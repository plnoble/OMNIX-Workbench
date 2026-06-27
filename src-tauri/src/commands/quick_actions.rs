//! Custom Quick Assistant actions (划词助手深挖, Cherry Studio inspired). The
//! popup ships 6 built-in actions (translate/explain/summarize/refine/search/
//! copy); this lets the user define their own prompt-based actions on top —
//! e.g. "改写为正式语气", "提取要点为清单", "翻译并解释生词". Each custom action
//! is a label + emoji + a prompt template where `{{text}}` is replaced by the
//! selected text (appended if the placeholder is absent).

use std::sync::Arc;

use rusqlite::params;
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::db::DbManager;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuickAction {
    pub id: String,
    pub label: String,
    pub emoji: String,
    pub prompt_template: String,
    pub enabled: bool,
    pub order_num: i32,
    pub created_at: String,
}

fn ensure_table(db: &DbManager) -> Result<(), String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS quick_actions (
            id TEXT PRIMARY KEY,
            label TEXT NOT NULL DEFAULT '',
            emoji TEXT NOT NULL DEFAULT '✨',
            prompt_template TEXT NOT NULL DEFAULT '',
            enabled INTEGER NOT NULL DEFAULT 1,
            order_num INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        )",
        [],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn list_quick_actions(db: State<'_, Arc<DbManager>>) -> Result<Vec<QuickAction>, String> {
    ensure_table(&db)?;
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare("SELECT id, label, emoji, prompt_template, enabled, order_num, created_at FROM quick_actions ORDER BY order_num ASC, created_at ASC")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok(QuickAction {
                id: row.get(0)?,
                label: row.get(1)?,
                emoji: row.get(2)?,
                prompt_template: row.get(3)?,
                enabled: row.get::<_, i32>(4)? != 0,
                order_num: row.get(5)?,
                created_at: row.get(6)?,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub fn save_quick_action(
    id: Option<String>,
    label: String,
    emoji: String,
    prompt_template: String,
    enabled: bool,
    order_num: i32,
    db: State<'_, Arc<DbManager>>,
) -> Result<QuickAction, String> {
    ensure_table(&db)?;
    if label.trim().is_empty() {
        return Err("请填写动作名称".into());
    }
    if prompt_template.trim().is_empty() {
        return Err("请填写提示词模板".into());
    }
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let id = id.unwrap_or_else(|| format!("qa_{}", chrono::Utc::now().timestamp_micros()));
    let emoji = if emoji.trim().is_empty() { "✨".to_string() } else { emoji };
    conn.execute(
        "INSERT INTO quick_actions (id, label, emoji, prompt_template, enabled, order_num)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(id) DO UPDATE SET
            label = excluded.label, emoji = excluded.emoji,
            prompt_template = excluded.prompt_template, enabled = excluded.enabled,
            order_num = excluded.order_num",
        params![id, label.trim(), emoji, prompt_template, enabled as i32, order_num],
    )
    .map_err(|e| e.to_string())?;
    conn.query_row(
        "SELECT id, label, emoji, prompt_template, enabled, order_num, created_at FROM quick_actions WHERE id = ?1",
        params![id],
        |row| {
            Ok(QuickAction {
                id: row.get(0)?,
                label: row.get(1)?,
                emoji: row.get(2)?,
                prompt_template: row.get(3)?,
                enabled: row.get::<_, i32>(4)? != 0,
                order_num: row.get(5)?,
                created_at: row.get(6)?,
            })
        },
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_quick_action(id: String, db: State<'_, Arc<DbManager>>) -> Result<(), String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM quick_actions WHERE id = ?1", params![id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

//! Per-conversation long-term goal (`/goal`).
//!
//! A goal pins an objective to a conversation. While its status is `active`,
//! `runtime::build_goal_reminder` re-injects the objective into every turn's
//! prompt (see `runtime_manager::send_user_message`). Pausing stops injection
//! without losing the objective; completing/clearing ends it.

use std::sync::Arc;

use rusqlite::params;
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::db::DbManager;
use crate::input_validation;

/// Caps the stored objective length.
const MAX_GOAL_OBJECTIVE_CHARS: usize = 4_000;

#[derive(Debug, Serialize, Deserialize)]
pub struct ConversationGoal {
    pub conversation_id: String,
    pub objective: String,
    /// 'active' | 'paused' | 'complete'
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

fn read_goal(conn: &rusqlite::Connection, conversation_id: &str) -> Option<ConversationGoal> {
    conn.query_row(
        "SELECT conversation_id, objective, status, created_at, updated_at
         FROM conversation_goals WHERE conversation_id = ?1",
        params![conversation_id],
        |row| {
            Ok(ConversationGoal {
                conversation_id: row.get(0)?,
                objective: row.get(1)?,
                status: row.get(2)?,
                created_at: row.get(3)?,
                updated_at: row.get(4)?,
            })
        },
    )
    .ok()
}

#[tauri::command]
pub fn get_conversation_goal(
    conversation_id: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<Option<ConversationGoal>, String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    Ok(read_goal(&conn, &conversation_id))
}

/// Sets (or replaces) the conversation's goal and marks it active.
#[tauri::command]
pub fn set_conversation_goal(
    conversation_id: String,
    objective: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<ConversationGoal, String> {
    input_validation::validate_id(&conversation_id, "conversation_id")?;
    let trimmed = objective.trim();
    if trimmed.is_empty() {
        return Err("目标不能为空".into());
    }
    if trimmed.chars().count() > MAX_GOAL_OBJECTIVE_CHARS {
        return Err(format!("目标过长（最多 {MAX_GOAL_OBJECTIVE_CHARS} 字）"));
    }
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO conversation_goals (conversation_id, objective, status, updated_at)
         VALUES (?1, ?2, 'active', CURRENT_TIMESTAMP)
         ON CONFLICT(conversation_id) DO UPDATE SET
             objective = excluded.objective,
             status = 'active',
             updated_at = CURRENT_TIMESTAMP",
        params![conversation_id, trimmed],
    )
    .map_err(|e| e.to_string())?;
    read_goal(&conn, &conversation_id).ok_or_else(|| "读取目标失败".into())
}

/// Pauses / resumes / completes the goal. Only 'active' goals inject.
#[tauri::command]
pub fn set_conversation_goal_status(
    conversation_id: String,
    status: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<ConversationGoal, String> {
    let status = match status.as_str() {
        "active" | "paused" | "complete" => status,
        other => return Err(format!("未知目标状态：{other}")),
    };
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let changed = conn
        .execute(
            "UPDATE conversation_goals SET status = ?2, updated_at = CURRENT_TIMESTAMP
             WHERE conversation_id = ?1",
            params![conversation_id, status],
        )
        .map_err(|e| e.to_string())?;
    if changed == 0 {
        return Err("该对话还没有设定目标".into());
    }
    read_goal(&conn, &conversation_id).ok_or_else(|| "读取目标失败".into())
}

/// Removes the goal entirely.
#[tauri::command]
pub fn clear_conversation_goal(
    conversation_id: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.execute(
        "DELETE FROM conversation_goals WHERE conversation_id = ?1",
        params![conversation_id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

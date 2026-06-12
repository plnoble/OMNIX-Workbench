use tauri::State;
use std::sync::Arc;
use rusqlite::params;
use crate::db::DbManager;
use crate::token_economy::{TokenBudget, FileChange as TokenFileChange};

// ══════════════════════════════════════════════════
// DeepSeek-GUI Inspired Features
// ══════════════════════════════════════════════════

/// Compress tool result to fit token budget
#[tauri::command]
pub fn compress_tool_result(content: String, max_lines: Option<u32>, max_bytes: Option<u32>) -> String {
    let budget = TokenBudget {
        max_tool_result_lines: max_lines.unwrap_or(100),
        max_tool_result_bytes: max_bytes.unwrap_or(10000),
        ..Default::default()
    };
    crate::token_economy::compress_tool_result(&content, &budget)
}

/// Push a steering message to a session
#[tauri::command]
pub fn push_steering_message(
    session_id: String,
    content: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<String, String> {
    // Store in DB
    let conn = db.get_connection().map_err(|e: rusqlite::Error| e.to_string())?;
    let _ = conn.execute(
        "CREATE TABLE IF NOT EXISTS steering_queue (
            id TEXT PRIMARY KEY,
            session_id TEXT NOT NULL,
            content TEXT NOT NULL,
            consumed INTEGER NOT NULL DEFAULT 0,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP
        )", [],
    );
    let id = format!("steer_{}", chrono::Utc::now().timestamp_millis());
    conn.execute(
        "INSERT INTO steering_queue (id, session_id, content) VALUES (?1, ?2, ?3)",
        params![id, session_id, content],
    ).map_err(|e: rusqlite::Error| e.to_string())?;
    Ok(id)
}

/// Get unconsumed steering messages for a session
#[tauri::command]
pub fn get_steering_messages(
    session_id: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<Vec<serde_json::Value>, String> {
    let conn = db.get_connection().map_err(|e: rusqlite::Error| e.to_string())?;
    let _ = conn.execute(
        "CREATE TABLE IF NOT EXISTS steering_queue (
            id TEXT PRIMARY KEY, session_id TEXT NOT NULL, content TEXT NOT NULL,
            consumed INTEGER NOT NULL DEFAULT 0, created_at DATETIME DEFAULT CURRENT_TIMESTAMP
        )", [],
    );
    let mut stmt = conn.prepare(
        "SELECT id, content, created_at FROM steering_queue WHERE session_id = ?1 AND consumed = 0 ORDER BY created_at ASC"
    ).map_err(|e: rusqlite::Error| e.to_string())?;
    let rows = stmt.query_map(params![session_id], |row| {
        Ok(serde_json::json!({
            "id": row.get::<_, String>(0)?,
            "content": row.get::<_, String>(1)?,
            "created_at": row.get::<_, String>(2)?,
        }))
    }).map_err(|e: rusqlite::Error| e.to_string())?;
    Ok(rows.flatten().collect())
}

/// Mark steering messages as consumed
#[tauri::command]
pub fn consume_steering_messages(
    session_id: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    let conn = db.get_connection().map_err(|e: rusqlite::Error| e.to_string())?;
    conn.execute(
        "UPDATE steering_queue SET consumed = 1 WHERE session_id = ?1 AND consumed = 0",
        params![session_id],
    ).map_err(|e: rusqlite::Error| e.to_string())?;
    Ok(())
}

/// Detect file changes (diff summary)
#[tauri::command]
pub fn detect_file_change(
    file_path: String,
    old_content: Option<String>,
    new_content: Option<String>,
) -> TokenFileChange {
    crate::token_economy::detect_file_change(
        &file_path,
        old_content.as_deref(),
        new_content.as_deref(),
    )
}

//! In-session background tasks / sub-agents. A sub-agent is an **independent child agent session** that
//! runs concurrently with the parent in its **own Git worktree** (see
//! `commands/worktrees.rs`), so several agents make progress at once without
//! sharing a working tree. This is real session-level parallelism — NOT
//! intra-session turn pipelining (which would require unsafe turn-engine
//! surgery and is intentionally avoided).
//!
//! The frontend orchestrates the moving parts with existing APIs (worktree
//! create + conversation create + runtime start/send/stop + worktree merge);
//! this module is the persistence layer that links a parent conversation to its
//! child session + worktree and tracks status, so the panel survives reloads.

use std::sync::Arc;

use rusqlite::params;
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::db::DbManager;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubAgent {
    pub id: String,
    pub parent_conversation_id: String,
    pub title: String,
    pub prompt: String,
    pub agent: String,
    pub child_conversation_id: String,
    pub child_session_id: String,
    pub worktree_id: String,
    pub worktree_path: String,
    /// running | awaiting_approval | completed | failed | stopped
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

fn ensure_table(db: &DbManager) -> Result<(), String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS subagents (
            id TEXT PRIMARY KEY,
            parent_conversation_id TEXT NOT NULL,
            title TEXT NOT NULL DEFAULT '',
            prompt TEXT NOT NULL DEFAULT '',
            agent TEXT NOT NULL DEFAULT '',
            child_conversation_id TEXT NOT NULL DEFAULT '',
            child_session_id TEXT NOT NULL DEFAULT '',
            worktree_id TEXT NOT NULL DEFAULT '',
            worktree_path TEXT NOT NULL DEFAULT '',
            status TEXT NOT NULL DEFAULT 'running',
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now'))
        )",
        [],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

fn row_to_subagent(row: &rusqlite::Row) -> rusqlite::Result<SubAgent> {
    Ok(SubAgent {
        id: row.get(0)?,
        parent_conversation_id: row.get(1)?,
        title: row.get(2)?,
        prompt: row.get(3)?,
        agent: row.get(4)?,
        child_conversation_id: row.get(5)?,
        child_session_id: row.get(6)?,
        worktree_id: row.get(7)?,
        worktree_path: row.get(8)?,
        status: row.get(9)?,
        created_at: row.get(10)?,
        updated_at: row.get(11)?,
    })
}

const SELECT_COLS: &str = "id, parent_conversation_id, title, prompt, agent, child_conversation_id, child_session_id, worktree_id, worktree_path, status, created_at, updated_at";

#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub fn create_subagent(
    parent_conversation_id: String,
    title: String,
    prompt: String,
    agent: String,
    child_conversation_id: String,
    child_session_id: String,
    worktree_id: String,
    worktree_path: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<SubAgent, String> {
    ensure_table(&db)?;
    let id = format!("sub_{}", chrono::Utc::now().timestamp_micros());
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO subagents (id, parent_conversation_id, title, prompt, agent, child_conversation_id, child_session_id, worktree_id, worktree_path, status)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'running')",
        params![id, parent_conversation_id, title, prompt, agent, child_conversation_id, child_session_id, worktree_id, worktree_path],
    )
    .map_err(|e| e.to_string())?;
    conn.query_row(
        &format!("SELECT {SELECT_COLS} FROM subagents WHERE id = ?1"),
        params![id],
        row_to_subagent,
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_subagents(
    parent_conversation_id: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<Vec<SubAgent>, String> {
    ensure_table(&db)?;
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(&format!(
            "SELECT {SELECT_COLS} FROM subagents WHERE parent_conversation_id = ?1 ORDER BY created_at DESC"
        ))
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![parent_conversation_id], row_to_subagent)
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn update_subagent_status(
    id: String,
    status: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    ensure_table(&db)?;
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE subagents SET status = ?2, updated_at = datetime('now') WHERE id = ?1",
        params![id, status],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn delete_subagent(id: String, db: State<'_, Arc<DbManager>>) -> Result<(), String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM subagents WHERE id = ?1", params![id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

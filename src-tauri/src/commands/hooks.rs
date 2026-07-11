//! User-state hooks (Claude Code hooks inspired): event → action rules that
//! fire on agent runtime events. A hook matches an event kind (e.g.
//! `tool_completed`, `turn_completed`, `error`) and an optional text matcher,
//! then runs one of three actions: a desktop **notify**, a shell **command**, or
//! a plain **log** entry. Evaluation runs inside the existing runtime-event
//! consumer loop (`lib.rs`).
//!
//! Lock discipline (CLAUDE.md 坑点 2): the engine loads matching hooks into a
//! `Vec` and drops the DB connection BEFORE running any action or spawning a
//! process — no `MutexGuard` is ever held across a spawn/await.

use crate::proc::NoWindow;
use std::process::Command;
use std::sync::Arc;

use rusqlite::params;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use crate::db::DbManager;
use crate::runtime::RuntimeEventKind;
use crate::runtime_manager::SessionEventEnvelope;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hook {
    pub id: String,
    pub name: String,
    /// Event kind to match: a runtime kind string, or `*` for any hookable event.
    pub event: String,
    /// Optional case-insensitive substring required in the event text.
    pub matcher: String,
    /// `notify` | `command` | `log`.
    pub action_type: String,
    /// Notify → body; command → shell command line; log → message.
    pub action_payload: String,
    pub enabled: bool,
    pub created_at: String,
    pub fire_count: i64,
    pub last_fired_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookRun {
    pub id: i64,
    pub hook_id: String,
    pub hook_name: String,
    pub session_id: String,
    pub event: String,
    pub fired_at: String,
    pub ok: bool,
    pub detail: String,
}

/// Event kinds a hook may fire on. Noisy streaming kinds (assistant_delta,
/// raw_log, user/assistant messages, plan) are intentionally excluded.
pub const HOOKABLE_KINDS: &[&str] = &[
    "session_started",
    "tool_started",
    "tool_completed",
    "approval_requested",
    "turn_completed",
    "error",
];

fn kind_str(kind: RuntimeEventKind) -> &'static str {
    match kind {
        RuntimeEventKind::SessionStarted => "session_started",
        RuntimeEventKind::UserMessage => "user_message",
        RuntimeEventKind::AssistantDelta => "assistant_delta",
        RuntimeEventKind::AssistantMessage => "assistant_message",
        RuntimeEventKind::Plan => "plan",
        RuntimeEventKind::ToolStarted => "tool_started",
        RuntimeEventKind::ToolCompleted => "tool_completed",
        RuntimeEventKind::ApprovalRequested => "approval_requested",
        RuntimeEventKind::TurnCompleted => "turn_completed",
        RuntimeEventKind::Error => "error",
        RuntimeEventKind::RawLog => "raw_log",
    }
}

fn ensure_tables(db: &DbManager) -> Result<(), String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS hooks (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL DEFAULT '',
            event TEXT NOT NULL DEFAULT '*',
            matcher TEXT NOT NULL DEFAULT '',
            action_type TEXT NOT NULL DEFAULT 'notify',
            action_payload TEXT NOT NULL DEFAULT '',
            enabled INTEGER NOT NULL DEFAULT 1,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            fire_count INTEGER NOT NULL DEFAULT 0,
            last_fired_at TEXT
        )",
        [],
    )
    .map_err(|e| e.to_string())?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS hook_runs (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            hook_id TEXT NOT NULL,
            hook_name TEXT NOT NULL DEFAULT '',
            session_id TEXT NOT NULL DEFAULT '',
            event TEXT NOT NULL DEFAULT '',
            fired_at TEXT NOT NULL DEFAULT (datetime('now')),
            ok INTEGER NOT NULL DEFAULT 1,
            detail TEXT NOT NULL DEFAULT ''
        )",
        [],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// Run a single hook's action. Pure side-effects; returns `(ok, detail)`.
/// `app` is optional so this is reusable from a manual test command.
fn run_action(
    app: Option<&AppHandle>,
    hook_name: &str,
    action_type: &str,
    payload: &str,
    session_id: &str,
    event: &str,
    text: &str,
) -> (bool, String) {
    match action_type {
        "notify" => {
            let body = if payload.is_empty() { format!("{event}: {text}") } else { payload.to_string() };
            if let Some(app) = app {
                let _ = app.emit(
                    "omnix-notification",
                    serde_json::json!({ "title": format!("Hook · {hook_name}"), "body": body }),
                );
            }
            (true, "已发送通知".into())
        }
        "command" => {
            if payload.trim().is_empty() {
                return (false, "命令为空".into());
            }
            // Fire-and-forget so a slow command never stalls the event loop.
            #[cfg(windows)]
            let mut command = {
                let mut c = Command::new("cmd");
                c.args(["/C", payload]);
                c
            };
            #[cfg(not(windows))]
            let mut command = {
                let mut c = Command::new("sh");
                c.args(["-c", payload]);
                c
            };
            command
                .no_window()
                .env("OMNIX_HOOK_NAME", hook_name)
                .env("OMNIX_SESSION_ID", session_id)
                .env("OMNIX_EVENT", event)
                .env("OMNIX_EVENT_TEXT", text);
            match command.spawn() {
                Ok(child) => (true, format!("已执行命令 (pid {})", child.id())),
                Err(error) => (false, format!("命令执行失败: {error}")),
            }
        }
        "log" => {
            let msg = if payload.is_empty() { format!("{event}: {text}") } else { payload.to_string() };
            (true, msg)
        }
        other => (false, format!("未知动作类型: {other}")),
    }
}

fn record_run(db: &DbManager, hook: &Hook, session_id: &str, event: &str, ok: bool, detail: &str) {
    let Ok(conn) = db.get_connection() else { return };
    let _ = conn.execute(
        "INSERT INTO hook_runs (hook_id, hook_name, session_id, event, ok, detail)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![hook.id, hook.name, session_id, event, ok as i32, detail],
    );
    let _ = conn.execute(
        "UPDATE hooks SET fire_count = fire_count + 1, last_fired_at = datetime('now') WHERE id = ?1",
        params![hook.id],
    );
    // Keep the run log bounded.
    let _ = conn.execute(
        "DELETE FROM hook_runs WHERE id NOT IN (SELECT id FROM hook_runs ORDER BY id DESC LIMIT 500)",
        [],
    );
}

/// Evaluate enabled hooks against one runtime event. Called from the runtime
/// event consumer loop. Cheap for the common case (filters noisy kinds first,
/// then a single indexed-ish query); never holds a DB guard across the action.
pub fn evaluate_hooks(db: &DbManager, app: &AppHandle, envelope: &SessionEventEnvelope) {
    let event = kind_str(envelope.event.kind);
    if !HOOKABLE_KINDS.contains(&event) {
        return;
    }
    if ensure_tables(db).is_err() {
        return;
    }
    let text = envelope.event.text.clone().unwrap_or_default();

    // Load matching enabled hooks into a Vec, then drop the connection.
    let matched: Vec<Hook> = {
        let Ok(conn) = db.get_connection() else { return };
        let Ok(mut stmt) = conn.prepare(
            "SELECT id, name, event, matcher, action_type, action_payload, enabled, created_at, fire_count, last_fired_at
             FROM hooks WHERE enabled = 1 AND (event = ?1 OR event = '*')",
        ) else { return };
        let rows = stmt.query_map(params![event], |row| {
            Ok(Hook {
                id: row.get(0)?,
                name: row.get(1)?,
                event: row.get(2)?,
                matcher: row.get(3)?,
                action_type: row.get(4)?,
                action_payload: row.get(5)?,
                enabled: row.get::<_, i32>(6)? != 0,
                created_at: row.get(7)?,
                fire_count: row.get(8)?,
                last_fired_at: row.get(9)?,
            })
        });
        match rows {
            Ok(rows) => rows
                .flatten()
                .filter(|h| h.matcher.trim().is_empty() || text.to_lowercase().contains(&h.matcher.to_lowercase()))
                .collect(),
            Err(_) => return,
        }
    };

    for hook in matched {
        let (ok, detail) = run_action(
            Some(app),
            &hook.name,
            &hook.action_type,
            &hook.action_payload,
            &envelope.session_id,
            event,
            &text,
        );
        record_run(db, &hook, &envelope.session_id, event, ok, &detail);
    }
}

// ── Tauri commands ─────────────────────────────────────────────────────────

#[tauri::command]
pub fn list_hooks(db: State<'_, Arc<DbManager>>) -> Result<Vec<Hook>, String> {
    ensure_tables(&db)?;
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT id, name, event, matcher, action_type, action_payload, enabled, created_at, fire_count, last_fired_at
             FROM hooks ORDER BY created_at DESC",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok(Hook {
                id: row.get(0)?,
                name: row.get(1)?,
                event: row.get(2)?,
                matcher: row.get(3)?,
                action_type: row.get(4)?,
                action_payload: row.get(5)?,
                enabled: row.get::<_, i32>(6)? != 0,
                created_at: row.get(7)?,
                fire_count: row.get(8)?,
                last_fired_at: row.get(9)?,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub fn save_hook(
    id: Option<String>,
    name: String,
    event: String,
    matcher: String,
    action_type: String,
    action_payload: String,
    enabled: bool,
    db: State<'_, Arc<DbManager>>,
) -> Result<Hook, String> {
    ensure_tables(&db)?;
    if name.trim().is_empty() {
        return Err("请填写 Hook 名称".into());
    }
    if !matches!(action_type.as_str(), "notify" | "command" | "log") {
        return Err("动作类型必须是 notify / command / log".into());
    }
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let id = id.unwrap_or_else(|| format!("hook_{}", chrono::Utc::now().timestamp_micros()));
    conn.execute(
        "INSERT INTO hooks (id, name, event, matcher, action_type, action_payload, enabled)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(id) DO UPDATE SET
            name = excluded.name, event = excluded.event, matcher = excluded.matcher,
            action_type = excluded.action_type, action_payload = excluded.action_payload,
            enabled = excluded.enabled",
        params![id, name.trim(), event, matcher, action_type, action_payload, enabled as i32],
    )
    .map_err(|e| e.to_string())?;
    conn.query_row(
        "SELECT id, name, event, matcher, action_type, action_payload, enabled, created_at, fire_count, last_fired_at FROM hooks WHERE id = ?1",
        params![id],
        |row| {
            Ok(Hook {
                id: row.get(0)?,
                name: row.get(1)?,
                event: row.get(2)?,
                matcher: row.get(3)?,
                action_type: row.get(4)?,
                action_payload: row.get(5)?,
                enabled: row.get::<_, i32>(6)? != 0,
                created_at: row.get(7)?,
                fire_count: row.get(8)?,
                last_fired_at: row.get(9)?,
            })
        },
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn toggle_hook(id: String, enabled: bool, db: State<'_, Arc<DbManager>>) -> Result<(), String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.execute("UPDATE hooks SET enabled = ?2 WHERE id = ?1", params![id, enabled as i32])
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn delete_hook(id: String, db: State<'_, Arc<DbManager>>) -> Result<(), String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM hooks WHERE id = ?1", params![id]).map_err(|e| e.to_string())?;
    Ok(())
}

/// Manually fire a hook's action once (for the UI "测试" button).
#[tauri::command]
pub fn test_hook(id: String, app: AppHandle, db: State<'_, Arc<DbManager>>) -> Result<String, String> {
    ensure_tables(&db)?;
    let hook = {
        let conn = db.get_connection().map_err(|e| e.to_string())?;
        conn.query_row(
            "SELECT id, name, event, matcher, action_type, action_payload, enabled, created_at, fire_count, last_fired_at FROM hooks WHERE id = ?1",
            params![id],
            |row| {
                Ok(Hook {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    event: row.get(2)?,
                    matcher: row.get(3)?,
                    action_type: row.get(4)?,
                    action_payload: row.get(5)?,
                    enabled: row.get::<_, i32>(6)? != 0,
                    created_at: row.get(7)?,
                    fire_count: row.get(8)?,
                    last_fired_at: row.get(9)?,
                })
            },
        )
        .map_err(|e| format!("找不到 Hook: {e}"))?
    };
    let (ok, detail) = run_action(
        Some(&app),
        &hook.name,
        &hook.action_type,
        &hook.action_payload,
        "test-session",
        "test",
        "（手动测试触发）",
    );
    record_run(&db, &hook, "test-session", "test", ok, &detail);
    if ok { Ok(detail) } else { Err(detail) }
}

#[tauri::command]
pub fn get_hook_runs(limit: Option<u32>, db: State<'_, Arc<DbManager>>) -> Result<Vec<HookRun>, String> {
    ensure_tables(&db)?;
    let limit = limit.unwrap_or(50).min(500);
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT id, hook_id, hook_name, session_id, event, fired_at, ok, detail
             FROM hook_runs ORDER BY id DESC LIMIT ?1",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![limit], |row| {
            Ok(HookRun {
                id: row.get(0)?,
                hook_id: row.get(1)?,
                hook_name: row.get(2)?,
                session_id: row.get(3)?,
                event: row.get(4)?,
                fired_at: row.get(5)?,
                ok: row.get::<_, i32>(6)? != 0,
                detail: row.get(7)?,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn clear_hook_runs(db: State<'_, Arc<DbManager>>) -> Result<(), String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM hook_runs", []).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matcher_and_log_action() {
        let db_path = std::env::temp_dir().join(format!("omnix_hooks_{}.sqlite", chrono::Utc::now().timestamp_micros()));
        let db = DbManager::new_runtime_test(db_path.clone());
        ensure_tables(&db).expect("tables");

        // A log hook that only matches events whose text contains "deploy".
        let conn = db.get_connection().unwrap();
        conn.execute(
            "INSERT INTO hooks (id, name, event, matcher, action_type, action_payload, enabled) VALUES ('h1','build','turn_completed','deploy','log','done',1)",
            [],
        ).unwrap();
        drop(conn);

        let (ok, _) = run_action(None, "build", "log", "done", "s1", "turn_completed", "running deploy step");
        assert!(ok);

        // Unknown action type fails cleanly.
        let (ok2, _) = run_action(None, "x", "bogus", "", "s1", "turn_completed", "");
        assert!(!ok2);

        drop(db);
        let _ = std::fs::remove_file(db_path);
    }
}

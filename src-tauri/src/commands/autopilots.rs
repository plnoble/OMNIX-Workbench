//! Autopilots — scheduled agent work.
//!
//! Unlike cron (which runs a headless CLI and logs to a file), an autopilot fires
//! by enqueuing a **reviewable conversation**: on schedule (or manual trigger) it
//! creates a conversation for the chosen agent/workspace and records a `queued`
//! run. The frontend claims queued runs and executes them through the real
//! runtime, so the result is a normal conversation the user can open and review.

use std::sync::Arc;

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::db::DbManager;
use crate::input_validation;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Autopilot {
    pub id: String,
    pub title: String,
    pub prompt: String,
    pub agent_name: String,
    pub workspace_path: String,
    pub schedule: String,
    pub permission: String,
    pub work_mode: String,
    pub enabled: bool,
    pub last_run: Option<String>,
    pub created_at: String,
}

/// A queued run claimed by the frontend, carrying everything needed to start a
/// runtime session and send the prompt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueuedAutopilotRun {
    pub run_id: String,
    pub autopilot_id: String,
    pub title: String,
    pub conversation_id: String,
    pub prompt: String,
    pub agent_name: String,
    pub workspace_path: String,
    pub permission: String,
    pub work_mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutopilotRunInfo {
    pub id: String,
    pub autopilot_id: String,
    pub conversation_id: String,
    pub status: String,
    pub trigger_source: String,
    pub created_at: String,
}

fn validate_permission(p: &str) -> Result<(), String> {
    match p {
        "ask_every_time" | "ask_on_risk" | "full_access" => Ok(()),
        other => Err(format!("未知权限策略：{other}")),
    }
}

fn validate_work_mode(m: &str) -> Result<(), String> {
    match m {
        "direct" | "plan" => Ok(()),
        other => Err(format!("未知工作模式：{other}")),
    }
}

fn read_autopilot(conn: &Connection, id: &str) -> Option<Autopilot> {
    conn.query_row(
        "SELECT id, title, prompt, agent_name, workspace_path, schedule, permission, work_mode, enabled, last_run, created_at
         FROM autopilots WHERE id = ?1",
        params![id],
        row_to_autopilot,
    )
    .ok()
}

fn row_to_autopilot(row: &rusqlite::Row) -> rusqlite::Result<Autopilot> {
    Ok(Autopilot {
        id: row.get(0)?,
        title: row.get(1)?,
        prompt: row.get(2)?,
        agent_name: row.get(3)?,
        workspace_path: row.get(4)?,
        schedule: row.get(5)?,
        permission: row.get(6)?,
        work_mode: row.get(7)?,
        enabled: row.get::<_, i64>(8)? != 0,
        last_run: row.get(9)?,
        created_at: row.get(10)?,
    })
}

/// Creates a conversation + a `queued` run for an autopilot, and stamps
/// `last_run`. Shared by the scheduler and manual "run now". Returns the new
/// conversation id.
pub fn fire_autopilot_run(db: &DbManager, autopilot_id: &str, trigger_source: &str) -> Result<String, String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let autopilot = read_autopilot(&conn, autopilot_id).ok_or("未找到 Autopilot")?;

    let now = chrono::Local::now();
    let conversation_id = format!("conv_ap_{}", now.timestamp_micros());
    let run_id = format!("aprun_{}", now.timestamp_micros());
    let title = format!("🛫 {} · {}", autopilot.title, now.format("%m-%d %H:%M"));

    conn.execute(
        "INSERT INTO conversations (id, title, workspace_path, active_agent) VALUES (?1, ?2, ?3, ?4)",
        params![conversation_id, title, autopilot.workspace_path, autopilot.agent_name],
    )
    .map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO autopilot_runs (id, autopilot_id, conversation_id, status, trigger_source)
         VALUES (?1, ?2, ?3, 'queued', ?4)",
        params![run_id, autopilot_id, conversation_id, trigger_source],
    )
    .map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE autopilots SET last_run = CURRENT_TIMESTAMP WHERE id = ?1",
        params![autopilot_id],
    )
    .map_err(|e| e.to_string())?;
    Ok(conversation_id)
}

#[tauri::command]
pub fn autopilot_list(db: State<'_, Arc<DbManager>>) -> Result<Vec<Autopilot>, String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT id, title, prompt, agent_name, workspace_path, schedule, permission, work_mode, enabled, last_run, created_at
             FROM autopilots ORDER BY created_at DESC",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], row_to_autopilot)
        .map_err(|e| e.to_string())?;
    Ok(rows.flatten().collect())
}

#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub fn autopilot_create(
    title: String,
    prompt: String,
    agent_name: String,
    workspace_path: String,
    schedule: String,
    permission: String,
    work_mode: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<Autopilot, String> {
    input_validation::validate_content(&title, "title")?;
    input_validation::validate_content(&prompt, "prompt")?;
    input_validation::validate_workspace_path(&workspace_path, "workspace_path")?;
    validate_permission(&permission)?;
    validate_work_mode(&work_mode)?;
    if schedule.trim().is_empty() {
        return Err("请填写触发计划".into());
    }
    let id = format!("ap_{}", chrono::Local::now().timestamp_micros());
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO autopilots (id, title, prompt, agent_name, workspace_path, schedule, permission, work_mode)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![id, title.trim(), prompt.trim(), agent_name, workspace_path, schedule.trim(), permission, work_mode],
    )
    .map_err(|e| e.to_string())?;
    read_autopilot(&conn, &id).ok_or_else(|| "读取 Autopilot 失败".into())
}

#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub fn autopilot_update(
    id: String,
    title: String,
    prompt: String,
    agent_name: String,
    workspace_path: String,
    schedule: String,
    permission: String,
    work_mode: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<Autopilot, String> {
    input_validation::validate_content(&title, "title")?;
    input_validation::validate_content(&prompt, "prompt")?;
    input_validation::validate_workspace_path(&workspace_path, "workspace_path")?;
    validate_permission(&permission)?;
    validate_work_mode(&work_mode)?;
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let changed = conn
        .execute(
            "UPDATE autopilots SET title = ?2, prompt = ?3, agent_name = ?4, workspace_path = ?5,
                schedule = ?6, permission = ?7, work_mode = ?8 WHERE id = ?1",
            params![id, title.trim(), prompt.trim(), agent_name, workspace_path, schedule.trim(), permission, work_mode],
        )
        .map_err(|e| e.to_string())?;
    if changed == 0 {
        return Err("未找到 Autopilot".into());
    }
    read_autopilot(&conn, &id).ok_or_else(|| "读取 Autopilot 失败".into())
}

#[tauri::command]
pub fn autopilot_set_enabled(id: String, enabled: bool, db: State<'_, Arc<DbManager>>) -> Result<(), String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let changed = conn
        .execute(
            "UPDATE autopilots SET enabled = ?2 WHERE id = ?1",
            params![id, if enabled { 1 } else { 0 }],
        )
        .map_err(|e| e.to_string())?;
    if changed == 0 {
        return Err("未找到 Autopilot".into());
    }
    Ok(())
}

#[tauri::command]
pub fn autopilot_delete(id: String, db: State<'_, Arc<DbManager>>) -> Result<(), String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM autopilot_runs WHERE autopilot_id = ?1", params![id])
        .map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM autopilots WHERE id = ?1", params![id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Manually fires an autopilot right now (trigger_source = 'manual').
#[tauri::command]
pub fn autopilot_run_now(id: String, db: State<'_, Arc<DbManager>>) -> Result<String, String> {
    fire_autopilot_run(&db, &id, "manual")
}

/// Atomically claims all `queued` runs (queued → claimed) and returns them with
/// the parent autopilot's config, so the frontend can execute each exactly once.
#[tauri::command]
pub fn autopilot_take_queued_runs(db: State<'_, Arc<DbManager>>) -> Result<Vec<QueuedAutopilotRun>, String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT r.id, r.autopilot_id, a.title, r.conversation_id, a.prompt, a.agent_name,
                    a.workspace_path, a.permission, a.work_mode
             FROM autopilot_runs r JOIN autopilots a ON a.id = r.autopilot_id
             WHERE r.status = 'queued' ORDER BY r.created_at ASC",
        )
        .map_err(|e| e.to_string())?;
    let runs: Vec<QueuedAutopilotRun> = stmt
        .query_map([], |row| {
            Ok(QueuedAutopilotRun {
                run_id: row.get(0)?,
                autopilot_id: row.get(1)?,
                title: row.get(2)?,
                conversation_id: row.get(3)?,
                prompt: row.get(4)?,
                agent_name: row.get(5)?,
                workspace_path: row.get(6)?,
                permission: row.get(7)?,
                work_mode: row.get(8)?,
            })
        })
        .map_err(|e| e.to_string())?
        .flatten()
        .collect();
    for run in &runs {
        let _ = conn.execute(
            "UPDATE autopilot_runs SET status = 'claimed' WHERE id = ?1",
            params![run.run_id],
        );
    }
    Ok(runs)
}

/// Marks a claimed run done / failed after the frontend executed it.
#[tauri::command]
pub fn autopilot_mark_run(run_id: String, status: String, db: State<'_, Arc<DbManager>>) -> Result<(), String> {
    let status = match status.as_str() {
        "done" | "failed" => status,
        other => return Err(format!("未知运行状态：{other}")),
    };
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE autopilot_runs SET status = ?2 WHERE id = ?1",
        params![run_id, status],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// Recent runs for an autopilot (history view).
#[tauri::command]
pub fn autopilot_list_runs(autopilot_id: String, db: State<'_, Arc<DbManager>>) -> Result<Vec<AutopilotRunInfo>, String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT id, autopilot_id, conversation_id, status, trigger_source, created_at
             FROM autopilot_runs WHERE autopilot_id = ?1 ORDER BY created_at DESC LIMIT 30",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![autopilot_id], |row| {
            Ok(AutopilotRunInfo {
                id: row.get(0)?,
                autopilot_id: row.get(1)?,
                conversation_id: row.get(2)?,
                status: row.get(3)?,
                trigger_source: row.get(4)?,
                created_at: row.get(5)?,
            })
        })
        .map_err(|e| e.to_string())?;
    Ok(rows.flatten().collect())
}

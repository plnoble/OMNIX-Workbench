use tauri::State;
use std::sync::Arc;
use rusqlite::params;
use crate::db::DbManager;
use crate::agent::run_cron_task;
use crate::input_validation;
use super::*;

#[tauri::command]
pub fn get_cron_tasks(
    db: State<'_, Arc<DbManager>>,
) -> Result<Vec<CronTask>, String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let mut stmt = conn.prepare(
        "SELECT id, title, schedule, agent_name, args, workspace_dir, is_active, last_run, created_at 
         FROM cron_tasks ORDER BY created_at DESC"
    ).map_err(|e| e.to_string())?;
    
    let rows = stmt.query_map([], |row| {
        let is_active_int: i32 = row.get(6)?;
        Ok(CronTask {
            id: row.get(0)?,
            title: row.get(1)?,
            schedule: row.get(2)?,
            agent_name: row.get(3)?,
            args: row.get(4)?,
            workspace_dir: row.get(5)?,
            is_active: is_active_int != 0,
            last_run: row.get(7)?,
            created_at: row.get(8)?,
        })
    }).map_err(|e| e.to_string())?;
    
    let mut result = Vec::new();
    for r in rows {
        if let Ok(task) = r {
            result.push(task);
        }
    }
    Ok(result)
}

#[tauri::command]
pub fn save_cron_task(
    id: String,
    title: String,
    schedule: String,
    agent_name: String,
    args: String,
    workspace_dir: String,
    is_active: bool,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    input_validation::validate_id(&id, "id")?;
    input_validation::validate_name(&agent_name, "agent_name")?;
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO cron_tasks (id, title, schedule, agent_name, args, workspace_dir, is_active)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(id) DO UPDATE SET
            title = excluded.title,
            schedule = excluded.schedule,
            agent_name = excluded.agent_name,
            args = excluded.args,
            workspace_dir = excluded.workspace_dir,
            is_active = excluded.is_active",
        params![id, title, schedule, agent_name, args, workspace_dir, if is_active { 1 } else { 0 }],
    ).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn toggle_cron_task_active(
    id: String,
    is_active: bool,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    input_validation::validate_id(&id, "id")?;
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE cron_tasks SET is_active = ?1 WHERE id = ?2",
        params![if is_active { 1 } else { 0 }, id],
    ).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn delete_cron_task(
    id: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    input_validation::validate_id(&id, "id")?;
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.execute(
        "DELETE FROM cron_tasks WHERE id = ?1",
        params![id],
    ).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn get_cron_runs(
    db: State<'_, Arc<DbManager>>,
) -> Result<Vec<CronRun>, String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let mut stmt = conn.prepare(
        "SELECT id, task_id, status, log_path, started_at, finished_at 
         FROM cron_runs ORDER BY started_at DESC LIMIT 50"
    ).map_err(|e| e.to_string())?;
    
    let rows = stmt.query_map([], |row| {
        Ok(CronRun {
            id: row.get(0)?,
            task_id: row.get(1)?,
            status: row.get(2)?,
            log_path: row.get(3)?,
            started_at: row.get(4)?,
            finished_at: row.get(5)?,
        })
    }).map_err(|e| e.to_string())?;
    
    let mut result = Vec::new();
    for r in rows {
        if let Ok(run) = r {
            result.push(run);
        }
    }
    Ok(result)
}

#[tauri::command]
pub fn clear_cron_runs(
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM cron_runs", []).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn trigger_cron_task(
    id: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let mut stmt = conn.prepare("SELECT id, agent_name, args, workspace_dir FROM cron_tasks WHERE id = ?1")
        .map_err(|e| e.to_string())?;
    let row = stmt.query_row(params![id], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
        ))
    }).map_err(|e| format!("Cron task not found: {}", e))?;
    
    let (task_id, agent_name, args_str, workspace_dir) = row;
    let db_arc = db.inner().clone();
    tauri::async_runtime::spawn(async move {
        let _ = run_cron_task(db_arc, task_id, agent_name, args_str, workspace_dir).await;
    });
    Ok(())
}

// ── MCP Servers ──────────────────────────────────────────

/// MCP Server DTO
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct McpServer {
    pub id: String,
    pub name: String,
    pub command: String,
    pub args: String,
    pub env: String,
    pub url: String,
    pub server_type: String,
    pub is_enabled: bool,
}

/// Get all configured MCP servers.
#[tauri::command]
pub fn get_mcp_servers(
    db: State<'_, Arc<DbManager>>,
) -> Result<Vec<McpServer>, String> {
    let rows = db.get_mcp_servers().map_err(|e| e.to_string())?;
    Ok(rows.into_iter().map(|(id, name, command, args, env, url, server_type, is_enabled)| {
        McpServer { id, name, command, args, env, url, server_type, is_enabled }
    }).collect())
}

/// Save (upsert) an MCP server configuration.
#[tauri::command]
pub fn save_mcp_server(
    server: McpServer,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    db.save_mcp_server(&server.id, &server.name, &server.command, &server.args, &server.env, &server.url, &server.server_type, server.is_enabled)
        .map_err(|e| e.to_string())
}

/// Delete an MCP server.
#[tauri::command]
pub fn delete_mcp_server(
    id: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    db.delete_mcp_server(&id).map_err(|e| e.to_string())
}

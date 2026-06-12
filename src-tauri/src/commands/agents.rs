use tauri::{AppHandle, Emitter, State};
use std::sync::Arc;
use tokio::sync::mpsc;
use crate::db::DbManager;
use crate::agent::{AgentManager, DetectedAgent};
use crate::input_validation;
use rusqlite::params;

#[derive(Debug, Clone, serde::Serialize)]
struct AgentOutputPayload {
    session_id: String,
    stream_type: String,
    text: String,
}

#[tauri::command]
pub fn detect_installed_agents(
    agent_manager: State<'_, Arc<AgentManager>>,
) -> Result<Vec<DetectedAgent>, String> {
    Ok(agent_manager.detect_agents())
}

#[tauri::command]
pub fn start_agent_session(
    app_handle: AppHandle,
    session_id: String,
    agent_name: String,
    exe_path: String,
    args: Vec<String>,
    workspace_dir: String,
    agent_manager: State<'_, Arc<AgentManager>>,
) -> Result<(), String> {
    input_validation::validate_id(&session_id, "session_id")?;
    input_validation::validate_name(&agent_name, "agent_name")?;
    input_validation::validate_workspace_path(&workspace_dir, "workspace_dir")?;
    let (stdout_tx, mut stdout_rx) = mpsc::channel::<String>(100);

    let _stdin_tx = agent_manager.spawn_agent(
        session_id.clone(),
        agent_name,
        exe_path,
        args,
        workspace_dir,
        stdout_tx,
    )?;

    let session_id_clone = session_id.clone();
    tauri::async_runtime::spawn(async move {
        while let Some(raw_output) = stdout_rx.recv().await {
            if raw_output.starts_with("ACP: ") {
                let acp_json = raw_output[5..].trim();
                if let Ok(parsed_json) = serde_json::from_str::<serde_json::Value>(acp_json) {
                    let _ = app_handle.emit("agent-task-update", serde_json::json!({
                        "session_id": session_id_clone.clone(),
                        "payload": parsed_json,
                    }));
                }
                continue;
            }

            let (stream_type, text) = if let Some(stderr_text) = raw_output.strip_prefix("STDERR: ") {
                ("stderr", stderr_text)
            } else if let Some(stdout_text) = raw_output.strip_prefix("STDOUT: ") {
                ("stdout", stdout_text)
            } else {
                ("stdout", raw_output.as_str())
            };

            let payload = AgentOutputPayload {
                session_id: session_id_clone.clone(),
                stream_type: stream_type.to_string(),
                text: text.to_string(),
            };

            println!("OMNIX Commands: Emitting agent-output to frontend -> stream_type={}, len={}", payload.stream_type, payload.text.len());
            let _ = app_handle.emit("agent-output", payload);
        }
    });

    Ok(())
}

#[tauri::command]
pub fn send_agent_stdin(
    session_id: String,
    input: String,
    agent_manager: State<'_, Arc<AgentManager>>,
) -> Result<(), String> {
    input_validation::validate_id(&session_id, "session_id")?;
    agent_manager.send_stdin(&session_id, input)?;
    Ok(())
}

#[tauri::command]
pub fn stop_agent_session(
    session_id: String,
    agent_manager: State<'_, Arc<AgentManager>>,
) -> Result<(), String> {
    input_validation::validate_id(&session_id, "session_id")?;
    agent_manager.terminate_agent(&session_id);
    Ok(())
}

#[tauri::command]
pub async fn install_agent_cli(
    agent_name: String,
    agent_manager: State<'_, Arc<AgentManager>>,
) -> Result<(), String> {
    input_validation::validate_name(&agent_name, "agent_name")?;
    agent_manager.install_agent(&agent_name).await
}

#[tauri::command]
pub async fn uninstall_agent_cli(
    agent_name: String,
    agent_manager: State<'_, Arc<AgentManager>>,
) -> Result<(), String> {
    input_validation::validate_name(&agent_name, "agent_name")?;
    agent_manager.uninstall_agent(&agent_name).await
}

#[tauri::command]
pub async fn repair_installed_agent(
    agent_name: String,
    agent_manager: State<'_, Arc<AgentManager>>,
) -> Result<(), String> {
    input_validation::validate_name(&agent_name, "agent_name")?;
    agent_manager.repair_agent_cli(&agent_name).await
}

#[tauri::command]
pub fn sync_external_agent_configs(
    agent_manager: State<'_, Arc<AgentManager>>,
) -> Result<(), String> {
    agent_manager.sync_agent_configs()
}

#[tauri::command]
pub fn get_active_sessions(
    agent_manager: State<'_, Arc<AgentManager>>,
) -> Result<Vec<String>, String> {
    Ok(agent_manager.get_active_session_ids())
}

#[tauri::command]
pub fn get_active_agent_model(
    agent_name: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<String, String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let model_res: Result<String, _> = conn.query_row(
        "SELECT target_model FROM agent_accounts WHERE agent_name = ?1 AND is_active = 1 LIMIT 1",
        params![agent_name],
        |row| row.get(0),
    );
    match model_res {
        Ok(m) => Ok(m),
        Err(_) => {
            let global = db.get_setting("target_model").unwrap_or(None).unwrap_or_else(|| "Auto".to_string());
            Ok(global)
        }
    }
}

#[tauri::command]
pub fn update_active_agent_model(
    agent_name: String,
    model: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let rows_affected = conn.execute(
        "UPDATE agent_accounts SET target_model = ?1 WHERE agent_name = ?2 AND is_active = 1",
        params![model, agent_name],
    ).map_err(|e| e.to_string())?;

    if rows_affected == 0 {
        let id = format!("{}_default", agent_name.to_lowercase().replace(' ', "_"));
        let name = format!("{} 默认账户", agent_name);

        let api_key = db.get_setting("api_key").unwrap_or(None).unwrap_or_default();
        let api_host = db.get_setting("api_host").unwrap_or(None).unwrap_or_else(|| "https://api.openai.com/v1".to_string());

        let _ = conn.execute(
            "INSERT INTO agent_accounts (id, account_name, api_key, api_host, target_model, agent_name, is_active)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1)",
            params![id, name, api_key, api_host, model, agent_name],
        );
    }
    Ok(())
}

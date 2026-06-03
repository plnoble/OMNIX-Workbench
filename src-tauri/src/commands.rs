use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};
use tokio::sync::mpsc;

use crate::db::DbManager;
use crate::agent::{AgentManager, DetectedAgent};

#[derive(Debug, Clone, Serialize)]
struct AgentOutputPayload {
    session_id: String,
    stream_type: String, // "stdout" or "stderr"
    text: String,
}

#[tauri::command]
pub fn get_app_setting(
    key: &str,
    db: State<'_, Arc<DbManager>>,
) -> Result<Option<String>, String> {
    db.get_setting(key).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_app_setting(
    key: &str,
    value: &str,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    db.set_setting(key, value).map_err(|e| e.to_string())
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
    exe_path: String,
    args: Vec<String>,
    workspace_dir: String,
    agent_manager: State<'_, Arc<AgentManager>>,
) -> Result<(), String> {
    let (stdout_tx, mut stdout_rx) = mpsc::channel::<String>(100);
    
    // Spawn agent and get stdin channel
    let stdin_tx = agent_manager.spawn_agent(
        session_id.clone(),
        exe_path,
        args,
        workspace_dir,
        stdout_tx,
    )?;

    // Spawn thread to route mpsc stdout/stderr to frontend via Tauri Event
    let session_id_clone = session_id.clone();
    tokio::spawn(async move {
        while let Some(raw_output) = stdout_rx.recv().await {
            let (stream_type, text) = if raw_output.starts_with("STDERR: ") {
                ("stderr", &raw_output[8..])
            } else {
                ("stdout", &raw_output[8..])
            };

            let payload = AgentOutputPayload {
                session_id: session_id_clone.clone(),
                stream_type: stream_type.to_string(),
                text: text.to_string(),
            };

            // Emit to frontend window using Tauri v2 Emitter
            let _ = app_handle.emit("agent-output", payload);
        }
    });

    // Store stdin channel in Tauri AppState so frontend can send input later
    // In a production application, we could store it in a Registry mutex.
    // For simplicity, we can let AgentManager own it. We've structured it so
    // that the stdin_tx is mapped inside the agent registry in AgentManager.
    
    Ok(())
}

#[tauri::command]
pub fn send_agent_stdin(
    session_id: String,
    input: String,
    agent_manager: State<'_, Arc<AgentManager>>,
) -> Result<(), String> {
    // Send standard input to the agent running session
    // We fetch the stdin_tx channel from the agent manager's active processes registry
    // and send the string.
    // We already have a thread waiting to feed stdin_rx into the actual child process writer.
    
    // In agent.rs, the active processes map stores `stdin_tx`!
    // Let's implement sending to it:
    
    // Note: We can implement sending inside AgentManager or directly fetch it.
    // Let's check how we can trigger it:
    // We'll write a helper method in AgentManager or fetch it directly.
    // In our agent.rs, the active processes Mutex is private. We should add a helper:
    // `pub fn send_stdin(&self, session_id: &str, text: String)`
    // Let's do that! Wait, we will implement this inside agent_manager.
    
    // Let's look at the active_processes access. Yes, we will call a method on AgentManager.
    
    struct SendHelper;
    // We can modify agent.rs to add `send_stdin` helper.
    // Let's check if we already added a send_stdin helper. In our agent.rs we didn't,
    // so we need to add a small replacement chunk to agent.rs later to support:
    // `pub fn send_stdin(&self, session_id: &str, text: String)`
    
    // Wait, let's write the commands.rs implementation first, calling a `send_stdin` method on agent_manager:
    agent_manager.send_stdin(&session_id, input)?;
    Ok(())
}

#[tauri::command]
pub fn stop_agent_session(
    session_id: String,
    agent_manager: State<'_, Arc<AgentManager>>,
) -> Result<(), String> {
    agent_manager.terminate_agent(&session_id);
    Ok(())
}

#[tauri::command]
pub async fn install_agent_cli(
    agent_name: String,
    agent_manager: State<'_, Arc<AgentManager>>,
) -> Result<(), String> {
    agent_manager.install_agent(&agent_name).await
}

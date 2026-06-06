use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::path::PathBuf;
use rusqlite::params;
use tauri::{AppHandle, Emitter, State};
use tokio::sync::mpsc;

use crate::db::DbManager;
use crate::agent::{AgentManager, DetectedAgent, run_cron_task};

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
    agent_name: String,
    exe_path: String,
    args: Vec<String>,
    workspace_dir: String,
    agent_manager: State<'_, Arc<AgentManager>>,
) -> Result<(), String> {
    let (stdout_tx, mut stdout_rx) = mpsc::channel::<String>(100);
    
    // Spawn agent and get stdin channel
    let _stdin_tx = agent_manager.spawn_agent(
        session_id.clone(),
        agent_name,
        exe_path,
        args,
        workspace_dir,
        stdout_tx,
    )?;

    // Spawn thread to route mpsc stdout/stderr to frontend via Tauri Event
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

            // Emit to frontend window using Tauri v2 Emitter
            println!("OMNIX Commands: Emitting agent-output to frontend -> stream_type={}, len={}", payload.stream_type, payload.text.len());
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

#[tauri::command]
pub async fn uninstall_agent_cli(
    agent_name: String,
    agent_manager: State<'_, Arc<AgentManager>>,
) -> Result<(), String> {
    agent_manager.uninstall_agent(&agent_name).await
}


#[tauri::command]
pub async fn repair_installed_agent(
    agent_name: String,
    agent_manager: State<'_, Arc<AgentManager>>,
) -> Result<(), String> {
    agent_manager.repair_agent_cli(&agent_name).await
}

#[tauri::command]
pub fn sync_external_agent_configs(
    agent_manager: State<'_, Arc<AgentManager>>,
) -> Result<(), String> {
    agent_manager.sync_agent_configs()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub file_path: String,
    pub profile: String,
    pub is_active: bool,
    pub dependencies: Vec<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillFusionResult {
    pub name: String,
    pub description: String,
    pub fused_code: String,
    pub explanation: String,
}

#[tauri::command]
pub fn get_all_skills(
    db: State<'_, Arc<DbManager>>,
) -> Result<Vec<Skill>, String> {
    let conn = db.get_connection().map_err(|e: rusqlite::Error| e.to_string())?;
    let mut stmt = conn.prepare("SELECT name, description, file_path, profile, is_active, dependencies, updated_at FROM skills")
        .map_err(|e: rusqlite::Error| e.to_string())?;
    
    let rows = stmt.query_map([], |row: &rusqlite::Row| {
        let name: String = row.get(0)?;
        let description: String = row.get(1)?;
        let file_path: String = row.get(2)?;
        let profile: String = row.get(3)?;
        let is_active_int: i32 = row.get(4)?;
        let dependencies_str: String = row.get(5)?;
        let updated_at: String = row.get(6)?;
        
        let dependencies: Vec<String> = serde_json::from_str(&dependencies_str).unwrap_or_default();
        
        Ok(Skill {
            name,
            description,
            file_path,
            profile,
            is_active: is_active_int != 0,
            dependencies,
            updated_at,
        })
    }).map_err(|e: rusqlite::Error| e.to_string())?;
    
    let mut result = Vec::new();
    for r in rows {
        if let Ok(skill) = r {
            result.push(skill);
        }
    }
    Ok(result)
}

#[tauri::command]
pub fn get_skill_content(
    name: String,
    profile: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<String, String> {
    let conn = db.get_connection().map_err(|e: rusqlite::Error| e.to_string())?;
    let mut stmt = conn.prepare("SELECT file_path FROM skills WHERE name = ?1")
        .map_err(|e: rusqlite::Error| e.to_string())?;
    let file_path_str: String = stmt.query_row(params![name], |r: &rusqlite::Row| r.get(0))
        .map_err(|e: rusqlite::Error| format!("Skill not found: {}", e))?;
    
    let mut path = PathBuf::from(&file_path_str);
    
    let suffix = match profile.to_lowercase().as_str() {
        "minimal" => "minimal",
        "comprehensive" => "comprehensive",
        _ => "core",
    };
    path.set_file_name(format!("{}_{}.md", name, suffix));
    
    if !path.exists() {
        return Err(format!("Profile file not found at: {}", path.to_string_lossy()));
    }
    
    std::fs::read_to_string(&path)
        .map_err(|e| format!("Failed to read skill content: {}", e))
}

#[tauri::command]
pub fn save_skill_content(
    name: String,
    profile: String,
    content: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    let conn = db.get_connection().map_err(|e: rusqlite::Error| e.to_string())?;
    let mut stmt = conn.prepare("SELECT file_path FROM skills WHERE name = ?1")
        .map_err(|e: rusqlite::Error| e.to_string())?;
    let file_path_str: String = stmt.query_row(params![name], |r: &rusqlite::Row| r.get(0))
        .map_err(|e: rusqlite::Error| format!("Skill not found: {}", e))?;
    
    let mut path = PathBuf::from(&file_path_str);
    
    let suffix = match profile.to_lowercase().as_str() {
        "minimal" => "minimal",
        "comprehensive" => "comprehensive",
        _ => "core",
    };
    path.set_file_name(format!("{}_{}.md", name, suffix));
    
    let mut tmp_path = path.clone();
    tmp_path.set_extension("tmp");
    
    std::fs::write(&tmp_path, &content)
        .map_err(|e| format!("Failed to write temporary file: {}", e))?;
    
    std::fs::rename(&tmp_path, &path)
        .map_err(|e| format!("Failed to atomically replace skill file: {}", e))?;
        
    conn.execute(
        "UPDATE skills SET updated_at = CURRENT_TIMESTAMP WHERE name = ?1",
        params![name],
    ).map_err(|e: rusqlite::Error| e.to_string())?;
    
    Ok(())
}

#[tauri::command]
pub fn toggle_skill_active(
    name: String,
    is_active: bool,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    let conn = db.get_connection().map_err(|e: rusqlite::Error| e.to_string())?;
    conn.execute(
        "UPDATE skills SET is_active = ?1, updated_at = CURRENT_TIMESTAMP WHERE name = ?2",
        params![if is_active { 1 } else { 0 }, name],
    ).map_err(|e: rusqlite::Error| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn update_skill_profile(
    name: String,
    profile: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    let conn = db.get_connection().map_err(|e: rusqlite::Error| e.to_string())?;
    conn.execute(
        "UPDATE skills SET profile = ?1, updated_at = CURRENT_TIMESTAMP WHERE name = ?2",
        params![profile, name],
    ).map_err(|e: rusqlite::Error| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn fuse_skills_api(
    skills: Vec<String>,
    db: State<'_, Arc<DbManager>>,
) -> Result<SkillFusionResult, String> {
    if skills.len() < 2 {
        return Err("Please select at least 2 skills to fuse.".to_string());
    }

    let api_key_raw = db.get_setting("api_key").unwrap_or(None).unwrap_or_default();
    let keys: Vec<&str> = api_key_raw.split(',').map(|k| k.trim()).filter(|k| !k.is_empty()).collect();
    if keys.is_empty() {
        return Err("API Key is not configured. Please set it in Settings.".to_string());
    }
    let api_key = keys[0];
    
    let api_host = db.get_setting("api_host").unwrap_or(None)
        .unwrap_or_else(|| "https://api.openai.com/v1".to_string());
    let target_model = db.get_setting("target_model").unwrap_or(None)
        .unwrap_or_else(|| "deepseek-chat".to_string());

    let mut skills_payload = String::new();
    for name in &skills {
        let file_content = {
            let conn = db.get_connection().map_err(|e: rusqlite::Error| e.to_string())?;
            let mut stmt = conn.prepare("SELECT file_path FROM skills WHERE name = ?1").map_err(|e: rusqlite::Error| e.to_string())?;
            let file_path_str: String = stmt.query_row(params![name], |r: &rusqlite::Row| r.get(0)).map_err(|_: rusqlite::Error| format!("Skill {} path not found", name))?;
            let mut path = PathBuf::from(file_path_str);
            path.set_file_name(format!("{}_core.md", name));
            std::fs::read_to_string(&path).map_err(|_| format!("Failed to read core profile of skill {}", name))?
        };
        
        skills_payload.push_str(&format!("\n=== SKILL: {} ===\n{}\n", name, file_content));
    }

    let system_prompt = "You are a Meta-Evolution Engine designed to analyze and fuse AI agent programming guidelines and skills. \
You must merge the input skills into a single consolidated, ultra-optimized super skill without conflicts. \
Your output must be returned strictly as a JSON object with the following schema:
{
  \"name\": \"Fused Skill Name\",
  \"description\": \"Brief description of the fused super skill\",
  \"fused_code\": \"The full markdown code representing the fused skill. Make sure it contains markdown sections: # Role & Identity, # Core Knowledge, # Step-by-Step Workflow, # Quality Checklist, # Anti-Patterns\",
  \"explanation\": \"A short description of how the skills were merged and what conflicts were resolved\"
}
DO NOT output any wrapping markdown blocks like ```json outside of the raw JSON content.";

    let user_prompt = format!(
        "Please fuse the following skills into a single unified skill asset:\n{}\n\nRemember to return a valid JSON object matching the requested schema.",
        skills_payload
    );

    let client = reqwest::Client::new();
    let upstream_url = format!("{}/chat/completions", api_host.trim_end_matches('/'));
    
    let response = client.post(&upstream_url)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "model": target_model,
            "messages": [
                {"role": "system", "content": system_prompt},
                {"role": "user", "content": user_prompt}
            ],
            "temperature": 0.3,
            "response_format": {"type": "json_object"}
        }))
        .send()
        .await
        .map_err(|e| format!("Failed to connect to LLM upstream: {}", e))?;

    let status = response.status();
    if !status.is_success() {
        let err_body = response.text().await.unwrap_or_default();
        return Err(format!("Upstream LLM returned error ({}): {}", status, err_body));
    }

    let res_json: serde_json::Value = response.json().await
        .map_err(|e| format!("Failed to parse JSON response from LLM: {}", e))?;

    let text_content = res_json["choices"][0]["message"]["content"].as_str()
        .ok_or_else(|| "Failed to retrieve content from LLM response".to_string())?;

    let clean_json_str = text_content
        .trim()
        .trim_start_matches("```json")
        .trim_end_matches("```")
        .trim();

    let result: SkillFusionResult = serde_json::from_str(clean_json_str)
        .map_err(|e| format!("LLM output did not match expected JSON schema: {}. Raw output: {}", e, clean_json_str))?;

    Ok(result)
}

#[tauri::command]
pub fn create_skill(
    name: String,
    description: String,
    profile: String,
    dependencies: Vec<String>,
    content: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    let conn = db.get_connection().map_err(|e: rusqlite::Error| e.to_string())?;
    
    let home_dir = dirs::home_dir().expect("Failed to determine home directory");
    let mut skills_dir = home_dir.clone();
    skills_dir.push(".omnix");
    skills_dir.push("skills");
    if !skills_dir.exists() {
        let _ = std::fs::create_dir_all(&skills_dir).map_err(|e| e.to_string())?;
    }
    
    let mut base_path = skills_dir.clone();
    base_path.push(&name);
    let base_path_str = base_path.to_string_lossy().to_string();
    
    // Write target profile files
    let mut core_path = base_path.clone();
    core_path.set_file_name(format!("{}_core.md", name));
    std::fs::write(&core_path, &content).map_err(|e| e.to_string())?;
    
    let mut min_path = base_path.clone();
    min_path.set_file_name(format!("{}_minimal.md", name));
    std::fs::write(&min_path, &content).map_err(|e| e.to_string())?;
    
    let mut comp_path = base_path.clone();
    comp_path.set_file_name(format!("{}_comprehensive.md", name));
    std::fs::write(&comp_path, &content).map_err(|e| e.to_string())?;
    
    let deps_str = serde_json::to_string(&dependencies).unwrap_or_else(|_| "[]".to_string());
    
    conn.execute(
        "INSERT OR REPLACE INTO skills (name, description, file_path, profile, is_active, dependencies)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![name, description, base_path_str, profile, 1, deps_str],
    ).map_err(|e: rusqlite::Error| e.to_string())?;
    
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentAccount {
    pub id: String,
    pub account_name: String,
    pub api_key: String,
    pub api_host: String,
    pub target_model: String,
    pub agent_name: String,
    pub is_active: bool,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Memory {
    pub id: String,
    pub incident_desc: String,
    pub code_pattern: String,
    pub remediation: String,
    pub keywords: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemorySuggestion {
    pub incident_desc: String,
    pub code_pattern: String,
    pub remediation: String,
    pub keywords: String,
}

#[tauri::command]
pub fn get_agent_accounts(
    db: State<'_, Arc<DbManager>>,
) -> Result<Vec<AgentAccount>, String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let mut stmt = conn.prepare("SELECT id, account_name, api_key, api_host, target_model, agent_name, is_active, updated_at FROM agent_accounts ORDER BY updated_at DESC")
        .map_err(|e| e.to_string())?;
    let rows = stmt.query_map([], |row| {
        let is_active_int: i32 = row.get(6)?;
        Ok(AgentAccount {
            id: row.get(0)?,
            account_name: row.get(1)?,
            api_key: row.get(2)?,
            api_host: row.get(3)?,
            target_model: row.get(4)?,
            agent_name: row.get(5)?,
            is_active: is_active_int != 0,
            updated_at: row.get(7)?,
        })
    }).map_err(|e| e.to_string())?;

    let mut result = Vec::new();
    for r in rows {
        if let Ok(mut acc) = r {
            // Mask API key for frontend display security
            if acc.api_key.len() > 8 {
                let last4 = &acc.api_key[acc.api_key.len()-4..];
                acc.api_key = format!("{}...{}", &acc.api_key[..4], last4);
            }
            result.push(acc);
        }
    }
    Ok(result)
}

#[tauri::command]
pub fn create_agent_account(
    id: String,
    account_name: String,
    api_key: String,
    api_host: String,
    target_model: String,
    agent_name: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT OR REPLACE INTO agent_accounts (id, account_name, api_key, api_host, target_model, agent_name, is_active, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, CURRENT_TIMESTAMP)",
        params![id, account_name, api_key, api_host, target_model, agent_name],
    ).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn switch_agent_account(
    id: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    // Find the agent_name for this account
    let agent_name: String = conn.query_row(
        "SELECT agent_name FROM agent_accounts WHERE id = ?1",
        params![id],
        |r| r.get(0),
    ).map_err(|e| format!("Account not found: {}", e))?;

    // Deactivate all accounts for this agent
    conn.execute(
        "UPDATE agent_accounts SET is_active = 0 WHERE agent_name = ?1",
        params![agent_name],
    ).map_err(|e| e.to_string())?;

    // Activate this account
    conn.execute(
        "UPDATE agent_accounts SET is_active = 1 WHERE id = ?1",
        params![id],
    ).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn delete_agent_account(
    id: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM agent_accounts WHERE id = ?1", params![id]).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn get_all_memories(
    db: State<'_, Arc<DbManager>>,
) -> Result<Vec<Memory>, String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let mut stmt = conn.prepare("SELECT id, incident_desc, code_pattern, remediation, keywords, created_at FROM memories ORDER BY created_at DESC")
        .map_err(|e| e.to_string())?;
    let rows = stmt.query_map([], |row| {
        Ok(Memory {
            id: row.get(0)?,
            incident_desc: row.get(1)?,
            code_pattern: row.get(2)?,
            remediation: row.get(3)?,
            keywords: row.get(4)?,
            created_at: row.get(5)?,
        })
    }).map_err(|e| e.to_string())?;

    let mut result = Vec::new();
    for r in rows {
        if let Ok(mem) = r {
            result.push(mem);
        }
    }
    Ok(result)
}

#[tauri::command]
pub fn create_memory(
    id: String,
    incident_desc: String,
    code_pattern: String,
    remediation: String,
    keywords: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT OR REPLACE INTO memories (id, incident_desc, code_pattern, remediation, keywords, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, CURRENT_TIMESTAMP)",
        params![id, incident_desc, code_pattern, remediation, keywords],
    ).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn delete_memory(
    id: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM memories WHERE id = ?1", params![id]).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn distill_session_memory(
    conversation_id: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<MemorySuggestion, String> {
    let conversation_log = {
        let conn = db.get_connection().map_err(|e| e.to_string())?;
        let mut stmt = conn.prepare("SELECT role, content FROM messages WHERE conversation_id = ?1 ORDER BY timestamp ASC")
            .map_err(|e| e.to_string())?;
        let rows = stmt.query_map(params![conversation_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        }).map_err(|e| e.to_string())?;

        let mut log = String::new();
        let mut count = 0;
        for r in rows {
            if let Ok((role, content)) = r {
                count += 1;
                log.push_str(&format!("{}: {}\n\n", role.to_uppercase(), content));
            }
        }

        if count == 0 {
            return Err("No messages found in this session to distill.".to_string());
        }
        log
    };

    // Fetch upstream LLM credentials
    let active_acc = db.get_active_account().map_err(|e| e.to_string())?;
    let (api_key_raw, api_host, target_model) = if let Some(acc) = active_acc {
        (acc.api_key, acc.api_host, acc.target_model)
    } else {
        let api_key = db.get_setting("api_key").unwrap_or(None).unwrap_or_default();
        let api_host = db.get_setting("api_host").unwrap_or(None).unwrap_or_else(|| "https://api.openai.com/v1".to_string());
        let target_model = db.get_setting("target_model").unwrap_or(None).unwrap_or_else(|| "deepseek-chat".to_string());
        (api_key, api_host, target_model)
    };

    let keys: Vec<&str> = api_key_raw.split(',').map(|k| k.trim()).filter(|k| !k.is_empty()).collect();
    if keys.is_empty() {
        return Err("API Key is not configured. Please configure credentials first.".to_string());
    }
    let api_key = keys[0];

    let system_prompt = "You are a Senior Engineering Lead and Code Experience Distillation Engine. \
Your task is to analyze the developer's chat session timeline (questions, code diffs, errors, and fixes) and distill a single key anti-failure lesson/pitfall. \
You must extract:
1. Incident Description: What went wrong or what mistake was made (e.g., deadlock, API key leak, git push force, CORS conflict, etc.).
2. Code Pattern: The specific risky code pattern or CLI command that triggered the incident.
3. Remediation: How to resolve and prevent this error next time.
4. Keywords: Comma-separated tag keywords (e.g., tokio,cors,git).

Return the response strictly as a JSON object matching this schema:
{
  \"incident_desc\": \"...\",
  \"code_pattern\": \"...\",
  \"remediation\": \"...\",
  \"keywords\": \"tag1,tag2,...\"
}
DO NOT wrap the response in markdown blocks like ```json. Return ONLY the raw JSON string.";

    let user_prompt = format!(
        "Please analyze the following conversation history and distill a valuable anti-failure lesson card:\n\n{}",
        conversation_log
    );

    let client = reqwest::Client::new();
    let upstream_url = format!("{}/chat/completions", api_host.trim_end_matches('/'));

    let response = client.post(&upstream_url)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "model": target_model,
            "messages": [
                {"role": "system", "content": system_prompt},
                {"role": "user", "content": user_prompt}
            ],
            "temperature": 0.3
        }))
        .send()
        .await
        .map_err(|e| format!("Failed to connect to LLM upstream: {}", e))?;

    let status = response.status();
    if !status.is_success() {
        let err_body = response.text().await.unwrap_or_default();
        return Err(format!("Upstream LLM returned error ({}): {}", status, err_body));
    }

    let res_json: serde_json::Value = response.json().await
        .map_err(|e| format!("Failed to parse JSON response from LLM: {}", e))?;

    let text_content = res_json["choices"][0]["message"]["content"].as_str()
        .ok_or_else(|| "Failed to retrieve content from LLM response".to_string())?;

    let clean_json_str = text_content
        .trim()
        .trim_start_matches("```json")
        .trim_end_matches("```")
        .trim();

    let result: MemorySuggestion = serde_json::from_str(clean_json_str)
        .map_err(|e| format!("LLM output did not match expected JSON schema: {}. Raw output: {}", e, clean_json_str))?;

    Ok(result)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationInfo {
    pub id: String,
    pub title: String,
    pub workspace_path: String,
    pub active_agent: String,
    pub created_at: String,
}

#[tauri::command]
pub fn get_all_conversations(
    db: State<'_, Arc<DbManager>>,
) -> Result<Vec<ConversationInfo>, String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let mut stmt = conn.prepare("SELECT id, title, workspace_path, active_agent, created_at FROM conversations ORDER BY created_at DESC")
        .map_err(|e| e.to_string())?;
    let rows = stmt.query_map([], |row| {
        Ok(ConversationInfo {
            id: row.get(0)?,
            title: row.get(1)?,
            workspace_path: row.get(2)?,
            active_agent: row.get(3)?,
            created_at: row.get(4)?,
        })
    }).map_err(|e| e.to_string())?;

    let mut result = Vec::new();
    for r in rows {
        if let Ok(conv) = r {
            result.push(conv);
        }
    }
    Ok(result)
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageInfo {
    pub id: String,
    pub conversation_id: String,
    pub role: String,
    pub content: String,
    pub timestamp: String,
}

#[tauri::command]
pub fn get_conversation_messages(
    conversation_id: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<Vec<MessageInfo>, String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let mut stmt = conn.prepare("SELECT id, conversation_id, role, content, timestamp FROM messages WHERE conversation_id = ?1 ORDER BY timestamp ASC")
        .map_err(|e| e.to_string())?;
    let rows = stmt.query_map(params![conversation_id], |row| {
        Ok(MessageInfo {
            id: row.get(0)?,
            conversation_id: row.get(1)?,
            role: row.get(2)?,
            content: row.get(3)?,
            timestamp: row.get(4)?,
        })
    }).map_err(|e| e.to_string())?;

    let mut result = Vec::new();
    for r in rows {
        if let Ok(msg) = r {
            result.push(msg);
        }
    }
    Ok(result)
}

#[tauri::command]
pub fn create_conversation(
    id: String,
    title: String,
    workspace_path: String,
    active_agent: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT OR REPLACE INTO conversations (id, title, workspace_path, active_agent) VALUES (?1, ?2, ?3, ?4)",
        params![id, title, workspace_path, active_agent],
    ).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn add_conversation_message(
    id: String,
    conversation_id: String,
    role: String,
    content: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT OR REPLACE INTO messages (id, conversation_id, role, content) VALUES (?1, ?2, ?3, ?4)",
        params![id, conversation_id, role, content],
    ).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn delete_conversation(
    conversation_id: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let _ = conn.execute("DELETE FROM messages WHERE conversation_id = ?1", params![conversation_id]);
    conn.execute(
        "DELETE FROM conversations WHERE id = ?1",
        params![conversation_id],
    ).map_err(|e| e.to_string())?;
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]

pub struct DbTask {
    pub id: String,
    pub conversation_id: String,
    pub title: String,
    pub status: String,
    pub order_num: i32,
    pub dependencies: Vec<String>,
}

#[tauri::command]
pub fn get_conversation_tasks(
    conversation_id: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<Vec<DbTask>, String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let mut stmt = conn.prepare("SELECT id, conversation_id, title, status, order_num, dependencies FROM tasks WHERE conversation_id = ?1 ORDER BY order_num ASC")
        .map_err(|e| e.to_string())?;
    let rows = stmt.query_map(params![conversation_id], |row| {
        let deps_str: String = row.get(5)?;
        let dependencies: Vec<String> = serde_json::from_str(&deps_str).unwrap_or_default();
        Ok(DbTask {
            id: row.get(0)?,
            conversation_id: row.get(1)?,
            title: row.get(2)?,
            status: row.get(3)?,
            order_num: row.get(4)?,
            dependencies,
        })
    }).map_err(|e| e.to_string())?;

    let mut result = Vec::new();
    for r in rows {
        if let Ok(t) = r {
            result.push(t);
        }
    }
    Ok(result)
}

#[tauri::command]
pub fn simulate_team_task_dispatch(
    conversation_id: String,
    leader: String,
    teammate: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;

    // Clear existing tasks
    conn.execute(
        "DELETE FROM tasks WHERE conversation_id = ?1",
        params![conversation_id],
    ).map_err(|e| e.to_string())?;

    // Seed 4 mock tasks
    let mock_tasks = vec![
        ("task_1", format!("[Leader: {}] 分析工作空间结构并确定研发目标", leader), "done", 0),
        ("task_2", format!("[Teammate: {}] 读取核心文件与 PlanTree 逻辑 (Mailbox 任务)", teammate), "done", 1),
        ("task_3", format!("[Teammate: {}] 实施新的 PlanTree 视图组件 (开发中)", teammate), "in_progress", 2),
        ("task_4", format!("[Leader: {}] 执行测试套件并完成验证", leader), "todo", 3),
    ];

    for (id, title, status, order_num) in mock_tasks {
        conn.execute(
            "INSERT INTO tasks (id, conversation_id, title, status, order_num, dependencies)
             VALUES (?1, ?2, ?3, ?4, ?5, '[]')",
            params![id, conversation_id, title, status, order_num],
        ).map_err(|e| e.to_string())?;
    }

    // Set up mailbox directory
    let home_dir = dirs::home_dir().unwrap_or_else(|| PathBuf::from("C:\\Users\\87953"));
    let mut mailbox_dir = home_dir.clone();
    mailbox_dir.push(".omnix");
    mailbox_dir.push("mailbox");
    let _ = std::fs::create_dir_all(&mailbox_dir);

    // Write a sample message envelope to the mailbox for demonstration
    let msg_file = mailbox_dir.join("task_3_dispatch.msg.json");
    let payload = serde_json::json!({
        "sender": leader,
        "receiver": teammate,
        "command": "implement_component",
        "params": {
            "component": "PlanTree.tsx",
            "workspace": "d:/Agent/Project/OMNIX-Development Tools"
        },
        "status": "in_progress",
        "timestamp": "2026-06-03T23:20:50Z"
    });

    std::fs::write(&msg_file, payload.to_string())
        .map_err(|e| format!("Failed to write mailbox simulation packet: {}", e))?;

    // Spawn an async background task to simulate progress stepping over time (from 2/4 -> 3/4 -> 4/4)
    let db_cloned = db.inner().clone();
    let conversation_id_cloned = conversation_id.clone();
    let leader_cloned = leader.clone();
    let teammate_cloned = teammate.clone();
    let mailbox_dir_cloned = mailbox_dir.clone();

    tauri::async_runtime::spawn(async move {
        // Step 1: Wait 2.5 seconds, complete task 3, and mark task 4 as in_progress
        tokio::time::sleep(tokio::time::Duration::from_millis(2500)).await;
        if let Ok(conn) = db_cloned.get_connection() {
            let _ = conn.execute(
                "UPDATE tasks SET status = 'done' WHERE id = 'task_3' AND conversation_id = ?1",
                params![conversation_id_cloned],
            );
            let _ = conn.execute(
                "UPDATE tasks SET status = 'in_progress' WHERE id = 'task_4' AND conversation_id = ?1",
                params![conversation_id_cloned],
            );
            
            // Dispatch a teammate response msg to mailbox
            let msg_file = mailbox_dir_cloned.join("task_3_completed.msg.json");
            let payload = serde_json::json!({
                "sender": teammate_cloned,
                "receiver": leader_cloned,
                "command": "component_completed",
                "params": {
                    "component": "PlanTree.tsx",
                    "status": "success"
                },
                "status": "done",
                "timestamp": "2026-06-03T23:21:10Z"
            });
            let _ = std::fs::write(&msg_file, payload.to_string());
        }

        // Step 2: Wait another 2.5 seconds, complete task 4
        tokio::time::sleep(tokio::time::Duration::from_millis(2500)).await;
        if let Ok(conn) = db_cloned.get_connection() {
            let _ = conn.execute(
                "UPDATE tasks SET status = 'done' WHERE id = 'task_4' AND conversation_id = ?1",
                params![conversation_id_cloned],
            );
            
            // Dispatch final completion packet
            let msg_file = mailbox_dir_cloned.join("all_done.msg.json");
            let payload = serde_json::json!({
                "sender": leader_cloned,
                "receiver": "system",
                "command": "integration_tests_completed",
                "params": {
                    "result": "all green"
                },
                "status": "done",
                "timestamp": "2026-06-03T23:21:40Z"
            });
            let _ = std::fs::write(&msg_file, payload.to_string());
        }
    });

    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MailboxMessage {
    pub filename: String,
    pub sender: String,
    pub receiver: String,
    pub command: String,
    pub params: serde_json::Value,
    pub status: String,
    pub timestamp: String,
}

#[tauri::command]
pub fn get_mailbox_messages() -> Result<Vec<MailboxMessage>, String> {
    let home_dir = dirs::home_dir().unwrap_or_else(|| PathBuf::from("C:\\Users\\87953"));
    let mut mailbox_dir = home_dir.clone();
    mailbox_dir.push(".omnix");
    mailbox_dir.push("mailbox");
    
    if !mailbox_dir.exists() {
        return Ok(Vec::new());
    }

    let mut msgs = Vec::new();
    if let Ok(entries) = std::fs::read_dir(mailbox_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && path.extension().map_or(false, |ext| ext == "json") {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if let Ok(msg) = serde_json::from_str::<serde_json::Value>(&content) {
                        let filename = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                        let sender = msg["sender"].as_str().unwrap_or("Unknown").to_string();
                        let receiver = msg["receiver"].as_str().unwrap_or("Unknown").to_string();
                        let command = msg["command"].as_str().unwrap_or("Unknown").to_string();
                        let params = msg["params"].clone();
                        let status = msg["status"].as_str().unwrap_or("pending").to_string();
                        let timestamp = msg["timestamp"].as_str().unwrap_or("").to_string();
                        
                        msgs.push(MailboxMessage {
                            filename,
                            sender,
                            receiver,
                            command,
                            params,
                            status,
                            timestamp,
                        });
                    }
                }
            }
        }
    }
    
    msgs.sort_by(|a, b| b.filename.cmp(&a.filename));
    Ok(msgs)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteAccessInfo {
    pub local_ip: String,
    pub port: u16,
    pub token: String,
    pub connection_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelMetadata {
    pub name: String,
    pub source: String, // "API" or "Local"
    pub has_vision: bool,
    pub has_audio: bool,
    pub has_reasoning: bool,
    pub has_coding: bool,
    pub has_long_context: bool,
    pub has_tool_use: bool,
    pub has_embedding: bool,
    pub has_speedy: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronTask {
    pub id: String,
    pub title: String,
    pub schedule: String,
    pub agent_name: String,
    pub args: String,
    pub workspace_dir: String,
    pub is_active: bool,
    pub last_run: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronRun {
    pub id: String,
    pub task_id: String,
    pub status: String,
    pub log_path: String,
    pub started_at: String,
    pub finished_at: Option<String>,
}

#[tauri::command]
pub fn get_remote_access_info(
    db: State<'_, Arc<DbManager>>,
) -> Result<RemoteAccessInfo, String> {
    let local_ip = get_local_ip().unwrap_or_else(|| "127.0.0.1".to_string());
    let port_str = db.get_setting("proxy_port").unwrap_or(None).unwrap_or_else(|| "1421".to_string());
    let port = port_str.parse::<u16>().unwrap_or(1421);
    let token = db.get_setting("remote_token").unwrap_or(None).unwrap_or_default();
    
    let connection_url = format!("http://{}:{}/remote?token={}", local_ip, port, token);
    
    Ok(RemoteAccessInfo {
        local_ip,
        port,
        token,
        connection_url,
    })
}

fn get_local_ip() -> Option<String> {
    use std::net::UdpSocket;
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    socket.local_addr().ok().map(|addr| addr.ip().to_string())
}

#[derive(Debug, Deserialize)]
struct OllamaResponse {
    models: Vec<OllamaModel>,
}

#[derive(Debug, Deserialize)]
struct OllamaModel {
    name: String,
}

#[tauri::command]
pub async fn get_all_models_metadata(
    db: State<'_, Arc<DbManager>>,
) -> Result<Vec<ModelMetadata>, String> {
    let mut list = Vec::new();

    // 1. Static API Catalog (name, vis, aud, reas, cod, long_ctx, tool, embed, speedy)
    let api_models = vec![
        // OpenAI
        ("gpt-4o", true, true, false, true, true, true, false, false),
        ("gpt-4o-mini", true, true, false, true, true, true, false, true),
        ("o1", true, false, true, true, true, true, false, false),
        ("o1-mini", false, false, true, true, true, true, false, false),
        ("o3-mini", false, false, true, true, true, true, false, true),
        // Anthropic
        ("claude-3-5-sonnet", true, false, false, true, true, true, false, false),
        ("claude-3-opus", true, false, false, true, true, true, false, false),
        ("claude-3-5-haiku", false, false, false, true, true, true, false, true),
        // DeepSeek
        ("deepseek-chat", false, false, false, true, true, true, false, true),
        ("deepseek-reasoner", false, false, true, true, true, true, false, false),
        // Gemini
        ("gemini-1.5-pro", true, true, false, true, true, true, false, false),
        ("gemini-1.5-flash", true, true, false, true, true, true, false, true),
        ("gemini-2.0-flash", true, true, false, true, true, true, false, true),
    ];

    for (name, vis, aud, reas, cod, long_ctx, tool, embed, speedy) in api_models {
        list.push(ModelMetadata {
            name: name.to_string(),
            source: "API".to_string(),
            has_vision: vis,
            has_audio: aud,
            has_reasoning: reas,
            has_coding: cod,
            has_long_context: long_ctx,
            has_tool_use: tool,
            has_embedding: embed,
            has_speedy: speedy,
        });
    }

    // 2. Local Ollama Probe
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(1500))
        .build()
        .map_err(|e| e.to_string())?;

    if let Ok(resp) = client.get("http://localhost:11434/api/tags").send().await {
        if resp.status().is_success() {
            if let Ok(ollama_resp) = resp.json::<OllamaResponse>().await {
                for m in ollama_resp.models {
                    let name_lower = m.name.to_lowercase();
                    
                    // Cap tagging heuristics
                    let has_reasoning = name_lower.contains("r1") 
                        || name_lower.contains("reasoning") 
                        || name_lower.contains("qwq") 
                        || name_lower.contains("thinking");
                    
                    let has_vision = name_lower.contains("vision") 
                        || name_lower.contains("llava") 
                        || name_lower.contains("minicpm") 
                        || name_lower.contains("bakllava") 
                        || name_lower.contains("moondream");
                    
                    let has_audio = name_lower.contains("audio") 
                        || name_lower.contains("whisper");
                    
                    // Most general-purpose models do coding
                    let has_coding = name_lower.contains("coder") 
                        || name_lower.contains("code") 
                        || name_lower.contains("llama") 
                        || name_lower.contains("qwen") 
                        || name_lower.contains("deepseek") 
                        || name_lower.contains("mistral") 
                        || name_lower.contains("phi") 
                        || name_lower.contains("gemma") 
                        || name_lower.contains("command-r") 
                        || name_lower.contains("starcoder") 
                        || name_lower.contains("stable-code");

                    let has_long_context = name_lower.contains("long") 
                        || name_lower.contains("128k") 
                        || name_lower.contains("32k") 
                        || name_lower.contains("64k") 
                        || name_lower.contains("yarn") 
                        || name_lower.contains("command-r")
                        || name_lower.contains("llama3");

                    let has_tool_use = name_lower.contains("llama3") 
                        || name_lower.contains("qwen") 
                        || name_lower.contains("mistral") 
                        || name_lower.contains("command-r")
                        || name_lower.contains("tool") 
                        || name_lower.contains("agent");

                    let has_embedding = name_lower.contains("embed") 
                        || name_lower.contains("nomic") 
                        || name_lower.contains("bge") 
                        || name_lower.contains("mxbai");

                    let has_speedy = name_lower.contains("1.5b") 
                        || name_lower.contains("3b") 
                        || name_lower.contains("8b") 
                        || name_lower.contains("mini") 
                        || name_lower.contains("haiku") 
                        || name_lower.contains("flash") 
                        || name_lower.contains("speed");

                    list.push(ModelMetadata {
                        name: m.name.clone(),
                        source: "Local".to_string(),
                        has_vision,
                        has_audio,
                        has_reasoning,
                        has_coding,
                        has_long_context,
                        has_tool_use,
                        has_embedding,
                        has_speedy,
                    });
                }
            }
        }
    }

    // 3. Load Custom Models from Database
    if let Ok(conn) = db.get_connection() {
        if let Ok(mut stmt) = conn.prepare(
            "SELECT name, source, has_vision, has_audio, has_reasoning,
                    has_coding, has_long_context, has_tool_use, has_embedding, has_speedy
             FROM custom_models"
        ) {
            let rows = stmt.query_map([], |row| {
                let has_vis: i32 = row.get(2)?;
                let has_aud: i32 = row.get(3)?;
                let has_reas: i32 = row.get(4)?;
                let has_cod: i32 = row.get(5)?;
                let has_long: i32 = row.get(6)?;
                let has_tool: i32 = row.get(7)?;
                let has_embed: i32 = row.get(8)?;
                let has_spd: i32 = row.get(9)?;
                Ok(ModelMetadata {
                    name: row.get(0)?,
                    source: row.get(1)?,
                    has_vision: has_vis != 0,
                    has_audio: has_aud != 0,
                    has_reasoning: has_reas != 0,
                    has_coding: has_cod != 0,
                    has_long_context: has_long != 0,
                    has_tool_use: has_tool != 0,
                    has_embedding: has_embed != 0,
                    has_speedy: has_spd != 0,
                })
            });
            if let Ok(rows) = rows {
                for r in rows.flatten() {
                    list.push(r);
                }
            }
        }
    }

    Ok(list)
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

pub fn get_active_sessions(
    agent_manager: State<'_, Arc<AgentManager>>,
) -> Result<Vec<String>, String> {
    Ok(agent_manager.get_active_session_ids())
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

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WindowLayout {
    pub label: String,
    pub url: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[tauri::command]
pub async fn set_compare_windows_layout(
    app: AppHandle,
    layout: Vec<WindowLayout>,
) -> Result<(), String> {
    use tauri::Manager;
    
    let main_win = app.get_webview_window("main")
        .ok_or_else(|| "Main window not found".to_string())?;
        
    let main_logical_pos = main_win.outer_position()
        .map(|p| p.to_logical::<f64>(main_win.scale_factor().unwrap_or(1.0)))
        .map_err(|e| e.to_string())?;

    for item in layout {
        let target_x = main_logical_pos.x + item.x;
        let target_y = main_logical_pos.y + item.y;
        
        if let Some(win) = app.get_webview_window(&item.label) {
            win.set_size(tauri::Size::Logical(tauri::LogicalSize::new(item.width, item.height))).ok();
            win.set_position(tauri::Position::Logical(tauri::LogicalPosition::new(target_x, target_y))).ok();
            win.show().ok();
        } else {
            let url_parsed = item.url.parse().map_err(|e| format!("Invalid URL: {}", e))?;
            
            let mut builder = tauri::WebviewWindowBuilder::new(&app, &item.label, tauri::WebviewUrl::External(url_parsed))
                .decorations(false)
                .skip_taskbar(true)
                .inner_size(item.width, item.height)
                .position(target_x, target_y);
                
            builder = builder.owner(&main_win).map_err(|e| e.to_string())?;
            
            let _win = builder.build()
                .map_err(|e| format!("Failed to create compare webview: {}", e))?;
        }
    }
    
    Ok(())
}

#[tauri::command]
pub async fn hide_compare_windows(app: AppHandle) -> Result<(), String> {
    use tauri::Manager;
    for (label, win) in app.webview_windows() {
        if label.starts_with("expert-") {
            win.hide().ok();
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn close_compare_windows(app: AppHandle) -> Result<(), String> {
    use tauri::Manager;
    for (label, win) in app.webview_windows() {
        if label.starts_with("expert-") {
            win.close().ok();
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn eval_compare_window(
    app: AppHandle,
    label: String,
    js: String,
) -> Result<(), String> {
    use tauri::Manager;
    if let Some(win) = app.get_webview_window(&label) {
        win.eval(&js).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn focus_main_window(app_handle: AppHandle) -> Result<(), String> {
    use tauri::Manager;
    let mut target_win = app_handle.get_webview_window("main");
    if target_win.is_none() {
        for (label, win) in app_handle.webview_windows() {
            if label != "status-dock" {
                target_win = Some(win);
                break;
            }
        }
    }
    if let Some(win) = target_win {
        let _ = win.show();
        let _ = win.unminimize();
        let _ = win.set_focus();
    }
    Ok(())
}

#[tauri::command]
pub fn toggle_status_dock(app_handle: AppHandle, visible: bool) -> Result<(), String> {
    use tauri::Manager;
    if let Some(dock) = app_handle.get_webview_window("status-dock") {
        if visible {
            let _ = dock.show();
            let _ = dock.set_focus();
        } else {
            let _ = dock.hide();
        }
    }
    Ok(())
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ModelPlatform {
    pub id: String,
    pub name: String,
    pub api_type: String,
    pub api_key: String,
    pub api_address: String,
    pub is_enabled: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PlatformModel {
    pub id: String,
    pub platform_id: String,
    pub model_name: String,
    pub has_vision: bool,
    pub has_audio: bool,
    pub has_reasoning: bool,
    pub has_coding: bool,
    pub is_enabled: bool,
    pub status: String,
}

#[tauri::command]
pub fn get_model_platforms(
    db: State<'_, Arc<DbManager>>,
) -> Result<Vec<ModelPlatform>, String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let mut stmt = conn.prepare("SELECT id, name, api_type, api_key, api_address, is_enabled FROM model_platforms").map_err(|e| e.to_string())?;
    let rows = stmt.query_map([], |row| {
        let is_enabled_int: i32 = row.get(5)?;
        Ok(ModelPlatform {
            id: row.get(0)?,
            name: row.get(1)?,
            api_type: row.get(2)?,
            api_key: row.get(3)?,
            api_address: row.get(4)?,
            is_enabled: is_enabled_int != 0,
        })
    }).map_err(|e| e.to_string())?;

    let mut result = Vec::new();
    for r in rows {
        if let Ok(p) = r {
            result.push(p);
        }
    }
    Ok(result)
}

#[tauri::command]
pub fn save_model_platform(
    db: State<'_, Arc<DbManager>>,
    platform: ModelPlatform,
) -> Result<(), String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO model_platforms (id, name, api_type, api_key, api_address, is_enabled)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(id) DO UPDATE SET
            name = excluded.name,
            api_type = excluded.api_type,
            api_key = excluded.api_key,
            api_address = excluded.api_address,
            is_enabled = excluded.is_enabled",
        params![
            platform.id,
            platform.name,
            platform.api_type,
            platform.api_key,
            platform.api_address,
            if platform.is_enabled { 1 } else { 0 }
        ],
    ).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn delete_model_platform(
    db: State<'_, Arc<DbManager>>,
    id: String,
) -> Result<(), String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM model_platforms WHERE id = ?1", params![id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn get_platform_models(
    db: State<'_, Arc<DbManager>>,
    platform_id: String,
) -> Result<Vec<PlatformModel>, String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let mut stmt = conn.prepare(
        "SELECT id, platform_id, model_name, has_vision, has_audio, has_reasoning, has_coding, is_enabled, status
         FROM platform_models WHERE platform_id = ?1"
    ).map_err(|e| e.to_string())?;
    let rows = stmt.query_map(params![platform_id], |row| {
        let has_vis: i32 = row.get(3)?;
        let has_aud: i32 = row.get(4)?;
        let has_reas: i32 = row.get(5)?;
        let has_cod: i32 = row.get(6)?;
        let is_enabled_int: i32 = row.get(7)?;
        Ok(PlatformModel {
            id: row.get(0)?,
            platform_id: row.get(1)?,
            model_name: row.get(2)?,
            has_vision: has_vis != 0,
            has_audio: has_aud != 0,
            has_reasoning: has_reas != 0,
            has_coding: has_cod != 0,
            is_enabled: is_enabled_int != 0,
            status: row.get(8)?,
        })
    }).map_err(|e| e.to_string())?;

    let mut result = Vec::new();
    for r in rows {
        if let Ok(m) = r {
            result.push(m);
        }
    }
    Ok(result)
}

#[tauri::command]
pub fn save_platform_model(
    db: State<'_, Arc<DbManager>>,
    model: PlatformModel,
) -> Result<(), String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO platform_models (id, platform_id, model_name, has_vision, has_audio, has_reasoning, has_coding, is_enabled, status)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
         ON CONFLICT(id) DO UPDATE SET
            has_vision = excluded.has_vision,
            has_audio = excluded.has_audio,
            has_reasoning = excluded.has_reasoning,
            has_coding = excluded.has_coding,
            is_enabled = excluded.is_enabled,
            status = excluded.status",
        params![
            model.id,
            model.platform_id,
            model.model_name,
            if model.has_vision { 1 } else { 0 },
            if model.has_audio { 1 } else { 0 },
            if model.has_reasoning { 1 } else { 0 },
            if model.has_coding { 1 } else { 0 },
            if model.is_enabled { 1 } else { 0 },
            model.status
        ],
    ).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn delete_platform_model(
    db: State<'_, Arc<DbManager>>,
    id: String,
) -> Result<(), String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM platform_models WHERE id = ?1", params![id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn get_active_models(
    db: State<'_, Arc<DbManager>>,
) -> Result<Vec<PlatformModel>, String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let mut stmt = conn.prepare(
        "SELECT pm.id, pm.platform_id, pm.model_name, pm.has_vision, pm.has_audio, pm.has_reasoning, pm.has_coding, pm.is_enabled, pm.status
         FROM platform_models pm
         JOIN model_platforms mp ON pm.platform_id = mp.id
         WHERE pm.is_enabled = 1 AND mp.is_enabled = 1"
    ).map_err(|e| e.to_string())?;
    let rows = stmt.query_map([], |row| {
        let has_vis: i32 = row.get(3)?;
        let has_aud: i32 = row.get(4)?;
        let has_reas: i32 = row.get(5)?;
        let has_cod: i32 = row.get(6)?;
        let is_enabled_int: i32 = row.get(7)?;
        Ok(PlatformModel {
            id: row.get(0)?,
            platform_id: row.get(1)?,
            model_name: row.get(2)?,
            has_vision: has_vis != 0,
            has_audio: has_aud != 0,
            has_reasoning: has_reas != 0,
            has_coding: has_cod != 0,
            is_enabled: is_enabled_int != 0,
            status: row.get(8)?,
        })
    }).map_err(|e| e.to_string())?;

    let mut result = Vec::new();
    for r in rows {
        if let Ok(m) = r {
            result.push(m);
        }
    }
    Ok(result)
}

fn join_url(base: &str, path: &str) -> String {
    let base_trimmed = base.trim_end_matches('/');
    let path_trimmed = path.trim_start_matches('/');
    format!("{}/{}", base_trimmed, path_trimmed)
}

#[tauri::command]
pub async fn fetch_remote_models(
    db: State<'_, Arc<DbManager>>,
    platform_id: String,
) -> Result<Vec<PlatformModel>, String> {
    let (api_type, api_key, api_address) = {
        let conn = db.get_connection().map_err(|e| e.to_string())?;
        let mut stmt = conn.prepare("SELECT api_type, api_key, api_address FROM model_platforms WHERE id = ?1")
            .map_err(|e| e.to_string())?;
        stmt.query_row(params![platform_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        }).map_err(|e| e.to_string())?
    };

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(6))
        .build()
        .map_err(|e| e.to_string())?;

    let mut model_names = Vec::new();

    if api_type == "ollama" {
        let url = join_url(&api_address, "/api/tags");
        if let Ok(resp) = client.get(&url).send().await {
            if resp.status().is_success() {
                #[derive(serde::Deserialize)]
                struct OllamaModel { name: String }
                #[derive(serde::Deserialize)]
                struct OllamaTags { models: Vec<OllamaModel> }
                if let Ok(tags) = resp.json::<OllamaTags>().await {
                    for m in tags.models {
                        model_names.push(m.name);
                    }
                }
            }
        }
    } else if api_type == "openai" {
        let url = join_url(&api_address, "/models");
        let mut req = client.get(&url);
        if !api_key.trim().is_empty() {
            req = req.header("Authorization", format!("Bearer {}", api_key.trim()));
        }
        if let Ok(resp) = req.send().await {
            if resp.status().is_success() {
                #[derive(serde::Deserialize)]
                struct OpenAIModel { id: String }
                #[derive(serde::Deserialize)]
                struct OpenAIModels { data: Vec<OpenAIModel> }
                if let Ok(models_list) = resp.json::<OpenAIModels>().await {
                    for m in models_list.data {
                        model_names.push(m.id);
                    }
                }
            }
        }
    } else if api_type == "anthropic" {
        let url = join_url(&api_address, "/v1/models");
        let mut fetched = false;
        let mut req = client.get(&url);
        if !api_key.trim().is_empty() {
            req = req.header("x-api-key", api_key.trim())
                     .header("anthropic-version", "2023-06-01");
        }
        if let Ok(resp) = req.send().await {
            if resp.status().is_success() {
                #[derive(serde::Deserialize)]
                struct AntModel { id: String }
                #[derive(serde::Deserialize)]
                struct AntModels { data: Vec<AntModel> }
                if let Ok(models_list) = resp.json::<AntModels>().await {
                    for m in models_list.data {
                        model_names.push(m.id);
                    }
                    fetched = true;
                }
            }
        }
        if !fetched {
            model_names.push("claude-3-5-sonnet-20241022".to_string());
            model_names.push("claude-3-5-haiku-20241022".to_string());
            model_names.push("claude-3-opus-20240229".to_string());
            model_names.push("claude-3-5-sonnet".to_string());
            model_names.push("claude-3-5-haiku".to_string());
        }
    }

    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let mut imported_models = Vec::new();

    for name in model_names {
        let id = format!("{}:{}", platform_id, name);
        let name_lower = name.to_lowercase();
        
        let has_vision = name_lower.contains("vision") || name_lower.contains("gpt-4o") || name_lower.contains("gemini") || name_lower.contains("claude-3-5-sonnet") || name_lower.contains("vl");
        let has_audio = name_lower.contains("audio") || name_lower.contains("whisper");
        let has_reasoning = name_lower.contains("r1") || name_lower.contains("reason") || name_lower.contains("o1") || name_lower.contains("o3") || name_lower.contains("qwq") || name_lower.contains("thinking");
        let has_coding = true;
        
        let pm = PlatformModel {
            id: id.clone(),
            platform_id: platform_id.clone(),
            model_name: name.clone(),
            has_vision,
            has_audio,
            has_reasoning,
            has_coding,
            is_enabled: true,
            status: "unknown".to_string(),
        };

        let _ = conn.execute(
            "INSERT INTO platform_models (id, platform_id, model_name, has_vision, has_audio, has_reasoning, has_coding, is_enabled, status)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(id) DO UPDATE SET
                model_name = excluded.model_name",
            params![
                id,
                platform_id,
                name,
                if has_vision { 1 } else { 0 },
                if has_audio { 1 } else { 0 },
                if has_reasoning { 1 } else { 0 },
                if has_coding { 1 } else { 0 },
                1,
                "unknown"
            ],
        );
        imported_models.push(pm);
    }

    Ok(imported_models)
}

#[tauri::command]
pub async fn check_model_status(
    db: State<'_, Arc<DbManager>>,
    model_id: String,
) -> Result<String, String> {
    let (platform_id, model_name, api_type, api_key, api_address) = {
        let conn = db.get_connection().map_err(|e| e.to_string())?;
        let mut stmt = conn.prepare(
            "SELECT pm.platform_id, pm.model_name, mp.api_type, mp.api_key, mp.api_address
             FROM platform_models pm
             JOIN model_platforms mp ON pm.platform_id = mp.id
             WHERE pm.id = ?1"
        ).map_err(|e| e.to_string())?;
        stmt.query_row(params![model_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        }).map_err(|e| e.to_string())?
    };

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|e| e.to_string())?;

    let mut is_success = false;

    if api_type == "ollama" {
        let url = join_url(&api_address, "/api/chat");
        let body = serde_json::json!({
            "model": model_name,
            "messages": [{"role": "user", "content": "ping"}],
            "stream": false
        });
        if let Ok(resp) = client.post(&url).json(&body).send().await {
            if resp.status().is_success() {
                is_success = true;
            }
        }
    } else if api_type == "openai" {
        let url = join_url(&api_address, "/chat/completions");
        let body = serde_json::json!({
            "model": model_name,
            "messages": [{"role": "user", "content": "ping"}],
            "max_tokens": 1
        });
        let mut req = client.post(&url);
        if !api_key.trim().is_empty() {
            req = req.header("Authorization", format!("Bearer {}", api_key.trim()));
        }
        if let Ok(resp) = req.json(&body).send().await {
            let code = resp.status();
            if code.is_success() || code.as_u16() == 400 || code.as_u16() == 401 || code.as_u16() == 429 {
                is_success = true;
            }
        }
    } else if api_type == "anthropic" {
        let url = join_url(&api_address, "/v1/messages");
        let body = serde_json::json!({
            "model": model_name,
            "messages": [{"role": "user", "content": "ping"}],
            "max_tokens": 1
        });
        let mut req = client.post(&url)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json");
        if !api_key.trim().is_empty() {
            req = req.header("x-api-key", api_key.trim());
        }
        if let Ok(resp) = req.json(&body).send().await {
            let code = resp.status();
            if code.is_success() || code.as_u16() == 400 || code.as_u16() == 401 || code.as_u16() == 429 {
                is_success = true;
            }
        }
    }

    let status_str = if is_success { "success" } else { "error" };
    
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let _ = conn.execute(
        "UPDATE platform_models SET status = ?1 WHERE id = ?2",
        params![status_str, model_id],
    );

    Ok(status_str.to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreviewFile {
    pub path: String,
    pub name: String,
    pub ext: String,
    pub modified: u64,
}

#[tauri::command]
pub fn get_previewable_files(workspace_path: String) -> Result<Vec<PreviewFile>, String> {
    use std::path::Path;
    use std::fs;

    let workspace = Path::new(&workspace_path);
    if !workspace.exists() || !workspace.is_dir() {
        return Err("Workspace directory does not exist".to_string());
    }

    let mut files = Vec::new();
    
    fn scan_dir(dir: &Path, depth: usize, files: &mut Vec<PreviewFile>) {
        if depth > 4 {
            return;
        }
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
                
                if name.starts_with('.') || name == "node_modules" || name == "target" || name == "dist" || name == "build" {
                    continue;
                }

                if path.is_dir() {
                    scan_dir(&path, depth + 1, files);
                } else if path.is_file() {
                    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                        let ext_lower = ext.to_lowercase();
                        if ext_lower == "html" || ext_lower == "md" || ext_lower == "png" || ext_lower == "jpg" || ext_lower == "jpeg" || ext_lower == "gif" || ext_lower == "svg" {
                            let modified = path.metadata()
                                .and_then(|m| m.modified())
                                .and_then(|t| t.duration_since(std::time::SystemTime::UNIX_EPOCH).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e)))
                                .map(|d| d.as_secs())
                                .unwrap_or(0);
                            
                            files.push(PreviewFile {
                                path: path.to_string_lossy().to_string(),
                                name,
                                ext: ext_lower,
                                modified,
                            });
                        }
                    }
                }
            }
        }
    }

    scan_dir(workspace, 0, &mut files);
    files.sort_by(|a, b| b.modified.cmp(&a.modified));
    if files.len() > 50 {
        files.truncate(50);
    }

    Ok(files)
}

#[tauri::command]
pub fn read_file_content_utf8(file_path: String) -> Result<String, String> {
    use std::path::Path;
    use std::fs;
    let path = Path::new(&file_path);
    if !path.exists() || !path.is_file() {
        return Err("File does not exist".to_string());
    }
    fs::read_to_string(path).map_err(|e| format!("Failed to read file: {}", e))
}

fn base64_encode(data: &[u8]) -> String {
    const CHARSET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::with_capacity((data.len() + 2) / 3 * 4);
    let mut i = 0;
    while i < data.len() {
        let chunk = &data[i..std::cmp::min(i + 3, data.len())];
        let val = match chunk.len() {
            3 => ((chunk[0] as u32) << 16) | ((chunk[1] as u32) << 8) | (chunk[2] as u32),
            2 => ((chunk[0] as u32) << 16) | ((chunk[1] as u32) << 8),
            1 => (chunk[0] as u32) << 16,
            _ => unreachable!(),
        };
        
        let enc1 = (val >> 18) & 63;
        let enc2 = (val >> 12) & 63;
        let enc3 = (val >> 6) & 63;
        let enc4 = val & 63;
        
        result.push(CHARSET[enc1 as usize] as char);
        result.push(CHARSET[enc2 as usize] as char);
        if chunk.len() >= 2 {
            result.push(CHARSET[enc3 as usize] as char);
        } else {
            result.push('=');
        }
        if chunk.len() == 3 {
            result.push(CHARSET[enc4 as usize] as char);
        } else {
            result.push('=');
        }
        i += 3;
    }
    result
}

#[tauri::command]
pub fn read_file_as_base64(file_path: String) -> Result<String, String> {
    use std::path::Path;
    use std::fs;
    let path = Path::new(&file_path);
    if !path.exists() || !path.is_file() {
        return Err("File does not exist".to_string());
    }
    let bytes = fs::read(path).map_err(|e| format!("Failed to read file: {}", e))?;
    Ok(base64_encode(&bytes))
}

#[tauri::command]
pub fn get_workspace_git_diff(workspace_path: String) -> Result<String, String> {
    use std::path::Path;
    let workspace = Path::new(&workspace_path);
    if !workspace.exists() || !workspace.is_dir() {
        return Err("Workspace directory does not exist".to_string());
    }
    
    let output = std::process::Command::new("git")
        .arg("diff")
        .current_dir(workspace)
        .output()
        .map_err(|e| format!("Failed to run git diff: {}", e))?;
        
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    
    if !output.status.success() {
        return Err(format!("git diff error: {}", stderr));
    }
    
    Ok(stdout)
}

#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct DiagnosticsResult {
    pub node_installed: bool,
    pub node_version: String,
    pub git_installed: bool,
    pub git_version: String,
    pub rg_installed: bool,
    pub rg_version: String,
    pub claude_installed: bool,
    pub opencode_installed: bool,
    pub codex_installed: bool,
    pub gemini_installed: bool,
}

#[tauri::command]
pub fn run_env_diagnostics() -> Result<DiagnosticsResult, String> {
    let node_check = std::process::Command::new("node").arg("-v").output();
    let (node_installed, node_version) = match node_check {
        Ok(out) => (true, String::from_utf8_lossy(&out.stdout).trim().to_string()),
        Err(_) => (false, "未安装".to_string()),
    };

    let git_check = std::process::Command::new("git").arg("--version").output();
    let (git_installed, git_version) = match git_check {
        Ok(out) => (true, String::from_utf8_lossy(&out.stdout).trim().to_string()),
        Err(_) => (false, "未安装".to_string()),
    };

    let rg_check = std::process::Command::new("rg").arg("--version").output();
    let (rg_installed, rg_version) = match rg_check {
        Ok(out) => {
            let full_out = String::from_utf8_lossy(&out.stdout);
            let first_line = full_out.lines().next().unwrap_or("rg").to_string();
            (true, first_line)
        }
        Err(_) => (false, "未安装".to_string()),
    };

    let claude_installed = which::which("claude").is_ok() || which::which("claude.cmd").is_ok();
    let opencode_installed = which::which("opencode").is_ok() || which::which("opencode.cmd").is_ok();
    let codex_installed = which::which("codex").is_ok() || which::which("codex.cmd").is_ok();
    let gemini_installed = which::which("gemini-cli").is_ok() || which::which("gemini-cli.cmd").is_ok();

    Ok(DiagnosticsResult {
        node_installed,
        node_version,
        git_installed,
        git_version,
        rg_installed,
        rg_version,
        claude_installed,
        opencode_installed,
        codex_installed,
        gemini_installed,
    })
}

#[tauri::command]
pub async fn repair_env_tool(app: tauri::AppHandle, tool_name: String) -> Result<(), String> {
    let (cmd, args) = match tool_name.as_str() {
        "claude" => ("npm", vec!["install", "-g", "@anthropic-ai/claude-code"]),
        "gemini" => ("npm", vec!["install", "-g", "@google/gemini-cli"]),
        "opencode" => ("npm", vec!["install", "-g", "opencode-cli"]),
        "ripgrep" => {
            #[cfg(target_os = "windows")]
            {
                ("powershell", vec!["-Command", "winget install BurntSushi.ripgrep --silent"])
            }
            #[cfg(not(target_os = "windows"))]
            {
                ("brew", vec!["install", "ripgrep"])
            }
        }
        _ => return Err(format!("Unsupported repair tool: {}", tool_name)),
    };

    let mut child = tokio::process::Command::new(cmd)
        .args(&args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to spawn repair command: {}", e))?;

    use tokio::io::AsyncBufReadExt;
    
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    
    let app_clone1 = app.clone();
    tokio::spawn(async move {
        let mut reader = tokio::io::BufReader::new(stdout).lines();
        while let Ok(Some(line)) = reader.next_line().await {
            let _ = app_clone1.emit("omnix-repair-log", format!("[STDOUT] {}", line));
        }
    });

    let app_clone2 = app.clone();
    tokio::spawn(async move {
        let mut reader = tokio::io::BufReader::new(stderr).lines();
        while let Ok(Some(line)) = reader.next_line().await {
            let _ = app_clone2.emit("omnix-repair-log", format!("[STDERR] {}", line));
        }
    });

    let status = child.wait().await.map_err(|e| format!("Wait failed: {}", e))?;
    if status.success() {
        let _ = app.emit("omnix-repair-log", format!("[SUCCESS] {} 修复安装成功！", tool_name));
        Ok(())
    } else {
        let _ = app.emit("omnix-repair-log", format!("[ERROR] {} 修复安装失败，退出码: {:?}", tool_name, status.code()));
        Err(format!("Command exited with status: {:?}", status))
    }
}

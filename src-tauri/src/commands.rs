use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::path::PathBuf;
use rusqlite::params;
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
    
    let home_dir = dirs::home_dir().unwrap_or_else(|| PathBuf::from("C:\\Users\\87953"));
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
    let mut stmt = conn.prepare("SELECT id, account_name, api_key, api_host, target_model, is_active, updated_at FROM agent_accounts ORDER BY updated_at DESC")
        .map_err(|e| e.to_string())?;
    let rows = stmt.query_map([], |row| {
        let is_active_int: i32 = row.get(5)?;
        Ok(AgentAccount {
            id: row.get(0)?,
            account_name: row.get(1)?,
            api_key: row.get(2)?,
            api_host: row.get(3)?,
            target_model: row.get(4)?,
            is_active: is_active_int != 0,
            updated_at: row.get(6)?,
        })
    }).map_err(|e| e.to_string())?;

    let mut result = Vec::new();
    for r in rows {
        if let Ok(acc) = r {
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
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT OR REPLACE INTO agent_accounts (id, account_name, api_key, api_host, target_model, is_active, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, 0, CURRENT_TIMESTAMP)",
        params![id, account_name, api_key, api_host, target_model],
    ).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn switch_agent_account(
    id: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.execute("UPDATE agent_accounts SET is_active = 0", []).map_err(|e| e.to_string())?;
    conn.execute("UPDATE agent_accounts SET is_active = 1 WHERE id = ?1", params![id]).map_err(|e| e.to_string())?;
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



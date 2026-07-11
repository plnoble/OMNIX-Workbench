use tauri::State;
use std::sync::Arc;
use std::path::PathBuf;
use rusqlite::params;
use crate::db::DbManager;
use crate::agent_templates::{AgentTemplate, get_all_templates};
use crate::proc::NoWindow;
use super::*;

// ══════════════════════════════════════════════════
// Agent Template Commands
// ══════════════════════════════════════════════════

/// Get all built-in agent templates
#[tauri::command]
pub fn get_agent_templates() -> Vec<AgentTemplate> {
    get_all_templates()
}

/// Get a specific template by slug
#[tauri::command]
pub fn get_agent_template(slug: String) -> Option<AgentTemplate> {
    get_all_templates().into_iter().find(|t| t.slug == slug)
}

// ══════════════════════════════════════════════════
// Skills Lock File
// ══════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillLockEntry {
    pub source: String,
    pub source_type: String,    // "github" | "local" | "package"
    pub computed_hash: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skill_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillLockFile {
    pub version: u32,
    pub skills: std::collections::HashMap<String, SkillLockEntry>,
}

fn lock_file_path() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_default();
    home.join(".omnix").join("skills-lock.json")
}

/// Read the current skills-lock.json
#[tauri::command]
pub fn get_skill_lock() -> SkillLockFile {
    let path = lock_file_path();
    if path.exists() {
        if let Ok(content) = std::fs::read_to_string(&path) {
            if let Ok(lock) = serde_json::from_str::<SkillLockFile>(&content) {
                return lock;
            }
        }
    }
    SkillLockFile { version: 1, skills: std::collections::HashMap::new() }
}

/// Write/update skills-lock.json from current DB state
#[tauri::command]
pub fn update_skill_lock(db: State<'_, Arc<DbManager>>) -> Result<SkillLockFile, String> {
    let conn = db.get_connection().map_err(|e: rusqlite::Error| e.to_string())?;
    let mut stmt = conn.prepare(
        "SELECT name, source_type, source_ref, central_path, content_hash FROM skills"
    ).map_err(|e: rusqlite::Error| e.to_string())?;

    let mut lock = SkillLockFile { version: 1, skills: std::collections::HashMap::new() };

    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<String>>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, Option<String>>(4)?,
        ))
    }).map_err(|e: rusqlite::Error| e.to_string())?;

    for r in rows.flatten() {
        let (name, source_type, source_ref, central_path, content_hash) = r;
        let entry = SkillLockEntry {
            source: source_ref.unwrap_or_default(),
            source_type,
            computed_hash: content_hash.unwrap_or_default(),
            skill_path: Some(central_path),
        };
        lock.skills.insert(name, entry);
    }

    // Write to file
    let json = serde_json::to_string_pretty(&lock).map_err(|e| e.to_string())?;
    std::fs::write(lock_file_path(), json).map_err(|e| e.to_string())?;

    Ok(lock)
}

/// Verify skills-lock.json against actual DB state
/// Checks both directions: locked skills vs DB, and DB skills vs lock
#[tauri::command]
pub fn verify_skill_lock(db: State<'_, Arc<DbManager>>) -> Result<Vec<String>, String> {
    let lock = get_skill_lock();
    let conn = db.get_connection().map_err(|e: rusqlite::Error| e.to_string())?;
    let mut issues = Vec::new();

    // Check 1: Every locked skill must exist in DB with matching hash
    for (name, entry) in &lock.skills {
        let db_row: Option<(String, bool)> = conn.query_row(
            "SELECT content_hash, is_active FROM skills WHERE name = ?1",
            params![name],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, bool>(1)?)),
        ).ok();

        match db_row {
            None => issues.push(format!("❌ {}: locked but not in DB (deleted without lock update)", name)),
            Some((hash, is_active)) => {
                if !is_active {
                    issues.push(format!("⚠️ {}: locked but deactivated in DB", name));
                }
                if entry.computed_hash.is_empty() {
                    issues.push(format!("⚠️ {}: lock entry has empty hash (lock created before hash computation)", name));
                } else if hash != entry.computed_hash {
                    issues.push(format!("❌ {}: hash mismatch (locked={}, actual={}) — content was modified after lock", name, entry.computed_hash, hash));
                }
            }
        }
    }

    // Check 2: Every active DB skill should be in the lock file
    let db_skills: Vec<String> = {
        let mut stmt = conn.prepare(
            "SELECT name FROM skills WHERE is_active = 1"
        ).map_err(|e: rusqlite::Error| e.to_string())?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0)).map_err(|e: rusqlite::Error| e.to_string())?;
        rows.flatten().collect()
    };

    for name in &db_skills {
        if !lock.skills.contains_key(name) {
            issues.push(format!("⚠️ {}: active in DB but not in lock file (added after last lock update)", name));
        }
    }

    Ok(issues)
}

// ══════════════════════════════════════════════════
// Agent Execution Environment Config
// ══════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentExecConfig {
    pub agent_name: String,
    pub model: Option<String>,
    pub max_turns: Option<u32>,
    pub system_prompt_append: Option<String>,
    pub extra_args: Vec<String>,
    pub workspace_dir: Option<String>,
    pub timeout_minutes: Option<u32>,
    pub sandbox_mode: Option<String>,  // "none" | "read-only" | "full"
}

/// Get execution config for an agent
#[tauri::command]
pub fn get_agent_exec_config(
    agent_name: String,
    db: State<'_, Arc<DbManager>>,
) -> AgentExecConfig {
    let conn = match db.get_connection() { Ok(c) => c, Err(_) => {
        return AgentExecConfig { agent_name, model: None, max_turns: None, system_prompt_append: None, extra_args: vec![], workspace_dir: None, timeout_minutes: None, sandbox_mode: None };
    }};

    let get_val = |key: &str| -> Option<String> {
        conn.query_row(
            "SELECT config_value FROM agent_configs WHERE agent_name = ?1 AND config_key = ?2",
            params![agent_name, key],
            |r| r.get(0),
        ).ok()
    };

    AgentExecConfig {
        agent_name: agent_name.clone(),
        model: get_val("model"),
        max_turns: get_val("max_turns").and_then(|v| v.parse().ok()),
        system_prompt_append: get_val("system_prompt_append"),
        extra_args: get_val("extra_args")
            .map(|v| v.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect())
            .unwrap_or_default(),
        workspace_dir: get_val("workspace_dir"),
        timeout_minutes: get_val("timeout_minutes").and_then(|v| v.parse().ok()),
        sandbox_mode: get_val("sandbox_mode"),
    }
}

/// Save execution config for an agent
#[tauri::command]
pub fn save_agent_exec_config(
    config: AgentExecConfig,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    let conn = db.get_connection().map_err(|e: rusqlite::Error| e.to_string())?;

    // Ensure table exists
    conn.execute(
        "CREATE TABLE IF NOT EXISTS agent_configs (
            agent_name TEXT NOT NULL,
            config_key TEXT NOT NULL,
            config_value TEXT NOT NULL,
            updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            PRIMARY KEY (agent_name, config_key)
        )", [],
    ).map_err(|e: rusqlite::Error| e.to_string())?;

    let set_val = |key: &str, val: &Option<String>| -> Result<(), String> {
        if let Some(v) = val {
            conn.execute(
                "INSERT OR REPLACE INTO agent_configs (agent_name, config_key, config_value) VALUES (?1, ?2, ?3)",
                params![config.agent_name, key, v],
            ).map_err(|e: rusqlite::Error| e.to_string())?;
        }
        Ok(())
    };

    set_val("model", &config.model)?;
    set_val("max_turns", &config.max_turns.map(|v| v.to_string()))?;
    set_val("system_prompt_append", &config.system_prompt_append)?;
    if !config.extra_args.is_empty() {
        set_val("extra_args", &Some(config.extra_args.join(", ")))?;
    }
    set_val("workspace_dir", &config.workspace_dir)?;
    set_val("timeout_minutes", &config.timeout_minutes.map(|v| v.to_string()))?;
    set_val("sandbox_mode", &config.sandbox_mode)?;

    Ok(())
}

// ══════════════════════════════════════════════════
// Autopilot Enhancement
// ══════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutopilotConfig {
    pub task_id: String,
    pub agent_name: Option<String>,
    pub prompt_template: Option<String>,
    pub trigger_type: String,       // "cron" | "webhook"
    pub webhook_secret: Option<String>,
    pub webhook_url: Option<String>,
}

/// Get autopilot config for a cron task
#[tauri::command]
pub fn get_autopilot_config(
    task_id: String,
    db: State<'_, Arc<DbManager>>,
) -> AutopilotConfig {
    let conn = match db.get_connection() { Ok(c) => c, Err(_) => {
        return AutopilotConfig { task_id, agent_name: None, prompt_template: None, trigger_type: "cron".into(), webhook_secret: None, webhook_url: None };
    }};

    let get_val = |key: &str| -> Option<String> {
        conn.query_row(
            "SELECT config_value FROM autopilot_configs WHERE task_id = ?1 AND config_key = ?2",
            params![task_id, key],
            |r| r.get(0),
        ).ok()
    };

    AutopilotConfig {
        task_id: task_id.clone(),
        agent_name: get_val("agent_name"),
        prompt_template: get_val("prompt_template"),
        trigger_type: get_val("trigger_type").unwrap_or_else(|| "cron".into()),
        webhook_secret: get_val("webhook_secret"),
        webhook_url: get_val("webhook_url"),
    }
}

/// Save autopilot config
#[tauri::command]
pub fn save_autopilot_config(
    config: AutopilotConfig,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    let conn = db.get_connection().map_err(|e: rusqlite::Error| e.to_string())?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS autopilot_configs (
            task_id TEXT NOT NULL,
            config_key TEXT NOT NULL,
            config_value TEXT NOT NULL,
            updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            PRIMARY KEY (task_id, config_key)
        )", [],
    ).map_err(|e: rusqlite::Error| e.to_string())?;

    let set_val = |key: &str, val: &Option<String>| -> Result<(), String> {
        if let Some(v) = val {
            conn.execute(
                "INSERT OR REPLACE INTO autopilot_configs (task_id, config_key, config_value) VALUES (?1, ?2, ?3)",
                params![config.task_id, key, v],
            ).map_err(|e: rusqlite::Error| e.to_string())?;
        }
        Ok(())
    };

    set_val("agent_name", &config.agent_name)?;
    set_val("prompt_template", &config.prompt_template)?;
    set_val("trigger_type", &Some(config.trigger_type.clone()))?;
    set_val("webhook_secret", &config.webhook_secret)?;

    // Generate webhook URL if trigger_type is webhook
    if config.trigger_type == "webhook" {
        let port = db.get_setting("proxy_port").ok().flatten().unwrap_or_else(|| "1421".into());
        let url = format!("http://127.0.0.1:{}/webhook/{}", port, config.task_id);
        conn.execute(
            "INSERT OR REPLACE INTO autopilot_configs (task_id, config_key, config_value) VALUES (?1, 'webhook_url', ?2)",
            params![config.task_id, url],
        ).map_err(|e: rusqlite::Error| e.to_string())?;
    }

    Ok(())
}

/// Process prompt template variables: {{date}}, {{git_status}}, {{workspace}}
#[allow(dead_code)]
pub fn expand_prompt_template(template: &str, workspace: Option<&str>) -> String {
    let now = chrono::Utc::now();
    let mut result = template.replace("{{date}}", &now.format("%Y-%m-%d %H:%M:%S").to_string());

    if let Some(ws) = workspace {
        result = result.replace("{{workspace}}", ws);

        // Try to get git status
        if let Ok(output) = std::process::Command::new("git")
            .arg("-C").arg(ws)
            .arg("status").arg("--short")
            .no_window()
            .output()
        {
            let status = String::from_utf8_lossy(&output.stdout);
            result = result.replace("{{git_status}}", status.trim());
        }
    }

    if !result.contains("{{git_status}}") {
        // No workspace or git not available
        result = result.replace("{{git_status}}", "(not in a git repository)");
    }

    result
}

// ══════════════════════════════════════════════════
// Autopilot Enhancement — Result to Knowledge Base
// ══════════════════════════════════════════════════

/// Save autopilot execution result to knowledge base
#[tauri::command]
pub fn save_autopilot_result_to_kb(
    task_id: String,
    result_content: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<String, String> {
    let home = dirs::home_dir().ok_or("Cannot determine home directory")?;
    let kb_dir = home.join(".omnix").join("knowledge").join("autopilot_results");
    std::fs::create_dir_all(&kb_dir).map_err(|e| e.to_string())?;

    let filename = format!("{}_{}.md", task_id, chrono::Utc::now().format("%Y%m%d_%H%M%S"));
    let file_path = kb_dir.join(&filename);

    let content_with_frontmatter = format!(
        "---\nname: autopilot-{}\ncategory: autopilot\nsource: {}\n---\n\n# Autopilot Result: {}\n\n{}\n",
        task_id, task_id, task_id, result_content
    );

    std::fs::write(&file_path, &content_with_frontmatter).map_err(|e| e.to_string())?;

    // Record in knowledge_documents if the table exists
    if let Ok(conn) = db.get_connection() {
        let _ = conn.execute(
            "INSERT OR IGNORE INTO knowledge_documents (id, name, file_path, status, created_at) VALUES (?1, ?2, ?3, 'completed', datetime('now'))",
            params![format!("autopilot-{}-{}", task_id, chrono::Utc::now().timestamp()), filename, file_path.to_string_lossy()],
        );
    }

    Ok(file_path.to_string_lossy().to_string())
}

// ══════════════════════════════════════════════════
// Workspace GC
// ══════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceGcConfig {
    pub enabled: bool,
    pub retention_days: u32,
    pub mode: String,  // "full" | "artifacts-only" | "orphan-only"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GcResult {
    pub scanned: usize,
    pub cleaned: usize,
    pub freed_bytes: u64,
    pub details: Vec<String>,
}

/// Get workspace GC config from a connection
fn get_gc_config_from_conn(conn: &rusqlite::Connection) -> WorkspaceGcConfig {
    let get = |key: &str, default: &str| -> String {
        conn.query_row("SELECT value FROM settings WHERE key = ?1", params![key], |r| r.get::<_, String>(0))
            .ok()
            .unwrap_or_else(|| default.into())
    };

    WorkspaceGcConfig {
        enabled: get("gc_enabled", "false") == "true",
        retention_days: get("gc_retention_days", "7").parse().unwrap_or(7),
        mode: get("gc_mode", "full"),
    }
}

/// Get workspace GC config
#[tauri::command]
pub fn get_gc_config(db: State<'_, Arc<DbManager>>) -> WorkspaceGcConfig {
    let conn = match db.get_connection() { Ok(c) => c, Err(_) => {
        return WorkspaceGcConfig { enabled: false, retention_days: 7, mode: "full".into() };
    }};
    get_gc_config_from_conn(&conn)
}

/// Save workspace GC config
#[tauri::command]
pub fn save_gc_config(config: WorkspaceGcConfig, db: State<'_, Arc<DbManager>>) -> Result<(), String> {
    let conn = db.get_connection().map_err(|e: rusqlite::Error| e.to_string())?;

    let set = |key: &str, val: &str| -> Result<(), String> {
        conn.execute(
            "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)",
            params![key, val],
        ).map_err(|e: rusqlite::Error| e.to_string())?;
        Ok(())
    };

    set("gc_enabled", if config.enabled { "true" } else { "false" })?;
    set("gc_retention_days", &config.retention_days.to_string())?;
    set("gc_mode", &config.mode)?;

    Ok(())
}

/// Execute workspace garbage collection
#[tauri::command]
pub fn run_workspace_gc(db: State<'_, Arc<DbManager>>) -> Result<GcResult, String> {
    let conn_check = db.get_connection().map_err(|e: rusqlite::Error| e.to_string())?;
    let config = get_gc_config_from_conn(&conn_check);

    if !config.enabled {
        return Err("Workspace GC is disabled. Enable it in Settings first.".into());
    }

    let conn = db.get_connection().map_err(|e: rusqlite::Error| e.to_string())?;
    let cutoff = chrono::Utc::now() - chrono::Duration::days(config.retention_days as i64);
    let cutoff_str = cutoff.format("%Y-%m-%d %H:%M:%S").to_string();

    // Find old conversations with workspace paths
    let mut stmt = conn.prepare(
        "SELECT id, title, updated_at FROM conversations WHERE updated_at < ?1 AND is_active = 0"
    ).map_err(|e: rusqlite::Error| e.to_string())?;

    let rows = stmt.query_map(params![cutoff_str], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
        ))
    }).map_err(|e: rusqlite::Error| e.to_string())?;

    let mut result = GcResult { scanned: 0, cleaned: 0, freed_bytes: 0, details: vec![] };

    for r in rows.flatten() {
        result.scanned += 1;
        let (id, title, updated_at) = r;

        match config.mode.as_str() {
            "full" => {
                // Mark conversation as archived
                let _ = conn.execute(
                    "UPDATE conversations SET is_active = 0 WHERE id = ?1",
                    params![id],
                );
                result.cleaned += 1;
                result.details.push(format!("Archived: {} (last: {})", title, updated_at));
            }
            "artifacts-only" => {
                // Just log — actual artifact cleanup would need workspace path
                result.details.push(format!("Would clean artifacts: {} (last: {})", title, updated_at));
            }
            _ => {
                result.details.push(format!("Skipped: {} (mode: {})", title, config.mode));
            }
        }
    }

    Ok(result)
}

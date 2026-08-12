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

/// 读取本机隐藏的内置助手 slug 列表。
fn hidden_slugs(db: &DbManager) -> Vec<String> {
    db.get_setting(crate::agent_templates::HIDDEN_TEMPLATES_KEY)
        .ok()
        .flatten()
        .and_then(|raw| serde_json::from_str::<Vec<String>>(&raw).ok())
        .unwrap_or_default()
}

/// 内置助手清单，已剔除用户在本机隐藏的那些。
#[tauri::command]
pub fn get_agent_templates(db: State<'_, Arc<DbManager>>) -> Vec<AgentTemplate> {
    let hidden = hidden_slugs(&db);
    get_all_templates()
        .into_iter()
        .filter(|t| !hidden.contains(&t.slug))
        .collect()
}

/// 在本机隐藏 / 恢复一个内置助手。
///
/// 内置助手是编译进二进制的，删不掉——用户删了一个，下次更新它又原样回来。
/// 隐藏名单存在本机 `settings` 表里，**不随版本走，也不会被更新覆盖**。
#[tauri::command]
pub fn set_builtin_assistant_hidden(
    db: State<'_, Arc<DbManager>>,
    slug: String,
    hidden: bool,
) -> Result<(), String> {
    let mut list = hidden_slugs(&db);
    if hidden {
        if !list.contains(&slug) {
            list.push(slug);
        }
    } else {
        list.retain(|s| s != &slug);
    }
    let encoded = serde_json::to_string(&list).map_err(|e| e.to_string())?;
    db.set_setting(crate::agent_templates::HIDDEN_TEMPLATES_KEY, &encoded)
        .map_err(|e| e.to_string())
}

/// 本机隐藏了哪些内置助手（供「恢复」入口显示）。
#[tauri::command]
pub fn list_hidden_builtin_assistants(db: State<'_, Arc<DbManager>>) -> Vec<AgentTemplate> {
    let hidden = hidden_slugs(&db);
    get_all_templates()
        .into_iter()
        .filter(|t| hidden.contains(&t.slug))
        .collect()
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

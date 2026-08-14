use tauri::State;
use std::sync::Arc;
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

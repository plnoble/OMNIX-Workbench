use tauri::State;
use std::sync::Arc;
use crate::db::DbManager;
use crate::agent_templates::{AgentTemplate, get_all_templates};
use crate::proc::NoWindow;

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


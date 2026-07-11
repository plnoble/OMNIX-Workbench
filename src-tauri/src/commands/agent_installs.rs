//! Agent installation management (R3 统一安装).
//!
//! Agents get installed all over the place (npm global, standalone installers,
//! OMNIX's own managed root). This scans EVERY copy per agent, shows which one
//! OMNIX actually uses, lets the user delete redundant copies, and (via the
//! existing `install_agent_cli`) reinstall into the managed root — which the
//! user can point at D:\ through 设置 → 存储位置 → Agent 安装目录.
//!
//! Note: OMNIX's resolver prefers a PATH-visible CLI over the managed copy
//! (a user-managed system CLI is authoritative). So "统一到托管目录" =
//! install managed copy + delete the PATH copies — exactly this panel's flow.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::agent::AgentManager;
use crate::db::DbManager;
use crate::proc::NoWindow;

/// (display name, bin name, npm package for global uninstall)
const AGENTS: &[(&str, &str, &str)] = &[
    ("Claude Code", "claude", "@anthropic-ai/claude-code"),
    ("Codex", "codex", "@openai/codex"),
    ("Gemini CLI", "gemini", "@google/gemini-cli"),
    ("Qwen Code", "qwen", "@qwen-code/qwen-code"),
    ("OpenCode", "opencode", "opencode-ai"),
    ("GitHub Copilot CLI", "copilot", "@github/copilot-cli"),
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentInstallation {
    pub path: String,
    pub version: String,
    /// "managed" (OMNIX 托管目录) | "npm_global" | "other"
    pub kind: String,
    /// The copy OMNIX currently resolves to for spawning.
    pub is_active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentInstallGroup {
    pub agent: String,
    pub managed_root: String,
    pub installations: Vec<AgentInstallation>,
}

fn norm(p: &Path) -> String {
    let s = std::fs::canonicalize(p)
        .unwrap_or_else(|_| p.to_path_buf())
        .to_string_lossy()
        .to_string();
    if cfg!(windows) {
        s.trim_start_matches(r"\\?\").to_lowercase()
    } else {
        s
    }
}

fn probe_version(exe: &str) -> String {
    std::process::Command::new(exe)
        .arg("--version")
        .no_window()
        .output()
        .ok()
        .map(|o| {
            let out = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if out.is_empty() {
                String::from_utf8_lossy(&o.stderr).trim().to_string()
            } else {
                out
            }
        })
        .map(|v| v.lines().next().unwrap_or_default().chars().take(60).collect())
        .unwrap_or_default()
}

fn npm_global_prefix() -> Option<String> {
    let npm = if cfg!(windows) { "npm.cmd" } else { "npm" };
    std::process::Command::new(npm)
        .args(["config", "get", "prefix"])
        .no_window()
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Scan every discoverable installation of every known agent.
#[tauri::command]
pub fn scan_agent_installations(
    db: State<'_, Arc<DbManager>>,
) -> Result<Vec<AgentInstallGroup>, String> {
    let npm_prefix = npm_global_prefix().map(|p| norm(Path::new(&p)));
    let mut out = Vec::new();

    for (display, bin, _pkg) in AGENTS {
        let managed_root = crate::agent::managed_agent_root(&db, display);
        let managed_root_norm = norm(&managed_root);
        let active = AgentManager::find_agent_path_static(display, Some(&db))
            .map(|p| norm(Path::new(&p)));

        let mut seen = std::collections::HashSet::new();
        let mut installations = Vec::new();
        let mut push = |path: PathBuf| {
            let key = norm(&path);
            if !seen.insert(key.clone()) {
                return;
            }
            let kind = if key.starts_with(&managed_root_norm) {
                "managed"
            } else if npm_prefix.as_ref().map(|p| key.starts_with(p.as_str())).unwrap_or(false) {
                "npm_global"
            } else {
                "other"
            };
            let path_str = path.to_string_lossy().to_string();
            installations.push(AgentInstallation {
                version: probe_version(&path_str),
                is_active: active.as_deref() == Some(key.as_str()),
                path: path_str,
                kind: kind.to_string(),
            });
        };

        // Every PATH hit (not just the first).
        if let Ok(all) = which::which_all(bin) {
            for p in all {
                push(p);
            }
        }
        // The managed copy (may not be on PATH).
        if let Some(p) = crate::agent::executable_in_managed_root(&managed_root, bin) {
            push(PathBuf::from(p));
        }

        out.push(AgentInstallGroup {
            agent: (*display).to_string(),
            managed_root: managed_root.to_string_lossy().to_string(),
            installations,
        });
    }
    Ok(out)
}

/// Delete one redundant installation. Only kinds we can remove safely:
/// - `managed`  → OMNIX's own copy (existing uninstall flow)
/// - `npm_global` → `npm uninstall -g <package>`
/// Anything else (standalone installers, scoop, …) is refused with guidance.
#[tauri::command]
pub async fn remove_agent_installation(
    agent: String,
    kind: String,
    agent_manager: State<'_, Arc<AgentManager>>,
) -> Result<(), String> {
    let (_, _, pkg) = AGENTS
        .iter()
        .find(|(d, _, _)| *d == agent)
        .ok_or_else(|| format!("未知 agent: {agent}"))?;

    match kind.as_str() {
        "managed" => agent_manager.uninstall_agent(&agent).await,
        "npm_global" => {
            let npm = if cfg!(windows) { "npm.cmd" } else { "npm" };
            let mut cmd = tokio::process::Command::new(npm);
            cmd.args(["uninstall", "-g", pkg]).no_window();
            let output = cmd
                .output()
                .await
                .map_err(|e| format!("npm 卸载启动失败: {e}"))?;
            if output.status.success() {
                Ok(())
            } else {
                Err(format!(
                    "npm 卸载失败: {}",
                    String::from_utf8_lossy(&output.stderr).trim()
                ))
            }
        }
        _ => Err(
            "这个安装不是 npm 全局包也不是 OMNIX 托管副本（可能来自独立安装器/scoop 等），\
             为安全起见请手动卸载后重新扫描"
                .to_string(),
        ),
    }
}

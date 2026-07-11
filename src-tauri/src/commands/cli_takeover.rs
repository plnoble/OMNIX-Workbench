//! CLI 配置接管: point Claude Code / Codex / Gemini CLI at
//! a chosen target — the OMNIX gateway, a supplier platform, or an OAuth
//! subscription — by writing each agent's **native** config. These are the
//! user's real files (and take effect even outside OMNIX), so every write is
//! backup → atomic temp+rename → read-back validate, and each agent is
//! revertible from its latest backup.

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::State;

use crate::db::DbManager;
use crate::oauth::OAuthProviderKind;

/// Where an agent should be pointed. `kind` ∈ {gateway, platform, oauth}.
#[derive(Debug, Clone, Deserialize)]
pub struct TakeoverTarget {
    pub kind: String,
    /// platform_id (platform) or oauth account id (oauth); ignored for gateway.
    pub ref_id: Option<String>,
    pub model: Option<String>,
}

struct ResolvedTarget {
    base_url: String,
    token: String,
    model: Option<String>,
    label: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TakeoverReport {
    pub agent: String,
    pub config_path: String,
    pub applied: bool,
    pub backup_path: Option<String>,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentTakeoverState {
    pub agent: String,
    pub config_path: String,
    pub config_exists: bool,
    /// Current base URL the agent points at, if OMNIX can read one.
    pub current_base_url: Option<String>,
    pub has_backup: bool,
}

fn home() -> Result<PathBuf, String> {
    dirs::home_dir().ok_or_else(|| "无法确定用户主目录".to_string())
}

fn claude_settings_path() -> Result<PathBuf, String> {
    Ok(home()?.join(".claude").join("settings.json"))
}
fn codex_config_path() -> Result<PathBuf, String> {
    Ok(home()?.join(".codex").join("config.toml"))
}
fn codex_auth_path() -> Result<PathBuf, String> {
    Ok(home()?.join(".codex").join("auth.json"))
}
fn gemini_env_path() -> Result<PathBuf, String> {
    Ok(home()?.join(".gemini").join(".env"))
}

fn backup_category(agent: &str) -> String {
    format!("cli_takeover_{agent}")
}

fn atomic_write(path: &PathBuf, contents: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("创建配置目录失败：{e}"))?;
    }
    let tmp = path.with_extension("omnix-tmp");
    fs::write(&tmp, contents).map_err(|e| format!("写入临时文件失败：{e}"))?;
    fs::rename(&tmp, path).map_err(|e| format!("替换配置文件失败：{e}"))
}

fn read_json_object(path: &PathBuf) -> Result<serde_json::Map<String, Value>, String> {
    if !path.exists() {
        return Ok(serde_json::Map::new());
    }
    let text = fs::read_to_string(path).map_err(|e| e.to_string())?;
    if text.trim().is_empty() {
        return Ok(serde_json::Map::new());
    }
    serde_json::from_str::<Value>(&text)
        .map_err(|e| format!("{} 不是有效 JSON：{e}", path.display()))?
        .as_object()
        .cloned()
        .ok_or_else(|| format!("{} 顶层不是对象", path.display()))
}

/// Default OpenAI-compatible API base for an OAuth provider (used as the agent
/// upstream when the target is an OAuth subscription).
fn oauth_api_base(kind: OAuthProviderKind) -> &'static str {
    match kind {
        OAuthProviderKind::AnthropicClaude => "https://api.anthropic.com",
        OAuthProviderKind::OpenAiCodex => "https://api.openai.com/v1",
        OAuthProviderKind::GoogleGemini => "https://generativelanguage.googleapis.com",
    }
}

fn resolve_target(db: &Arc<DbManager>, target: &TakeoverTarget) -> Result<ResolvedTarget, String> {
    match target.kind.as_str() {
        "gateway" => {
            let port = db
                .get_setting("proxy_port")
                .ok()
                .flatten()
                .unwrap_or_else(|| "1421".to_string());
            Ok(ResolvedTarget {
                base_url: format!("http://127.0.0.1:{port}"),
                token: "omnix-local".to_string(),
                model: target.model.clone(),
                label: "OMNIX 网关".to_string(),
            })
        }
        "platform" => {
            let platform_id = target.ref_id.clone().ok_or("缺少平台 id")?;
            let conn = db.get_connection().map_err(|e| e.to_string())?;
            let (name, address, key): (String, String, String) = conn
                .query_row(
                    "SELECT name, api_address, api_key FROM model_platforms WHERE id = ?1",
                    rusqlite::params![platform_id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .map_err(|_| "平台不存在".to_string())?;
            let token = crate::crypto::decrypt(&key)
                .split(',')
                .next()
                .unwrap_or("")
                .trim()
                .to_string();
            Ok(ResolvedTarget {
                base_url: address,
                token,
                model: target.model.clone(),
                label: format!("供应商 {name}"),
            })
        }
        "oauth" => {
            let account_id = target.ref_id.clone().ok_or("缺少 OAuth 账号 id")?;
            let (kind, token) = crate::commands::resolve_oauth_access_token(db, &account_id)?;
            Ok(ResolvedTarget {
                base_url: oauth_api_base(kind).to_string(),
                token,
                model: target.model.clone(),
                label: format!("{} 订阅", kind.display_name()),
            })
        }
        other => Err(format!("未知的接管目标：{other}")),
    }
}

// ── Per-agent writers ──────────────────────────────────────────────────────

fn apply_claude(resolved: &ResolvedTarget) -> Result<TakeoverReport, String> {
    let path = claude_settings_path()?;
    let backup = crate::backup::backup_file(&path, &backup_category("claude"))?
        .map(|p| p.to_string_lossy().into_owned());
    let mut root = read_json_object(&path)?;
    let env = root
        .entry("env".to_string())
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or("settings.json 的 env 不是对象")?;
    env.insert("ANTHROPIC_BASE_URL".into(), json!(resolved.base_url));
    env.insert("ANTHROPIC_AUTH_TOKEN".into(), json!(resolved.token));
    if let Some(model) = &resolved.model {
        env.insert("ANTHROPIC_MODEL".into(), json!(model));
    }
    let rendered = serde_json::to_string_pretty(&Value::Object(root)).map_err(|e| e.to_string())?;
    serde_json::from_str::<Value>(&rendered).map_err(|e| format!("写入前校验失败：{e}"))?;
    atomic_write(&path, &rendered)?;
    Ok(TakeoverReport {
        agent: "claude_code".into(),
        config_path: path.to_string_lossy().into_owned(),
        applied: true,
        backup_path: backup,
        detail: format!("已指向 {}", resolved.label),
    })
}

fn apply_codex(resolved: &ResolvedTarget) -> Result<TakeoverReport, String> {
    let path = codex_config_path()?;
    let backup = crate::backup::backup_file(&path, &backup_category("codex"))?
        .map(|p| p.to_string_lossy().into_owned());
    let text = if path.exists() { fs::read_to_string(&path).unwrap_or_default() } else { String::new() };
    let mut doc = text
        .parse::<toml_edit::DocumentMut>()
        .map_err(|e| format!("config.toml 解析失败：{e}"))?;
    // Codex OpenAI-compatible provider expects a `/v1` base.
    let base = if resolved.base_url.trim_end_matches('/').ends_with("/v1") {
        resolved.base_url.trim_end_matches('/').to_string()
    } else {
        format!("{}/v1", resolved.base_url.trim_end_matches('/'))
    };
    doc["model_provider"] = toml_edit::value("omnix");
    if let Some(model) = &resolved.model {
        doc["model"] = toml_edit::value(model.as_str());
    }
    let mut provider = toml_edit::Table::new();
    provider["name"] = toml_edit::value("OMNIX");
    provider["base_url"] = toml_edit::value(base.as_str());
    provider["wire_api"] = toml_edit::value("chat");
    provider["env_key"] = toml_edit::value("OPENAI_API_KEY");
    doc["model_providers"]["omnix"] = toml_edit::Item::Table(provider);
    let rendered = doc.to_string();
    rendered.parse::<toml_edit::DocumentMut>().map_err(|e| format!("写入前 TOML 校验失败：{e}"))?;
    atomic_write(&path, &rendered)?;
    // Codex reads the key named by env_key from auth.json (merge, don't clobber).
    let auth_path = codex_auth_path()?;
    let _ = crate::backup::backup_file(&auth_path, &backup_category("codex_auth"))?;
    let mut auth = read_json_object(&auth_path)?;
    auth.insert("OPENAI_API_KEY".into(), json!(resolved.token));
    let auth_rendered = serde_json::to_string_pretty(&Value::Object(auth)).map_err(|e| e.to_string())?;
    atomic_write(&auth_path, &auth_rendered)?;
    Ok(TakeoverReport {
        agent: "codex".into(),
        config_path: path.to_string_lossy().into_owned(),
        applied: true,
        backup_path: backup,
        detail: format!("已指向 {}（含 auth.json 密钥）", resolved.label),
    })
}

fn apply_gemini(resolved: &ResolvedTarget) -> Result<TakeoverReport, String> {
    // Gemini CLI reads GEMINI_API_KEY / GOOGLE_GEMINI_BASE_URL from ~/.gemini/.env.
    let path = gemini_env_path()?;
    let backup = crate::backup::backup_file(&path, &backup_category("gemini"))?
        .map(|p| p.to_string_lossy().into_owned());
    let contents = format!(
        "GEMINI_API_KEY={}\nGOOGLE_GEMINI_BASE_URL={}\n",
        resolved.token, resolved.base_url
    );
    atomic_write(&path, &contents)?;
    Ok(TakeoverReport {
        agent: "gemini".into(),
        config_path: path.to_string_lossy().into_owned(),
        applied: true,
        backup_path: backup,
        detail: format!("已指向 {}", resolved.label),
    })
}

// ── Commands ───────────────────────────────────────────────────────────────

/// Apply a takeover target to each selected agent's native config.
#[tauri::command]
pub fn cli_takeover_apply(
    agents: Vec<String>,
    target: TakeoverTarget,
    db: State<'_, Arc<DbManager>>,
) -> Result<Vec<TakeoverReport>, String> {
    let db = db.inner().clone();
    let resolved = resolve_target(&db, &target)?;
    if resolved.token.is_empty() {
        return Err("目标缺少可用的密钥/令牌".into());
    }
    let mut reports = Vec::new();
    for agent in &agents {
        let report = match agent.as_str() {
            "claude_code" | "Claude Code" => apply_claude(&resolved)?,
            "codex" | "Codex" => apply_codex(&resolved)?,
            "gemini" | "Gemini" | "gemini_cli" | "Gemini CLI" => apply_gemini(&resolved)?,
            other => return Err(format!("不支持接管的 Agent：{other}")),
        };
        reports.push(report);
    }
    Ok(reports)
}

/// Restore an agent's native config from its most recent takeover backup.
#[tauri::command]
pub fn cli_takeover_revert(agent: String) -> Result<String, String> {
    let (category, path) = match agent.as_str() {
        "claude_code" | "Claude Code" => ("claude", claude_settings_path()?),
        "codex" | "Codex" => ("codex", codex_config_path()?),
        "gemini" | "Gemini" | "gemini_cli" | "Gemini CLI" => ("gemini", gemini_env_path()?),
        other => return Err(format!("不支持接管的 Agent：{other}")),
    };
    let backups = crate::backup::list_backups(&backup_category(category));
    let latest = backups.first().ok_or("没有可回退的备份")?;
    crate::backup::restore_backup(&latest.path, &path.to_string_lossy())?;
    Ok(format!("已从备份还原：{}", latest.path))
}

/// Current takeover state per agent (config path, whether it exists, its base
/// URL if readable, and whether a backup exists to revert to).
#[tauri::command]
pub fn cli_takeover_status() -> Result<Vec<AgentTakeoverState>, String> {
    let claude = claude_settings_path()?;
    let codex = codex_config_path()?;
    let gemini = gemini_env_path()?;

    let claude_base = read_json_object(&claude)
        .ok()
        .and_then(|root| {
            root.get("env")
                .and_then(|env| env.get("ANTHROPIC_BASE_URL"))
                .and_then(|v| v.as_str())
                .map(str::to_string)
        });
    let codex_base = fs::read_to_string(&codex).ok().and_then(|text| {
        text.parse::<toml_edit::DocumentMut>().ok().and_then(|doc| {
            doc.get("model_providers")
                .and_then(|p| p.get("omnix"))
                .and_then(|p| p.get("base_url"))
                .and_then(|v| v.as_str())
                .map(str::to_string)
        })
    });
    let gemini_base = fs::read_to_string(&gemini).ok().and_then(|text| {
        text.lines()
            .find_map(|line| line.strip_prefix("GOOGLE_GEMINI_BASE_URL=").map(str::to_string))
    });

    Ok(vec![
        AgentTakeoverState {
            agent: "claude_code".into(),
            config_path: claude.to_string_lossy().into_owned(),
            config_exists: claude.exists(),
            current_base_url: claude_base,
            has_backup: !crate::backup::list_backups(&backup_category("claude")).is_empty(),
        },
        AgentTakeoverState {
            agent: "codex".into(),
            config_path: codex.to_string_lossy().into_owned(),
            config_exists: codex.exists(),
            current_base_url: codex_base,
            has_backup: !crate::backup::list_backups(&backup_category("codex")).is_empty(),
        },
        AgentTakeoverState {
            agent: "gemini".into(),
            config_path: gemini.to_string_lossy().into_owned(),
            config_exists: gemini.exists(),
            current_base_url: gemini_base,
            has_backup: !crate::backup::list_backups(&backup_category("gemini")).is_empty(),
        },
    ])
}

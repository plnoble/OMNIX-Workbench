//! Sync OMNIX-managed MCP servers into the native config of Claude Code and
//! Codex, so a user configures a tool once instead of editing each agent's
//! config by hand (AionUi "configure once, sync everywhere" + cc-switch atomic
//! native-config writes).
//!
//! Safety rules, because these are the user's real config files (a broken MCP
//! entry previously made Codex's `thread/start` slow):
//! - Back up the target file before writing (via `crate::backup`).
//! - Merge only: never bulk-delete; upsert the selected servers by name and
//!   remove only the specific name the user unsyncs.
//! - Preserve everything else: JSON is merged with `serde_json`; TOML is edited
//!   with `toml_edit` so comments/ordering/other tables survive.
//! - Validate by re-parsing the rendered output before replacing the file, and
//!   write atomically (temp file + rename).

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use serde::Serialize;
use serde_json::{json, Value};
use tauri::State;

use crate::db::DbManager;

#[derive(Debug, Clone)]
struct McpRow {
    name: String,
    command: String,
    args: String,
    env: String,
    url: String,
    server_type: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentMcpState {
    pub agent: String,
    pub config_path: String,
    pub config_exists: bool,
    pub server_names: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct McpSyncReport {
    pub agent: String,
    pub synced: Vec<String>,
    pub skipped: Vec<String>,
    pub backup_path: Option<String>,
}

fn home() -> Result<PathBuf, String> {
    dirs::home_dir().ok_or_else(|| "无法确定用户主目录".to_string())
}

fn claude_config_path() -> Result<PathBuf, String> {
    Ok(home()?.join(".claude.json"))
}

fn codex_config_path() -> Result<PathBuf, String> {
    Ok(home()?.join(".codex").join("config.toml"))
}

fn agent_config_path(agent: &str) -> Result<PathBuf, String> {
    match agent {
        "claude_code" | "Claude Code" => claude_config_path(),
        "codex" | "Codex" => codex_config_path(),
        other => Err(format!("不支持的 Agent：{other}")),
    }
}

/// Temp-file + rename atomic write so a crash never leaves a half-written config.
fn atomic_write(path: &PathBuf, contents: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("创建配置目录失败：{error}"))?;
    }
    let tmp = path.with_extension("omnix-tmp");
    fs::write(&tmp, contents).map_err(|error| format!("写入临时文件失败：{error}"))?;
    fs::rename(&tmp, path).map_err(|error| format!("替换配置文件失败：{error}"))
}

fn load_servers(db: &DbManager, server_ids: &[String]) -> Result<Vec<McpRow>, String> {
    if server_ids.is_empty() {
        return Ok(Vec::new());
    }
    let conn = db.get_connection().map_err(|error| error.to_string())?;
    let mut rows = Vec::new();
    for id in server_ids {
        let row = conn
            .query_row(
                "SELECT name, command, args, env, url, server_type FROM mcp_servers WHERE id = ?1",
                rusqlite::params![id],
                |row| {
                    Ok(McpRow {
                        name: row.get(0)?,
                        command: row.get(1)?,
                        args: row.get(2)?,
                        env: row.get(3)?,
                        url: row.get(4)?,
                        server_type: row.get(5)?,
                    })
                },
            )
            .map_err(|error| format!("读取 MCP 服务 {id} 失败：{error}"))?;
        rows.push(row);
    }
    Ok(rows)
}

fn parse_args(raw: &str) -> Vec<String> {
    serde_json::from_str(raw).unwrap_or_default()
}

fn parse_env(raw: &str) -> BTreeMap<String, String> {
    serde_json::from_str(raw).unwrap_or_default()
}

fn is_remote(server_type: &str) -> bool {
    matches!(server_type, "http" | "sse")
}

// ── Claude (~/.claude.json, JSON) ──────────────────────────────────────────

fn claude_server_spec(row: &McpRow) -> Value {
    if is_remote(&row.server_type) {
        json!({ "type": row.server_type, "url": row.url })
    } else {
        json!({
            "type": "stdio",
            "command": row.command,
            "args": parse_args(&row.args),
            "env": parse_env(&row.env),
        })
    }
}

fn read_claude_root(path: &PathBuf) -> Result<Value, String> {
    if !path.exists() {
        return Ok(json!({}));
    }
    let text = fs::read_to_string(path).map_err(|error| format!("读取 ~/.claude.json 失败：{error}"))?;
    if text.trim().is_empty() {
        return Ok(json!({}));
    }
    let value: Value =
        serde_json::from_str(&text).map_err(|error| format!("~/.claude.json 不是有效 JSON：{error}"))?;
    if !value.is_object() {
        return Err("~/.claude.json 顶层不是对象".into());
    }
    Ok(value)
}

fn sync_claude(rows: &[McpRow], backup: &mut Option<String>) -> Result<(Vec<String>, Vec<String>), String> {
    let path = claude_config_path()?;
    *backup = crate::backup::backup_file(&path, "claude_mcp")?.map(|p| p.to_string_lossy().into_owned());
    let mut root = read_claude_root(&path)?;
    let obj = root.as_object_mut().expect("claude root object");
    let servers = obj
        .entry("mcpServers")
        .or_insert_with(|| json!({}));
    let map = servers
        .as_object_mut()
        .ok_or_else(|| "~/.claude.json 的 mcpServers 不是对象".to_string())?;

    let synced = rows.iter().map(|row| row.name.clone()).collect();
    for row in rows {
        map.insert(row.name.clone(), claude_server_spec(row));
    }

    let rendered = serde_json::to_string_pretty(&root).map_err(|error| error.to_string())?;
    serde_json::from_str::<Value>(&rendered)
        .map_err(|error| format!("写入前 JSON 校验失败：{error}"))?;
    atomic_write(&path, &rendered)?;
    Ok((synced, Vec::new()))
}

fn remove_claude(server_name: &str, backup: &mut Option<String>) -> Result<(), String> {
    let path = claude_config_path()?;
    if !path.exists() {
        return Ok(());
    }
    *backup = crate::backup::backup_file(&path, "claude_mcp")?.map(|p| p.to_string_lossy().into_owned());
    let mut root = read_claude_root(&path)?;
    if let Some(map) = root
        .get_mut("mcpServers")
        .and_then(|servers| servers.as_object_mut())
    {
        map.remove(server_name);
    }
    let rendered = serde_json::to_string_pretty(&root).map_err(|error| error.to_string())?;
    atomic_write(&path, &rendered)
}

fn read_claude_names() -> Result<Vec<String>, String> {
    let path = claude_config_path()?;
    let root = read_claude_root(&path)?;
    Ok(root
        .get("mcpServers")
        .and_then(|servers| servers.as_object())
        .map(|map| map.keys().cloned().collect())
        .unwrap_or_default())
}

// ── Codex (~/.codex/config.toml, TOML via toml_edit) ───────────────────────

fn read_codex_doc(path: &PathBuf) -> Result<toml_edit::DocumentMut, String> {
    if !path.exists() {
        return Ok(toml_edit::DocumentMut::new());
    }
    let text = fs::read_to_string(path).map_err(|error| format!("读取 config.toml 失败：{error}"))?;
    text.parse::<toml_edit::DocumentMut>()
        .map_err(|error| format!("~/.codex/config.toml 解析失败：{error}"))
}

fn sync_codex(rows: &[McpRow], backup: &mut Option<String>) -> Result<(Vec<String>, Vec<String>), String> {
    let path = codex_config_path()?;
    *backup = crate::backup::backup_file(&path, "codex_mcp")?.map(|p| p.to_string_lossy().into_owned());
    let mut doc = read_codex_doc(&path)?;

    if doc.get("mcp_servers").is_none() {
        let mut parent = toml_edit::Table::new();
        parent.set_implicit(true);
        doc["mcp_servers"] = toml_edit::Item::Table(parent);
    }

    let mut synced = Vec::new();
    let mut skipped = Vec::new();
    for row in rows {
        // Codex only supports stdio MCP servers via config.toml today.
        if is_remote(&row.server_type) {
            skipped.push(format!("{}（Codex 暂不支持 {} 类型）", row.name, row.server_type));
            continue;
        }
        let mut table = toml_edit::Table::new();
        table["command"] = toml_edit::value(row.command.as_str());
        let mut args = toml_edit::Array::new();
        for arg in parse_args(&row.args) {
            args.push(arg.as_str());
        }
        table["args"] = toml_edit::value(args);
        let env = parse_env(&row.env);
        if !env.is_empty() {
            let mut env_table = toml_edit::Table::new();
            for (key, value) in &env {
                env_table[key] = toml_edit::value(value.as_str());
            }
            table["env"] = toml_edit::Item::Table(env_table);
        }
        doc["mcp_servers"][row.name.as_str()] = toml_edit::Item::Table(table);
        synced.push(row.name.clone());
    }

    let rendered = doc.to_string();
    rendered
        .parse::<toml_edit::DocumentMut>()
        .map_err(|error| format!("写入前 TOML 校验失败：{error}"))?;
    atomic_write(&path, &rendered)?;
    Ok((synced, skipped))
}

fn remove_codex(server_name: &str, backup: &mut Option<String>) -> Result<(), String> {
    let path = codex_config_path()?;
    if !path.exists() {
        return Ok(());
    }
    *backup = crate::backup::backup_file(&path, "codex_mcp")?.map(|p| p.to_string_lossy().into_owned());
    let mut doc = read_codex_doc(&path)?;
    if let Some(table) = doc.get_mut("mcp_servers").and_then(|item| item.as_table_mut()) {
        table.remove(server_name);
    }
    let rendered = doc.to_string();
    atomic_write(&path, &rendered)
}

fn read_codex_names() -> Result<Vec<String>, String> {
    let path = codex_config_path()?;
    let doc = read_codex_doc(&path)?;
    Ok(doc
        .get("mcp_servers")
        .and_then(|item| item.as_table())
        .map(|table| table.iter().map(|(key, _)| key.to_string()).collect())
        .unwrap_or_default())
}

// ── Tauri commands ─────────────────────────────────────────────────────────

#[tauri::command]
pub fn mcp_sync_to_agents(
    agents: Vec<String>,
    server_ids: Vec<String>,
    db: State<'_, Arc<DbManager>>,
) -> Result<Vec<McpSyncReport>, String> {
    let rows = load_servers(&db, &server_ids)?;
    if rows.is_empty() {
        return Err("请先选择要同步的 MCP 服务".into());
    }
    let mut reports = Vec::new();
    for agent in &agents {
        let mut backup = None;
        let (synced, skipped) = match agent.as_str() {
            "claude_code" | "Claude Code" => sync_claude(&rows, &mut backup)?,
            "codex" | "Codex" => sync_codex(&rows, &mut backup)?,
            other => return Err(format!("不支持的 Agent：{other}")),
        };
        reports.push(McpSyncReport {
            agent: agent.clone(),
            synced,
            skipped,
            backup_path: backup,
        });
    }
    Ok(reports)
}

#[tauri::command]
pub fn mcp_remove_from_agent(agent: String, server_name: String) -> Result<Option<String>, String> {
    let mut backup = None;
    match agent.as_str() {
        "claude_code" | "Claude Code" => remove_claude(&server_name, &mut backup)?,
        "codex" | "Codex" => remove_codex(&server_name, &mut backup)?,
        other => return Err(format!("不支持的 Agent：{other}")),
    }
    Ok(backup)
}

#[tauri::command]
pub fn mcp_get_agent_states() -> Result<Vec<AgentMcpState>, String> {
    let claude_path = claude_config_path()?;
    let codex_path = codex_config_path()?;
    Ok(vec![
        AgentMcpState {
            agent: "claude_code".into(),
            config_path: claude_path.to_string_lossy().into_owned(),
            config_exists: claude_path.exists(),
            server_names: read_claude_names()?,
        },
        AgentMcpState {
            agent: "codex".into(),
            config_path: codex_path.to_string_lossy().into_owned(),
            config_exists: codex_path.exists(),
            server_names: read_codex_names()?,
        },
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(name: &str, server_type: &str) -> McpRow {
        McpRow {
            name: name.into(),
            command: "npx".into(),
            args: "[\"-y\",\"@modelcontextprotocol/server-fetch\"]".into(),
            env: "{\"TOKEN\":\"x\"}".into(),
            url: "https://example.test/mcp".into(),
            server_type: server_type.into(),
        }
    }

    #[test]
    fn codex_toml_merge_preserves_existing_config_and_adds_stdio_server() {
        let existing = "model = \"gpt-5\"\n\n[model_providers.custom]\nbase_url = \"https://x/v1\"\n";
        let mut doc = existing.parse::<toml_edit::DocumentMut>().unwrap();
        // mirror sync_codex body for a single stdio row
        if doc.get("mcp_servers").is_none() {
            let mut parent = toml_edit::Table::new();
            parent.set_implicit(true);
            doc["mcp_servers"] = toml_edit::Item::Table(parent);
        }
        let r = row("fetch", "stdio");
        let mut table = toml_edit::Table::new();
        table["command"] = toml_edit::value(r.command.as_str());
        let mut args = toml_edit::Array::new();
        for a in parse_args(&r.args) {
            args.push(a.as_str());
        }
        table["args"] = toml_edit::value(args);
        doc["mcp_servers"][r.name.as_str()] = toml_edit::Item::Table(table);
        let rendered = doc.to_string();

        assert!(rendered.contains("model = \"gpt-5\""));
        assert!(rendered.contains("[model_providers.custom]"));
        assert!(rendered.contains("[mcp_servers.fetch]"));
        assert!(rendered.contains("command = \"npx\""));
        // Output is valid TOML.
        rendered.parse::<toml_edit::DocumentMut>().expect("valid toml");
    }

    #[test]
    fn claude_spec_distinguishes_stdio_and_remote() {
        let stdio = claude_server_spec(&row("fetch", "stdio"));
        assert_eq!(stdio["type"], "stdio");
        assert_eq!(stdio["command"], "npx");
        assert!(stdio["args"].is_array());

        let remote = claude_server_spec(&row("api", "http"));
        assert_eq!(remote["type"], "http");
        assert_eq!(remote["url"], "https://example.test/mcp");
    }

    #[test]
    fn remote_server_is_skipped_for_codex() {
        assert!(is_remote("http"));
        assert!(is_remote("sse"));
        assert!(!is_remote("stdio"));
    }
}

use tauri::State;
use std::sync::Arc;
use rusqlite::params;
use crate::db::DbManager;
use crate::input_validation;
use super::*;

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
pub fn save_agent_account(
    account: serde_json::Value,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    let id = account["id"].as_str().unwrap_or_default().to_string();
    let account_name = account["account_name"].as_str().unwrap_or_default().to_string();
    let api_key = account["api_key"].as_str().unwrap_or_default().to_string();
    let api_host = account["api_host"].as_str().unwrap_or_default().to_string();
    let target_model = account["target_model"].as_str().unwrap_or_default().to_string();
    let is_active = account["is_active"].as_bool().unwrap_or(false);
    // Derive agent_name from the account context (default to "claude-code" if not specified)
    let agent_name = account["agent_name"].as_str().unwrap_or("claude-code").to_string();

    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT OR REPLACE INTO agent_accounts (id, account_name, api_key, api_host, target_model, agent_name, is_active, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, CURRENT_TIMESTAMP)",
        params![id, account_name, api_key, api_host, target_model, agent_name, is_active],
    ).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn switch_agent_account(
    id: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    input_validation::validate_id(&id, "id")?;
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
    input_validation::validate_id(&id, "id")?;
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM agent_accounts WHERE id = ?1", params![id]).map_err(|e| e.to_string())?;
    Ok(())
}

// ── F1: unified per-agent upstream account switcher (multi-account) ────
//
// Lets an agent's active upstream be switched between OAuth subscriptions (2A)
// and api-key accounts mid-conversation. The choice is a setting; the session
// gateway (`resolve_session_model_upstream`) reads it per request, so switching
// only changes the next turn's upstream — the conversation/context is untouched.

/// Settings key holding an agent's active upstream account ref.
pub(crate) fn active_upstream_setting_key(agent_name: &str) -> String {
    format!("active_upstream_{agent_name}")
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct UpstreamAccountOption {
    /// `oauth:<id>` | `apikey:<id>` — opaque ref the proxy resolves.
    pub account_ref: String,
    pub kind: String, // "oauth" | "apikey"
    pub label: String,
    pub provider: Option<String>,
    pub expired: bool,
    pub is_active: bool,
}

/// List the upstream accounts an agent can switch between: every OAuth
/// subscription plus this agent's api-key accounts. Marks the active one.
#[tauri::command]
pub fn list_agent_upstream_accounts(
    agent_name: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<Vec<UpstreamAccountOption>, String> {
    let active = db
        .get_setting(&active_upstream_setting_key(&agent_name))
        .ok()
        .flatten()
        .unwrap_or_default();
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let mut out = Vec::new();

    // OAuth subscriptions (any provider — user picks what fits the agent).
    if let Ok(mut stmt) = conn.prepare(
        "SELECT id, provider, label,
                CASE WHEN expires_at IS NOT NULL AND expires_at <= datetime('now') THEN 1 ELSE 0 END
         FROM oauth_accounts ORDER BY created_at DESC",
    ) {
        if let Ok(rows) = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)? != 0,
            ))
        }) {
            for (id, provider, label, expired) in rows.flatten() {
                let account_ref = format!("oauth:{id}");
                let provider_name = crate::oauth::OAuthProviderKind::from_str(&provider)
                    .map(|k| k.display_name().to_string())
                    .unwrap_or_else(|_| provider.clone());
                out.push(UpstreamAccountOption {
                    is_active: active == account_ref,
                    account_ref,
                    kind: "oauth".into(),
                    label,
                    provider: Some(provider_name),
                    expired,
                });
            }
        }
    }

    // This agent's api-key accounts.
    if let Ok(mut stmt) = conn.prepare(
        "SELECT id, account_name FROM agent_accounts WHERE agent_name = ?1 ORDER BY updated_at DESC",
    ) {
        if let Ok(rows) = stmt.query_map(params![agent_name], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        }) {
            for (id, name) in rows.flatten() {
                let account_ref = format!("apikey:{id}");
                out.push(UpstreamAccountOption {
                    is_active: active == account_ref,
                    account_ref,
                    kind: "apikey".into(),
                    label: name,
                    provider: None,
                    expired: false,
                });
            }
        }
    }
    Ok(out)
}

/// Set (or clear with empty) the agent's active upstream account.
#[tauri::command]
pub fn set_active_upstream_account(
    agent_name: String,
    account_ref: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    db.set_setting(&active_upstream_setting_key(&agent_name), account_ref.trim())
        .map_err(|e| e.to_string())
}

/// Read the agent's active upstream account ref (empty = agent/platform default).
#[tauri::command]
pub fn get_active_upstream_account(
    agent_name: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<String, String> {
    Ok(db
        .get_setting(&active_upstream_setting_key(&agent_name))
        .ok()
        .flatten()
        .unwrap_or_default())
}

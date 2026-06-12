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

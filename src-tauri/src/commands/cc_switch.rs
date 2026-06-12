use tauri::State;
use std::sync::Arc;
use std::collections::HashMap;
use rusqlite::params;
use crate::db::DbManager;
use super::*;

// ══════════════════════════════════════════════════
// Agent-Platform Bindings (CC Switch inspired)
// ══════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentPlatformBinding {
    pub agent_name: String,
    pub platform_id: String,
    pub platform_name: String,
    pub model_name: Option<String>,
    pub enabled: bool,
}

/// Get all agent-platform bindings
#[tauri::command]
pub fn get_agent_bindings(db: State<'_, Arc<DbManager>>) -> Result<Vec<AgentPlatformBinding>, String> {
    let conn = db.get_connection().map_err(|e: rusqlite::Error| e.to_string())?;
    let mut stmt = conn.prepare(
        "SELECT apb.agent_name, apb.platform_id, mp.name, apb.model_name, apb.enabled
         FROM agent_platform_bindings apb
         LEFT JOIN model_platforms mp ON apb.platform_id = mp.id
         ORDER BY apb.agent_name"
    ).map_err(|e: rusqlite::Error| e.to_string())?;

    let rows = stmt.query_map([], |row| {
        Ok(AgentPlatformBinding {
            agent_name: row.get(0)?,
            platform_id: row.get(1)?,
            platform_name: row.get::<_, Option<String>>(2)?.unwrap_or_default(),
            model_name: row.get(3)?,
            enabled: row.get::<_, i32>(4)? != 0,
        })
    }).map_err(|e: rusqlite::Error| e.to_string())?;

    Ok(rows.flatten().collect())
}

/// Bind an agent to a specific platform
#[tauri::command]
pub fn set_agent_binding(
    agent_name: String,
    platform_id: String,
    model_name: Option<String>,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    let conn = db.get_connection().map_err(|e: rusqlite::Error| e.to_string())?;
    conn.execute(
        "INSERT OR REPLACE INTO agent_platform_bindings (agent_name, platform_id, model_name, enabled, updated_at)
         VALUES (?1, ?2, ?3, 1, datetime('now'))",
        params![agent_name, platform_id, model_name],
    ).map_err(|e: rusqlite::Error| e.to_string())?;
    Ok(())
}

/// Remove an agent-platform binding
#[tauri::command]
pub fn remove_agent_binding(
    agent_name: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    let conn = db.get_connection().map_err(|e: rusqlite::Error| e.to_string())?;
    conn.execute(
        "DELETE FROM agent_platform_bindings WHERE agent_name = ?1",
        params![agent_name],
    ).map_err(|e: rusqlite::Error| e.to_string())?;
    Ok(())
}

/// Toggle binding enabled/disabled
#[tauri::command]
pub fn toggle_agent_binding(
    agent_name: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    let conn = db.get_connection().map_err(|e: rusqlite::Error| e.to_string())?;
    conn.execute(
        "UPDATE agent_platform_bindings SET enabled = CASE WHEN enabled = 1 THEN 0 ELSE 1 END, updated_at = datetime('now') WHERE agent_name = ?1",
        params![agent_name],
    ).map_err(|e: rusqlite::Error| e.to_string())?;
    Ok(())
}

// ══════════════════════════════════════════════════
// Circuit Breaker & Session Usage (CC Switch inspired)
// ══════════════════════════════════════════════════

/// Get circuit breaker status for all platforms
#[tauri::command]
pub fn get_circuit_status(db: State<'_, Arc<DbManager>>) -> Vec<crate::circuit_breaker::CircuitBreakerStatus> {
    crate::circuit_breaker::get_all_circuit_status(&db)
}

/// Reset circuit breaker for a platform
#[tauri::command]
pub fn reset_circuit_breaker(
    platform_id: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    crate::circuit_breaker::reset_circuit(&db, &platform_id);
    Ok(())
}

/// Get model pricing table
#[tauri::command]
pub fn get_model_pricing() -> HashMap<String, (f64, f64)> {
    crate::circuit_breaker::get_model_pricing()
}

/// Estimate cost for model usage
#[tauri::command]
pub fn estimate_model_cost(
    model: String,
    prompt_tokens: i64,
    completion_tokens: i64,
) -> f64 {
    crate::circuit_breaker::estimate_cost(&model, prompt_tokens, completion_tokens)
}

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
    pub binding_kind: String,
    pub builtin_model: Option<String>,
    pub enabled: bool,
}

/// Get all agent-platform bindings
#[tauri::command]
pub fn get_agent_bindings(db: State<'_, Arc<DbManager>>) -> Result<Vec<AgentPlatformBinding>, String> {
    let conn = db.get_connection().map_err(|e: rusqlite::Error| e.to_string())?;
    let mut stmt = conn.prepare(
        "SELECT apb.agent_name, apb.platform_id, mp.name, apb.model_name,
                COALESCE(apb.binding_kind, 'omnix'), apb.builtin_model, apb.enabled
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
            binding_kind: row.get(4)?,
            builtin_model: row.get(5)?,
            enabled: row.get::<_, i32>(6)? != 0,
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
    binding_kind: Option<String>,
    builtin_model: Option<String>,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    let conn = db.get_connection().map_err(|e: rusqlite::Error| e.to_string())?;
    let kind = binding_kind.unwrap_or_else(|| "omnix".to_string());
    conn.execute(
        "INSERT INTO agent_platform_bindings (agent_name, platform_id, model_name, binding_kind, builtin_model, enabled, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, 1, datetime('now'))
         ON CONFLICT(agent_name) DO UPDATE SET
            platform_id = excluded.platform_id,
            model_name = excluded.model_name,
            binding_kind = excluded.binding_kind,
            builtin_model = excluded.builtin_model,
            enabled = 1,
            updated_at = datetime('now')",
        params![agent_name, platform_id, model_name, kind, builtin_model],
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

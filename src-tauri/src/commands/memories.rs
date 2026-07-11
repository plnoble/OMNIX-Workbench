use tauri::State;
use std::sync::Arc;
use rusqlite::params;
use crate::db::DbManager;
use crate::input_validation;
use super::*;

#[tauri::command]
pub fn get_all_memories(
    db: State<'_, Arc<DbManager>>,
) -> Result<Vec<Memory>, String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let mut stmt = conn.prepare(
        "SELECT id, incident_desc, code_pattern, remediation, keywords, created_at, \
                confidence, seen_count, repeated_count, status \
         FROM memories WHERE status IS NULL OR status != 'merged' ORDER BY created_at DESC")
        .map_err(|e| e.to_string())?;
    let rows = stmt.query_map([], |row| {
        Ok(Memory {
            id: row.get(0)?,
            incident_desc: row.get(1)?,
            code_pattern: row.get(2)?,
            remediation: row.get(3)?,
            keywords: row.get(4)?,
            created_at: row.get(5)?,
            confidence: row.get::<_, Option<f64>>(6)?.unwrap_or(1.0),
            seen_count: row.get::<_, Option<i64>>(7)?.unwrap_or(0),
            repeated_count: row.get::<_, Option<i64>>(8)?.unwrap_or(0),
            status: row.get::<_, Option<String>>(9)?.unwrap_or_else(|| "active".into()),
        })
    }).map_err(|e| e.to_string())?;

    let mut result = Vec::new();
    for r in rows {
        if let Ok(mem) = r {
            result.push(mem);
        }
    }
    Ok(result)
}

#[tauri::command]
pub fn create_memory(
    id: String,
    incident_desc: String,
    code_pattern: String,
    remediation: String,
    keywords: String,
    mem_type: Option<String>,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    input_validation::validate_id(&id, "id")?;
    let mem_type = mem_type.unwrap_or_else(|| "experience".to_string());
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT OR REPLACE INTO memories (id, incident_desc, code_pattern, remediation, keywords, type, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, CURRENT_TIMESTAMP)",
        params![id, incident_desc, code_pattern, remediation, keywords, mem_type],
    ).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn delete_memory(
    id: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    input_validation::validate_id(&id, "id")?;
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM memories WHERE id = ?1", params![id]).map_err(|e| e.to_string())?;
    Ok(())
}

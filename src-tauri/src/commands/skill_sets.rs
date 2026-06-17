use std::sync::Arc;

use chrono::Utc;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::db::DbManager;
use crate::sync_engine::{BatchSyncResult, ConflictStrategy, SyncEngine};
use crate::tool_adapters::SyncMode;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillSetItem {
    pub id: String,
    pub skill_set_id: String,
    pub skill_id: String,
    pub order_num: i32,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillSet {
    pub id: String,
    pub name: String,
    pub description: String,
    pub sync_targets: Vec<String>,
    pub items: Vec<SkillSetItem>,
    pub created_at: String,
    pub updated_at: String,
}

fn make_id(prefix: &str) -> String {
    let nanos = Utc::now()
        .timestamp_nanos_opt()
        .unwrap_or_else(|| Utc::now().timestamp_millis() * 1_000_000);
    format!("{prefix}_{nanos}_{}", std::process::id())
}

fn parse_sync_targets(raw: String) -> Vec<String> {
    serde_json::from_str(&raw).unwrap_or_default()
}

fn load_skill_set(db: &DbManager, skill_set_id: &str) -> Result<SkillSet, String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let mut set = conn
        .query_row(
            "SELECT id, name, description, sync_targets, created_at, updated_at
             FROM skill_sets WHERE id = ?1",
            params![skill_set_id],
            |r| {
                Ok(SkillSet {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    description: r.get(2)?,
                    sync_targets: parse_sync_targets(r.get(3)?),
                    items: Vec::new(),
                    created_at: r.get(4)?,
                    updated_at: r.get(5)?,
                })
            },
        )
        .map_err(|e| e.to_string())?;

    let mut stmt = conn
        .prepare(
            "SELECT id, skill_set_id, skill_id, order_num, created_at
             FROM skill_set_items
             WHERE skill_set_id = ?1
             ORDER BY order_num, skill_id",
        )
        .map_err(|e| e.to_string())?;
    let items = stmt
        .query_map(params![skill_set_id], |r| {
            Ok(SkillSetItem {
                id: r.get(0)?,
                skill_set_id: r.get(1)?,
                skill_id: r.get(2)?,
                order_num: r.get(3)?,
                created_at: r.get(4)?,
            })
        })
        .map_err(|e| e.to_string())?
        .flatten()
        .collect::<Vec<_>>();
    set.items = items;
    Ok(set)
}

fn replace_items(conn: &rusqlite::Connection, skill_set_id: &str, skill_ids: &[String]) -> Result<(), String> {
    conn.execute(
        "DELETE FROM skill_set_items WHERE skill_set_id = ?1",
        params![skill_set_id],
    )
    .map_err(|e| e.to_string())?;

    for (index, skill_id) in skill_ids.iter().enumerate() {
        conn.execute(
            "INSERT INTO skill_set_items (id, skill_set_id, skill_id, order_num)
             VALUES (?1, ?2, ?3, ?4)",
            params![make_id("skill_set_item"), skill_set_id, skill_id, index as i32],
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn create_skill_set(
    name: String,
    description: String,
    skill_ids: Vec<String>,
    sync_targets: Vec<String>,
    db: State<'_, Arc<DbManager>>,
) -> Result<SkillSet, String> {
    if name.trim().is_empty() {
        return Err("Skill set name cannot be empty".to_string());
    }
    if skill_ids.is_empty() {
        return Err("Select at least one skill".to_string());
    }

    let id = make_id("skill_set");
    let targets_json = serde_json::to_string(&sync_targets).unwrap_or_else(|_| "[]".to_string());
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO skill_sets (id, name, description, sync_targets)
         VALUES (?1, ?2, ?3, ?4)",
        params![id, name, description, targets_json],
    )
    .map_err(|e| e.to_string())?;
    replace_items(&conn, &id, &skill_ids)?;
    load_skill_set(&db, &id)
}

#[tauri::command]
pub fn list_skill_sets(db: State<'_, Arc<DbManager>>) -> Result<Vec<SkillSet>, String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare("SELECT id FROM skill_sets ORDER BY updated_at DESC, name")
        .map_err(|e| e.to_string())?;
    let ids = stmt
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(|e| e.to_string())?
        .flatten()
        .collect::<Vec<_>>();

    ids.into_iter().map(|id| load_skill_set(&db, &id)).collect()
}

#[tauri::command]
pub fn update_skill_set(
    skill_set_id: String,
    name: String,
    description: String,
    skill_ids: Vec<String>,
    sync_targets: Vec<String>,
    db: State<'_, Arc<DbManager>>,
) -> Result<SkillSet, String> {
    if name.trim().is_empty() {
        return Err("Skill set name cannot be empty".to_string());
    }
    if skill_ids.is_empty() {
        return Err("Select at least one skill".to_string());
    }

    let targets_json = serde_json::to_string(&sync_targets).unwrap_or_else(|_| "[]".to_string());
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE skill_sets SET name = ?1, description = ?2, sync_targets = ?3, updated_at = datetime('now') WHERE id = ?4",
        params![name, description, targets_json, skill_set_id],
    )
    .map_err(|e| e.to_string())?;
    replace_items(&conn, &skill_set_id, &skill_ids)?;
    load_skill_set(&db, &skill_set_id)
}

#[tauri::command]
pub fn delete_skill_set(
    skill_set_id: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM skill_sets WHERE id = ?1", params![skill_set_id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn sync_skill_set_to_tools(
    skill_set_id: String,
    tool_ids: Vec<String>,
    mode: String,
    strategy: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<Vec<BatchSyncResult>, String> {
    let set = load_skill_set(&db, &skill_set_id)?;
    let target_tools = if tool_ids.is_empty() { set.sync_targets.clone() } else { tool_ids };
    if target_tools.is_empty() {
        return Err("Select at least one target tool".to_string());
    }

    let sync_mode = match mode.as_str() {
        "symlink" => SyncMode::Symlink,
        _ => SyncMode::Copy,
    };
    let conflict_strategy = match strategy.as_str() {
        "skip" => ConflictStrategy::Skip,
        "rename" => ConflictStrategy::Rename,
        _ => ConflictStrategy::Overwrite,
    };

    let engine = SyncEngine::new(Arc::clone(&db));
    let mut results = Vec::new();
    for item in set.items {
        results.push(engine.sync_one_to_many(&item.skill_id, &target_tools, &sync_mode, &conflict_strategy));
    }
    Ok(results)
}

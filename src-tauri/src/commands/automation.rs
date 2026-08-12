use tauri::State;
use std::sync::Arc;
use rusqlite::params;
use crate::db::DbManager;
use crate::skill_dag::{SkillGraph, SkillEdge, EdgeType, SkillSearchResult, SetValidation};
use super::*;

// ══════════════════════════════════════════════════
// Skill DAG
// ══════════════════════════════════════════════════

/// Search skills with conflict awareness
#[tauri::command]
pub fn search_skills_dag(
    query: String,
    top_k: Option<usize>,
    db: State<'_, Arc<DbManager>>,
) -> Result<SkillSearchResult, String> {
    let graph = build_skill_graph(&db)?;
    Ok(graph.search(&query, top_k.unwrap_or(10)))
}

/// Validate a skill set
#[tauri::command]
pub fn check_skill_set(
    skill_ids: Vec<String>,
    db: State<'_, Arc<DbManager>>,
) -> Result<SetValidation, String> {
    let graph = build_skill_graph(&db)?;
    Ok(graph.check_set(&skill_ids))
}

/// Expand skill set with transitive dependencies
#[tauri::command]
pub fn expand_skill_set(
    skill_ids: Vec<String>,
    db: State<'_, Arc<DbManager>>,
) -> Result<Vec<String>, String> {
    let graph = build_skill_graph(&db)?;
    Ok(graph.expand_set(&skill_ids))
}

/// Add an edge between skills (with cycle detection)
#[tauri::command]
pub fn add_skill_edge(
    source: String,
    target: String,
    edge_type: String,
    reason: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<String, String> {
    let et = EdgeType::from_str(&edge_type).ok_or("Invalid edge type")?;

    let mut graph = build_skill_graph(&db)?;

    // Cycle check for directed edges
    if graph.would_create_cycle(&source, &target, &et) {
        return Err(format!("Adding {} → {} would create a cycle", source, target));
    }

    let edge = SkillEdge {
        source: source.clone(),
        target: target.clone(),
        edge_type: et,
        reason,
        origin: "manual".into(),
    };

    if graph.commit_add(edge) {
        // Persist to DB: store as dependency in skills table
        persist_edge_to_db(&db, &source, &target, &edge_type)?;
        Ok(format!("Edge added: {} → {} ({})", source, target, edge_type))
    } else {
        Err("Edge already exists".into())
    }
}

/// Remove an edge between skills
#[tauri::command]
pub fn remove_skill_edge(
    source: String,
    target: String,
    edge_type: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<String, String> {
    let et = EdgeType::from_str(&edge_type).ok_or("Invalid edge type")?;
    let mut graph = build_skill_graph(&db)?;

    if graph.commit_remove(&source, &target, &et) {
        Ok(format!("Edge removed: {} → {} ({})", source, target, edge_type))
    } else {
        Err("Edge not found".into())
    }
}

/// Build skill graph from database (helper)
fn build_skill_graph(db: &DbManager) -> Result<SkillGraph, String> {
    let conn = db.get_connection().map_err(|e: rusqlite::Error| e.to_string())?;
    let mut stmt = conn.prepare(
        "SELECT name, description, file_path, COALESCE(dependencies, '[]') FROM skills WHERE is_active = 1"
    ).map_err(|e: rusqlite::Error| e.to_string())?;

    let skills: Vec<(String, String, String, String)> = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
        ))
    }).map_err(|e: rusqlite::Error| e.to_string())?
        .flatten()
        .collect();

    Ok(SkillGraph::from_skills(&skills))
}

/// Persist an edge to the skills table (as dependency)
fn persist_edge_to_db(db: &DbManager, source: &str, target: &str, edge_type: &str) -> Result<(), String> {
    let conn = db.get_connection().map_err(|e: rusqlite::Error| e.to_string())?;

    if edge_type == "depends_on" {
        // Add target to source's dependencies JSON array
        let current_deps: String = conn.query_row(
            "SELECT COALESCE(dependencies, '[]') FROM skills WHERE name = ?1",
            params![source],
            |r| r.get(0),
        ).unwrap_or_else(|_| "[]".into());

        let mut deps: Vec<String> = serde_json::from_str(&current_deps).unwrap_or_default();
        if !deps.contains(&target.to_string()) {
            deps.push(target.to_string());
            let deps_json = serde_json::to_string(&deps).unwrap_or_else(|_| "[]".into());
            conn.execute(
                "UPDATE skills SET dependencies = ?1 WHERE name = ?2",
                params![deps_json, source],
            ).map_err(|e: rusqlite::Error| e.to_string())?;
        }
    }
    // Other edge types are stored only in memory for now
    Ok(())
}

// ══════════════════════════════════════════════════
// Async Agent Mailbox
// ══════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MailMessage {
    pub id: String,
    pub from_agent: String,
    pub to_agent: String,
    pub subject: String,
    pub body: String,
    pub read: bool,
    pub created_at: String,
}

/// Send a message to another agent's mailbox
#[tauri::command]
pub fn send_mail(
    from_agent: String,
    to_agent: String,
    subject: String,
    body: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<String, String> {
    let conn = db.get_connection().map_err(|e: rusqlite::Error| e.to_string())?;
    let id = format!("mail_{}", chrono::Utc::now().timestamp_millis());
    conn.execute(
        "INSERT INTO agent_mailbox (id, from_agent, to_agent, subject, body) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![id, from_agent, to_agent, subject, body],
    ).map_err(|e: rusqlite::Error| e.to_string())?;
    Ok(id)
}

/// Get unread messages for an agent
#[tauri::command]
pub fn get_mail(
    agent_name: String,
    include_read: Option<bool>,
    db: State<'_, Arc<DbManager>>,
) -> Result<Vec<MailMessage>, String> {
    let conn = db.get_connection().map_err(|e: rusqlite::Error| e.to_string())?;

    let sql = if include_read.unwrap_or(false) {
        "SELECT id, from_agent, to_agent, subject, body, is_read, created_at FROM agent_mailbox WHERE to_agent = ?1 ORDER BY created_at DESC"
    } else {
        "SELECT id, from_agent, to_agent, subject, body, is_read, created_at FROM agent_mailbox WHERE to_agent = ?1 AND is_read = 0 ORDER BY created_at DESC"
    };

    let mut stmt = conn.prepare(sql).map_err(|e: rusqlite::Error| e.to_string())?;
    let rows = stmt.query_map(params![agent_name], |row| {
        Ok(MailMessage {
            id: row.get(0)?,
            from_agent: row.get(1)?,
            to_agent: row.get(2)?,
            subject: row.get(3)?,
            body: row.get(4)?,
            read: row.get::<_, i32>(5)? != 0,
            created_at: row.get(6)?,
        })
    }).map_err(|e: rusqlite::Error| e.to_string())?;
    Ok(rows.flatten().collect())
}

/// Mark messages as read
#[tauri::command]
pub fn mark_mail_read(
    message_ids: Vec<String>,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    let conn = db.get_connection().map_err(|e: rusqlite::Error| e.to_string())?;
    for id in &message_ids {
        let _ = conn.execute("UPDATE agent_mailbox SET is_read = 1 WHERE id = ?1", params![id]);
    }
    Ok(())
}

// ══════════════════════════════════════════════════
// Enhanced Task Board with Dependency Tracking
// ══════════════════════════════════════════════════

/// Update task with blocks dependency (reverse of blocked_by)
#[tauri::command]
pub fn set_task_blocks(
    task_id: String,
    blocks_ids: Vec<String>,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    let conn = db.get_connection().map_err(|e: rusqlite::Error| e.to_string())?;
    // Clear existing
    let _ = conn.execute("DELETE FROM task_dependencies WHERE task_id = ?1", params![task_id]);
    // Add new
    for dep in &blocks_ids {
        let _ = conn.execute(
            "INSERT INTO task_dependencies (task_id, blocks_id) VALUES (?1, ?2)",
            params![task_id, dep],
        );
    }
    Ok(())
}

/// Auto-unblock tasks when a blocking task completes
#[tauri::command]
pub fn auto_unblock_tasks(
    completed_task_id: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<Vec<String>, String> {
    let conn = db.get_connection().map_err(|e: rusqlite::Error| e.to_string())?;
    // Find tasks blocked by this completed task
    let mut stmt = conn.prepare(
        "SELECT task_id FROM task_dependencies WHERE blocks_id = ?1"
    ).map_err(|e: rusqlite::Error| e.to_string())?;
    let blocked: Vec<String> = stmt.query_map(params![completed_task_id], |r| r.get(0))
        .map_err(|e: rusqlite::Error| e.to_string())?
        .flatten()
        .collect();

    // Remove the dependency
    let _ = conn.execute("DELETE FROM task_dependencies WHERE blocks_id = ?1", params![completed_task_id]);

    Ok(blocked)
}

// ══════════════════════════════════════════════════
// YOLO Full-Auto Mode
// ══════════════════════════════════════════════════

/// YOLO mode permission levels (graded permissions)
/// - "off":    All tool calls require manual confirmation
/// - "safe":   Auto-approve read-only/safe operations, confirm moderate, block dangerous
/// - "moderate": Auto-approve safe + moderate operations, confirm dangerous
/// - "full":   Auto-approve all operations (original YOLO behavior, use with caution)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct YoloModeConfig {
    /// Current permission level: "off" | "safe" | "moderate" | "full"
    pub level: String,
    /// Whether auto-retry is enabled for failed operations
    pub auto_retry: bool,
    /// Max consecutive auto-retries before requiring manual confirmation
    pub max_retries: u32,
}

impl Default for YoloModeConfig {
    fn default() -> Self {
        Self {
            level: "off".to_string(),
            auto_retry: false,
            max_retries: 3,
        }
    }
}

/// Check if a tool call should be auto-approved under current YOLO mode
#[tauri::command]
pub fn check_yolo_permission(
    tool_name: String,
    danger_level: String, // "safe" | "moderate" | "dangerous"
    db: State<'_, Arc<DbManager>>,
) -> Result<serde_json::Value, String> {
    let config = get_yolo_mode_config(db)?;
    let auto_approved = match config.level.as_str() {
        "off" => false,
        "safe" => danger_level == "safe",
        "moderate" => danger_level == "safe" || danger_level == "moderate",
        "full" => true,
        _ => false,
    };

    Ok(serde_json::json!({
        "auto_approved": auto_approved,
        "yolo_level": config.level,
        "tool_name": tool_name,
        "danger_level": danger_level,
        "auto_retry": config.auto_retry && auto_approved,
        "max_retries": config.max_retries,
    }))
}

/// Get YOLO mode configuration (graded permission)
#[tauri::command]
pub fn get_yolo_mode_config(db: State<'_, Arc<DbManager>>) -> Result<YoloModeConfig, String> {
    let level = db.get_setting("yolo_mode")
        .ok().flatten()
        .unwrap_or_else(|| "off".to_string());

    // Backward compat: old "true"/"false" values
    let level = match level.as_str() {
        "true" => "full".to_string(),
        "false" => "off".to_string(),
        v => v.to_string(),
    };

    let auto_retry = db.get_setting("yolo_auto_retry")
        .ok().flatten()
        .map(|v| v == "true")
        .unwrap_or(false);

    let max_retries: u32 = db.get_setting("yolo_max_retries")
        .ok().flatten()
        .and_then(|v| v.parse().ok())
        .unwrap_or(3);

    Ok(YoloModeConfig { level, auto_retry, max_retries })
}

/// Set YOLO mode configuration (graded permission)
#[tauri::command]
pub fn set_yolo_mode_config(
    config: serde_json::Value,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    let conn = db.get_connection().map_err(|e: rusqlite::Error| e.to_string())?;

    let level = config["level"].as_str().unwrap_or("off");
    // Validate level
    if !matches!(level, "off" | "safe" | "moderate" | "full") {
        return Err(format!("Invalid YOLO level: {}. Must be off/safe/moderate/full", level));
    }

    conn.execute(
        "INSERT OR REPLACE INTO settings (key, value) VALUES ('yolo_mode', ?1)",
        params![level],
    ).map_err(|e: rusqlite::Error| e.to_string())?;

    let auto_retry = config["auto_retry"].as_bool().unwrap_or(false);
    conn.execute(
        "INSERT OR REPLACE INTO settings (key, value) VALUES ('yolo_auto_retry', ?1)",
        params![if auto_retry { "true" } else { "false" }],
    ).map_err(|e: rusqlite::Error| e.to_string())?;

    let max_retries = config["max_retries"].as_u64().unwrap_or(3);
    conn.execute(
        "INSERT OR REPLACE INTO settings (key, value) VALUES ('yolo_max_retries', ?1)",
        params![max_retries.to_string()],
    ).map_err(|e: rusqlite::Error| e.to_string())?;

    Ok(())
}

/// Get YOLO mode status (backward compatible — returns true for safe/moderate/full)
#[tauri::command]
pub fn get_yolo_mode(db: State<'_, Arc<DbManager>>) -> bool {
    let level = db.get_setting("yolo_mode").ok().flatten().unwrap_or_else(|| "off".to_string());
    match level.as_str() {
        "true" | "safe" | "moderate" | "full" => true,
        _ => false,
    }
}

/// Toggle YOLO mode (backward compatible — true sets "moderate", false sets "off")
#[tauri::command]
pub fn set_yolo_mode(enabled: bool, db: State<'_, Arc<DbManager>>) -> Result<(), String> {
    let conn = db.get_connection().map_err(|e: rusqlite::Error| e.to_string())?;
    conn.execute(
        "INSERT OR REPLACE INTO settings (key, value) VALUES ('yolo_mode', ?1)",
        params![if enabled { "moderate" } else { "off" }],
    ).map_err(|e: rusqlite::Error| e.to_string())?;
    Ok(())
}

// ══════════════════════════════════════════════════
// Persistent Cron with Timezone + Missed Detection
// ══════════════════════════════════════════════════

/// Get persistent cron tasks with timezone support
#[tauri::command]
pub fn get_persistent_cron_tasks(
    db: State<'_, Arc<DbManager>>,
) -> Result<Vec<serde_json::Value>, String> {
    let conn = db.get_connection().map_err(|e: rusqlite::Error| e.to_string())?;
    let mut stmt = conn.prepare(
        "SELECT id, name, schedule, timezone, agent_name, prompt_template, mode, keep_awake, enabled, last_run_at, next_run_at FROM cron_tasks_persistent ORDER BY created_at DESC"
    ).map_err(|e: rusqlite::Error| e.to_string())?;
    let rows = stmt.query_map([], |row| {
        Ok(serde_json::json!({
            "id": row.get::<_, String>(0)?,
            "name": row.get::<_, String>(1)?,
            "schedule": row.get::<_, String>(2)?,
            "timezone": row.get::<_, String>(3)?,
            "agent_name": row.get::<_, Option<String>>(4)?,
            "prompt_template": row.get::<_, Option<String>>(5)?,
            "mode": row.get::<_, String>(6)?,
            "keep_awake": row.get::<_, i32>(7)? != 0,
            "enabled": row.get::<_, i32>(8)? != 0,
            "last_run_at": row.get::<_, Option<String>>(9)?,
            "next_run_at": row.get::<_, Option<String>>(10)?,
        }))
    }).map_err(|e: rusqlite::Error| e.to_string())?;
    Ok(rows.flatten().collect())
}

/// Create a persistent cron task
#[tauri::command]
pub fn create_persistent_cron(
    name: String,
    schedule: String,
    timezone: Option<String>,
    agent_name: Option<String>,
    prompt_template: Option<String>,
    mode: Option<String>,
    keep_awake: Option<bool>,
    db: State<'_, Arc<DbManager>>,
) -> Result<String, String> {
    let conn = db.get_connection().map_err(|e: rusqlite::Error| e.to_string())?;
    let id = format!("pcron_{}", chrono::Utc::now().timestamp_millis());
    conn.execute(
        "INSERT INTO cron_tasks_persistent (id, name, schedule, timezone, agent_name, prompt_template, mode, keep_awake) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            id, name, schedule,
            timezone.unwrap_or_else(|| "UTC".into()),
            agent_name,
            prompt_template,
            mode.unwrap_or_else(|| "new_conversation".into()),
            if keep_awake.unwrap_or(false) { 1 } else { 0 },
        ],
    ).map_err(|e: rusqlite::Error| e.to_string())?;
    Ok(id)
}

/// Delete a persistent cron task
#[tauri::command]
pub fn delete_persistent_cron(
    task_id: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    let conn = db.get_connection().map_err(|e: rusqlite::Error| e.to_string())?;
    conn.execute("DELETE FROM cron_tasks_persistent WHERE id = ?1", params![task_id])
        .map_err(|e: rusqlite::Error| e.to_string())?;
    Ok(())
}

// ══════════════════════════════════════════════════
// Skill Rule Generator
// ══════════════════════════════════════════════════

// ══════════════════════════════════════════════════
// Conversation Skills Indicator
// ══════════════════════════════════════════════════

/// Get skills loaded in a specific conversation
#[tauri::command]
pub fn get_conversation_skills(
    conversation_id: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<Vec<serde_json::Value>, String> {
    let conn = db.get_connection().map_err(|e: rusqlite::Error| e.to_string())?;

    // Get skills that are relevant to this conversation's workspace
    let _workspace: Option<String> = conn.query_row(
        "SELECT workspace_path FROM conversations WHERE id = ?1",
        params![conversation_id],
        |r| r.get(0),
    ).ok();

    // Get active skills
    let mut stmt = conn.prepare(
        "SELECT name, description, category, usage_count, priority_score FROM skills WHERE is_active = 1 ORDER BY priority_score DESC, usage_count DESC"
    ).map_err(|e: rusqlite::Error| e.to_string())?;

    let skills: Vec<serde_json::Value> = stmt.query_map([], |row| {
        Ok(serde_json::json!({
            "name": row.get::<_, String>(0)?,
            "description": row.get::<_, String>(1)?,
            "category": row.get::<_, Option<String>>(2)?,
            "usage_count": row.get::<_, i32>(3)?,
            "priority_score": row.get::<_, f64>(4)?,
        }))
    }).map_err(|e: rusqlite::Error| e.to_string())?
        .flatten()
        .collect();

    Ok(skills)
}

// 工具审批队列（已移除）
//
// 这里曾有 tool_confirmations 表 + 四个命令（queue/resolve/get_pending/count）。
// 四个都注册了、前端也定义了 API 包装，但**两边都没有任何地方调用过**——
// 表永远是空的，读它的 UI 也不存在。
//
// 留着比删掉更危险：它让人以为「OMNIX 有工具审批」，而实际上 OMNIX 是把
// agent CLI 当子进程拉起来的，工具审批完全由那些 CLI 自己在内部处理，
// 网关看不见也管不着。要真做拦截式审批得改网关（解析响应里的 tool_use、
// 拦住、等人批），那是另一件事，不是补几个函数。
//
// 建库语句一并从 db_schema 移除；已存在的空表不动——删表是不可逆的，
// 留一张空表无害。

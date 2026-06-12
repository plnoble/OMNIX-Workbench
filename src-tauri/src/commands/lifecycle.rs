use tauri::State;
use std::sync::Arc;
use rusqlite::params;
use crate::db::DbManager;
use super::*;

// ══════════════════════════════════════════════════
// Request Logs & Usage Stats (New API/Sub2API inspired)
// ══════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestLogEntry {
    pub id: i64,
    pub timestamp: String,
    pub model: String,
    pub platform: String,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub total_tokens: i64,
    pub latency_ms: i64,
    pub status_code: i32,
    pub is_stream: bool,
    pub is_error: bool,
    pub error_message: String,
    pub request_id: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageStats {
    pub total_requests: i64,
    pub total_tokens: i64,
    pub total_errors: i64,
    pub avg_latency_ms: f64,
    pub requests_today: i64,
    pub tokens_today: i64,
    pub top_models: Vec<ModelUsage>,
    pub hourly_distribution: Vec<HourlyCount>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelUsage {
    pub model: String,
    pub request_count: i64,
    pub total_tokens: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HourlyCount {
    pub hour: String,
    pub count: i64,
}

/// Get request logs with pagination and optional model filter
#[tauri::command]
pub fn get_request_logs(
    page: Option<u32>,
    limit: Option<u32>,
    model_filter: Option<String>,
    db: State<'_, Arc<DbManager>>,
) -> Result<Vec<RequestLogEntry>, String> {
    let conn = db.get_connection().map_err(|e: rusqlite::Error| e.to_string())?;
    let page = page.unwrap_or(1).max(1);
    let limit = limit.unwrap_or(50).min(200);
    let offset = (page - 1) * limit;

    let (sql, query_params): (String, Vec<Box<dyn rusqlite::types::ToSql>>) = if let Some(ref model) = model_filter {
        (
            format!("SELECT id, timestamp, model, platform, prompt_tokens, completion_tokens, total_tokens, latency_ms, status_code, is_stream, is_error, error_message, request_id, source FROM request_logs WHERE model LIKE ?1 ORDER BY id DESC LIMIT ?2 OFFSET ?3"),
            vec![Box::new(format!("%{}%", model)), Box::new(limit), Box::new(offset)],
        )
    } else {
        (
            "SELECT id, timestamp, model, platform, prompt_tokens, completion_tokens, total_tokens, latency_ms, status_code, is_stream, is_error, error_message, request_id, source FROM request_logs ORDER BY id DESC LIMIT ?1 OFFSET ?2".to_string(),
            vec![Box::new(limit), Box::new(offset)],
        )
    };

    let mut stmt = conn.prepare(&sql).map_err(|e: rusqlite::Error| e.to_string())?;
    let param_refs: Vec<&dyn rusqlite::types::ToSql> = query_params.iter().map(|p| p.as_ref()).collect();
    let rows = stmt.query_map(param_refs.as_slice(), |row| {
        Ok(RequestLogEntry {
            id: row.get(0)?,
            timestamp: row.get(1)?,
            model: row.get(2)?,
            platform: row.get(3)?,
            prompt_tokens: row.get(4)?,
            completion_tokens: row.get(5)?,
            total_tokens: row.get(6)?,
            latency_ms: row.get(7)?,
            status_code: row.get(8)?,
            is_stream: row.get::<_, i32>(9)? != 0,
            is_error: row.get::<_, i32>(10)? != 0,
            error_message: row.get(11)?,
            request_id: row.get(12)?,
            source: row.get(13)?,
        })
    }).map_err(|e: rusqlite::Error| e.to_string())?;

    let mut result = Vec::new();
    for r in rows.flatten() { result.push(r); }
    Ok(result)
}

/// Get usage statistics summary
#[tauri::command]
pub fn get_usage_stats(db: State<'_, Arc<DbManager>>) -> Result<UsageStats, String> {
    let conn = db.get_connection().map_err(|e: rusqlite::Error| e.to_string())?;

    // Total stats
    let total_requests: i64 = conn.query_row("SELECT COUNT(*) FROM request_logs", [], |r| r.get(0)).unwrap_or(0);
    let total_tokens: i64 = conn.query_row("SELECT COALESCE(SUM(total_tokens), 0) FROM request_logs", [], |r| r.get(0)).unwrap_or(0);
    let total_errors: i64 = conn.query_row("SELECT COUNT(*) FROM request_logs WHERE is_error = 1", [], |r| r.get(0)).unwrap_or(0);
    let avg_latency: f64 = conn.query_row("SELECT COALESCE(AVG(latency_ms), 0) FROM request_logs", [], |r| r.get(0)).unwrap_or(0.0);

    // Today's stats
    let requests_today: i64 = conn.query_row("SELECT COUNT(*) FROM request_logs WHERE date(timestamp) = date('now')", [], |r| r.get(0)).unwrap_or(0);
    let tokens_today: i64 = conn.query_row("SELECT COALESCE(SUM(total_tokens), 0) FROM request_logs WHERE date(timestamp) = date('now')", [], |r| r.get(0)).unwrap_or(0);

    // Top models
    let mut stmt = conn.prepare("SELECT model, COUNT(*) as cnt, SUM(total_tokens) as tokens FROM request_logs GROUP BY model ORDER BY cnt DESC LIMIT 10").map_err(|e| e.to_string())?;
    let top_models: Vec<ModelUsage> = stmt.query_map([], |row| {
        Ok(ModelUsage {
            model: row.get(0)?,
            request_count: row.get(1)?,
            total_tokens: row.get(2)?,
        })
    }).map_err(|e| e.to_string())?.flatten().collect();

    // Hourly distribution (last 24h)
    let mut stmt = conn.prepare("SELECT strftime('%H:00', timestamp) as hour, COUNT(*) FROM request_logs WHERE timestamp >= datetime('now', '-24 hours') GROUP BY hour ORDER BY hour").map_err(|e| e.to_string())?;
    let hourly_distribution: Vec<HourlyCount> = stmt.query_map([], |row| {
        Ok(HourlyCount {
            hour: row.get(0)?,
            count: row.get(1)?,
        })
    }).map_err(|e| e.to_string())?.flatten().collect();

    Ok(UsageStats {
        total_requests,
        total_tokens,
        total_errors,
        avg_latency_ms: avg_latency,
        requests_today,
        tokens_today,
        top_models,
        hourly_distribution,
    })
}

/// Delete old request logs (cleanup)
#[tauri::command]
pub fn cleanup_request_logs(
    keep_days: Option<u32>,
    db: State<'_, Arc<DbManager>>,
) -> Result<usize, String> {
    let days = keep_days.unwrap_or(30);
    let conn = db.get_connection().map_err(|e: rusqlite::Error| e.to_string())?;
    let deleted = conn.execute(
        "DELETE FROM request_logs WHERE timestamp < datetime('now', ?1)",
        params![format!("-{} days", days)],
    ).map_err(|e: rusqlite::Error| e.to_string())?;
    Ok(deleted)
}

// ══════════════════════════════════════════════════
// Platform Health Management (New API/Sub2API inspired)
// ══════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformHealth {
    pub id: String,
    pub name: String,
    pub api_type: String,
    pub is_enabled: bool,
    pub is_healthy: bool,
    pub weight: i32,
    pub priority: i32,
    pub consecutive_failures: i32,
    pub last_error: Option<String>,
    pub last_used_at: Option<String>,
    pub model_count: i64,
}

/// Get health status of all platforms
#[tauri::command]
pub fn get_platform_health(db: State<'_, Arc<DbManager>>) -> Result<Vec<PlatformHealth>, String> {
    let conn = db.get_connection().map_err(|e: rusqlite::Error| e.to_string())?;
    let mut stmt = conn.prepare(
        "SELECT mp.id, mp.name, mp.api_type, mp.is_enabled, mp.is_healthy,
                mp.weight, mp.priority, mp.consecutive_failures, mp.last_error, mp.last_used_at,
                (SELECT COUNT(*) FROM platform_models pm WHERE pm.platform_id = mp.id) as model_count
         FROM model_platforms mp ORDER BY mp.priority DESC, mp.weight DESC"
    ).map_err(|e: rusqlite::Error| e.to_string())?;

    let rows = stmt.query_map([], |row| {
        Ok(PlatformHealth {
            id: row.get(0)?,
            name: row.get(1)?,
            api_type: row.get(2)?,
            is_enabled: row.get::<_, i32>(3)? != 0,
            is_healthy: row.get::<_, i32>(4)? != 0,
            weight: row.get(5)?,
            priority: row.get(6)?,
            consecutive_failures: row.get(7)?,
            last_error: row.get(8)?,
            last_used_at: row.get(9)?,
            model_count: row.get(10)?,
        })
    }).map_err(|e: rusqlite::Error| e.to_string())?;

    let mut result = Vec::new();
    for r in rows.flatten() { result.push(r); }
    Ok(result)
}

/// Reset a platform's health status (mark as healthy)
#[tauri::command]
pub fn reset_platform_health(
    platform_id: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    let conn = db.get_connection().map_err(|e: rusqlite::Error| e.to_string())?;
    conn.execute(
        "UPDATE model_platforms SET is_healthy = 1, consecutive_failures = 0, last_error = NULL WHERE id = ?1",
        params![platform_id],
    ).map_err(|e: rusqlite::Error| e.to_string())?;
    Ok(())
}

/// Update platform weight and priority
#[tauri::command]
pub fn update_platform_routing(
    platform_id: String,
    weight: i32,
    priority: i32,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    let conn = db.get_connection().map_err(|e: rusqlite::Error| e.to_string())?;
    conn.execute(
        "UPDATE model_platforms SET weight = ?1, priority = ?2 WHERE id = ?3",
        params![weight.max(1).min(100), priority.max(0).min(100), platform_id],
    ).map_err(|e: rusqlite::Error| e.to_string())?;
    Ok(())
}

// ══════════════════════════════════════════════════
// Upstream Model Auto-Sync (New API inspired)
// ══════════════════════════════════════════════════

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpstreamModel {
    pub id: String,
    pub owned_by: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelSyncResult {
    pub platform_id: String,
    pub platform_name: String,
    pub upstream_models: Vec<String>,
    pub local_models: Vec<String>,
    pub new_models: Vec<String>,
    pub removed_models: Vec<String>,
    pub unchanged_models: Vec<String>,
    pub error: Option<String>,
}

/// Fetch models from a single upstream platform
async fn fetch_upstream_models(api_address: &str, api_key: &str, api_type: &str) -> Result<Vec<String>, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;

    let models_url = if api_type == "ollama" {
        format!("{}/api/tags", api_address.trim_end_matches('/'))
    } else {
        format!("{}/v1/models", api_address.trim_end_matches('/'))
    };

    let mut req = client.get(&models_url);
    if !api_key.is_empty() && api_type != "ollama" {
        req = req.header("Authorization", format!("Bearer {}", api_key));
    }

    let res = req.send().await.map_err(|e| format!("Request failed: {}", e))?;
    if !res.status().is_success() {
        return Err(format!("HTTP {}", res.status()));
    }

    let body: serde_json::Value = res.json().await.map_err(|e| format!("Parse failed: {}", e))?;

    let mut models = Vec::new();

    if api_type == "ollama" {
        // Ollama: { "models": [{ "name": "llama3" }, ...] }
        if let Some(arr) = body["models"].as_array() {
            for m in arr {
                if let Some(name) = m["name"].as_str() {
                    models.push(name.to_string());
                }
            }
        }
    } else {
        // OpenAI-compatible: { "data": [{ "id": "gpt-4o", "owned_by": "openai" }, ...] }
        if let Some(arr) = body["data"].as_array() {
            for m in arr {
                if let Some(id) = m["id"].as_str() {
                    models.push(id.to_string());
                }
            }
        }
    }

    Ok(models)
}

/// Internal: sync upstream models for a single platform (shared logic)
async fn sync_upstream_models_internal(
    platform_id: &str,
    db: &std::sync::Arc<DbManager>,
) -> Result<ModelSyncResult, String> {
    let conn = db.get_connection().map_err(|e: rusqlite::Error| e.to_string())?;

    // Get platform config
    let (name, api_type, api_key, api_address): (String, String, String, String) = conn.query_row(
        "SELECT name, api_type, api_key, api_address FROM model_platforms WHERE id = ?1",
        params![platform_id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
    ).map_err(|e| format!("Platform not found: {}", e))?;

    // Fetch upstream models
    let upstream_models = match fetch_upstream_models(&api_address, &api_key, &api_type).await {
        Ok(models) => models,
        Err(e) => {
            return Ok(ModelSyncResult {
                platform_id: platform_id.to_string(),
                platform_name: name,
                upstream_models: vec![],
                local_models: vec![],
                new_models: vec![],
                removed_models: vec![],
                unchanged_models: vec![],
                error: Some(e),
            });
        }
    };

    // Get local models for this platform
    let mut stmt = conn.prepare("SELECT model_name FROM platform_models WHERE platform_id = ?1")
        .map_err(|e: rusqlite::Error| e.to_string())?;
    let local_models: Vec<String> = stmt.query_map(params![platform_id], |r| r.get(0))
        .map_err(|e: rusqlite::Error| e.to_string())?
        .flatten()
        .collect();

    // Compare
    let upstream_set: std::collections::HashSet<&String> = upstream_models.iter().collect();
    let local_set: std::collections::HashSet<&String> = local_models.iter().collect();

    let new_models: Vec<String> = upstream_models.iter()
        .filter(|m| !local_set.contains(m))
        .cloned()
        .collect();

    let removed_models: Vec<String> = local_models.iter()
        .filter(|m| !upstream_set.contains(m))
        .cloned()
        .collect();

    let unchanged_models: Vec<String> = upstream_models.iter()
        .filter(|m| local_set.contains(m))
        .cloned()
        .collect();

    Ok(ModelSyncResult {
        platform_id: platform_id.to_string(),
        platform_name: name,
        upstream_models,
        local_models,
        new_models,
        removed_models,
        unchanged_models,
        error: None,
    })
}

/// Apply model sync: add new models, optionally remove missing ones
#[tauri::command]
pub fn apply_model_sync(
    platform_id: String,
    models_to_add: Vec<String>,
    models_to_remove: Vec<String>,
    db: State<'_, std::sync::Arc<DbManager>>,
) -> Result<(usize, usize), String> {
    let conn = db.get_connection().map_err(|e: rusqlite::Error| e.to_string())?;

    let mut added = 0;
    for model_name in &models_to_add {
        let id = format!("{}:{}", platform_id, model_name);
        let result = conn.execute(
            "INSERT OR IGNORE INTO platform_models (id, platform_id, model_name, is_enabled) VALUES (?1, ?2, ?3, 1)",
            params![id, platform_id, model_name],
        );
        if result.unwrap_or(0) > 0 { added += 1; }
    }

    let mut removed = 0;
    for model_name in &models_to_remove {
        let id = format!("{}:{}", platform_id, model_name);
        let result = conn.execute(
            "DELETE FROM platform_models WHERE id = ?1",
            params![id],
        );
        if result.unwrap_or(0) > 0 { removed += 1; }
    }

    Ok((added, removed))
}

/// Sync upstream models for a single platform (tauri command wrapper)
#[tauri::command]
pub async fn sync_upstream_models(
    platform_id: String,
    db: State<'_, std::sync::Arc<DbManager>>,
) -> Result<ModelSyncResult, String> {
    sync_upstream_models_internal(&platform_id, &db).await
}

/// Sync all enabled platforms at once
#[tauri::command]
pub async fn sync_all_upstream_models(
    db: State<'_, std::sync::Arc<DbManager>>,
) -> Result<Vec<ModelSyncResult>, String> {
    // Collect platform IDs first, then drop the statement (avoids Send issue with rusqlite Statement)
    let platform_ids: Vec<String> = {
        let conn = db.get_connection().map_err(|e: rusqlite::Error| e.to_string())?;
        let mut stmt = conn.prepare("SELECT id FROM model_platforms WHERE is_enabled = 1")
            .map_err(|e: rusqlite::Error| e.to_string())?;
        let ids: Vec<String> = stmt.query_map([], |r| r.get(0))
            .map_err(|e: rusqlite::Error| e.to_string())?
            .flatten()
            .collect();
        ids
    };

    let mut results = Vec::new();
    for pid in platform_ids {
        match sync_upstream_models_internal(&pid, &db).await {
            Ok(r) => results.push(r),
            Err(e) => results.push(ModelSyncResult {
                platform_id: pid,
                platform_name: "unknown".into(),
                upstream_models: vec![],
                local_models: vec![],
                new_models: vec![],
                removed_models: vec![],
                unchanged_models: vec![],
                error: Some(e),
            }),
        }
    }

    Ok(results)
}

// ══════════════════════════════════════════════════
// Platform Health Check (New API/Sub2API inspired)
// ══════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckResult {
    pub platform_id: String,
    pub platform_name: String,
    pub is_reachable: bool,
    pub latency_ms: i64,
    pub model_count: i64,
    pub error: Option<String>,
}

/// Check health of all enabled platforms
#[tauri::command]
pub async fn check_all_platform_health(
    db: State<'_, std::sync::Arc<DbManager>>,
) -> Result<Vec<HealthCheckResult>, String> {
    let platforms: Vec<(String, String, String, String, String)> = {
        let conn = db.get_connection().map_err(|e: rusqlite::Error| e.to_string())?;
        let mut stmt = conn.prepare(
            "SELECT id, name, api_address, api_key, api_type FROM model_platforms WHERE is_enabled = 1"
        ).map_err(|e: rusqlite::Error| e.to_string())?;
        let rows: Vec<(String, String, String, String, String)> = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        }).map_err(|e: rusqlite::Error| e.to_string())?
            .flatten()
            .collect();
        rows
    };

    let mut results = Vec::new();
    for (id, name, address, key, api_type) in platforms {
        let start = std::time::Instant::now();
        let url = if api_type == "ollama" {
            format!("{}/api/tags", address.trim_end_matches('/'))
        } else {
            format!("{}/v1/models", address.trim_end_matches('/'))
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build();
        let (is_reachable, model_count, error) = match client {
            Ok(c) => {
                let mut req = c.get(&url);
                if !key.is_empty() && api_type != "ollama" {
                    req = req.header("Authorization", format!("Bearer {}", key));
                }
                match req.send().await {
                    Ok(res) => {
                        let status = res.status();
                        let ok = status.is_success();
                        let count = if ok {
                            match res.json::<serde_json::Value>().await {
                                Ok(body) => {
                                    if api_type == "ollama" {
                                        body["models"].as_array().map(|a| a.len() as i64).unwrap_or(0)
                                    } else {
                                        body["data"].as_array().map(|a| a.len() as i64).unwrap_or(0)
                                    }
                                }
                                Err(_) => 0,
                            }
                        } else { 0 };
                        (ok, count, if ok { None } else { Some(format!("HTTP {}", status)) })
                    }
                    Err(e) => (false, 0, Some(e.to_string())),
                }
            }
            Err(e) => (false, 0, Some(e.to_string())),
        };

        let latency_ms = start.elapsed().as_millis() as i64;

        // Update health status in DB (synchronous, within the same connection)
        {
            let conn = db.get_connection();
            if let Ok(conn) = conn {
                if is_reachable {
                    let _ = conn.execute(
                        "UPDATE model_platforms SET is_healthy = 1, consecutive_failures = 0, last_error = NULL WHERE id = ?1",
                        params![id],
                    );
                } else {
                    let err_msg = error.clone().unwrap_or_default();
                    let _ = conn.execute(
                        "UPDATE model_platforms SET consecutive_failures = consecutive_failures + 1, last_error = ?1 WHERE id = ?2",
                        params![err_msg, id],
                    );
                    let _ = conn.execute(
                        "UPDATE model_platforms SET is_healthy = 0 WHERE id = ?1 AND consecutive_failures >= 5",
                        params![id],
                    );
                }
            }
        }

        results.push(HealthCheckResult {
            platform_id: id,
            platform_name: name,
            is_reachable,
            latency_ms,
            model_count,
            error,
        });
    }

    Ok(results)
}

// ══════════════════════════════════════════════════
// Agent Task Lifecycle (Multica inspired)
// ══════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskInfo {
    pub id: String,
    pub title: String,
    pub active_agent: String,
    pub workspace_path: String,
    pub task_status: String,
    pub task_started_at: Option<String>,
    pub task_completed_at: Option<String>,
    pub task_duration_ms: Option<i64>,
    pub task_summary: Option<String>,
    pub task_files_changed: i32,
    pub task_exit_code: Option<i32>,
    pub is_archived: bool,
    pub created_at: String,
}

/// Get all tasks with lifecycle info
#[tauri::command]
pub fn get_task_list(
    include_archived: bool,
    db: State<'_, Arc<DbManager>>,
) -> Result<Vec<TaskInfo>, String> {
    let conn = db.get_connection().map_err(|e: rusqlite::Error| e.to_string())?;
    let sql = if include_archived {
        "SELECT id, title, active_agent, workspace_path, task_status, task_started_at, task_completed_at, task_duration_ms, task_summary, task_files_changed, task_exit_code, is_archived, created_at FROM conversations ORDER BY created_at DESC"
    } else {
        "SELECT id, title, active_agent, workspace_path, task_status, task_started_at, task_completed_at, task_duration_ms, task_summary, task_files_changed, task_exit_code, is_archived, created_at FROM conversations WHERE is_archived = 0 ORDER BY created_at DESC"
    };

    let mut stmt = conn.prepare(sql).map_err(|e: rusqlite::Error| e.to_string())?;
    let rows = stmt.query_map([], |row| {
        Ok(TaskInfo {
            id: row.get(0)?,
            title: row.get(1)?,
            active_agent: row.get(2)?,
            workspace_path: row.get(3)?,
            task_status: row.get(4)?,
            task_started_at: row.get(5)?,
            task_completed_at: row.get(6)?,
            task_duration_ms: row.get(7)?,
            task_summary: row.get(8)?,
            task_files_changed: row.get(9)?,
            task_exit_code: row.get(10)?,
            is_archived: row.get::<_, i32>(11)? != 0,
            created_at: row.get(12)?,
        })
    }).map_err(|e: rusqlite::Error| e.to_string())?;

    let mut result = Vec::new();
    for r in rows.flatten() { result.push(r); }
    Ok(result)
}

/// Transition task status: pending → running
#[tauri::command]
pub fn task_start(
    conversation_id: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    let conn = db.get_connection().map_err(|e: rusqlite::Error| e.to_string())?;
    conn.execute(
        "UPDATE conversations SET task_status = 'running', task_started_at = datetime('now') WHERE id = ?1 AND task_status IN ('pending', 'failed')",
        params![conversation_id],
    ).map_err(|e: rusqlite::Error| e.to_string())?;
    Ok(())
}

/// Transition task status: running → completed
#[tauri::command]
pub fn task_complete(
    conversation_id: String,
    summary: Option<String>,
    files_changed: Option<i32>,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    let conn = db.get_connection().map_err(|e: rusqlite::Error| e.to_string())?;
    // Calculate duration from task_started_at
    conn.execute(
        "UPDATE conversations SET task_status = 'completed', task_completed_at = datetime('now'), task_duration_ms = CAST((julianday('now') - julianday(task_started_at)) * 86400000 AS INTEGER), task_summary = ?2, task_files_changed = ?3 WHERE id = ?1 AND task_status = 'running'",
        params![conversation_id, summary, files_changed.unwrap_or(0)],
    ).map_err(|e: rusqlite::Error| e.to_string())?;
    Ok(())
}

/// Transition task status: running → failed
#[tauri::command]
pub fn task_fail(
    conversation_id: String,
    exit_code: Option<i32>,
    error_summary: Option<String>,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    let conn = db.get_connection().map_err(|e: rusqlite::Error| e.to_string())?;
    conn.execute(
        "UPDATE conversations SET task_status = 'failed', task_completed_at = datetime('now'), task_duration_ms = CAST((julianday('now') - julianday(task_started_at)) * 86400000 AS INTEGER), task_exit_code = ?2, task_summary = ?3 WHERE id = ?1 AND task_status = 'running'",
        params![conversation_id, exit_code, error_summary],
    ).map_err(|e: rusqlite::Error| e.to_string())?;
    Ok(())
}

/// Archive a completed/failed task
#[tauri::command]
pub fn task_archive(
    conversation_id: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    let conn = db.get_connection().map_err(|e: rusqlite::Error| e.to_string())?;
    conn.execute(
        "UPDATE conversations SET is_archived = 1 WHERE id = ?1",
        params![conversation_id],
    ).map_err(|e: rusqlite::Error| e.to_string())?;
    Ok(())
}

/// Get task statistics summary
#[tauri::command]
pub fn get_task_stats(db: State<'_, Arc<DbManager>>) -> Result<serde_json::Value, String> {
    let conn = db.get_connection().map_err(|e: rusqlite::Error| e.to_string())?;

    let total: i64 = conn.query_row("SELECT COUNT(*) FROM conversations WHERE is_archived = 0", [], |r| r.get(0)).unwrap_or(0);
    let running: i64 = conn.query_row("SELECT COUNT(*) FROM conversations WHERE task_status = 'running' AND is_archived = 0", [], |r| r.get(0)).unwrap_or(0);
    let completed: i64 = conn.query_row("SELECT COUNT(*) FROM conversations WHERE task_status = 'completed' AND is_archived = 0", [], |r| r.get(0)).unwrap_or(0);
    let failed: i64 = conn.query_row("SELECT COUNT(*) FROM conversations WHERE task_status = 'failed' AND is_archived = 0", [], |r| r.get(0)).unwrap_or(0);
    let avg_duration: f64 = conn.query_row("SELECT COALESCE(AVG(task_duration_ms), 0) FROM conversations WHERE task_status = 'completed' AND task_duration_ms IS NOT NULL", [], |r| r.get(0)).unwrap_or(0.0);

    Ok(serde_json::json!({
        "total": total,
        "running": running,
        "completed": completed,
        "failed": failed,
        "avg_duration_ms": avg_duration,
    }))
}

// ══════════════════════════════════════════════════
// Skill Compound Interest System (Multica inspired)
// ══════════════════════════════════════════════════

/// Record a skill usage (compound interest: usage_count++, priority_score increases)
#[tauri::command]
pub fn record_skill_usage(
    skill_name: String,
    success: bool,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    let conn = db.get_connection().map_err(|e: rusqlite::Error| e.to_string())?;

    conn.execute(
        "UPDATE skills SET usage_count = usage_count + 1, last_used_at = datetime('now') WHERE name = ?1",
        params![skill_name],
    ).map_err(|e: rusqlite::Error| e.to_string())?;

    if success {
        conn.execute(
            "UPDATE skills SET success_count = success_count + 1, priority_score = priority_score + 0.1 WHERE name = ?1",
            params![skill_name],
        ).map_err(|e: rusqlite::Error| e.to_string())?;
    } else {
        conn.execute(
            "UPDATE skills SET priority_score = MAX(0.1, priority_score - 0.05) WHERE name = ?1",
            params![skill_name],
        ).map_err(|e: rusqlite::Error| e.to_string())?;
    }

    Ok(())
}

/// Get top skills by usage (compound interest ranking)
#[tauri::command]
pub fn get_top_skills_by_usage(
    limit: Option<u32>,
    db: State<'_, Arc<DbManager>>,
) -> Result<Vec<serde_json::Value>, String> {
    let limit = limit.unwrap_or(10);
    let conn = db.get_connection().map_err(|e: rusqlite::Error| e.to_string())?;
    let mut stmt = conn.prepare(
        "SELECT name, description, category, usage_count, success_count, priority_score, starred
         FROM skills WHERE usage_count > 0
         ORDER BY priority_score DESC, usage_count DESC
         LIMIT ?1"
    ).map_err(|e: rusqlite::Error| e.to_string())?;

    let rows = stmt.query_map(params![limit], |row| {
        Ok(serde_json::json!({
            "name": row.get::<_, String>(0)?,
            "description": row.get::<_, String>(1)?,
            "category": row.get::<_, Option<String>>(2)?,
            "usage_count": row.get::<_, i32>(3)?,
            "success_count": row.get::<_, i32>(4)?,
            "priority_score": row.get::<_, f64>(5)?,
            "starred": row.get::<_, i32>(6)? != 0,
        }))
    }).map_err(|e: rusqlite::Error| e.to_string())?;

    let mut result = Vec::new();
    for r in rows.flatten() { result.push(r); }
    Ok(result)
}

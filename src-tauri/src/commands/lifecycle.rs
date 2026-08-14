use tauri::State;
use std::sync::Arc;
use rusqlite::params;
use crate::db::DbManager;
use super::*;

/// Enable/disable remote phone access. When ON, the proxy binds to 0.0.0.0 so a
/// phone on the LAN (or via the user's own tunnel) can reach `/remote`; OFF keeps
/// it localhost-only. Restarts the proxy so the bind change applies immediately.
#[tauri::command]
pub async fn set_remote_access(
    enabled: bool,
    proxy: State<'_, std::sync::Mutex<crate::proxy::ProxyServer>>,
    db: State<'_, Arc<DbManager>>,
    agent_manager: State<'_, Arc<crate::agent::AgentManager>>,
    runtime_manager: State<'_, Arc<crate::runtime_manager::RuntimeManager>>,
) -> Result<(), String> {
    db.set_setting("remote_access_enabled", if enabled { "true" } else { "false" })
        .map_err(|e| e.to_string())?;
    let port: u16 = db
        .get_setting("proxy_port")
        .unwrap_or(None)
        .and_then(|s| s.parse().ok())
        .unwrap_or(1421);
    {
        let mut server = proxy.lock().map_err(|e| format!("proxy lock: {e}"))?;
        server.stop();
    }
    // Let the old listener release the port before re-binding.
    tokio::time::sleep(std::time::Duration::from_millis(800)).await;
    {
        let mut server = proxy.lock().map_err(|e| format!("proxy lock: {e}"))?;
        server.start(
            Arc::clone(&db),
            Arc::clone(&agent_manager),
            Arc::clone(&runtime_manager),
            port,
        );
    }
    Ok(())
}

/// Rotate the remote-access token: mint a fresh CSPRNG token and persist it.
/// Same no-fallback stance as first-boot generation — if the OS CSPRNG fails we
/// error out.
///
/// 这是**一键踢掉所有设备**：会话 Cookie 是用这个令牌签的，换掉密钥，已经配对的
/// 手机下一次请求就验不过；没用掉的配对码也一并作废。UI 上「旧链接与二维码全部
/// 失效」这句话，现在对已连上的设备也成立。
#[tauri::command]
pub fn rotate_remote_token(db: State<'_, Arc<DbManager>>) -> Result<String, String> {
    let mut bytes = [0u8; 32];
    getrandom::getrandom(&mut bytes).map_err(|e| format!("CSPRNG (getrandom) unavailable: {e}"))?;
    let hex: String = bytes.iter().map(|b| format!("{:02x}", b)).collect();
    let token = format!("tok_{hex}");
    db.set_setting("remote_token", &token).map_err(|e| e.to_string())?;
    crate::remote_session::clear_codes();
    Ok(token)
}

/// Devices that recently authenticated against the remote panel (`/remote`,
/// `/api/remote/*`). Tracked in-memory by the proxy middleware; restarting the
/// app clears the list.
#[tauri::command]
pub fn get_remote_clients() -> Vec<crate::proxy::RemoteClientInfo> {
    crate::proxy::remote_clients_snapshot()
}

// ══════════════════════════════════════════════════
// Request Logs & Usage Stats
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
    pub total_cost_usd: f64,
    pub cost_today_usd: f64,
    pub top_models: Vec<ModelUsage>,
    pub hourly_distribution: Vec<HourlyCount>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelUsage {
    pub model: String,
    pub request_count: i64,
    pub total_tokens: i64,
    pub cost_usd: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HourlyCount {
    pub hour: String,
    pub count: i64,
}

/// One day's aggregated token/cost activity (for the activity chart).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyUsage {
    pub date: String,
    pub requests: i64,
    pub tokens: i64,
    pub cost_usd: f64,
}

/// Per-platform usage rollup for the cost dashboard's by-platform breakdown.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformUsage {
    pub platform: String,
    pub request_count: i64,
    pub total_tokens: i64,
    pub error_count: i64,
    pub cost_usd: f64,
}

/// 成本口径的「等价普通输入 token」。
///
/// `prompt_tokens` 存的是**真实 token 数**（含缓存命中与写入），但缓存不按原价
/// 计费：Anthropic 的口径是命中读取 0.1×、写入 1.25×。直接拿 prompt_tokens 去乘
/// 输入单价，会把 Claude Code 这类高缓存命中的用量高报好几倍——那种「看起来
/// 很合理但是错的」数字比原来的零更难被发现。
///
/// 折算成等价普通输入后即可直接套 [`crate::circuit_breaker::estimate_cost`]，
/// 于是定价表仍然只有一份。`total_tokens` 不折算：它是 token 计数，不是钱。
const BILLED_INPUT: &str = "CAST(ROUND(
        (prompt_tokens - cache_read_tokens - cache_creation_tokens)
        + cache_read_tokens * 0.1
        + cache_creation_tokens * 1.25
    ) AS INTEGER)";

/// Sum estimated cost across every model in a query that yields
/// `(model, SUM(<billed input>), SUM(completion_tokens))` rows.
fn sum_cost(conn: &rusqlite::Connection, sql: &str) -> f64 {
    let mut stmt = match conn.prepare(sql) {
        Ok(stmt) => stmt,
        Err(_) => return 0.0,
    };
    stmt.query_map([], |row| {
        let model: String = row.get(0)?;
        let prompt: i64 = row.get::<_, Option<i64>>(1)?.unwrap_or(0);
        let completion: i64 = row.get::<_, Option<i64>>(2)?.unwrap_or(0);
        Ok(crate::circuit_breaker::estimate_cost(&model, prompt, completion))
    })
    .map(|rows| rows.flatten().sum())
    .unwrap_or(0.0)
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

    // Estimated cost (priced via the model pricing table; unknown models use a
    // default rate). Summed per-model so each model uses its own rate.
    let total_cost_usd = sum_cost(&conn, &format!("SELECT model, SUM({BILLED_INPUT}), SUM(completion_tokens) FROM request_logs GROUP BY model"));
    let cost_today_usd = sum_cost(&conn, &format!("SELECT model, SUM({BILLED_INPUT}), SUM(completion_tokens) FROM request_logs WHERE date(timestamp) = date('now') GROUP BY model"));

    // Top models (with per-model estimated cost)
    let mut stmt = conn.prepare(&format!("SELECT model, COUNT(*) as cnt, SUM(total_tokens) as tokens, SUM({BILLED_INPUT}), SUM(completion_tokens) FROM request_logs GROUP BY model ORDER BY cnt DESC LIMIT 10")).map_err(|e| e.to_string())?;
    let top_models: Vec<ModelUsage> = stmt.query_map([], |row| {
        let model: String = row.get(0)?;
        let prompt: i64 = row.get::<_, Option<i64>>(3)?.unwrap_or(0);
        let completion: i64 = row.get::<_, Option<i64>>(4)?.unwrap_or(0);
        Ok(ModelUsage {
            model: model.clone(),
            request_count: row.get(1)?,
            total_tokens: row.get::<_, Option<i64>>(2)?.unwrap_or(0),
            cost_usd: crate::circuit_breaker::estimate_cost(&model, prompt, completion),
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
        total_cost_usd,
        cost_today_usd,
        top_models,
        hourly_distribution,
    })
}

/// Per-platform usage rollup (requests, tokens, errors, estimated cost). Cost is
/// summed per-model within each platform so each model uses its own rate; the
/// empty platform (direct/unknown upstream) is labelled for display.
#[tauri::command]
pub fn get_platform_usage(db: State<'_, Arc<DbManager>>) -> Result<Vec<PlatformUsage>, String> {
    let conn = db.get_connection().map_err(|e: rusqlite::Error| e.to_string())?;
    let mut stmt = conn
        .prepare(&format!(
            "SELECT platform, model, COUNT(*), SUM(total_tokens), SUM({BILLED_INPUT}),
                    SUM(completion_tokens), SUM(is_error)
             FROM request_logs GROUP BY platform, model",
        ))
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, Option<String>>(0)?.unwrap_or_default(),
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, Option<i64>>(3)?.unwrap_or(0),
                row.get::<_, Option<i64>>(4)?.unwrap_or(0),
                row.get::<_, Option<i64>>(5)?.unwrap_or(0),
                row.get::<_, Option<i64>>(6)?.unwrap_or(0),
            ))
        })
        .map_err(|e| e.to_string())?;

    let mut agg: std::collections::HashMap<String, PlatformUsage> = std::collections::HashMap::new();
    for (platform, model, count, tokens, prompt, completion, errors) in rows.flatten() {
        let key = if platform.trim().is_empty() {
            "(直连/未知)".to_string()
        } else {
            platform
        };
        let entry = agg.entry(key.clone()).or_insert_with(|| PlatformUsage {
            platform: key,
            request_count: 0,
            total_tokens: 0,
            error_count: 0,
            cost_usd: 0.0,
        });
        entry.request_count += count;
        entry.total_tokens += tokens;
        entry.error_count += errors;
        entry.cost_usd += crate::circuit_breaker::estimate_cost(&model, prompt, completion);
    }
    let mut out: Vec<PlatformUsage> = agg.into_values().collect();
    out.sort_by(|a, b| b.cost_usd.partial_cmp(&a.cost_usd).unwrap_or(std::cmp::Ordering::Equal));
    Ok(out)
}

/// Daily token / request / cost activity for the last `days` days (ascending).
/// Days with no traffic are omitted; the frontend fills gaps for the chart.
#[tauri::command]
pub fn get_usage_timeseries(
    days: Option<u32>,
    db: State<'_, Arc<DbManager>>,
) -> Result<Vec<DailyUsage>, String> {
    let days = days.unwrap_or(14).clamp(1, 90);
    let conn = db.get_connection().map_err(|e: rusqlite::Error| e.to_string())?;
    let mut stmt = conn
        .prepare(&format!(
            "SELECT date(timestamp) AS d, model, COUNT(*), SUM(total_tokens), SUM({BILLED_INPUT}), SUM(completion_tokens)
             FROM request_logs
             WHERE timestamp >= datetime('now', ?1)
             GROUP BY d, model
             ORDER BY d ASC",
        ))
        .map_err(|e| e.to_string())?;
    let offset = format!("-{} days", days);

    // Accumulate per-model rows into per-day totals so cost uses each model's rate.
    let mut by_day: std::collections::BTreeMap<String, (i64, i64, f64)> = std::collections::BTreeMap::new();
    let rows = stmt
        .query_map(params![offset], |row| {
            let date: String = row.get(0)?;
            let model: String = row.get(1)?;
            let requests: i64 = row.get(2)?;
            let tokens: i64 = row.get::<_, Option<i64>>(3)?.unwrap_or(0);
            let prompt: i64 = row.get::<_, Option<i64>>(4)?.unwrap_or(0);
            let completion: i64 = row.get::<_, Option<i64>>(5)?.unwrap_or(0);
            Ok((date, requests, tokens, crate::circuit_breaker::estimate_cost(&model, prompt, completion)))
        })
        .map_err(|e| e.to_string())?;
    for row in rows.flatten() {
        let entry = by_day.entry(row.0).or_insert((0, 0, 0.0));
        entry.0 += row.1;
        entry.1 += row.2;
        entry.2 += row.3;
    }

    Ok(by_day
        .into_iter()
        .map(|(date, (requests, tokens, cost_usd))| DailyUsage { date, requests, tokens, cost_usd })
        .collect())
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
// Platform Health Management
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
// Upstream Model Auto-Sync
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
// Platform Health Check
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
// Agent Task Lifecycle
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

/// R2：成本口径。
///
/// `prompt_tokens` 现在记的是含缓存的真实 token 数，若直接按输入单价计费，
/// Claude Code 这类九成输入来自缓存命中的用量会被高报数倍。这里测的就是
/// [`BILLED_INPUT`] 那段折算确实生效了。
#[cfg(test)]
mod cost_weighting_tests {
    use super::*;

    fn conn_with(rows: &[(i64, i64, i64)]) -> rusqlite::Connection {
        let conn = rusqlite::Connection::open_in_memory().expect("内存库");
        conn.execute_batch(
            "CREATE TABLE request_logs (
                model TEXT NOT NULL,
                prompt_tokens INTEGER NOT NULL DEFAULT 0,
                completion_tokens INTEGER NOT NULL DEFAULT 0,
                cache_read_tokens INTEGER NOT NULL DEFAULT 0,
                cache_creation_tokens INTEGER NOT NULL DEFAULT 0
            );",
        )
        .expect("建表");
        for (prompt, cache_read, cache_creation) in rows {
            conn.execute(
                "INSERT INTO request_logs (model, prompt_tokens, completion_tokens, cache_read_tokens, cache_creation_tokens)
                 VALUES ('mystery-model', ?1, 0, ?2, ?3)",
                params![prompt, cache_read, cache_creation],
            )
            .expect("插入");
        }
        conn
    }

    fn cost(conn: &rusqlite::Connection) -> f64 {
        sum_cost(conn, &format!("SELECT model, SUM({BILLED_INPUT}), SUM(completion_tokens) FROM request_logs GROUP BY model"))
    }

    #[test]
    fn cache_hits_are_billed_at_a_tenth_not_full_price() {
        // 两行 token 计数完全相同（都是 1,000,000 输入），只是一行全新、
        // 一行全是缓存命中。按真实计费，后者应当只值前者的十分之一。
        let all_fresh = cost(&conn_with(&[(1_000_000, 0, 0)]));
        let all_cached = cost(&conn_with(&[(1_000_000, 1_000_000, 0)]));
        // 未知模型的默认输入单价 1.0 / 1M。
        assert!((all_fresh - 1.0).abs() < 1e-6, "全新输入按原价：{all_fresh}");
        assert!((all_cached - 0.1).abs() < 1e-6, "缓存命中按 0.1×：{all_cached}");
    }

    #[test]
    fn cache_writes_are_billed_at_a_premium() {
        let all_written = cost(&conn_with(&[(1_000_000, 0, 1_000_000)]));
        assert!((all_written - 1.25).abs() < 1e-6, "缓存写入按 1.25×：{all_written}");
    }

    #[test]
    fn a_realistic_claude_code_request_is_not_overbilled() {
        // 典型形状：真实输入 46,212 个 token，其中 45,000 是缓存命中。
        // 不折算的话会按 46,212 全价算——高报约 8 倍。
        let weighted = cost(&conn_with(&[(46_212, 45_000, 1_200)]));
        let naive = 46_212.0 / 1_000_000.0;
        assert!(weighted < naive / 5.0, "折算后应显著低于全价：{weighted} vs {naive}");
        // 12 + 4500 + 1500 = 6012 等价 token。
        assert!((weighted - 0.006012).abs() < 1e-6, "{weighted}");
    }
}

//! Key 轮换与健康记录，从 proxy.rs 拆出（纯移动）。
//!
//! 一个平台可以挂多个 Key。`send_with_key_failover` 负责「这个被拒就换下一个」，
//! 同时把每次尝试的成败写进 Key 健康表、把最终结果喂给熔断器。
//!
//! 这块曾经有个静默失效：上游把 Key 列表 `.next()` 砍成一个再送进来，于是轮换
//! 逻辑收到的数组长度永远是 1——配了三个 Key 也不会换。修在 f712df2，判据是
//! `proxy_wire_tests.rs::key_failover_tests` 那 3 条（换 Key 成功且按顺序都试过、
//! 全部失败要如实报错、400 不触发轮换）。拆完它们必须原样通过。
//!
//! 作为子模块能看到父模块的私有项，`use super::*;` 把 imports 一并带过来。
#![allow(clippy::module_inception)]

use super::*;

#[derive(Clone, Copy)]
pub(super) enum ApiKeyHeader {
    Anthropic,
    Bearer,
}

#[derive(Clone)]
pub(super) struct KeyHealthContext {
    pub(super) db: Arc<DbManager>,
    pub(super) key_ids: Vec<Option<String>>,
    /// Platform behind this request, for per-platform circuit breaking. `None`
    /// when the upstream isn't an OMNIX-managed platform (e.g. bare relay).
    pub(super) platform_id: Option<String>,
}

pub(super) fn record_key_health(
    context: &KeyHealthContext,
    index: usize,
    status: &str,
    error: Option<&str>,
    latency_ms: i64,
) {
    let Some(Some(key_id)) = context.key_ids.get(index) else {
        return;
    };
    if let Ok(conn) = context.db.get_connection() {
        let _ = conn.execute(
            "UPDATE platform_api_keys
             SET last_status = ?1, last_error = ?2, latency_ms = ?3,
                 last_checked_at = datetime('now')
             WHERE id = ?4",
            params![status, error, latency_ms, key_id],
        );
    }
}

/// Feed a request's final outcome into the platform circuit breaker: a 2xx
/// closes/keeps the circuit healthy, a 5xx or network error trips it toward
/// open. 4xx (auth/rate/bad-request) is a key/client issue — left neutral so a
/// bad key never marks the whole platform down. Called once per request.
pub(super) fn record_circuit_outcome(context: &KeyHealthContext, status: Option<reqwest::StatusCode>, error: Option<&str>) {
    let Some(platform_id) = context.platform_id.as_deref() else {
        return;
    };
    match status {
        Some(code) if code.is_success() => {
            crate::circuit_breaker::record_success(&context.db, platform_id);
        }
        Some(code) if code.is_server_error() => {
            crate::circuit_breaker::record_failure(
                &context.db,
                platform_id,
                &format!("HTTP {code}"),
            );
        }
        Some(_) => {} // 4xx: not a platform-health signal.
        None => {
            crate::circuit_breaker::record_failure(
                &context.db,
                platform_id,
                error.unwrap_or("upstream network error"),
            );
        }
    }
}

pub(super) async fn send_with_key_failover(
    request: reqwest::RequestBuilder,
    keys: &[String],
    header: ApiKeyHeader,
    health: Option<KeyHealthContext>,
) -> Result<reqwest::Response, String> {
    let attempts: Vec<Option<&str>> = if keys.is_empty() {
        vec![None]
    } else {
        keys.iter().map(|key| Some(key.as_str())).collect()
    };
    let mut last_error = None;
    for (index, key) in attempts.iter().enumerate() {
        let started_at = std::time::Instant::now();
        let mut attempt = request
            .try_clone()
            .ok_or_else(|| "Unable to clone upstream request for key failover".to_string())?;
        if let Some(key) = key {
            attempt = match header {
                ApiKeyHeader::Anthropic => attempt.header("x-api-key", *key),
                ApiKeyHeader::Bearer => attempt.header("Authorization", format!("Bearer {key}")),
            };
        }
        match attempt.send().await {
            Ok(response) => {
                let status = response.status();
                let latency_ms = started_at.elapsed().as_millis() as i64;
                let can_retry = matches!(
                    status.as_u16(),
                    401 | 403 | 408 | 409 | 425 | 429 | 500 | 502 | 503 | 504
                );
                if let Some(context) = health.as_ref() {
                    if status.is_success() {
                        record_key_health(context, index, "success", None, latency_ms);
                    } else {
                        let message = format!("HTTP {status}");
                        record_key_health(context, index, "error", Some(&message), latency_ms);
                    }
                }
                if can_retry && index + 1 < attempts.len() {
                    last_error = Some(format!("upstream returned {status}"));
                    continue;
                }
                if let Some(context) = health.as_ref() {
                    record_circuit_outcome(context, Some(status), None);
                }
                return Ok(response);
            }
            Err(error) if index + 1 < attempts.len() => {
                if let Some(context) = health.as_ref() {
                    record_key_health(
                        context,
                        index,
                        "error",
                        Some(&error.to_string()),
                        started_at.elapsed().as_millis() as i64,
                    );
                }
                last_error = Some(describe_request_error(&error));
            }
            Err(error) => {
                if let Some(context) = health.as_ref() {
                    record_key_health(
                        context,
                        index,
                        "error",
                        Some(&error.to_string()),
                        started_at.elapsed().as_millis() as i64,
                    );
                    record_circuit_outcome(context, None, Some(&error.to_string()));
                }
                return Err(describe_request_error(&error));
            }
        }
    }
    // Every attempt exhausted without a returnable response (all keys failed).
    if let Some(context) = health.as_ref() {
        record_circuit_outcome(context, None, last_error.as_deref());
    }
    Err(last_error.unwrap_or_else(|| "No upstream API key attempt was made".into()))
}

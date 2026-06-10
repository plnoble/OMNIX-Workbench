//! Circuit Breaker & Session Log Tracking (CC Switch inspired)
//!
//! 1. Circuit Breaker — per-platform health monitoring with auto failover
//! 2. Session Log Usage Tracking — parse session logs for token usage
//! 3. Provider preset expansion (handled in constants.ts frontend)

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ══════════════════════════════════════════════════
// Circuit Breaker
// ══════════════════════════════════════════════════

/// Circuit breaker state
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CircuitState {
    /// Normal operation — requests pass through
    Closed,
    /// Too many failures — requests are blocked
    Open,
    /// Testing if service recovered — limited requests pass through
    HalfOpen,
}

/// Per-platform circuit breaker status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitBreakerStatus {
    pub platform_id: String,
    pub state: CircuitState,
    pub consecutive_failures: i32,
    pub total_failures: i64,
    pub total_successes: i64,
    pub last_failure_at: Option<String>,
    pub last_success_at: Option<String>,
    pub last_error: Option<String>,
    /// Number of successes needed in HalfOpen to close the circuit
    pub half_open_threshold: i32,
    /// Number of failures to trip the circuit open
    pub failure_threshold: i32,
}

impl Default for CircuitBreakerStatus {
    fn default() -> Self {
        Self {
            platform_id: String::new(),
            state: CircuitState::Closed,
            consecutive_failures: 0,
            total_failures: 0,
            total_successes: 0,
            last_failure_at: None,
            last_success_at: None,
            last_error: None,
            half_open_threshold: 2,
            failure_threshold: 5,
        }
    }
}

/// Record a successful request for a platform
pub fn record_success(db: &crate::db::DbManager, platform_id: &str) {
    if let Ok(conn) = db.get_connection() {
        let _ = conn.execute(
            "UPDATE model_platforms SET
                consecutive_failures = 0,
                is_healthy = 1,
                last_error = NULL
            WHERE id = ?1",
            rusqlite::params![platform_id],
        );
    }
}

/// Record a failed request for a platform
pub fn record_failure(db: &crate::db::DbManager, platform_id: &str, error: &str) {
    if let Ok(conn) = db.get_connection() {
        let _ = conn.execute(
            "UPDATE model_platforms SET
                consecutive_failures = consecutive_failures + 1,
                last_error = ?1
            WHERE id = ?2",
            rusqlite::params![error, platform_id],
        );
        // Auto-disable after 5 consecutive failures
        let _ = conn.execute(
            "UPDATE model_platforms SET is_healthy = 0 WHERE id = ?1 AND consecutive_failures >= 5",
            rusqlite::params![platform_id],
        );
    }
}

/// Get circuit breaker status for all platforms
pub fn get_all_circuit_status(db: &crate::db::DbManager) -> Vec<CircuitBreakerStatus> {
    let conn = match db.get_connection() { Ok(c) => c, Err(_) => return Vec::new() };
    let mut stmt = match conn.prepare(
        "SELECT id, is_healthy, consecutive_failures, last_error FROM model_platforms WHERE is_enabled = 1"
    ) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };

    let rows = stmt.query_map([], |row| {
        let id: String = row.get(0)?;
        let is_healthy: i32 = row.get(1)?;
        let consecutive_failures: i32 = row.get(2)?;
        let last_error: Option<String> = row.get(3)?;

        let state = if is_healthy == 0 {
            CircuitState::Open
        } else if consecutive_failures > 0 {
            CircuitState::HalfOpen
        } else {
            CircuitState::Closed
        };

        Ok(CircuitBreakerStatus {
            platform_id: id,
            state,
            consecutive_failures,
            total_failures: 0,
            total_successes: 0,
            last_failure_at: None,
            last_success_at: None,
            last_error,
            half_open_threshold: 2,
            failure_threshold: 5,
        })
    });

    match rows {
        Ok(r) => r.flatten().collect(),
        Err(_) => Vec::new(),
    }
}

/// Reset circuit breaker for a platform
pub fn reset_circuit(db: &crate::db::DbManager, platform_id: &str) {
    if let Ok(conn) = db.get_connection() {
        let _ = conn.execute(
            "UPDATE model_platforms SET is_healthy = 1, consecutive_failures = 0, last_error = NULL WHERE id = ?1",
            rusqlite::params![platform_id],
        );
    }
}

// ══════════════════════════════════════════════════
// Session Log Usage Tracking
// ══════════════════════════════════════════════════

/// Usage entry parsed from session logs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionUsageEntry {
    pub session_id: String,
    pub agent: String,
    pub model: String,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub total_tokens: i64,
    pub cost_estimate: f64,
    pub timestamp: String,
}

/// Model pricing table (cost per 1M tokens)
pub fn get_model_pricing() -> HashMap<String, (f64, f64)> {
    // (input_cost_per_1m, output_cost_per_1m)
    let mut pricing = HashMap::new();
    pricing.insert("gpt-4o".into(), (2.50, 10.00));
    pricing.insert("gpt-4o-mini".into(), (0.15, 0.60));
    pricing.insert("claude-sonnet-4-20250514".into(), (3.00, 15.00));
    pricing.insert("claude-3-5-haiku-20241022".into(), (0.80, 4.00));
    pricing.insert("deepseek-chat".into(), (0.14, 0.28));
    pricing.insert("deepseek-reasoner".into(), (0.55, 2.19));
    pricing.insert("qwen-plus".into(), (0.80, 2.00));
    pricing.insert("gemini-2.5-pro".into(), (1.25, 10.00));
    pricing.insert("gemini-2.5-flash".into(), (0.15, 0.60));
    pricing
}

/// Estimate cost for a model usage
pub fn estimate_cost(model: &str, prompt_tokens: i64, completion_tokens: i64) -> f64 {
    let pricing = get_model_pricing();
    let (input_rate, output_rate) = pricing.get(model)
        .copied()
        .unwrap_or((1.0, 3.0)); // default rates

    (prompt_tokens as f64 / 1_000_000.0 * input_rate) +
    (completion_tokens as f64 / 1_000_000.0 * output_rate)
}

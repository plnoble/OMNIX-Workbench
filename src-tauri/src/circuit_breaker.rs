//! Circuit Breaker & Session Log Tracking
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

/// Consecutive upstream failures that trip a platform's circuit open.
pub const CIRCUIT_FAILURE_THRESHOLD: i32 = 5;
/// Seconds an Open circuit waits before a single half-open probe is allowed.
pub const CIRCUIT_COOLDOWN_SECS: i64 = 60;

/// Derive the circuit state from stored fields — pure so it is unit-tested and
/// shared by status reads and the proxy's platform-availability filter.
/// `opened_ago_secs` is how long ago the circuit tripped open (None = not open).
pub fn derive_circuit_state(
    is_healthy: bool,
    consecutive_failures: i32,
    opened_ago_secs: Option<i64>,
) -> CircuitState {
    if !is_healthy {
        // Tripped open; eligible for a half-open probe once the cooldown elapses.
        match opened_ago_secs {
            Some(secs) if secs >= CIRCUIT_COOLDOWN_SECS => CircuitState::HalfOpen,
            _ => CircuitState::Open,
        }
    } else if consecutive_failures > 0 {
        // Healthy but degraded (some recent failures, not yet tripped).
        CircuitState::HalfOpen
    } else {
        CircuitState::Closed
    }
}

/// Record a successful request for a platform — closes the circuit.
pub fn record_success(db: &crate::db::DbManager, platform_id: &str) {
    if let Ok(conn) = db.get_connection() {
        let _ = conn.execute(
            "UPDATE model_platforms SET
                consecutive_failures = 0,
                is_healthy = 1,
                last_error = NULL,
                circuit_opened_at = NULL
            WHERE id = ?1",
            rusqlite::params![platform_id],
        );
    }
}

/// Record a failed request for a platform. Trips the circuit open after
/// `CIRCUIT_FAILURE_THRESHOLD` consecutive failures and (re)stamps the open time
/// so a failed half-open probe restarts the cooldown instead of hammering.
pub fn record_failure(db: &crate::db::DbManager, platform_id: &str, error: &str) {
    if let Ok(conn) = db.get_connection() {
        let _ = conn.execute(
            "UPDATE model_platforms SET
                consecutive_failures = consecutive_failures + 1,
                last_error = ?1
            WHERE id = ?2",
            rusqlite::params![error, platform_id],
        );
        let _ = conn.execute(
            "UPDATE model_platforms
                SET is_healthy = 0, circuit_opened_at = datetime('now')
                WHERE id = ?1 AND consecutive_failures >= ?2",
            rusqlite::params![platform_id, CIRCUIT_FAILURE_THRESHOLD],
        );
    }
}

/// Get circuit breaker status for all platforms
pub fn get_all_circuit_status(db: &crate::db::DbManager) -> Vec<CircuitBreakerStatus> {
    let conn = match db.get_connection() { Ok(c) => c, Err(_) => return Vec::new() };
    let mut stmt = match conn.prepare(
        "SELECT id, is_healthy, consecutive_failures, last_error,
                CAST((julianday('now') - julianday(circuit_opened_at)) * 86400 AS INTEGER)
         FROM model_platforms WHERE is_enabled = 1"
    ) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };

    let rows = stmt.query_map([], |row| {
        let id: String = row.get(0)?;
        let is_healthy: i32 = row.get(1)?;
        let consecutive_failures: i32 = row.get(2)?;
        let last_error: Option<String> = row.get(3)?;
        let opened_ago_secs: Option<i64> = row.get(4)?;

        Ok(CircuitBreakerStatus {
            platform_id: id,
            state: derive_circuit_state(is_healthy != 0, consecutive_failures, opened_ago_secs),
            consecutive_failures,
            total_failures: 0,
            total_successes: 0,
            last_failure_at: None,
            last_success_at: None,
            last_error,
            half_open_threshold: 2,
            failure_threshold: CIRCUIT_FAILURE_THRESHOLD,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn circuit_state_machine_covers_closed_degraded_open_halfopen() {
        // Healthy, no failures → Closed.
        assert_eq!(derive_circuit_state(true, 0, None), CircuitState::Closed);
        // Healthy but recent failures (not tripped) → HalfOpen (degraded).
        assert_eq!(derive_circuit_state(true, 3, None), CircuitState::HalfOpen);
        // Tripped open, still within cooldown → Open (skipped by the proxy).
        assert_eq!(derive_circuit_state(false, 6, Some(10)), CircuitState::Open);
        assert_eq!(derive_circuit_state(false, 6, None), CircuitState::Open);
        // Tripped open, cooldown elapsed → HalfOpen (one probe allowed through).
        assert_eq!(
            derive_circuit_state(false, 6, Some(CIRCUIT_COOLDOWN_SECS)),
            CircuitState::HalfOpen
        );
        assert_eq!(
            derive_circuit_state(false, 6, Some(CIRCUIT_COOLDOWN_SECS + 120)),
            CircuitState::HalfOpen
        );
    }

    #[test]
    fn cost_estimate_uses_table_then_default() {
        // Known model uses its rate (gpt-4o: 2.50 in / 10.00 out per 1M).
        let known = estimate_cost("gpt-4o", 1_000_000, 1_000_000);
        assert!((known - 12.50).abs() < 1e-9);
        // Unknown model falls back to default (1.0 in / 3.0 out).
        let unknown = estimate_cost("mystery-model", 1_000_000, 1_000_000);
        assert!((unknown - 4.0).abs() < 1e-9);
    }
}

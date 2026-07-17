//! 订阅额度（监督台顶部）— Claude Code / Codex 的 5 小时窗口与周额度。
//!
//! 数据源都是各 CLI 落在本机的日志，**不打任何网络请求**：
//! - **Codex**：`~/.codex/sessions/**/rollout-*.jsonl` 的 `token_count` 事件里
//!   带 OpenAI 官方回传的 `rate_limits`（used_percent / window_minutes /
//!   resets_at / plan_type，primary+secondary 两个窗口）。取最新一条，就是
//!   官方口径的精确百分比——不估算。
//! - **Claude Code**：`~/.claude/projects/**/*.jssonl` 每条 assistant 消息带
//!   usage（输入/输出/缓存 token）。Anthropic 不在本地回传限额百分比，所以
//!   这里只算**消耗与窗口重置时间**（5 小时块口径与 ccusage 一致：块起点
//!   锚定在首条消息的整点，5 小时后重置），绝不编造百分比。

use std::path::PathBuf;
use std::sync::Arc;

use chrono::{DateTime, Duration, DurationRound, Utc};
use serde::Serialize;
use tauri::State;

use crate::db::DbManager;

#[derive(Debug, Clone, Serialize, Default)]
pub struct TokenTally {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_create: u64,
    pub requests: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClaudeQuota {
    /// Current 5h block (ccusage semantics). None = no activity in the block.
    pub window_started_at: Option<String>,
    pub window_resets_at: Option<String>,
    pub window: TokenTally,
    /// Rolling last 7 days.
    pub week: TokenTally,
    /// (model, output_tokens) for the current window, largest first.
    pub window_models: Vec<(String, u64)>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CodexWindow {
    pub used_percent: f64,
    pub window_minutes: u64,
    pub resets_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CodexQuota {
    pub plan_type: String,
    pub primary: Option<CodexWindow>,
    pub secondary: Option<CodexWindow>,
    /// When the newest rate_limits snapshot was captured (last Codex activity).
    pub captured_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct QuotaOverview {
    pub claude: Option<ClaudeQuota>,
    pub codex: Option<CodexQuota>,
}

// ── Claude Code ─────────────────────────────────────────────────────────────

struct UsageEvent {
    ts: DateTime<Utc>,
    model: String,
    input: u64,
    output: u64,
    cache_read: u64,
    cache_create: u64,
}

fn parse_claude_line(line: &str) -> Option<UsageEvent> {
    // Fast reject before JSON parsing: only assistant messages carry usage.
    if !line.contains("\"usage\"") {
        return None;
    }
    let value: serde_json::Value = serde_json::from_str(line).ok()?;
    let usage = value.pointer("/message/usage")?;
    let ts = value.pointer("/timestamp")?.as_str()?;
    let ts = DateTime::parse_from_rfc3339(ts).ok()?.with_timezone(&Utc);
    let n = |key: &str| usage.pointer(key).and_then(|v| v.as_u64()).unwrap_or(0);
    Some(UsageEvent {
        ts,
        model: value
            .pointer("/message/model")
            .and_then(|m| m.as_str())
            .unwrap_or("unknown")
            .to_string(),
        input: n("/input_tokens"),
        output: n("/output_tokens"),
        cache_read: n("/cache_read_input_tokens"),
        cache_create: n("/cache_creation_input_tokens"),
    })
}

/// ccusage-compatible 5h blocks: the current block starts at the floor-to-hour
/// of the first message after the previous block ended; resets 5h later.
fn current_block_start(mut events_asc: impl Iterator<Item = DateTime<Utc>>) -> Option<DateTime<Utc>> {
    let first = events_asc.next()?;
    let mut block_start = first.duration_trunc(Duration::hours(1)).unwrap_or(first);
    for ts in events_asc {
        if ts >= block_start + Duration::hours(5) {
            block_start = ts.duration_trunc(Duration::hours(1)).unwrap_or(ts);
        }
    }
    Some(block_start)
}

fn scan_claude(now: DateTime<Utc>) -> Option<ClaudeQuota> {
    let projects = dirs::home_dir()?.join(".claude").join("projects");
    let week_ago = now - Duration::days(7);
    // Only files touched within the window matter.
    let cutoff = std::time::SystemTime::now() - std::time::Duration::from_secs(8 * 24 * 3600);

    let mut events: Vec<UsageEvent> = Vec::new();
    let dirs_iter = std::fs::read_dir(&projects).ok()?;
    for dir in dirs_iter.flatten() {
        let Ok(files) = std::fs::read_dir(dir.path()) else { continue };
        for file in files.flatten() {
            let path = file.path();
            if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }
            if file
                .metadata()
                .and_then(|m| m.modified())
                .map(|m| m < cutoff)
                .unwrap_or(true)
            {
                continue;
            }
            let Ok(content) = std::fs::read_to_string(&path) else { continue };
            for line in content.lines() {
                if let Some(ev) = parse_claude_line(line) {
                    if ev.ts >= week_ago && ev.ts <= now + Duration::minutes(5) {
                        events.push(ev);
                    }
                }
            }
        }
    }
    if events.is_empty() {
        return None;
    }
    events.sort_by_key(|e| e.ts);

    let block_start = current_block_start(events.iter().map(|e| e.ts))?;
    let block_end = block_start + Duration::hours(5);
    let in_window = now < block_end;

    let mut window = TokenTally::default();
    let mut week = TokenTally::default();
    let mut models: std::collections::HashMap<String, u64> = std::collections::HashMap::new();
    for ev in &events {
        week.input += ev.input;
        week.output += ev.output;
        week.cache_read += ev.cache_read;
        week.cache_create += ev.cache_create;
        week.requests += 1;
        if in_window && ev.ts >= block_start {
            window.input += ev.input;
            window.output += ev.output;
            window.cache_read += ev.cache_read;
            window.cache_create += ev.cache_create;
            window.requests += 1;
            *models.entry(ev.model.clone()).or_default() += ev.output;
        }
    }
    let mut window_models: Vec<(String, u64)> = models.into_iter().collect();
    window_models.sort_by(|a, b| b.1.cmp(&a.1));
    window_models.truncate(4);

    Some(ClaudeQuota {
        window_started_at: in_window.then(|| block_start.to_rfc3339()),
        window_resets_at: in_window.then(|| block_end.to_rfc3339()),
        window,
        week,
        window_models,
    })
}

// ── Codex ───────────────────────────────────────────────────────────────────

fn parse_codex_rate_limits(line: &str) -> Option<(String, CodexQuota)> {
    if !line.contains("\"rate_limits\"") {
        return None;
    }
    let value: serde_json::Value = serde_json::from_str(line).ok()?;
    let ts = value.pointer("/timestamp")?.as_str()?.to_string();
    let limits = value.pointer("/payload/rate_limits")?;
    let window = |node: &serde_json::Value| -> Option<CodexWindow> {
        let used = node.pointer("/used_percent")?.as_f64()?;
        Some(CodexWindow {
            used_percent: used,
            window_minutes: node.pointer("/window_minutes").and_then(|v| v.as_u64()).unwrap_or(0),
            resets_at: node
                .pointer("/resets_at")
                .and_then(|v| v.as_i64())
                .and_then(|secs| DateTime::<Utc>::from_timestamp(secs, 0))
                .map(|dt| dt.to_rfc3339()),
        })
    };
    let primary = limits.pointer("/primary").and_then(window);
    let secondary = limits.pointer("/secondary").and_then(window);
    if primary.is_none() && secondary.is_none() {
        return None;
    }
    Some((
        ts.clone(),
        CodexQuota {
            plan_type: limits
                .pointer("/plan_type")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string(),
            primary,
            secondary,
            captured_at: ts,
        },
    ))
}

fn scan_codex() -> Option<CodexQuota> {
    let sessions = dirs::home_dir()?.join(".codex").join("sessions");
    // Newest files first; the newest rate_limits line wins.
    let mut files: Vec<(std::time::SystemTime, PathBuf)> = Vec::new();
    let mut stack = vec![sessions];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().and_then(|e| e.to_str()) == Some("jsonl") {
                if let Ok(modified) = entry.metadata().and_then(|m| m.modified()) {
                    files.push((modified, path));
                }
            }
        }
    }
    files.sort_by(|a, b| b.0.cmp(&a.0));

    for (_, path) in files.into_iter().take(12) {
        let Ok(content) = std::fs::read_to_string(&path) else { continue };
        // Scan from the end: the last snapshot in the newest file is authoritative.
        if let Some((_, quota)) = content.lines().rev().find_map(parse_codex_rate_limits) {
            return Some(quota);
        }
    }
    None
}

#[tauri::command]
pub fn agent_quota_overview(_db: State<'_, Arc<DbManager>>) -> Result<QuotaOverview, String> {
    let now = Utc::now();
    Ok(QuotaOverview { claude: scan_claude(now), codex: scan_codex() })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Scans this machine's real logs and prints what the UI would show
    /// (`cargo test --lib real_quota -- --ignored --nocapture`).
    #[test]
    #[ignore]
    fn real_quota_scan_smoke() {
        let now = Utc::now();
        let claude = scan_claude(now);
        match &claude {
            Some(q) => println!(
                "claude: window {:?}..{:?} req={} in={} out={} cache_r={} | week req={} out={} models={:?}",
                q.window_started_at, q.window_resets_at, q.window.requests, q.window.input,
                q.window.output, q.window.cache_read, q.week.requests, q.week.output, q.window_models
            ),
            None => println!("claude: none"),
        }
        match scan_codex() {
            Some(q) => println!(
                "codex: plan={} primary={:?} secondary={:?} captured={}",
                q.plan_type, q.primary.map(|w| (w.used_percent, w.window_minutes, w.resets_at)),
                q.secondary.map(|w| (w.used_percent, w.window_minutes)), q.captured_at
            ),
            None => println!("codex: none"),
        }
    }

    /// Verbatim line shape from a real rollout file on this machine (2026-07-15).
    const CODEX_LINE: &str = r#"{"timestamp":"2026-07-15T01:40:56.067Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":27838,"cached_input_tokens":7552,"output_tokens":60,"reasoning_output_tokens":42,"total_tokens":27898},"model_context_window":258400},"rate_limits":{"limit_id":"codex","limit_name":null,"primary":{"used_percent":21.0,"window_minutes":10080,"resets_at":1784671423},"secondary":null,"credits":null,"individual_limit":null,"plan_type":"plus","rate_limit_reached_type":null}}}"#;

    #[test]
    fn codex_official_limits_parse() {
        let (_, quota) = parse_codex_rate_limits(CODEX_LINE).expect("parsed");
        assert_eq!(quota.plan_type, "plus");
        let primary = quota.primary.expect("primary window");
        assert_eq!(primary.used_percent, 21.0);
        assert_eq!(primary.window_minutes, 10080, "weekly window");
        assert!(primary.resets_at.unwrap().starts_with("2026-"));
        assert!(quota.secondary.is_none());
        assert!(parse_codex_rate_limits(r#"{"timestamp":"t","payload":{}}"#).is_none());
    }

    /// Verbatim structure of a Claude Code transcript line (fields OMNIX reads).
    #[test]
    fn claude_usage_line_parses() {
        let line = r#"{"type":"assistant","timestamp":"2026-07-16T12:10:47.323Z","message":{"model":"claude-fable-5","usage":{"input_tokens":2,"cache_creation_input_tokens":64,"cache_read_input_tokens":488944,"output_tokens":2627}}}"#;
        let ev = parse_claude_line(line).expect("parsed");
        assert_eq!(ev.model, "claude-fable-5");
        assert_eq!(ev.output, 2627);
        assert_eq!(ev.cache_read, 488944);
        assert!(parse_claude_line(r#"{"type":"user","timestamp":"x"}"#).is_none());
    }

    #[test]
    fn five_hour_blocks_anchor_to_first_message_hour() {
        let t = |s: &str| DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc);
        // First activity 09:20 → block starts 09:00, resets 14:00.
        let events = vec![t("2026-07-16T09:20:00Z"), t("2026-07-16T10:00:00Z")];
        let start = current_block_start(events.into_iter()).unwrap();
        assert_eq!(start.to_rfc3339(), "2026-07-16T09:00:00+00:00");
        // A message after the block ends starts a NEW block at ITS hour floor.
        let events = vec![t("2026-07-16T02:10:00Z"), t("2026-07-16T09:20:00Z")];
        let start = current_block_start(events.into_iter()).unwrap();
        assert_eq!(start.to_rfc3339(), "2026-07-16T09:00:00+00:00");
    }
}

//! Personal "AI-coding profile" stats — powers the activity heatmap, streaks
//! and shareable card. (Borrowed from Synara's Profile/stats feature.)
//!
//! All figures come from data OMNIX already records: `messages` (prompts),
//! `conversations`, `agent_sessions` (runs, per agent), and `request_logs`
//! (tokens). No new telemetry is collected.

use std::collections::BTreeMap;
use std::sync::Arc;

use rusqlite::{params, OptionalExtension};
use serde::Serialize;
use tauri::State;

use crate::db::DbManager;

/// Trailing window rendered by the contribution heatmap (~26 weeks).
const HEATMAP_DAYS: i64 = 182;

#[derive(Debug, Clone, Serialize)]
pub struct ProfileDay {
    pub day: String,
    pub count: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProfileAgentCount {
    pub agent: String,
    pub count: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProfileStats {
    pub total_prompts: i64,
    pub total_conversations: i64,
    pub total_sessions: i64,
    pub total_tokens: i64,
    pub active_days: i64,
    pub current_streak: i64,
    pub longest_streak: i64,
    pub first_active: Option<String>,
    pub per_agent: Vec<ProfileAgentCount>,
    pub daily: Vec<ProfileDay>,
}

#[tauri::command]
pub fn get_profile_stats(db: State<'_, Arc<DbManager>>) -> Result<ProfileStats, String> {
    let conn = db.get_connection().map_err(|error| error.to_string())?;
    let scalar = |sql: &str| -> i64 { conn.query_row(sql, [], |row| row.get(0)).unwrap_or(0) };

    let total_prompts = scalar("SELECT COUNT(*) FROM messages WHERE role = 'user'");
    let total_conversations = scalar("SELECT COUNT(*) FROM conversations");
    let total_sessions = scalar("SELECT COUNT(*) FROM agent_sessions");
    let total_tokens = scalar("SELECT COALESCE(SUM(total_tokens), 0) FROM request_logs");
    let first_active: Option<String> = conn
        .query_row(
            "SELECT date(MIN(timestamp)) FROM messages WHERE role = 'user'",
            [],
            |row| row.get(0),
        )
        .optional()
        .ok()
        .flatten();

    // Daily prompt counts over the heatmap window, keyed by YYYY-MM-DD.
    let mut counts: BTreeMap<String, i64> = BTreeMap::new();
    {
        let window = format!("-{HEATMAP_DAYS} days");
        let mut stmt = conn
            .prepare(
                "SELECT date(timestamp) AS d, COUNT(*)
                 FROM messages
                 WHERE role = 'user' AND date(timestamp) >= date('now', ?1)
                 GROUP BY d",
            )
            .map_err(|error| error.to_string())?;
        let rows = stmt
            .query_map(params![window], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })
            .map_err(|error| error.to_string())?;
        for row in rows.flatten() {
            counts.insert(row.0, row.1);
        }
    }

    // Fill every day in the window (oldest first) so the heatmap grid is dense.
    let today = chrono::Local::now().date_naive();
    let mut daily = Vec::with_capacity(HEATMAP_DAYS as usize);
    let mut active_days = 0i64;
    for offset in (0..HEATMAP_DAYS).rev() {
        let day = today - chrono::Duration::days(offset);
        let key = day.format("%Y-%m-%d").to_string();
        let count = counts.get(&key).copied().unwrap_or(0);
        if count > 0 {
            active_days += 1;
        }
        daily.push(ProfileDay { day: key, count });
    }

    // Longest run of consecutive active days in the window.
    let mut longest_streak = 0i64;
    let mut run = 0i64;
    for entry in &daily {
        if entry.count > 0 {
            run += 1;
            longest_streak = longest_streak.max(run);
        } else {
            run = 0;
        }
    }
    // Current streak: consecutive active days ending today.
    let mut current_streak = 0i64;
    for entry in daily.iter().rev() {
        if entry.count > 0 {
            current_streak += 1;
        } else {
            break;
        }
    }

    let mut per_agent = Vec::new();
    {
        let mut stmt = conn
            .prepare(
                "SELECT agent_id, COUNT(*) FROM agent_sessions
                 GROUP BY agent_id ORDER BY COUNT(*) DESC",
            )
            .map_err(|error| error.to_string())?;
        let rows = stmt
            .query_map([], |row| {
                Ok(ProfileAgentCount {
                    agent: display_agent(&row.get::<_, String>(0)?),
                    count: row.get(1)?,
                })
            })
            .map_err(|error| error.to_string())?;
        for row in rows.flatten() {
            per_agent.push(row);
        }
    }

    Ok(ProfileStats {
        total_prompts,
        total_conversations,
        total_sessions,
        total_tokens,
        active_days,
        current_streak,
        longest_streak,
        first_active,
        per_agent,
        daily,
    })
}

/// Prettifies a stored `agent_id` for display; unknown ids pass through as-is.
fn display_agent(agent_id: &str) -> String {
    match agent_id {
        "claude_code" => "Claude Code",
        "codex" => "Codex",
        "gemini_cli" => "Gemini CLI",
        "qwen_code" => "Qwen Code",
        "opencode" => "OpenCode",
        "copilot_cli" => "GitHub Copilot CLI",
        other => other,
    }
    .to_string()
}

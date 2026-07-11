//! Event Bus
//!
//! Lightweight pub-sub for triggering tasks on system events.
//! Events: session_created, message_sent, skill_synced, agent_started, task_completed.
//! Each event maintains a counter per-task, fires when threshold reached.
//! Counter state persists to DB to survive reboots.

use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::db::DbManager;

/// Event types that can trigger tasks
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum EventType {
    SessionCreated,
    MessageSent,
    SkillSynced,
    AgentStarted,
    TaskCompleted,
    AgentFailed,
}

impl EventType {
    pub fn as_str(&self) -> &'static str {
        match self {
            EventType::SessionCreated => "session_created",
            EventType::MessageSent => "message_sent",
            EventType::SkillSynced => "skill_synced",
            EventType::AgentStarted => "agent_started",
            EventType::TaskCompleted => "task_completed",
            EventType::AgentFailed => "agent_failed",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "session_created" => Some(Self::SessionCreated),
            "message_sent" => Some(Self::MessageSent),
            "skill_synced" => Some(Self::SkillSynced),
            "agent_started" => Some(Self::AgentStarted),
            "task_completed" => Some(Self::TaskCompleted),
            "agent_failed" => Some(Self::AgentFailed),
            _ => None,
        }
    }
}

/// A registered event trigger
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventTrigger {
    pub id: String,
    pub event_type: String,
    pub threshold: u32,         // fire every N events
    pub task_id: String,        // cron_task to execute
    pub current_count: u32,     // current counter
    pub enabled: bool,
}

/// Initialize event_bus tables in DB
pub fn init_event_bus_tables(conn: &rusqlite::Connection) -> rusqlite::Result<()> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS event_triggers (
            id TEXT PRIMARY KEY,
            event_type TEXT NOT NULL,
            threshold INTEGER NOT NULL DEFAULT 1,
            task_id TEXT NOT NULL,
            current_count INTEGER NOT NULL DEFAULT 0,
            enabled INTEGER NOT NULL DEFAULT 1,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP
        )", [],
    )?;
    Ok(())
}

/// Emit an event — increments counters for all matching triggers.
/// Returns list of task_ids that should be fired (threshold reached).
pub fn emit_event(db: &DbManager, event: EventType) -> Vec<String> {
    let conn = match db.get_connection() {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    let event_str = event.as_str();

    // Increment counters for matching enabled triggers
    let _ = conn.execute(
        "UPDATE event_triggers SET current_count = current_count + 1 WHERE event_type = ?1 AND enabled = 1",
        params![event_str],
    );

    // Find triggers that reached threshold
    let mut stmt = match conn.prepare(
        "SELECT id, task_id, threshold, current_count FROM event_triggers WHERE event_type = ?1 AND enabled = 1 AND current_count >= threshold"
    ) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };

    let mut fired_tasks = Vec::new();
    let rows = stmt.query_map(params![event_str], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, u32>(2)?,
            row.get::<_, u32>(3)?,
        ))
    });

    if let Ok(rows) = rows {
        for r in rows.flatten() {
            let (trigger_id, task_id, _threshold, _count) = r;
            fired_tasks.push(task_id);
            // Reset counter
            let _ = conn.execute(
                "UPDATE event_triggers SET current_count = 0 WHERE id = ?1",
                params![trigger_id],
            );
        }
    }

    fired_tasks
}

/// Register a new event trigger
pub fn register_trigger(
    db: &DbManager,
    event_type: &str,
    threshold: u32,
    task_id: &str,
) -> Result<String, String> {
    let conn = db.get_connection().map_err(|e: rusqlite::Error| e.to_string())?;
    let id = format!("evtrig_{}_{}", event_type, chrono::Utc::now().timestamp_millis());
    conn.execute(
        "INSERT INTO event_triggers (id, event_type, threshold, task_id) VALUES (?1, ?2, ?3, ?4)",
        params![id, event_type, threshold, task_id],
    ).map_err(|e: rusqlite::Error| e.to_string())?;
    Ok(id)
}

/// Get all event triggers
pub fn list_triggers(db: &DbManager) -> Vec<EventTrigger> {
    let conn = match db.get_connection() { Ok(c) => c, Err(_) => return Vec::new() };
    let mut stmt = match conn.prepare(
        "SELECT id, event_type, threshold, task_id, current_count, enabled FROM event_triggers ORDER BY created_at DESC"
    ) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };

    let rows = stmt.query_map([], |row| {
        Ok(EventTrigger {
            id: row.get(0)?,
            event_type: row.get(1)?,
            threshold: row.get(2)?,
            task_id: row.get(3)?,
            current_count: row.get(4)?,
            enabled: row.get::<_, i32>(5)? != 0,
        })
    });

    match rows {
        Ok(r) => r.flatten().collect(),
        Err(_) => Vec::new(),
    }
}

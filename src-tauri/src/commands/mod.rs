// OMNIX Workbench - Commands Module
// This mod.rs re-exports all command functions from submodules,
// keeping the `commands::*` namespace unchanged for lib.rs.

mod accounts;
mod agent_installs;
mod agents;
mod automation;
mod autopilots;
mod cc_switch;
mod conversation_goals;
mod conversations;
mod cron;
mod deepseek;
mod distillation;
mod evolution;
mod knowledge;
mod lifecycle;
mod checkpoints;
mod cli_takeover;
mod local_models;
mod mcp_sync;
mod media;
mod memories;
mod oauth;
mod safety;
mod platforms;
mod profile;
mod project_protocol;
mod qa;
mod runs;
mod runtime;
mod sdd;
mod search;
mod selection;
mod settings;
mod skill_library;
mod skill_pool;
mod skill_sync;
mod skills;
mod storage;
mod slides;
mod team_runtime;
mod templates;
mod windows;
mod custom_assistants;
mod hooks;
mod kb_transfer;
mod notes;
mod quick_actions;
mod subagents;
mod workspace;
mod worktrees;
mod write;
mod config_presets;

// ── Shared Structs / Enums used across multiple submodules ──

use serde::{Deserialize, Serialize};

// ── Shared DTOs ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub file_path: String,
    pub profile: String,
    pub is_active: bool,
    pub dependencies: Vec<String>,
    pub updated_at: String,
    pub source_type: String,
    pub source_ref: Option<String>,
    pub source_revision: Option<String>,
    pub central_path: String,
    pub content_hash: Option<String>,
    pub starred: bool,
    pub category: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentAccount {
    pub id: String,
    pub account_name: String,
    pub api_key: String,
    pub api_host: String,
    pub target_model: String,
    pub agent_name: String,
    pub is_active: bool,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Memory {
    pub id: String,
    pub incident_desc: String,
    pub code_pattern: String,
    pub remediation: String,
    pub keywords: String,
    pub created_at: String,
    #[serde(default)]
    pub confidence: f64,
    #[serde(default)]
    pub seen_count: i64,
    #[serde(default)]
    pub repeated_count: i64,
    #[serde(default)]
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbTask {
    pub id: String,
    pub conversation_id: String,
    pub title: String,
    pub status: String,
    pub order_num: i32,
    pub dependencies: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelMetadata {
    pub name: String,
    pub source: String,
    pub has_vision: bool,
    pub has_audio: bool,
    pub has_reasoning: bool,
    pub has_coding: bool,
    pub has_long_context: bool,
    pub has_tool_use: bool,
    pub has_embedding: bool,
    pub has_speedy: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronTask {
    pub id: String,
    pub title: String,
    pub schedule: String,
    pub agent_name: String,
    pub args: String,
    pub workspace_dir: String,
    pub is_active: bool,
    pub last_run: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronRun {
    pub id: String,
    pub task_id: String,
    pub status: String,
    pub log_path: String,
    pub started_at: String,
    pub finished_at: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WindowLayout {
    pub label: String,
    pub url: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

// ── Re-export all command functions from submodules ──

pub use accounts::*;
pub use agent_installs::*;
pub use agents::*;
pub use automation::*;
pub use autopilots::*;
pub use cc_switch::*;
pub use conversation_goals::*;
pub use conversations::*;
pub use cron::*;
pub use deepseek::*;
pub use distillation::*;
pub use evolution::*;
pub use knowledge::*;
pub use checkpoints::*;
pub use cli_takeover::*;
pub use local_models::*;
pub use custom_assistants::*;
pub use hooks::*;
pub use kb_transfer::*;
pub use notes::*;
pub use quick_actions::*;
pub use subagents::*;
pub use lifecycle::*;
pub use mcp_sync::*;
pub use media::*;
pub use memories::*;
pub use oauth::*;
pub use safety::*;
pub use platforms::*;
pub use profile::*;
pub use project_protocol::*;
pub use qa::*;
pub use runs::*;
pub use runtime::*;
pub use sdd::*;
pub use search::*;
pub use selection::*;
pub use settings::*;
pub use skill_library::*;
pub use skill_pool::*;
pub use skill_sync::*;
pub use skills::*;
pub use slides::*;
pub use storage::*;
pub use team_runtime::*;
pub use templates::*;
pub use windows::*;
pub use workspace::*;
pub use worktrees::*;
pub use write::*;
pub use config_presets::*;

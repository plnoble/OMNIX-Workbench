// OMNIX Workbench - Commands Module
// This mod.rs re-exports all command functions from submodules,
// keeping the `commands::*` namespace unchanged for lib.rs.

mod accounts;
mod agents;
mod aionui;
mod cc_switch;
mod conversations;
mod cron;
mod deepseek;
mod distillation;
mod evolution;
mod knowledge;
mod lifecycle;
mod checkpoints;
mod mcp_sync;
mod memories;
mod odysseus;
mod platforms;
mod project_protocol;
mod qa;
mod runs;
mod runtime;
mod search;
mod selection;
mod settings;
mod skill_library;
mod skill_sets;
mod skill_sync;
mod skills;
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
mod zcf;

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
pub struct SkillFusionResult {
    pub draft_id: String,
    pub name: String,
    pub description: String,
    pub fused_code: String,
    pub explanation: String,
    pub conflicts: Vec<String>,
    pub status: String,
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
pub struct MemorySuggestion {
    pub incident_desc: String,
    pub code_pattern: String,
    pub remediation: String,
    pub keywords: String,
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
pub use agents::*;
pub use aionui::*;
pub use cc_switch::*;
pub use conversations::*;
pub use cron::*;
pub use deepseek::*;
pub use distillation::*;
pub use evolution::*;
pub use knowledge::*;
pub use checkpoints::*;
pub use custom_assistants::*;
pub use hooks::*;
pub use kb_transfer::*;
pub use notes::*;
pub use quick_actions::*;
pub use subagents::*;
pub use lifecycle::*;
pub use mcp_sync::*;
pub use memories::*;
pub use odysseus::*;
pub use platforms::*;
pub use project_protocol::*;
pub use qa::*;
pub use runs::*;
pub use runtime::*;
pub use search::*;
pub use selection::*;
pub use settings::*;
pub use skill_library::*;
pub use skill_sets::*;
pub use skill_sync::*;
pub use skills::*;
pub use team_runtime::*;
pub use templates::*;
pub use windows::*;
pub use workspace::*;
pub use worktrees::*;
pub use zcf::*;

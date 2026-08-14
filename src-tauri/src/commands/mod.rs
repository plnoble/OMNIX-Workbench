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
mod distillation;
mod evolution;
mod knowledge;
mod lifecycle;
mod checkpoints;
mod cli_takeover;
mod mcp_sync;
mod media;
mod memories;
mod oauth;
mod safety;
mod platform_api_keys;
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
mod grok_auth;
mod office;
mod skill_updates;
mod quota;
mod supervision;
mod hooks;
mod kb_transfer;
mod notes;
mod quick_actions;
mod remote_dev;
mod subagents;
mod workspace;
mod worktrees;
mod write;
mod config_presets;

// ── Shared Structs / Enums used across multiple submodules ──

use serde::{Deserialize, Serialize};

// ── Shared DTOs ──


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
    /// 这次运行的时间窗内观察到的对外动作摘要（`summarize_run` 写的）。
    ///
    /// 这一列一直是**只写不读**：查询没选它、DTO 没有它、界面也就无从显示。
    /// 而它记的恰恰是「这次定时任务期间往外发了什么」——最值得被人看见的那类
    /// 信息，收不回来。
    pub action_summary: String,
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
pub use distillation::*;
pub use evolution::*;
pub use knowledge::*;
pub use checkpoints::*;
pub use cli_takeover::*;
pub use custom_assistants::*;
pub use hooks::*;
pub use kb_transfer::*;
pub use notes::*;
pub use quick_actions::*;
pub use remote_dev::*;
pub use subagents::*;
pub use lifecycle::*;
pub use mcp_sync::*;
pub use media::*;
pub use memories::*;
pub use grok_auth::*;
pub use office::*;
pub use skill_updates::*;
pub use quota::*;
pub use supervision::*;
pub use oauth::*;
pub use safety::*;
pub use platform_api_keys::*;
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

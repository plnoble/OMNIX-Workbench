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

use crate::knowledge::{self as crate_knowledge};

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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemorySuggestion {
    pub incident_desc: String,
    pub code_pattern: String,
    pub remediation: String,
    pub keywords: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationInfo {
    pub id: String,
    pub title: String,
    pub workspace_path: String,
    pub active_agent: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageInfo {
    pub id: String,
    pub conversation_id: String,
    pub role: String,
    pub content: String,
    pub timestamp: String,
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
pub struct MailboxMessage {
    pub filename: String,
    pub sender: String,
    pub receiver: String,
    pub command: String,
    pub params: serde_json::Value,
    pub status: String,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteAccessInfo {
    pub local_ip: String,
    pub port: u16,
    pub token: String,
    pub connection_url: String,
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

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ModelPlatform {
    pub id: String,
    pub name: String,
    pub api_type: String,
    pub api_key: String,
    pub api_address: String,
    pub is_enabled: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PlatformModel {
    pub id: String,
    pub platform_id: String,
    pub model_name: String,
    pub has_vision: bool,
    pub has_audio: bool,
    pub has_reasoning: bool,
    pub has_coding: bool,
    pub has_long_context: bool,
    pub has_tool_use: bool,
    pub has_embedding: bool,
    pub has_speedy: bool,
    pub is_enabled: bool,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreviewFile {
    pub path: String,
    pub name: String,
    pub ext: String,
    pub modified: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KbDocument {
    pub id: String,
    pub title: String,
    pub source_path: String,
    pub file_type: String,
    pub file_hash: String,
    pub chunk_count: i32,
    pub total_chars: i32,
    pub embedding_model: String,
    pub embedding_status: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KbChunk {
    pub id: String,
    pub document_id: String,
    pub chunk_index: i32,
    pub content: String,
    pub char_start: i32,
    pub char_end: i32,
    pub metadata: serde_json::Value,
    pub has_embedding: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkConfigPayload {
    pub max_chunk_chars: Option<usize>,
    pub overlap_chars: Option<usize>,
    pub respect_boundaries: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingProgress {
    pub document_id: String,
    pub total_chunks: i32,
    pub embedded_chunks: i32,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingModelInfo {
    pub model_name: String,
    pub platform_id: String,
    pub platform_name: String,
    pub api_type: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SearchProvider {
    pub id: String,
    pub name: String,
    pub api_type: String,
    pub api_key: String,
    pub api_address: String,
    pub is_enabled: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WebSearchResult {
    pub title: String,
    pub url: String,
    pub snippet: String,
    pub source: String,
    pub position: i32,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SearchHistoryEntry {
    pub id: String,
    pub query: String,
    pub provider_id: String,
    pub result_count: i32,
    pub created_at: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct McpServer {
    pub id: String,
    pub name: String,
    pub command: String,
    pub args: String,
    pub env: String,
    pub url: String,
    pub server_type: String,
    pub is_enabled: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BackupTableInfo {
    pub table_name: String,
    pub row_count: i64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BackupExport {
    pub version: String,
    pub timestamp: String,
    pub source: String,
    pub tables: std::collections::HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ImportResult {
    pub tables_restored: Vec<(String, usize)>,
    pub total_rows: usize,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PromptEntry {
    pub id: String,
    pub title: String,
    pub content: String,
    pub category: String,
    pub order_key: i32,
    pub created_at: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ActivityLogEntry {
    pub id: String,
    pub action: String,
    pub target: String,
    pub details: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillTargetRecord {
    pub id: String,
    pub skill_id: String,
    pub tool: String,
    pub target_path: String,
    pub mode: String,
    pub status: String,
    pub last_error: Option<String>,
    pub synced_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentPlatformBinding {
    pub agent_name: String,
    pub platform_id: String,
    pub platform_name: String,
    pub model_name: Option<String>,
    pub binding_kind: String,
    pub builtin_model: Option<String>,
    pub enabled: bool,
}

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckResult {
    pub platform_id: String,
    pub platform_name: String,
    pub is_reachable: bool,
    pub latency_ms: i64,
    pub model_count: i64,
    pub error: Option<String>,
}

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
    pub top_models: Vec<ModelUsage>,
    pub hourly_distribution: Vec<HourlyCount>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelUsage {
    pub model: String,
    pub request_count: i64,
    pub total_tokens: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HourlyCount {
    pub hour: String,
    pub count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChecklistItem {
    pub id: String,
    pub session_id: String,
    pub title: String,
    pub status: String,
    pub priority: i32,
    pub source: String,
    pub created_at: String,
    pub completed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillAuditResult {
    pub skill_name: String,
    pub score: u32,
    pub issues: Vec<String>,
    pub suggestion: String,
    pub auto_fixed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QaResponse {
    pub answer: String,
    pub sources: Vec<crate_knowledge::SearchResult>,
    pub used_kb: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MailMessage {
    pub id: String,
    pub from_agent: String,
    pub to_agent: String,
    pub subject: String,
    pub body: String,
    pub read: bool,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallConfirmation {
    pub id: String,
    pub session_id: String,
    pub tool_name: String,
    pub tool_input: String,
    pub status: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillLockEntry {
    pub source: String,
    pub source_type: String,
    pub computed_hash: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skill_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillLockFile {
    pub version: u32,
    pub skills: std::collections::HashMap<String, SkillLockEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentExecConfig {
    pub agent_name: String,
    pub model: Option<String>,
    pub max_turns: Option<u32>,
    pub system_prompt_append: Option<String>,
    pub extra_args: Vec<String>,
    pub workspace_dir: Option<String>,
    pub timeout_minutes: Option<u32>,
    pub sandbox_mode: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutopilotConfig {
    pub task_id: String,
    pub agent_name: Option<String>,
    pub prompt_template: Option<String>,
    pub trigger_type: String,
    pub webhook_secret: Option<String>,
    pub webhook_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceGcConfig {
    pub enabled: bool,
    pub retention_days: u32,
    pub mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GcResult {
    pub scanned: usize,
    pub cleaned: usize,
    pub freed_bytes: u64,
    pub details: Vec<String>,
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

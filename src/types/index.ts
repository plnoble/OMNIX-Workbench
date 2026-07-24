/**
 * OMNIX Workbench - Shared Type Definitions
 *
 * All domain interfaces extracted from the monolithic App.tsx.
 * Each type maps to a Tauri backend command's request/response shape.
 */

/** A CLI agent detected on the host system */
export interface DetectedAgent {
  name: string;
  path: string;
  version: string;
  status: "installed" | "not_installed" | "not_found" | "broken";
}

/** An installed agent CLI's version vs the latest published on npm. */
export interface AgentUpdateInfo {
  name: string;
  current: string;
  latest: string | null;
  has_update: boolean;
  package: string | null;
}

/** Personal AI-coding activity stats (heatmap + streaks + share card). */
export interface ProfileDay {
  day: string;
  count: number;
}
export interface ProfileAgentCount {
  agent: string;
  count: number;
}
export interface ProfileStats {
  total_prompts: number;
  total_conversations: number;
  total_sessions: number;
  total_tokens: number;
  active_days: number;
  current_streak: number;
  longest_streak: number;
  first_active: string | null;
  per_agent: ProfileAgentCount[];
  daily: ProfileDay[];
}

/** A media generation task (image or async video) shown in the 创作 Studio. */
export interface MediaTask {
  id: string;
  platform_id: string;
  kind: "image" | "video";
  model: string;
  prompt: string;
  params_json: string;
  status: "pending" | "running" | "completed" | "failed";
  progress: number;
  external_id: string | null;
  result_path: string | null;
  error: string | null;
  created_at: string;
}

export interface MediaModelSuggestions {
  image: string[];
  video: string[];
}

/** An agent account with API credentials for model routing */
export interface AgentAccount {
  id: string;
  account_name: string;
  api_key: string;
  api_host: string;
  target_model: string;
  is_active: boolean;
  updated_at: string;
  agent_name?: string;
}

/** A conversation session metadata record */
export interface ConversationInfo {
  id: string;
  title: string;
  workspace_path: string;
  active_agent: string;
  created_at: string;
}

/** A single message within a conversation */
export interface ConversationMessage {
  id: string;
  conversation_id: string;
  role: "user" | "assistant" | "system";
  content: string;
  timestamp: string;
  /** Runtime enrichment, e.g. {"attachments": [paths]} for vision inputs. */
  metadata_json?: string | null;
}

/** An image the user attaches to a chat message (vision input). */
export interface ChatImageAttachment {
  mime: string;
  /** Raw base64 (no data-URL prefix) — sent to the agent. */
  data: string;
  /** Data URL for composer/bubble preview (frontend only). */
  preview: string;
  name: string;
}

/** A top-level workspace or team run */
export interface WorkspaceRun {
  id: string;
  title: string;
  workspace_path: string;
  manager_agent: string;
  status: string;
  summary: string;
  is_archived: boolean;
  created_at: string;
  updated_at: string;
}

/** A worker execution record attached to a workspace run */
export interface AgentRun {
  id: string;
  run_id: string;
  agent_name: string;
  task_title: string;
  status: string;
  session_id: string | null;
  started_at: string | null;
  completed_at: string | null;
  log_excerpt: string;
  assignment_id: string;
  dependencies: string[];
  acceptance_criteria: string[];
  retry_count: number;
  max_retries: number;
  result_summary: string;
  validation_status: string;
}

/** Draft assignment sent to the manager plan command */
export interface TeamAssignmentInput {
  agent_name: string;
  task_title: string;
  depends_on?: string[];
  acceptance_criteria?: string[];
  max_retries?: number;
}

/** A planned task owned by a worker agent */
export interface TeamAssignment {
  id: string;
  agent_name: string;
  task_title: string;
  status: string;
  depends_on: string[];
  acceptance_criteria: string[];
  max_retries: number;
}

/** Semi-automatic manager plan that must be approved before workers start */
export interface TeamPlan {
  run_id: string;
  goal: string;
  assignments: TeamAssignment[];
  status: string;
  created_at: string;
  approved_at: string | null;
}

export interface TeamRunDetail {
  run: WorkspaceRun;
  plan: TeamPlan | null;
  workers: AgentRun[];
}

/** Visible experimental surface tracked outside the core workflow */
export interface LabFeature {
  id: string;
  title: string;
  layer: string;
  status: string;
  risk: string;
  description: string;
  is_visible: boolean;
}

/** Canonical skill package metadata for package/sync workflows */
export interface SkillPackage {
  id: string;
  name: string;
  source_type: "local" | "git" | "imported";
  source_ref: string;
  version: string;
  status: string;
  updated_at: string;
}

/** Sync target for a skill package and an external agent CLI */
export interface SkillSyncTarget {
  id: string;
  skill_id: string;
  tool: string;
  target_path: string;
  mode: "copy" | "symlink";
  status: string;
  synced_at: string | null;
}

/** Binding from a run/team/agent to shared resource-layer services */
export interface ResourceBinding {
  id: string;
  owner_type: "run" | "agent" | "team" | "skill";
  owner_id: string;
  resource_type: "model" | "knowledge" | "memory" | "mcp" | "search";
  resource_id: string;
  status: string;
}

/** Where a product surface is shown in the configurable app shell */
export type NavigationPlacement = "pinned" | "launcher" | "hidden";

/** User-customizable navigation layout persisted in settings */
export interface NavigationLayout {
  pinned: string[];
  launcher: string[];
  hidden: string[];
}

/** A navigable product surface in OMNIX */
export interface AppEntry {
  id: string;
  label: string;
  title: string;
  description: string;
  group: "core" | "resource" | "assistant" | "labs" | "system";
  placement: NavigationPlacement;
  is_core?: boolean;
  is_experimental?: boolean;
  is_incomplete?: boolean;
}

/** Permission behavior for agent execution */
export type PermissionPolicy = "ask_every_time" | "ask_on_risk" | "full_access";

/** Work behavior independent from permission policy */
export type WorkMode = "direct" | "plan";

// Wire values match the Rust `AgentId` serde (rename_all = "snake_case"),
// so `OpenCode` serializes as "open_code".
export type RuntimeAgentId =
  | "claude_code"
  | "codex"
  | "gemini_cli"
  | "qwen_code"
  | "open_code"
  | "copilot_cli"
  | "grok";

export type RuntimeModelSelection =
  | { kind: "agent_default" }
  | { kind: "builtin"; model_name: string }
  | { kind: "omnix"; platform_id: string; model_name: string };

export type RuntimePermissionPolicy =
  | { kind: "ask_every_time" }
  | { kind: "ask_on_risk" }
  | { kind: "full_access"; confirmed: boolean };

export type ModelCompatibilityLevel = "native" | "gateway" | "unsupported" | "unhealthy";

export interface RuntimeModelOption {
  id: string;
  label: string;
  provider_name: string | null;
  provider_type: ProviderType | null;
  model_name: string | null;
  health_status: string;
  selection: RuntimeModelSelection;
  compatibility: {
    level: ModelCompatibilityLevel;
    selectable: boolean;
    reason: string;
  };
  /** The option the Work page should pre-select (Agent binding or global default). */
  is_default: boolean;
}

export type AgentSessionStatus =
  | "created"
  | "starting"
  | "running"
  | "awaiting_approval"
  | "stopping"
  | "completed"
  | "failed"
  | "cancelled";

export interface AgentSessionRecord {
  id: string;
  config: {
    conversation_id: string;
    agent: RuntimeAgentId;
    executable_path: string;
    workspace_path: string;
    model: RuntimeModelSelection;
    permission: RuntimePermissionPolicy;
    work_mode: WorkMode;
  };
  status: AgentSessionStatus;
  external_session_id: string | null;
  external_turn_id: string | null;
  last_error: string | null;
  created_at: string;
  updated_at: string;
}

export type RuntimeEventKind =
  | "session_started"
  | "user_message"
  | "assistant_delta"
  | "assistant_message"
  | "plan"
  | "tool_started"
  | "tool_completed"
  | "approval_requested"
  | "turn_completed"
  | "error"
  | "raw_log";

export interface RuntimeEvent {
  kind: RuntimeEventKind;
  text: string | null;
  external_session_id: string | null;
  external_turn_id: string | null;
  item_id: string | null;
  request_id: string | null;
  metadata: Record<string, unknown>;
}

export interface RuntimeSessionEvent {
  session_id: string;
  event: RuntimeEvent;
}

/// Model selection an ACP agent exposes via `session/new` configOptions.
/// Captured from the SessionStarted event so OMNIX can render a picker.
export interface AcpModelOption {
  config_id: string;
  current: string | null;
  options: Array<{ value: string; name: string }>;
}

export interface RuntimeApprovalRequest {
  session_id: string;
  request_id: string;
  approval_method: string;
  requested_permissions: unknown | null;
  title: string;
  detail: string;
}

export interface RuntimeAgentCatalogEntry {
  id: string;
  name: string;
  status: string;
  runtime_status: "supported" | "pending";
  installation_source: "system" | "managed" | null;
  executable_path: string | null;
  version: string | null;
  supports_structured_events: boolean;
  supports_resume: boolean;
  /** Runtime adapter: "claude_stream_json" | "codex_app_server" | "acp". */
  adapter: string;
  detail: string;
}

export interface WorkspaceSnapshot {
  root_path: string;
  root_name: string;
  branch: string | null;
  changes: Array<{ status: string; path: string }>;
  files: Array<{ path: string; name: string; is_dir: boolean; depth: number }>;
  truncated: boolean;
}

/** Local CLI agent runtime capability and model binding summary */
export interface AgentRuntimeProfile {
  id: string;
  name: string;
  icon: string;
  status: "installed" | "missing" | "unknown";
  version: string;
  executable_path: string;
  default_model: string;
  built_in_models: string[];
  supports_install: boolean;
  supports_update: boolean;
}

/** Named knowledge base containing documents */
export interface KnowledgeBase {
  id: string;
  name: string;
  description: string;
  document_count: number;
  embedding_status: "pending" | "in_progress" | "completed" | "failed";
  updated_at: string;
}

/** Manual knowledge binding for ordinary single-agent chat */
export interface ChatKnowledgeBinding {
  conversation_id: string | null;
  knowledge_base_ids: string[];
  enabled: boolean;
}

/** Supported LLM provider API protocol types */
export type ProviderType =
  | "openai"
  | "openai-response"
  | "anthropic"
  | "gemini"
  | "azure-openai"
  | "ollama"
  | "mistral"
  | "new-api"
  | "openai-compatible";

/** A model provider platform (e.g., OpenAI, Anthropic, Ollama) */
export interface ModelPlatform {
  id: string;
  name: string;
  api_type: ProviderType;
  api_key: string;
  api_address: string;
  is_enabled: boolean;
}

/** A model available under a platform, with capability flags */
export interface PlatformModel {
  id: string;
  platform_id: string;
  model_name: string;
  has_vision: boolean;
  has_audio: boolean;
  has_reasoning: boolean;
  has_coding: boolean;
  has_long_context: boolean;
  has_tool_use: boolean;
  has_embedding: boolean;
  has_speedy: boolean;
  is_enabled: boolean;
  status: string;
}

/** A cron scheduled task */
export interface CronTask {
  id: string;
  title: string;
  schedule: string;
  agent_name: string;
  args: string;
  workspace_dir: string;
  is_active: boolean;
  last_run: string | null;
  created_at: string;
}

/** A single execution run of a cron task */
export interface CronRun {
  id: string;
  cron_task_id: string;
  status: "success" | "failed" | "running";
  log_path: string;
  started_at: string;
  finished_at: string;
}

/** Remote access connection info for cross-device debugging */
export interface RemoteAccessInfo {
  ip: string;
  port: string;
  token: string;
  url: string;
}

/** Gateway status indicator */
export type GatewayStatus = "idle" | "busy" | "error";

/** PTY interactive prompt type detected from terminal output */
export type PromptType = "none" | "trust" | "update" | "menu" | "editor";

/** Preview file content type */
export type PreviewType = "html" | "markdown" | "image" | "diff";

/** Model connectivity test state */
export type ModelTestState =
  | "idle"
  | "testing"
  | "success"
  | "auth_error"
  | "rate_limited"
  | "error"
  | "unreachable"
  | "no_api_key";

/** Detailed health check result from backend */
export interface HealthCheckDetail {
  status: ModelTestState;
  http_code: number | null;
  latency_ms: number | null;
  message: string;
}

/** A platform API key entry (masked for display) */
export interface PlatformApiKey {
  id: string;
  platform_id: string;
  label: string;
  masked_key: string;
  is_active: boolean;
  last_status: "unknown" | "success" | "error";
  last_error: string | null;
  latency_ms: number | null;
  last_checked_at: string | null;
  created_at: string;
}

/** Settings sub-tab selection */
export type SettingsSubTab = "platform" | "system" | "diagnostics" | "mcp" | "backup";

// ── Knowledge Base Types ────────────────────────────────

export interface KnowledgeBase {
  id: string;
  name: string;
  description: string;
  document_count: number;
  created_at: string;
  updated_at: string;
}

/** A document in the Knowledge Base */
export interface KbDocument {
  id: string;
  knowledge_base_id: string;
  title: string;
  source_path: string;
  file_type: string;
  file_hash: string;
  chunk_count: number;
  total_chars: number;
  embedding_model: string;
  embedding_status: "pending" | "in_progress" | "completed" | "failed";
  created_at: string;
  updated_at: string;
}

/** A text chunk within a document */
export interface KbChunk {
  id: string;
  document_id: string;
  chunk_index: number;
  content: string;
  char_start: number;
  char_end: number;
  metadata: Record<string, unknown>;
  has_embedding: boolean;
}

/** A hybrid search result */
export interface SearchResult {
  chunk_id: string;
  document_id: string;
  document_title: string;
  knowledge_base_id: string;
  knowledge_base_name: string;
  content: string;
  metadata: Record<string, unknown>;
  bm25_score: number | null;
  vector_score: number | null;
  rrf_score: number;
  rank: number;
}

/** RAG query response */
export interface RagResponse {
  answer: string;
  sources: SearchResult[];
  query: string;
}

/** Embedding model info for selector dropdowns */
export interface EmbeddingModelInfo {
  model_name: string;
  platform_id: string;
  platform_name: string;
  api_type: string;
}

/** Chunking configuration */
export interface ChunkConfig {
  max_chunk_chars?: number;
  overlap_chars?: number;
  respect_boundaries?: boolean;
}

/** Embedding generation progress */
export interface EmbeddingProgress {
  document_id: string;
  total_chunks: number;
  embedded_chunks: number;
  status: string;
}

// ── Quick Assistant Types ──────────────────────────────

/** Quick Assistant query response */
export interface QaResponse {
  answer: string;
  sources: SearchResult[];
  used_kb: boolean;
}

// ── Selection Assistant Types ──────────────────────────

/** Capture result returned from the Rust selection engine */
export interface SelectionCaptureResult {
  text: string;
  source: "uia" | "clipboard";
  window_title: string;
  process_name: string;
  timestamp: string;
}

/** A selection history entry persisted in DB */
export interface SelectionHistoryEntry {
  id: string;
  captured_text: string;
  source: "uia" | "clipboard";
  window_title: string;
  process_name: string;
  created_at: string;
}

// ── Translation Types ─────────────────────────────────

/** A translation language entry */
export interface TranslateLanguage {
  langCode: string;   // e.g. "zh-cn", "en-us"
  value: string;      // e.g. "Chinese (Simplified)", "English"
  emoji: string;      // e.g. "🇨🇳", "🇺🇸"
}

/** Translation response from LLM */
export interface TranslateResponse {
  translatedText: string;
  detectedLang: string;
  targetLang: string;
}

/** Translation history entry */
export interface TranslateHistoryEntry {
  id: string;
  source_text: string;
  target_text: string;
  source_lang: string;
  target_lang: string;
  created_at: string;
}

// ── Search Types ────────────────────────────────────────

/** A configured search provider */
export interface SearchProvider {
  id: string;
  name: string;
  api_type: string;
  api_key: string;
  api_address: string;
  is_enabled: boolean;
}

/** A single web search result */
export interface WebSearchResult {
  title: string;
  url: string;
  snippet: string;
  source: string;
  position: number;
}

/** A search history entry */
export interface SearchHistoryEntry {
  id: string;
  query: string;
  provider_id: string;
  result_count: number;
  created_at: string;
}

// ── MCP Server Types ────────────────────────────────────

/** An MCP server configuration */
export interface McpServer {
  id: string;
  name: string;
  command: string;
  args: string;
  env: string;
  url: string;
  server_type: "stdio" | "sse";
  is_enabled: boolean;
}

// ── Backup Types ────────────────────────────────────────

/** Backup info for a single table */
export interface BackupTableInfo {
  table_name: string;
  row_count: number;
}

/** Import result */
export interface ImportResult {
  tables_restored: [string, number][];
  total_rows: number;
}

// ── Prompt Library Types ────────────────────────────────

/** A prompt library entry */
export interface PromptEntry {
  id: string;
  title: string;
  content: string;
  category: string;
  order_key: number;
  created_at: string;
}

// ── Activity Log Types ──────────────────────────────────

/** An activity log entry */
export interface ActivityLogEntry {
  id: string;
  action: string;
  target: string;
  details: string;
  created_at: string;
}

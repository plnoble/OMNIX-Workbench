/**
 * OMNIX DevFlow — Shared Type Definitions
 *
 * All domain interfaces extracted from the monolithic App.tsx.
 * Each type maps to a Tauri backend command's request/response shape.
 */

/** A CLI agent detected on the host system */
export interface DetectedAgent {
  name: string;
  path: string;
  version: string;
  status: "installed" | "not_found";
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
  created_at: string;
}

/** Settings sub-tab selection */
export type SettingsSubTab = "platform" | "system" | "mcp" | "backup";

// ── Knowledge Base Types ────────────────────────────────

/** A document in the Knowledge Base */
export interface KbDocument {
  id: string;
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

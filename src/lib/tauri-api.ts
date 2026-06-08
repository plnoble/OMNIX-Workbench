/**
 * OMNIX DevFlow — Typed Tauri IPC API Wrapper
 *
 * A thin typed layer over invoke() calls. Provides:
 * - Single source of truth for all Tauri command names
 * - Typed parameters and return values
 * - Easy mocking for future tests
 * - Centralized call-site discovery
 */

import { invoke } from "@tauri-apps/api/core";
import type {
  DetectedAgent,
  AgentAccount,
  ConversationInfo,
  ConversationMessage,
  ModelPlatform,
  PlatformModel,
  CronTask,
  CronRun,
  RemoteAccessInfo,
  KbDocument,
  KbChunk,
  SearchResult,
  RagResponse,
  EmbeddingModelInfo,
  EmbeddingProgress,
  ChunkConfig,
  QaResponse,
  SelectionCaptureResult,
  SelectionHistoryEntry,
  TranslateResponse,
  TranslateHistoryEntry,
  SearchProvider,
  WebSearchResult,
  SearchHistoryEntry,
  McpServer,
  BackupTableInfo,
  ImportResult,
  PromptEntry,
  ActivityLogEntry,
} from "@/types";

// ── Settings ──────────────────────────────────────────

export const settingsApi = {
  get: (key: string) => invoke<string | null>("get_app_setting", { key }),
  set: (key: string, value: string) => invoke("set_app_setting", { key, value }),
  syncExternalConfigs: () => invoke("sync_external_agent_configs"),
};

// ── Model Platforms ───────────────────────────────────

export const platformApi = {
  list: () => invoke<ModelPlatform[]>("get_model_platforms"),
  save: (platform: ModelPlatform) => invoke("save_model_platform", { platform }),
  delete: (id: string) => invoke("delete_model_platform", { id }),
  fetchRemoteModels: (platformId: string) => invoke<PlatformModel[]>("fetch_remote_models", { platformId }),
};

// ── Platform Models ───────────────────────────────────

export const modelApi = {
  listByPlatform: (platformId: string) => invoke<PlatformModel[]>("get_platform_models", { platformId }),
  save: (model: PlatformModel) => invoke("save_platform_model", { model }),
  delete: (id: string) => invoke("delete_platform_model", { id }),
  getActive: () => invoke<PlatformModel[]>("get_active_models"),
  checkStatus: (modelId: string) => invoke<string>("check_model_status", { modelId }),
  batchCheck: (platformId: string) => invoke<PlatformModel[]>("batch_check_models", { platformId }),
};

// ── Agent Accounts ────────────────────────────────────

export const accountApi = {
  list: () => invoke<AgentAccount[]>("get_agent_accounts"),
  save: (account: AgentAccount) => invoke("save_agent_account", { account }),
  switch: (id: string) => invoke("switch_agent_account", { id }),
  delete: (id: string) => invoke("delete_agent_account", { id }),
};

// ── Conversations ─────────────────────────────────────

export const conversationApi = {
  list: () => invoke<ConversationInfo[]>("get_all_conversations"),
  create: (params: { id: string; title: string; workspacePath: string; activeAgent: string }) =>
    invoke("create_conversation", params),
  delete: (id: string) => invoke("delete_conversation", { id }),
  getMessages: (conversationId: string) =>
    invoke<ConversationMessage[]>("get_conversation_messages", { conversationId }),
  addMessage: (params: { id: string; conversationId: string; role: string; content: string }) =>
    invoke("add_conversation_message", params),
};

// ── PTY Sessions ──────────────────────────────────────

export const ptyApi = {
  start: (params: { sessionId: string; agentName: string; exePath: string; args: string[]; workspaceDir: string }) =>
    invoke("start_agent_session", params),
  sendStdin: (params: { sessionId: string; input: string }) =>
    invoke("send_agent_stdin", params),
  stop: (sessionId: string) => invoke("stop_agent_session", { sessionId }),
};

// ── Agent Detection ───────────────────────────────────

export const agentApi = {
  detectInstalled: () => invoke<DetectedAgent[]>("detect_installed_agents"),
};

// ── Cron Tasks ────────────────────────────────────────

export const cronApi = {
  listTasks: () => invoke<CronTask[]>("get_cron_tasks"),
  saveTask: (task: CronTask) => invoke("save_cron_task", { task }),
  deleteTask: (id: string) => invoke("delete_cron_task", { id }),
  toggleActive: (params: { id: string; isActive: boolean }) =>
    invoke("toggle_cron_task_active", params),
  trigger: (id: string) => invoke("trigger_cron_task", { id }),
  listRuns: () => invoke<CronRun[]>("get_cron_runs"),
  clearRuns: () => invoke("clear_cron_runs"),
};

// ── File Preview ──────────────────────────────────────

export const previewApi = {
  listFiles: (workspacePath: string) =>
    invoke<string[]>("get_previewable_files", { workspacePath }),
  readFileAsBase64: (params: { workspacePath: string; fileName: string }) =>
    invoke<string>("read_file_as_base64", params),
  readFileContent: (params: { workspacePath: string; fileName: string }) =>
    invoke<string>("read_file_content_utf8", params),
  getGitDiff: (workspacePath: string) =>
    invoke<string>("get_workspace_git_diff", { workspacePath }),
};

// ── Environment Diagnostics ───────────────────────────

export const diagnosticsApi = {
  run: () => invoke<Record<string, string>>("run_env_diagnostics"),
  repair: (toolName: string) => invoke("repair_env_tool", { toolName }),
};

// ── Remote Access ─────────────────────────────────────

export const remoteApi = {
  getInfo: () => invoke<RemoteAccessInfo>("get_remote_access_info"),
};

// ── Knowledge Base ─────────────────────────────────────

export const knowledgeApi = {
  listDocuments: () => invoke<KbDocument[]>("kb_list_documents"),
  importDocument: (params: { title: string; sourcePath: string; fileType: string; content: string; chunkConfig?: ChunkConfig }) =>
    invoke<KbDocument>("kb_import_document", params),
  importFile: (params: { filePath: string; chunkConfig?: ChunkConfig }) =>
    invoke<KbDocument>("kb_import_file", params),
  importDirectory: (params: { directoryPath: string; extensions?: string }) =>
    invoke<KbDocument[]>("kb_import_directory", params),
  deleteDocument: (documentId: string) => invoke("kb_delete_document", { documentId }),
  getChunks: (documentId: string) => invoke<KbChunk[]>("kb_get_chunks", { documentId }),
  generateEmbeddings: (params: { documentId: string; modelName: string }) =>
    invoke<EmbeddingProgress>("kb_generate_embeddings", params),
  hybridSearch: (params: { query: string; embeddingModel: string; limit?: number }) =>
    invoke<SearchResult[]>("kb_hybrid_search", params),
  ragQuery: (params: { query: string; embeddingModel: string; chatModel: string; topK?: number }) =>
    invoke<RagResponse>("kb_rag_query", params),
  getEmbeddingModels: () => invoke<EmbeddingModelInfo[]>("kb_get_embedding_models"),
};

// ── Quick Assistant ────────────────────────────────────

export const qaApi = {
  toggle: (visible: boolean) => invoke("toggle_quick_assistant", { visible }),
  showWithText: (text: string) => invoke("show_quick_assistant_with_text", { text }),
  query: (params: { query: string; useKb: boolean; chatModel: string; embeddingModel?: string }) =>
    invoke<QaResponse>("qa_query", params),
  queryStream: (params: { query: string; useKb: boolean; chatModel: string; embeddingModel?: string }) =>
    invoke<string>("qa_query_stream", params),
};

// ── Selection Assistant ──────────────────────────────────

export const selectionApi = {
  captureAndShow: () => invoke("capture_selection_and_show"),
  getText: () => invoke<string>("get_selection_text"),
  getWithContext: () => invoke<SelectionCaptureResult>("get_selection_with_context"),
  getHistory: (limit?: number) =>
    invoke<SelectionHistoryEntry[]>("get_selection_history", { limit: limit ?? 50 }),
  deleteHistoryItem: (id: string) => invoke("delete_selection_history_item", { id }),
  clearHistory: () => invoke("clear_selection_history"),
};

// ── Translation ──────────────────────────────────────────

export const translationApi = {
  translate: (params: { text: string; targetLang: string; sourceLang?: string; chatModel?: string; prompt?: string }) =>
    invoke<TranslateResponse>("translate_text", params),
  detectLanguage: (params: { text: string; chatModel?: string }) =>
    invoke<string>("detect_language", params),
  getHistory: (limit?: number) =>
    invoke<TranslateHistoryEntry[]>("get_translation_history", { limit: limit ?? 50 }),
  deleteHistoryItem: (id: string) =>
    invoke("delete_translation_history_item", { id }),
  clearHistory: () => invoke("clear_translation_history"),
};

// ── Web Search ──────────────────────────────────────────

export const searchApi = {
  listProviders: () => invoke<SearchProvider[]>("get_search_providers"),
  saveProvider: (provider: SearchProvider) => invoke("save_search_provider", { provider }),
  deleteProvider: (id: string) => invoke("delete_search_provider", { id }),
  search: (query: string, providerId?: string, limit?: number) =>
    invoke<WebSearchResult[]>("web_search", { query, providerId, limit }),
  getHistory: (limit?: number) =>
    invoke<SearchHistoryEntry[]>("get_search_history", { limit: limit ?? 50 }),
  deleteHistoryItem: (id: string) => invoke("delete_search_history_item", { id }),
  clearHistory: () => invoke("clear_search_history"),
};

// ── MCP Servers ─────────────────────────────────────────

export const mcpApi = {
  list: () => invoke<McpServer[]>("get_mcp_servers"),
  save: (server: McpServer) => invoke("save_mcp_server", { server }),
  delete: (id: string) => invoke("delete_mcp_server", { id }),
};

// ── Data Backup ─────────────────────────────────────────

export const backupApi = {
  getInfo: () => invoke<BackupTableInfo[]>("get_backup_info"),
  exportBackup: (tables?: string[]) => invoke<string>("export_backup", { tables }),
  importBackup: (jsonStr: string, tables?: string[]) =>
    invoke<ImportResult>("import_backup", { jsonStr, tables }),
};

// ── Prompt Library ──────────────────────────────────────

export const promptApi = {
  list: () => invoke<PromptEntry[]>("get_prompt_library"),
  save: (entry: PromptEntry) => invoke("save_prompt_entry", { entry }),
  delete: (id: string) => invoke("delete_prompt_entry", { id }),
};

// ── Activity Log ────────────────────────────────────────

export const activityApi = {
  log: (action: string, target: string, details: string) =>
    invoke("log_activity", { action, target, details }),
  getRecent: (limit?: number) =>
    invoke<ActivityLogEntry[]>("get_activity_log", { limit: limit ?? 50 }),
};

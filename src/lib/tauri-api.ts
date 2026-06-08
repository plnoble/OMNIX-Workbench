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

// ── Skill Sync (P1 — DEC-018) ─────────────────────────

export interface ToolStatus {
  tool_id: string;
  display_name: string;
  is_installed: boolean;
  skill_base_path: string;
}

export interface SkillTargetRecord {
  id: string;
  skill_id: string;
  tool: string;
  target_path: string;
  mode: string;
  status: string;
  last_error: string | null;
  synced_at: number | null;
}

export interface DiscoveredSkill {
  name: string;
  path: string;
  tool: string;
  content_hash: string;
}

export interface SyncResult {
  tool: string;
  target_path: string;
  success: boolean;
  error: string | null;
}

export const skillSyncApi = {
  /** Get all tool adapters and their installation status */
  getToolStatus: () =>
    invoke<ToolStatus[]>("get_skill_tool_status"),

  /** Sync a skill to one or more tools */
  syncToTools: (skillName: string, toolIds: string[], mode?: "copy" | "symlink") =>
    invoke<SyncResult[]>("sync_skill_to_tools", { skillName, toolIds, mode: mode ?? "copy" }),

  /** Unsync (remove) a skill from a tool's directory */
  unsyncFromTool: (skillName: string, toolId: string) =>
    invoke<SyncResult>("unsync_skill_from_tool", { skillName, toolId }),

  /** Scan all tool directories for existing skills */
  scanAllToolSkills: () =>
    invoke<DiscoveredSkill[]>("scan_all_tool_skills"),

  /** Toggle skill starred status */
  toggleStarred: (skillName: string) =>
    invoke("toggle_skill_starred", { skillName }),

  /** Get sync targets for a specific skill */
  getSkillTargets: (skillName: string) =>
    invoke<SkillTargetRecord[]>("get_skill_targets", { skillName }),

  // ── P2: Sync Engine ──────────────────────────────────

  /** Check for conflicts before syncing */
  checkConflicts: (skillName: string, toolIds: string[]) =>
    invoke<ConflictInfo[]>("check_sync_conflicts", { skillName, toolIds }),

  /** Sync one skill to one tool with conflict strategy */
  syncDetailed: (skillName: string, toolId: string, mode?: "copy" | "symlink", strategy?: "skip" | "overwrite" | "rename") =>
    invoke<DetailedSyncResult>("sync_skill_detailed", { skillName, toolId, mode: mode ?? "copy", strategy: strategy ?? "overwrite" }),

  /** Sync one skill to multiple tools */
  syncToMany: (skillName: string, toolIds: string[], mode?: "copy" | "symlink", strategy?: "skip" | "overwrite" | "rename") =>
    invoke<BatchSyncResult>("sync_skill_to_many", { skillName, toolIds, mode: mode ?? "copy", strategy: strategy ?? "overwrite" }),

  /** Batch sync: sync multiple skills to all installed tools */
  syncBatch: (skillNames: string[], mode?: "copy" | "symlink", strategy?: "skip" | "overwrite" | "rename") =>
    invoke<BatchSyncResult[]>("sync_skills_batch", { skillNames, mode: mode ?? "copy", strategy: strategy ?? "overwrite" }),

  /** Check drift for a specific skill+tool */
  checkDrift: (skillName: string, toolId: string) =>
    invoke<DriftReport>("check_skill_drift", { skillName, toolId }),

  /** Check drift for all synced skills */
  checkAllDrift: () =>
    invoke<DriftReport[]>("check_all_drift"),

  /** Re-sync all skills that have drifted */
  resyncAllDrifted: (mode?: "copy" | "symlink") =>
    invoke<DetailedSyncResult[]>("resync_all_drifted", { mode: mode ?? "copy" }),

  // ── P4: Disk Scanner ─────────────────────────────────

  /** Scan all tool directories and classify every discovered skill */
  scanDiskSkills: () =>
    invoke<ScanReport>("scan_disk_skills"),

  /** Import unmanaged skills into the OMNIX database */
  importUnmanaged: (items: ScanItem[]) =>
    invoke<number>("import_unmanaged_skills", { items }),

  // ── P6: Package & Category ──────────────────────────

  /** Export a single skill as a .skill package */
  exportPackage: (skillName: string) =>
    invoke<string>("export_skill_package", { skillName }),

  /** Import a skill from a .zip/.skill package */
  importPackage: (zipPath: string) =>
    invoke<string>("import_skill_package", { zipPath }),

  /** Export all skills as individual .skill packages */
  exportAll: () =>
    invoke<string[]>("export_all_skills"),

  /** Update skill category */
  updateCategory: (skillName: string, category: string) =>
    invoke("update_skill_category", { skillName, category }),

  /** List available .skill packages in exports dir */
  listPackages: () =>
    invoke<string[]>("list_skill_packages"),

  // ── P5: Git Skill Source ────────────────────────────

  /** Clone a Git repository and discover skill candidates */
  cloneRepo: (repoUrl: string, branch?: string) =>
    invoke<GitCloneResult>("clone_skill_repo", { repoUrl, branch }),

  /** List skill candidates from a cached Git repo */
  listRepoSkills: (repoUrl: string) =>
    invoke<GitSkillCandidate[]>("list_repo_skills", { repoUrl }),

  /** Import a skill from a Git repo */
  importGitSkill: (repoUrl: string, skillName: string, revision: string) =>
    invoke<string>("import_git_skill", { repoUrl, skillName, revision }),

  /** Check for updates on Git-sourced skills */
  checkGitUpdates: () =>
    invoke<GitUpdateCheck[]>("check_git_updates"),

  /** Pull updates for a specific Git-sourced skill */
  pullAndUpdateSkill: (skillName: string) =>
    invoke<string>("pull_and_update_skill", { skillName }),

  /** Clean up expired Git skill cache */
  cleanupCache: () =>
    invoke<number>("cleanup_skill_cache"),
};

// ── P2 Sync Engine Types ──────────────────────────────

export interface ConflictInfo {
  tool_id: string;
  target_path: string;
  exists: boolean;
  existing_hash: string | null;
  source_hash: string;
  is_identical: boolean;
}

export interface DetailedSyncResult {
  skill_name: string;
  tool_id: string;
  target_path: string;
  success: boolean;
  conflict: ConflictInfo | null;
  strategy_used: "skip" | "overwrite" | "rename" | null;
  error: string | null;
}

export interface BatchSyncResult {
  total: number;
  succeeded: number;
  skipped: number;
  failed: number;
  details: DetailedSyncResult[];
}

export type DriftStatus = "InSync" | "Drifted" | "Missing" | "Modified" | "Unknown";

export interface DriftReport {
  skill_name: string;
  tool_id: string;
  status: DriftStatus;
  source_hash: string | null;
  target_hash: string | null;
  last_synced_hash: string | null;
}

// ── P4 Scanner Types ──────────────────────────────────

export type ScanClass = "Managed" | "Unmanaged" | "Drifted" | "Orphaned";

export interface ScanItem {
  name: string;
  tool_id: string;
  tool_display_name: string;
  path: string;
  content_hash: string;
  class: ScanClass;
  size_bytes: number;
  preview: string;
}

export interface ScannedTool {
  tool_id: string;
  display_name: string;
  is_installed: boolean;
  skill_count: number;
  skill_base_path: string;
}

export interface ScanReport {
  total_found: number;
  managed: ScanItem[];
  unmanaged: ScanItem[];
  drifted: ScanItem[];
  orphaned: ScanItem[];
  tools_scanned: ScannedTool[];
}

// ── P5 Git Skill Source Types ────────────────────────

export interface GitCloneResult {
  repo_url: string;
  cache_path: string;
  skill_count: number;
  revision: string;
}

export interface GitSkillCandidate {
  name: string;
  relative_path: string;
  local_path: string;
  preview: string;
  content_hash: string;
  already_imported: boolean;
}

export interface GitUpdateCheck {
  skill_name: string;
  source_ref: string;
  current_revision: string;
  latest_revision: string;
  has_update: boolean;
}

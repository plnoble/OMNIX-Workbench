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
  HealthCheckDetail,
  PlatformApiKey,
  WorkspaceRun,
  AgentRun,
  TeamAssignmentInput,
  TeamPlan,
  LabFeature,
} from "@/types";

// ── Settings ──────────────────────────────────────────

export const settingsApi = {
  get: (key: string) => invoke<string | null>("get_app_setting", { key }),
  set: (key: string, value: string) => invoke("set_app_setting", { key, value }),
  syncExternalConfigs: () => invoke("sync_external_agent_configs"),
};

export const shellApi = {
  pickDirectory: () => invoke<string | null>("pick_directory"),
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
  getAvailableNames: () => invoke<string[]>("get_available_models"),
  checkStatus: (modelId: string) => invoke<HealthCheckDetail>("check_model_status", { modelId }),
  batchCheck: (platformId: string) => invoke<PlatformModel[]>("batch_check_models", { platformId }),
  reinferCapabilities: (opts: { modelId?: string; platformId?: string }) => invoke<number>("reinfer_model_capabilities", opts),
};

// ── Platform API Keys (multi-key, encrypted) ──────────

export const apiKeyApi = {
  add: (platformId: string, key: string, label?: string) => invoke<PlatformApiKey>("add_platform_api_key", { platformId, key, label }),
  list: (platformId: string) => invoke<PlatformApiKey[]>("list_platform_api_keys", { platformId }),
  select: (keyId: string) => invoke("select_platform_api_key", { keyId }),
  delete: (keyId: string) => invoke("delete_platform_api_key", { keyId }),
  reveal: (keyId: string) => invoke<string>("reveal_platform_api_key", { keyId }),
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
  delete: (id: string) => invoke("delete_conversation", { conversationId: id }),
  archive: (id: string) => invoke("archive_conversation", { conversationId: id }),
  unarchive: (id: string) => invoke("unarchive_conversation", { conversationId: id }),
  listArchived: () => invoke<ConversationInfo[]>("get_archived_conversations"),
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
  install: (agentName: string) => invoke("install_agent_cli", { agentName }),
  update: (agentName: string) => invoke("repair_installed_agent", { agentName }),
};

// Workbench Runs

export const workbenchApi = {
  createRun: (title: string, workspacePath: string, managerAgent: string) =>
    invoke<WorkspaceRun>("create_workspace_run", { title, workspacePath, managerAgent }),
  listRuns: (includeArchived?: boolean) =>
    invoke<WorkspaceRun[]>("list_workspace_runs", { includeArchived }),
  getRun: (runId: string) =>
    invoke<WorkspaceRun>("get_workspace_run", { runId }),
  proposePlan: (runId: string, goal: string, assignments: TeamAssignmentInput[]) =>
    invoke<TeamPlan>("propose_team_plan", { runId, goal, assignments }),
  getPlan: (runId: string) =>
    invoke<TeamPlan>("get_team_plan", { runId }),
  approvePlan: (runId: string) =>
    invoke<TeamPlan>("approve_team_plan", { runId }),
  startAgentRun: (runId: string, agentName: string, taskTitle: string, status?: string) =>
    invoke<AgentRun>("start_agent_run", { runId, agentName, taskTitle, status }),
  listAgentRuns: (runId: string) =>
    invoke<AgentRun[]>("list_agent_runs", { runId }),
};

export const labsApi = {
  listFeatures: () => invoke<LabFeature[]>("list_lab_features"),
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
  toggleAutoCapture: (enabled: boolean) => invoke<boolean>("toggle_selection_auto_capture", { enabled }),
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

// ── Agent Templates (Multica-inspired) ───────────────

export interface TemplateSkill {
  name: string;
  description: string;
}

export interface AgentTemplate {
  slug: string;
  name: string;
  description: string;
  category: string;
  icon: string;
  accent: string;
  instructions: string;
  skills: TemplateSkill[];
}

export const agentTemplateApi = {
  /** Get all built-in agent templates */
  getAll: () =>
    invoke<AgentTemplate[]>("get_agent_templates"),

  /** Get a specific template by slug */
  getBySlug: (slug: string) =>
    invoke<AgentTemplate | null>("get_agent_template", { slug }),
};

// ── Skills Lock File (Multica-inspired) ──────────────

export interface SkillLockEntry {
  source: string;
  source_type: string;
  computed_hash: string;
  skill_path?: string;
}

export interface SkillLockFile {
  version: number;
  skills: Record<string, SkillLockEntry>;
}

export const skillLockApi = {
  /** Read current skills-lock.json */
  get: () => invoke<SkillLockFile>("get_skill_lock"),

  /** Update lock file from current DB state */
  update: () => invoke<SkillLockFile>("update_skill_lock"),

  /** Verify lock file against DB, returns list of issues */
  verify: () => invoke<string[]>("verify_skill_lock"),
};

// ── Agent Execution Environment (Multica-inspired) ───

export interface AgentExecConfig {
  agent_name: string;
  model: string | null;
  max_turns: number | null;
  system_prompt_append: string | null;
  extra_args: string[];
  workspace_dir: string | null;
  timeout_minutes: number | null;
  sandbox_mode: string | null;
}

export const agentExecApi = {
  /** Get execution config for an agent */
  getConfig: (agentName: string) =>
    invoke<AgentExecConfig>("get_agent_exec_config", { agentName }),

  /** Save execution config */
  saveConfig: (config: AgentExecConfig) =>
    invoke("save_agent_exec_config", { config }),
};

// ── Autopilot (Multica-inspired) ─────────────────────

export interface AutopilotConfig {
  task_id: string;
  agent_name: string | null;
  prompt_template: string | null;
  trigger_type: string;     // "cron" | "webhook"
  webhook_secret: string | null;
  webhook_url: string | null;
}

export const autopilotApi = {
  /** Get autopilot config for a cron task */
  getConfig: (taskId: string) =>
    invoke<AutopilotConfig>("get_autopilot_config", { taskId }),

  /** Save autopilot config */
  saveConfig: (config: AutopilotConfig) =>
    invoke("save_autopilot_config", { config }),

  /** Save autopilot execution result to knowledge base */
  saveResultToKb: (taskId: string, resultContent: string) =>
    invoke<string>("save_autopilot_result_to_kb", { taskId, resultContent }),
};

// ── Workspace GC (Multica-inspired) ──────────────────

export interface WorkspaceGcConfig {
  enabled: boolean;
  retention_days: number;
  mode: string;  // "full" | "artifacts-only" | "orphan-only"
}

export interface GcResult {
  scanned: number;
  cleaned: number;
  freed_bytes: number;
  details: string[];
}

export const workspaceGcApi = {
  /** Get GC config */
  getConfig: () => invoke<WorkspaceGcConfig>("get_gc_config"),

  /** Save GC config */
  saveConfig: (config: WorkspaceGcConfig) =>
    invoke("save_gc_config", { config }),

  /** Execute garbage collection */
  run: () => invoke<GcResult>("run_workspace_gc"),
};

// ── Request Logs & Usage Stats (New API/Sub2API inspired)

export interface RequestLogEntry {
  id: number;
  timestamp: string;
  model: string;
  platform: string;
  prompt_tokens: number;
  completion_tokens: number;
  total_tokens: number;
  latency_ms: number;
  status_code: number;
  is_stream: boolean;
  is_error: boolean;
  error_message: string;
  request_id: string;
  source: string;
}

export interface ModelUsage {
  model: string;
  request_count: number;
  total_tokens: number;
}

export interface HourlyCount {
  hour: string;
  count: number;
}

export interface UsageStats {
  total_requests: number;
  total_tokens: number;
  total_errors: number;
  avg_latency_ms: number;
  requests_today: number;
  tokens_today: number;
  top_models: ModelUsage[];
  hourly_distribution: HourlyCount[];
}

export const requestLogApi = {
  /** Get request logs with pagination */
  getLogs: (page?: number, limit?: number, modelFilter?: string) =>
    invoke<RequestLogEntry[]>("get_request_logs", { page, limit, modelFilter }),

  /** Get usage statistics summary */
  getStats: () => invoke<UsageStats>("get_usage_stats"),

  /** Delete old logs */
  cleanup: (keepDays?: number) =>
    invoke<number>("cleanup_request_logs", { keepDays }),
};

// ── Platform Health (New API/Sub2API inspired) ───────

export interface PlatformHealth {
  id: string;
  name: string;
  api_type: string;
  is_enabled: boolean;
  is_healthy: boolean;
  weight: number;
  priority: number;
  consecutive_failures: number;
  last_error: string | null;
  last_used_at: string | null;
  model_count: number;
}

export const platformHealthApi = {
  /** Get health status of all platforms */
  getAll: () => invoke<PlatformHealth[]>("get_platform_health"),

  /** Reset a platform's health status */
  reset: (platformId: string) =>
    invoke("reset_platform_health", { platformId }),

  /** Update platform weight and priority */
  updateRouting: (platformId: string, weight: number, priority: number) =>
    invoke("update_platform_routing", { platformId, weight, priority }),
};

// ── Upstream Model Auto-Sync (New API inspired) ──────

export interface ModelSyncResult {
  platform_id: string;
  platform_name: string;
  upstream_models: string[];
  local_models: string[];
  new_models: string[];
  removed_models: string[];
  unchanged_models: string[];
  error: string | null;
}

export const modelSyncApi = {
  /** Sync upstream models for a single platform */
  syncPlatform: (platformId: string) =>
    invoke<ModelSyncResult>("sync_upstream_models", { platformId }),

  /** Apply model sync: add/remove models */
  apply: (platformId: string, modelsToAdd: string[], modelsToRemove: string[]) =>
    invoke<[number, number]>("apply_model_sync", { platformId, modelsToAdd, modelsToRemove }),

  /** Sync all enabled platforms */
  syncAll: () =>
    invoke<ModelSyncResult[]>("sync_all_upstream_models"),
};

// ── Platform Health Check (New API/Sub2API inspired) ──

export interface HealthCheckResult {
  platform_id: string;
  platform_name: string;
  is_reachable: boolean;
  latency_ms: number;
  model_count: number;
  error: string | null;
}

export const healthCheckApi = {
  /** Check health of all enabled platforms */
  checkAll: () =>
    invoke<HealthCheckResult[]>("check_all_platform_health"),
};

// ── Agent Task Lifecycle (Multica inspired) ──────────

export interface TaskInfo {
  id: string;
  title: string;
  active_agent: string;
  workspace_path: string;
  task_status: "pending" | "running" | "completed" | "failed";
  task_started_at: string | null;
  task_completed_at: string | null;
  task_duration_ms: number | null;
  task_summary: string | null;
  task_files_changed: number;
  task_exit_code: number | null;
  is_archived: boolean;
  created_at: string;
}

export interface TaskStats {
  total: number;
  running: number;
  completed: number;
  failed: number;
  avg_duration_ms: number;
}

export const taskLifecycleApi = {
  /** Get all tasks with lifecycle info */
  getList: (includeArchived?: boolean) =>
    invoke<TaskInfo[]>("get_task_list", { includeArchived: includeArchived ?? false }),

  /** Transition task to running */
  start: (conversationId: string) =>
    invoke("task_start", { conversationId }),

  /** Transition task to completed */
  complete: (conversationId: string, summary?: string, filesChanged?: number) =>
    invoke("task_complete", { conversationId, summary, filesChanged }),

  /** Transition task to failed */
  fail: (conversationId: string, exitCode?: number, errorSummary?: string) =>
    invoke("task_fail", { conversationId, exitCode, errorSummary }),

  /** Archive a task */
  archive: (conversationId: string) =>
    invoke("task_archive", { conversationId }),

  /** Get task statistics */
  getStats: () =>
    invoke<TaskStats>("get_task_stats"),
};

// ── Skill Compound Interest (Multica inspired) ───────

export const skillCompoundApi = {
  /** Record a skill usage (boosts priority on success) */
  recordUsage: (skillName: string, success: boolean) =>
    invoke("record_skill_usage", { skillName, success }),

  /** Get top skills ranked by compound interest */
  getTopByUsage: (limit?: number) =>
    invoke<Array<{
      name: string;
      description: string;
      category: string | null;
      usage_count: number;
      success_count: number;
      priority_score: number;
      starred: boolean;
    }>>("get_top_skills_by_usage", { limit }),
};

// ── Odysseus-Inspired APIs ────────────────────────────

// Prompt Injection Guard
export const promptGuardApi = {
  wrap: (content: string, source: string) =>
    invoke<string>("wrap_untrusted_content", { content, source }),
};

// Development Checklist
export interface ChecklistItem {
  id: string;
  session_id: string;
  title: string;
  status: "pending" | "in_progress" | "done";
  priority: number;
  source: string;
  created_at: string;
  completed_at: string | null;
}

export const checklistApi = {
  add: (sessionId: string, title: string, priority?: number, source?: string) =>
    invoke<ChecklistItem>("checklist_add", { sessionId, title, priority, source }),
  update: (itemId: string, status: string) =>
    invoke("checklist_update", { itemId, status }),
  get: (sessionId?: string, includeDone?: boolean) =>
    invoke<ChecklistItem[]>("checklist_get", { sessionId, includeDone }),
  summary: (sessionId: string) =>
    invoke<string>("checklist_summary", { sessionId }),
};

// Context Budget
export interface ContextBudget {
  model_limit: number;
  estimated_tokens: number;
  remaining_tokens: number;
  usage_percent: number;
  status: "ok" | "warning" | "critical";
}

export const contextBudgetApi = {
  estimateTokens: (text: string) =>
    invoke<number>("estimate_tokens", { text }),
  getBudget: (conversationId: string, modelLimit?: number) =>
    invoke<ContextBudget>("get_context_budget", { conversationId, modelContextLimit: modelLimit }),
};

// Skill Audit
export interface SkillAuditResult {
  skill_name: string;
  score: number;
  issues: string[];
  suggestion: string;
  auto_fixed: boolean;
}

export const skillAuditApi = {
  run: () => invoke<SkillAuditResult[]>("run_skill_audit"),
};

// Event Bus
export const eventBusApi = {
  register: (eventType: string, threshold: number, taskId: string) =>
    invoke<string>("register_event_trigger", { eventType, threshold, taskId }),
  list: () => invoke<Array<{
    id: string; event_type: string; threshold: number;
    task_id: string; current_count: number; enabled: boolean;
  }>>("get_event_triggers"),
};

// Encryption
export const encryptionApi = {
  encrypt: (plaintext: string) => invoke<string>("encrypt_value", { plaintext }),
  decrypt: (encrypted: string) => invoke<string>("decrypt_value", { encrypted }),
};

// Desktop Notifications
export const notificationApi = {
  send: (title: string, body: string) =>
    invoke("send_desktop_notification", { title, body }),
  sendNtfy: (server: string, topic: string, title: string, message: string, priority?: string) =>
    invoke("send_ntfy_notification", { server, topic, title, message, priority }),
};

// Context Compaction
export interface CompactResult {
  compacted: number; total: number; summary: string | null; message: string;
}
export const contextCompactApi = {
  compact: (conversationId: string, keepRecent?: number) =>
    invoke<CompactResult>("compact_conversation_context", { conversationId, keepRecent }),
};

// Cookbook Model Recommendations
export type EvidenceTier = "Direct" | "Variant" | "BaseModel" | "LineInterp" | "SelfReported";
export interface ModelEntry {
  name: string; display_name: string; size_gb: number; min_vram_gb: number;
  categories: string[]; quality: number; description: string; ollama_cmd: string; speed_rating: string;
  family: string; generation: number; evidence_tier: EvidenceTier; confidence: number;
  is_moe: boolean; active_params_gb: number | null;
}
export interface ModelRecommendation {
  model: ModelEntry; fits_vram: boolean; fits_ram: boolean;
  overall_fit: "perfect" | "tight" | "impossible"; install_cmd: string;
  effective_quality: number; confidence_label: string;
}
export interface HardwareInfo {
  gpu: { name: string; vram_mb: number; vendor: string } | null;
  ram_mb: number; cpu_cores: number; os: string;
}
export interface GpuSpec {
  name: string; vram_mb: number; bandwidth_gb_s: number; vendor: string; generation: string;
}
export const cookbookApi = {
  getRecommendations: () => invoke<{ hardware: HardwareInfo; recommendations: ModelRecommendation[] }>("get_model_recommendations"),
  getDatabase: () => invoke<ModelEntry[]>("get_model_database"),
  /** Simulate recommendations for a hypothetical GPU */
  recommendForGpu: (gpuName: string) => invoke<{ gpu: GpuSpec | null; recommendations: ModelRecommendation[] }>("recommend_for_gpu", { gpuName }),
  /** Get full GPU database */
  getGpuDatabase: () => invoke<GpuSpec[]>("get_gpu_database"),
};

// Code Deep Analysis
export interface CodebaseAnalysis {
  path: string; total_files: number; total_lines: number;
  languages: Record<string, number>; largest_files: Array<{ name: string; size_bytes: number }>;
}
export const codeAnalysisApi = {
  analyze: (path: string) => invoke<CodebaseAnalysis>("analyze_codebase", { path }),
};

// Config Backup (ZCF inspired)
export interface BackupEntry {
  name: string; path: string; size_bytes: number; created_at: number;
}
export const configBackupApi = {
  backup: (filePath: string, category: string) =>
    invoke<string | null>("backup_config_file", { filePath, category }),
  list: (category: string) =>
    invoke<BackupEntry[]>("list_backups", { category }),
  restore: (backupPath: string, targetPath: string) =>
    invoke("restore_backup", { backupPath, targetPath }),
};

// API Provider Preset (ZCF inspired)
export const apiPresetApi = {
  apply: (presetId: string, apiKey: string) =>
    invoke<string>("apply_api_preset", { presetId, apiKey }),
};

// Architecture Graph (Understand-Anything inspired)
export type NodeType = "file" | "directory" | "module" | "function" | "class" | "interface" | "component" | "hook" | "route" | "config" | "test" | "style" | "asset" | "domain" | "flow" | "external";
export type EdgeType = "contains" | "imports" | "exports" | "calls" | "extends" | "implements" | "depends_on" | "belongs_to" | "configures" | "tests" | "styles";
export type ArchLayer = "api" | "service" | "data" | "ui" | "utility" | "config" | "test" | "infrastructure" | "unknown";

export interface GraphNode {
  id: string; name: string; node_type: NodeType; path: string; layer: ArchLayer;
  language: string | null; summary: string | null; line_count: number;
  fingerprint: string; complexity: string | null; tags: string[];
}
export interface GraphEdge { source: string; target: string; edge_type: EdgeType; weight: number; }
export interface GraphStats {
  total_files: number; total_lines: number;
  languages: Record<string, number>; layers: Record<string, number>;
  node_count: number; edge_count: number;
}
export interface ArchitectureGraph {
  version: number; project_path: string; project_name: string; generated_at: string;
  nodes: GraphNode[]; edges: GraphEdge[];
  layers: Record<string, string[]>; stats: GraphStats;
}
export const architectureApi = {
  build: (projectPath: string) => invoke<ArchitectureGraph>("build_architecture_graph", { projectPath }),
  save: (graph: ArchitectureGraph) => invoke<string>("save_architecture_graph", { graph }),
  load: (projectName: string) => invoke<ArchitectureGraph>("load_architecture_graph", { projectName }),
  getIgnorePatterns: (projectPath: string) => invoke<string[]>("get_ignore_patterns", { projectPath }),
};

// Skill Library Features (Skill Library inspired)
export interface SkillMatch {
  skill_name: string; relevance_score: number;
  matched_keywords: string[]; content_preview: string;
}
export interface SandboxTestCase { input: string; expected_behavior: string; }
export interface TestCaseScore { input: string; agent_response: string; auditor_score: number; auditor_feedback: string; }
export interface SandboxResult {
  skill_name: string; test_cases_total: number; test_cases_passed: number;
  average_score: number; scores: TestCaseScore[]; overall_verdict: string;
}
export interface ProtocolAction { action_type: string; target: string; content: string; raw_block: string; }
export interface MarketSkill {
  source: string; name: string; description: string; url: string;
  author: string; stars: number | null; downloaded: boolean;
}
export interface DistillRecommendation {
  suggested_name: string; suggested_category: string; reason: string;
  source_evidence: string[]; confidence: number;
}
export const skillLibraryApi = {
  /** Find skills matching a message (semantic injection) */
  matchForInjection: (message: string) =>
    invoke<SkillMatch[]>("match_skills_for_injection", { message }),
  /** Test a skill in sandbox */
  testSandbox: (skillName: string) =>
    invoke<SandboxResult>("test_skill_sandbox", { skillName }),
  /** Parse protocol blocks from AI output */
  interceptProtocols: (output: string) =>
    invoke<ProtocolAction[]>("intercept_protocols", { output }),
  /** Execute a protocol action */
  executeProtocol: (action: ProtocolAction) =>
    invoke<string>("execute_protocol", { action }),
  /** Search external skill markets */
  searchMarket: (query: string) =>
    invoke<MarketSkill[]>("search_skill_market", { query }),
  /** Distill skills from project history */
  distill: (projectPath: string) =>
    invoke<DistillRecommendation[]>("distill_from_project", { projectPath }),
};

export interface ProtocolFilePreview {
  path: string; label: string; exists: boolean; action: string; description: string;
}
export interface ProtocolInitPreview {
  workspace_path: string; project_name: string; files: ProtocolFilePreview[];
  will_create_count: number; will_skip_count: number;
}
export interface ProjectProtocolStatus {
  workspace_path: string; project_name: string; enabled: boolean; initialized: boolean;
  run_id: string | null; last_event_at: string | null; pending_actions: number; pending_proposals: number;
}
export interface ProjectProtocolEvent {
  id: string; workspace_path: string; event_type: string; summary: string;
  details_json: string; created_at: string;
}
export interface ProtocolActionDraft {
  id: string; workspace_path: string; action_type: string; title: string;
  content: string; diff_json: string; status: string; created_at: string; applied_at: string | null;
}
export interface DistillationRun {
  id: string; workspace_path: string; source_summary: string;
  memory_count: number; proposal_count: number; status: string; created_at: string;
}
export interface EvolutionProposal {
  id: string; workspace_path: string; proposal_type: string; title: string;
  rationale: string; diff_json: string; status: string; created_at: string; applied_at: string | null;
}
export const projectProtocolApi = {
  getStatus: (workspacePath: string) =>
    invoke<ProjectProtocolStatus>("protocol_get_status", { workspacePath }),
  previewInit: (workspacePath: string, projectName?: string) =>
    invoke<ProtocolInitPreview>("protocol_preview_init", { workspacePath, projectName }),
  initWorkspace: (workspacePath: string, projectName: string | undefined, enable: boolean) =>
    invoke<ProjectProtocolStatus>("protocol_init_workspace", { workspacePath, projectName, enable }),
  recordEvent: (workspacePath: string, eventType: string, summary: string, detailsJson?: string) =>
    invoke<ProjectProtocolEvent>("protocol_record_event", { workspacePath, eventType, summary, detailsJson }),
  archiveAndDistill: (workspacePath: string, summary?: string) =>
    invoke<DistillationRun>("protocol_archive_and_distill", { workspacePath, summary }),
  listActions: (workspacePath: string, status?: string) =>
    invoke<ProtocolActionDraft[]>("protocol_list_actions", { workspacePath, status }),
  applyAction: (actionId: string, approved: boolean) =>
    invoke<ProtocolActionDraft>("protocol_apply_action", { actionId, approved }),
  listEvolutionProposals: (workspacePath: string, status?: string) =>
    invoke<EvolutionProposal[]>("protocol_list_evolution_proposals", { workspacePath, status }),
  applyEvolutionProposal: (proposalId: string, approved: boolean) =>
    invoke<EvolutionProposal>("protocol_apply_evolution_proposal", { proposalId, approved }),
};

export interface SkillSetItem {
  id: string; skill_set_id: string; skill_id: string; order_num: number; created_at: string;
}
export interface SkillSet {
  id: string; name: string; description: string; sync_targets: string[];
  items: SkillSetItem[]; created_at: string; updated_at: string;
}
export const skillSetApi = {
  create: (name: string, description: string, skillIds: string[], syncTargets: string[]) =>
    invoke<SkillSet>("create_skill_set", { name, description, skillIds, syncTargets }),
  list: () => invoke<SkillSet[]>("list_skill_sets"),
  update: (skillSetId: string, name: string, description: string, skillIds: string[], syncTargets: string[]) =>
    invoke<SkillSet>("update_skill_set", { skillSetId, name, description, skillIds, syncTargets }),
  delete: (skillSetId: string) =>
    invoke("delete_skill_set", { skillSetId }),
  syncToTools: (skillSetId: string, toolIds: string[], mode = "copy", strategy = "overwrite") =>
    invoke("sync_skill_set_to_tools", { skillSetId, toolIds, mode, strategy }),
};

// DeepSeek-GUI Inspired APIs
export interface FileChange {
  file_path: string; change_type: string;
  old_content: string | null; new_content: string | null;
  diff_summary: string; timestamp: number;
}
export const tokenEconomyApi = {
  compressToolResult: (content: string, maxLines?: number, maxBytes?: number) =>
    invoke<string>("compress_tool_result", { content, maxLines, maxBytes }),
  pushSteering: (sessionId: string, content: string) =>
    invoke<string>("push_steering_message", { sessionId, content }),
  getSteeringMessages: (sessionId: string) =>
    invoke<Array<{ id: string; content: string; created_at: string }>>("get_steering_messages", { sessionId }),
  consumeSteering: (sessionId: string) =>
    invoke("consume_steering_messages", { sessionId }),
  detectFileChange: (filePath: string, oldContent?: string, newContent?: string) =>
    invoke<FileChange>("detect_file_change", { filePath, oldContent, newContent }),
};

// Agent-Platform Bindings (CC Switch inspired)
export interface AgentPlatformBinding {
  agent_name: string; platform_id: string; platform_name: string;
  model_name: string | null; binding_kind: "default" | "builtin" | "omnix";
  builtin_model: string | null; enabled: boolean;
}
export const agentBindingApi = {
  getAll: () => invoke<AgentPlatformBinding[]>("get_agent_bindings"),
  set: (
    agentName: string,
    platformId: string,
    modelName?: string,
    bindingKind: "builtin" | "omnix" = "omnix",
    builtinModel?: string,
  ) =>
    invoke("set_agent_binding", { agentName, platformId, modelName, bindingKind, builtinModel }),
  setBuiltin: (agentName: string, builtinModel: string) =>
    invoke("set_agent_binding", {
      agentName,
      platformId: "__agent_builtin__",
      modelName: null,
      bindingKind: "builtin",
      builtinModel,
    }),
  remove: (agentName: string) =>
    invoke("remove_agent_binding", { agentName }),
  toggle: (agentName: string) =>
    invoke("toggle_agent_binding", { agentName }),
};

// Circuit Breaker & Session Usage (CC Switch inspired)
export type CircuitState = "Closed" | "Open" | "HalfOpen";
export interface CircuitBreakerStatus {
  platform_id: string; state: CircuitState; consecutive_failures: number;
  total_failures: number; total_successes: number;
  last_failure_at: string | null; last_success_at: string | null;
  last_error: string | null; half_open_threshold: number; failure_threshold: number;
}
export const circuitBreakerApi = {
  getStatus: () => invoke<CircuitBreakerStatus[]>("get_circuit_status"),
  reset: (platformId: string) => invoke("reset_circuit_breaker", { platformId }),
  getModelPricing: () => invoke<Record<string, [number, number]>>("get_model_pricing"),
  estimateCost: (model: string, promptTokens: number, completionTokens: number) =>
    invoke<number>("estimate_model_cost", { model, promptTokens, completionTokens }),
};

// Skill DAG (SkillDAG inspired)
export type DagEdgeType = "depends_on" | "specializes" | "composes_with" | "similar_to" | "conflicts_with";
export interface ConflictPair { skill_a: string; skill_b: string; reason: string; }
export interface SkillSearchResult { matches: string[]; neighbors: string[]; conflicts: ConflictPair[]; }
export interface SetValidation {
  valid: boolean; missing_deps: string[]; conflicts: ConflictPair[];
  redundant: [string, string][]; suggestions: string[];
}
export const skillDagApi = {
  search: (query: string, topK?: number) =>
    invoke<SkillSearchResult>("search_skills_dag", { query, topK }),
  checkSet: (skillIds: string[]) =>
    invoke<SetValidation>("check_skill_set", { skillIds }),
  expandSet: (skillIds: string[]) =>
    invoke<string[]>("expand_skill_set", { skillIds }),
  addEdge: (source: string, target: string, edgeType: string, reason: string) =>
    invoke<string>("add_skill_edge", { source, target, edgeType, reason }),
  removeEdge: (source: string, target: string, edgeType: string) =>
    invoke<string>("remove_skill_edge", { source, target, edgeType }),
};

// Async Agent Mailbox (AionUi inspired)
export interface MailMessage {
  id: string; from_agent: string; to_agent: string;
  subject: string; body: string; read: boolean; created_at: string;
}
export const mailboxApi = {
  send: (fromAgent: string, toAgent: string, subject: string, body: string) =>
    invoke<string>("send_mail", { fromAgent, toAgent, subject, body }),
  get: (agentName: string, includeRead?: boolean) =>
    invoke<MailMessage[]>("get_mail", { agentName, includeRead }),
  markRead: (messageIds: string[]) =>
    invoke("mark_mail_read", { messageIds }),
};

// Enhanced Task Dependencies (AionUi inspired)
export const taskDependencyApi = {
  setBlocks: (taskId: string, blocksIds: string[]) =>
    invoke("set_task_blocks", { taskId, blocksIds }),
  autoUnblock: (completedTaskId: string) =>
    invoke<string[]>("auto_unblock_tasks", { completedTaskId }),
};

// YOLO Full-Auto Mode (AionUi inspired)
export interface YoloModeConfig {
  /** Permission level: "off" | "safe" | "moderate" | "full" */
  level: string;
  /** Whether auto-retry is enabled for failed operations */
  auto_retry: boolean;
  /** Max consecutive auto-retries before requiring manual confirmation */
  max_retries: number;
}

export const yoloApi = {
  /** Get YOLO mode on/off status (backward compatible) */
  getStatus: () => invoke<boolean>("get_yolo_mode"),
  /** Toggle YOLO mode on/off (backward compatible) */
  set: (enabled: boolean) => invoke("set_yolo_mode", { enabled }),
  /** Get full YOLO mode configuration with graded permissions */
  getConfig: () => invoke<YoloModeConfig>("get_yolo_mode_config"),
  /** Set YOLO mode configuration with graded permissions */
  setConfig: (config: Partial<YoloModeConfig>) => invoke("set_yolo_mode_config", { config }),
  /** Check if a specific tool call should be auto-approved under current YOLO mode */
  checkPermission: (toolName: string, dangerLevel: "safe" | "moderate" | "dangerous") =>
    invoke<{ auto_approved: boolean; yolo_level: string; tool_name: string; danger_level: string; auto_retry: boolean; max_retries: number }>(
      "check_yolo_permission", { toolName, dangerLevel }
    ),
};

// Persistent Cron (AionUi inspired)
export interface PersistentCronTask {
  id: string; name: string; schedule: string; timezone: string;
  agent_name: string | null; prompt_template: string | null;
  mode: string; keep_awake: boolean; enabled: boolean;
  last_run_at: string | null; next_run_at: string | null;
}
export const persistentCronApi = {
  getAll: () => invoke<PersistentCronTask[]>("get_persistent_cron_tasks"),
  create: (name: string, schedule: string, timezone?: string, agentName?: string, promptTemplate?: string, mode?: string, keepAwake?: boolean) =>
    invoke<string>("create_persistent_cron", { name, schedule, timezone, agentName, promptTemplate, mode, keepAwake }),
  delete: (taskId: string) => invoke("delete_persistent_cron", { taskId }),
};

// Skill Rule Generator (AionUi inspired)
export interface WorkspaceFile {
  name: string; path: string; relativePath: string; extension: string; size: number;
}
export interface SkillDraft {
  name: string; draft: string; files_analyzed: number; total_chars: number;
}
export const skillGeneratorApi = {
  scanWorkspace: (workspacePath: string) =>
    invoke<WorkspaceFile[]>("scan_workspace_for_skills", { workspacePath }),
  generate: (skillName: string, filePaths: string[], workspacePath: string) =>
    invoke<SkillDraft>("generate_skill_from_files", { skillName, filePaths, workspacePath }),
};

// Conversation Skills Indicator (AionUi inspired)
export const conversationSkillsApi = {
  get: (conversationId: string) =>
    invoke<Array<{ name: string; description: string; category: string | null; usage_count: number; priority_score: number }>>("get_conversation_skills", { conversationId }),
};

// Tool Call Confirmation Queue (AionUi inspired)
export interface ToolCallConfirmation {
  id: string; session_id: string; tool_name: string;
  tool_input: string; status: string; created_at: string;
}
export const toolConfirmationApi = {
  queue: (sessionId: string, toolName: string, toolInput: string) =>
    invoke<string>("queue_tool_confirmation", { sessionId, toolName, toolInput }),
  resolve: (confirmationId: string, approved: boolean) =>
    invoke("resolve_tool_confirmation", { confirmationId, approved }),
  getPending: (sessionId: string) =>
    invoke<ToolCallConfirmation[]>("get_pending_confirmations", { sessionId }),
  getPendingCount: (sessionId: string) =>
    invoke<number>("get_pending_confirmation_count", { sessionId }),
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

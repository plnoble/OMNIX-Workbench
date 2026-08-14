/** Auto-split from tauri-api.ts — domain: monitor. Import via "@/lib/tauri-api". */
import { invoke } from "@tauri-apps/api/core";

// ── Request Logs & Usage Stats

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
  cost_usd: number;
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
  total_cost_usd: number;
  cost_today_usd: number;
  top_models: ModelUsage[];
  hourly_distribution: HourlyCount[];
}

export interface DailyUsage {
  date: string;
  requests: number;
  tokens: number;
  cost_usd: number;
}

export interface PlatformUsage {
  platform: string;
  request_count: number;
  total_tokens: number;
  error_count: number;
  cost_usd: number;
}

export const requestLogApi = {
  /** Get request logs with pagination */
  getLogs: (page?: number, limit?: number, modelFilter?: string) =>
    invoke<RequestLogEntry[]>("get_request_logs", { page, limit, modelFilter }),

  /** Get usage statistics summary */
  getStats: () => invoke<UsageStats>("get_usage_stats"),

  /** Get per-platform usage rollup (cost/tokens/errors) */
  platformUsage: () => invoke<PlatformUsage[]>("get_platform_usage"),

  /** Get daily token/cost activity for the last N days (ascending) */
  timeseries: (days?: number) =>
    invoke<DailyUsage[]>("get_usage_timeseries", { days }),

  /** Delete old logs */
  cleanup: (keepDays?: number) =>
    invoke<number>("cleanup_request_logs", { keepDays }),
};

// ── Platform Health ───────

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

// ── Upstream Model Auto-Sync ──────

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

// ── Platform Health Check ──

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

// ── Agent Task Lifecycle ──────────

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

// ── Security & safety APIs ────────────────────────────

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
  message_count: number;
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

// Agent-Platform Bindings

export interface PendingApprovalInfo {
  request_id: string;
  approval_method: string;
  summary: string;
  requested_permissions: unknown | null;
}
export interface SupervisedSession {
  session_id: string;
  conversation_id: string;
  conversation_title: string;
  agent_id: string;
  workspace_path: string;
  work_mode: string;
  status: string;
  started_at: string;
  last_event_at: string | null;
  approval: PendingApprovalInfo | null;
}
export interface SupervisionOverview {
  sessions: SupervisedSession[];
  recent_done: SupervisedSession[];
}
export const supervisionApi = {
  overview: () => invoke<SupervisionOverview>("supervision_overview"),
};

// 订阅额度 — Claude Code 5h 块 + Codex 官方限额窗口（读本机日志，零联网）。
export interface TokenTally { input: number; output: number; cache_read: number; cache_create: number; requests: number; }
export interface ClaudeQuota {
  window_started_at: string | null;
  window_resets_at: string | null;
  window: TokenTally;
  week: TokenTally;
  window_models: [string, number][];
}
export interface CodexWindow { used_percent: number; window_minutes: number; resets_at: string | null; }
export interface CodexQuota { plan_type: string; primary: CodexWindow | null; secondary: CodexWindow | null; captured_at: string; }
export interface QuotaOverview { claude: ClaudeQuota | null; codex: CodexQuota | null; }
export const quotaApi = {
  overview: () => invoke<QuotaOverview>("agent_quota_overview"),
};


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

// ── 平台路由权重 ───────

/**
 * 改一个平台的路由权重。
 *
 * 网关按 `priority DESC, weight DESC` + 模型名哈希决胜（见 PlatformSubTab 的说明）。
 * priority 早就能改——列表顺序就是它；但 **weight 一直没有任何界面入口**，也就是
 * 同优先级的两个平台之间怎么分流，用户完全改不了。
 *
 * 同批曾有 getAll / reset 两条读健康状态的，已删：健康展示由网关健康卡承担
 * （熔断器实时状态 + 主动探测），再放一份「另一套口径的健康」只会让人问哪个准。
 */
export interface RouterDecisionRow {
  id: number;
  created_at: string;
  /** 逗号分隔的枚举 token（vision/reasoning/coding/speedy/tools）。不是自由文本。 */
  needs: string;
  chosen_model: string;
  chosen_price: number;
  baseline_model: string;
  baseline_price: number;
  anti_downgrade: boolean;
}

export interface RouterDecisionReport {
  total: number;
  anti_downgrade_count: number;
  cheaper_than_baseline: number;
  /** 相对基线的平均费率降幅（0~1）。 */
  avg_rate_cut: number;
  recent: RouterDecisionRow[];
}

export const routerDecisionApi = {
  get: () => invoke<RouterDecisionReport>("get_router_decisions"),
};

export const platformRoutingApi = {
  update: (platformId: string, weight: number, priority: number) =>
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


// ── Security & safety APIs ────────────────────────────

// Prompt Injection Guard
export const promptGuardApi = {
  wrap: (content: string, source: string) =>
    invoke<string>("wrap_untrusted_content", { content, source }),
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


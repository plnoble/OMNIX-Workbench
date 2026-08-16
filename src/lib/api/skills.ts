/** Auto-split from tauri-api.ts — domain: skills. Import via "@/lib/tauri-api". */
import { invoke } from "@tauri-apps/api/core";

// ── Skill pool governance (#3 技能池: 待定/审核/正式 + 网关直调) ──

export interface SkillPoolItem {
  name: string;
  description: string;
  category: string | null;
  pool: "pending" | "official" | string;
  source_ref: string | null;
  central_path: string;
  usage_count: number;
  starred: boolean;
  review_score: number | null;
  review_verdict: "pass" | "needs_work" | "reject" | null;
  review_summary: string;
  review_problems: string[];
  review_improve: string;
  summary_zh: string;
  reviewed_at: string | null;
  updated_at: string;
  needs_re_review: boolean;
}
export interface SkillReformProposal {
  new_content: string;
  explanation: string;
}
export interface SkillFusionProposal {
  name: string;
  description: string;
  content: string;
  explanation: string;
  /** 被融合的源技能——落盘时用于记录血缘并（可选）让它们退出正式池。 */
  sources: string[];
}
export interface CollectReport {
  tools_scanned: number;
  found_total: number;
  imported: number;
  already_managed: number;
}
export interface CleanupReport {
  cleaned: number;
  backup_dir: string;
  errors: string[];
}
export interface SkillReview {
  score: number;
  verdict: "pass" | "needs_work" | "reject";
  summary: string;
  problems: string[];
  improve: string;
}
export interface SkillPoolStats {
  pending: number;
  official: number;
  unreviewed_pending: number;
}

export const skillPoolApi = {
  list: () => invoke<SkillPoolItem[]>("list_skill_pool"),
  stats: () => invoke<SkillPoolStats>("skill_pool_stats"),
  collectAll: () => invoke<CollectReport>("collect_all_skills"),
  cleanupScattered: () => invoke<CleanupReport>("cleanup_scattered_skills"),
  review: (name: string, chatModel: string) =>
    invoke<SkillReview>("review_skill_ai", { name, chatModel }),
  setPool: (name: string, pool: "pending" | "official") =>
    invoke<void>("set_skill_pool", { name, pool }),
  content: (name: string) => invoke<string>("get_skill_pool_content", { name }),
  /** T3：把正式池镜像到跨 harness 公共目录 ~/.agents/skills/，返回结果摘要。 */
  exportToAgentsDir: () => invoke<string>("export_skills_to_agents_dir"),
  summarize: (name: string, chatModel: string) =>
    invoke<string>("summarize_skill_ai", { name, chatModel }),
  reform: (name: string, chatModel: string, instruction?: string) =>
    invoke<SkillReformProposal>("reform_skill_ai", {
      name,
      chatModel,
      instruction: instruction ?? null,
    }),
  applyReform: (name: string, newContent: string) =>
    invoke<void>("apply_skill_reform", { name, newContent }),
  fuse: (names: string[], chatModel: string) =>
    invoke<SkillFusionProposal>("fuse_pool_skills_ai", { names, chatModel }),
  /** 落盘融合结果。retireSources=true 时源技能退回待定池（可逆，不删除）。 */
  applyFusion: (
    name: string,
    description: string,
    content: string,
    sources: string[],
    retireSources: boolean,
  ) =>
    invoke<void>("apply_pool_fusion", { name, description, content, sources, retireSources }),
  remove: (name: string) => invoke<void>("delete_pool_skill", { name }),
};


// ── Skill Sync (P1 — DEC-018) ─────────────────────────

export interface ToolStatus {
  verification: "verified" | "experimental";
  verification_note: string;
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

// ── Agent Templates ───────────────

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

  /** 在本机隐藏 / 恢复一个内置助手（不随版本走，更新不会带回来）。 */
  setHidden: (slug: string, hidden: boolean) =>
    invoke<void>("set_builtin_assistant_hidden", { slug, hidden }),

  /** 本机隐藏了哪些内置助手。 */
  listHidden: () => invoke<AgentTemplate[]>("list_hidden_builtin_assistants"),

  /** Get a specific template by slug */
  getBySlug: (slug: string) =>
    invoke<AgentTemplate | null>("get_agent_template", { slug }),
};

// ── Custom assistants (助手库: 自定义 + 分享) ──
export interface CustomAssistant {
  slug: string; name: string; description: string;
  category: string; instructions: string; created_at: string;
}
export const customAssistantApi = {
  list: () => invoke<CustomAssistant[]>("list_custom_assistants"),
  save: (a: { slug?: string; name: string; description: string; category?: string; instructions: string }) =>
    invoke<CustomAssistant>("save_custom_assistant", a),
  remove: (slug: string) => invoke<void>("delete_custom_assistant", { slug }),
};

/** 技能风险审阅的一条发现。带行号和原文，误报你能一眼认出来。 */
export interface SkillFinding {
  kind: "secrecy" | "credential_access" | "network_exfil" | "persistence" | "destructive" | "privilege_escalation" | "remote_code";
  level: "medium" | "high" | "critical";
  why: string;
  line: number;
  excerpt: string;
}

export interface SkillRisk {
  name: string;
  pool: string;
  level: "medium" | "high" | "critical";
  findings: SkillFinding[];
}

export const skillSafetyApi = {
  /** 把所有技能过一遍风险审阅，只返回有发现的，按严重度降序。 */
  scanAll: () => invoke<SkillRisk[]>("scan_all_skills"),
};

// ── 技能存证（正式池内容是否还是审核时那份） ──────────────

/**
 * 锁状态。**「没锁」和「对不上」必须分开**——前者是本功能上线前晋升的老技能，
 * 后者是内容真的被改过，处理方式完全不同。后端用 `#[serde(tag = "state")]`
 * 序列化，所以这里是带判别字段的联合类型。
 */
export type LockStatus =
  | { state: "ok" }
  | { state: "drifted"; approved: string; current: string }
  | { state: "unlocked" }
  | { state: "missing"; reason: string };

export interface SkillProvenance {
  name: string;
  status: LockStatus;
  /** local / git / builtin */
  source_type: string;
  /** Git URL、导入路径，或 `omnix:fusion(a+b)` */
  source_ref: string;
  source_revision: string;
  approved_at: string;
}

export const skillProvenanceApi = {
  /** 正式池每条技能的存证清单。 */
  audit: () => invoke<SkillProvenance[]>("skill_lock_audit"),

  /**
   * 重新上锁：把指纹更新到当前内容。
   *
   * 单独一个动作而不是自动跟随——「内容变了」和「我认可这个变化」是两件事。
   */
  relock: (name: string) => invoke<string>("relock_skill", { name }),
};



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
  repo_url: string; revision: string; path: string; content_sha: string;
}
export interface MarketSkillPreview { skill: MarketSkill; content: string; content_hash: string; }
export interface DistillRecommendation {
  suggested_name: string; suggested_category: string; reason: string;
  source_evidence: string[]; confidence: number;
}
export const skillLibraryApi = {
  /** Create a local skill from generated/edited content */
  create: (name: string, description: string, profile: string, dependencies: string[], content: string) =>
    invoke<void>("create_skill", { name, description, profile, dependencies, content }),
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
  previewMarket: (skill: MarketSkill) =>
    invoke<MarketSkillPreview>("preview_market_skill", { skill }),
  importMarket: (skill: MarketSkill, overwrite = false) =>
    invoke<string>("import_market_skill", { skill, overwrite }),
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
  listRuns: () => invoke<ProjectProtocolStatus[]>("protocol_list_runs"),
  listEvents: (workspacePath: string, limit?: number) =>
    invoke<ProjectProtocolEvent[]>("protocol_list_events", { workspacePath, limit }),
  previewInit: (workspacePath: string, projectName?: string) =>
    invoke<ProtocolInitPreview>("protocol_preview_init", { workspacePath, projectName }),
  initWorkspace: (workspacePath: string, projectName: string | undefined, enable: boolean) =>
    invoke<ProjectProtocolStatus>("protocol_init_workspace", { workspacePath, projectName, enable }),
  setEnabled: (workspacePath: string, enabled: boolean) =>
    invoke<void>("protocol_set_enabled", { workspacePath, enabled }),
  removeWorkspace: (workspacePath: string) =>
    invoke<void>("protocol_remove_workspace", { workspacePath }),
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

// Session control APIs

export interface SkillUpdated { name: string; from_tool: string; backup_dir: string; needs_re_review: boolean; }
export interface SkillConflict { name: string; source_path: string; from_tool: string; reason: string; }
export interface SkillUpdateReport {
  checked: number;
  updated: SkillUpdated[];
  conflicts: SkillConflict[];
  errors: string[];
}
export const skillUpdatesApi = {
  check: (apply: boolean) => invoke<SkillUpdateReport>("check_skill_updates", { apply }),
  resolveConflict: (name: string, sourcePath: string, takeSource: boolean) =>
    invoke<void>("resolve_skill_conflict", { name, sourcePath, takeSource }),
};

// Grok 账号登录 — OMNIX drives `grok login --device-auth` and relays xAI's own
// link + code. Grok owns the credentials (~/.grok/auth.json); OMNIX never sees
// the password or token, so there is no account list to store here.

export type DagEdgeType = "depends_on" | "specializes" | "composes_with" | "similar_to" | "conflicts_with";
export interface ConflictPair { skill_a: string; skill_b: string; reason: string; }
export interface SkillSearchResult { matches: string[]; neighbors: string[]; conflicts: ConflictPair[]; }
export interface SetValidation {
  valid: boolean; missing_deps: string[]; conflicts: ConflictPair[];
  redundant: [string, string][]; suggestions: string[];
}

// Async Agent Mailbox

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

// 悬浮状态坞：默认不开机自启，用户在系统设置里开关（即时生效）。

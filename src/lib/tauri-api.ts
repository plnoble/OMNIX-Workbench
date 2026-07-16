/**
 * OMNIX Workbench - Typed Tauri IPC API Wrapper
 *
 * A thin typed layer over invoke() calls. Provides:
 * - Single source of truth for all Tauri command names
 * - Typed parameters and return values
 * - Easy mocking for future tests
 * - Centralized call-site discovery
 */

import { invoke } from "@tauri-apps/api/core";
import type {
  AgentUpdateInfo,
  DetectedAgent,
  MediaModelSuggestions,
  MediaTask,
  ProfileStats,
  AgentAccount,
  ConversationInfo,
  ConversationMessage,
  ModelPlatform,
  PlatformModel,
  CronTask,
  CronRun,
  RemoteAccessInfo,
  KnowledgeBase,
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
  TeamRunDetail,
  LabFeature,
  AgentSessionRecord,
  RuntimeAgentCatalogEntry,
  RuntimeAgentId,
  RuntimeEvent,
  RuntimeModelOption,
  RuntimeModelSelection,
  RuntimePermissionPolicy,
  WorkMode,
  WorkspaceSnapshot,
} from "@/types";

// ── Settings ──────────────────────────────────────────

export const settingsApi = {
  get: (key: string) => invoke<string | null>("get_app_setting", { key }),
  set: (key: string, value: string) => invoke("set_app_setting", { key, value }),
  syncExternalConfigs: () => invoke("sync_external_agent_configs"),
};

export const shellApi = {
  pickDirectory: () => invoke<string | null>("pick_directory"),
  pickFile: () => invoke<string | null>("pick_file"),
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

export interface DistillationCandidate {
  id: string;
  conversation_id: string;
  workspace_path: string;
  candidate_type: "memory" | "skill" | "protocol";
  title: string;
  summary: string;
  payload_json: string;
  evidence_json: string;
  model_id: string;
  status: "pending" | "approved" | "rejected";
  created_at: string;
  reviewed_at: string | null;
}

export const distillationApi = {
  generate: (conversationId: string, modelId: string) =>
    invoke<DistillationCandidate[]>("distill_conversation_to_inbox", { conversationId, modelId }),
  /** Distill an external/pre-existing workspace folder from its .omx/development records. */
  generateFromWorkspace: (workspacePath: string, modelId: string) =>
    invoke<DistillationCandidate[]>("distill_workspace_to_inbox", { workspacePath, modelId }),
  list: (status: "pending" | "approved" | "rejected" | "all" = "pending") =>
    invoke<DistillationCandidate[]>("list_distillation_inbox", { status }),
  review: (candidateId: string, approved: boolean) =>
    invoke<DistillationCandidate>("review_distillation_candidate", { candidateId, approved }),
};

// ── Evolution loop — experience auto-injected back into every agent ──
export interface LessonsInfo { count: number; content: string; }
export const evolutionApi = {
  /** Preview the memory block OMNIX auto-injects into agents' context files (CLAUDE.md/AGENTS.md). */
  preview: (workspacePath?: string) =>
    invoke<LessonsInfo>("get_lessons_preview", { workspacePath }),
  /** Embed experience memories lacking an embedding so injection can rank by relevance. */
  reindex: () => invoke<number>("reindex_memory_embeddings", {}),
  /** Merge near-duplicate memories (requires embeddings). Returns merged count. */
  consolidate: () => invoke<number>("consolidate_memories"),
  /** Cache a workspace's embedding/signals for relevance scoring. */
  refreshWorkspace: (workspacePath: string) =>
    invoke<boolean>("refresh_workspace_profile", { workspacePath }),
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

// F1: unified per-agent upstream account switcher (OAuth + api-key)
export interface UpstreamAccountOption {
  account_ref: string; kind: "oauth" | "apikey"; label: string;
  provider: string | null; expired: boolean; is_active: boolean;
}
export const upstreamAccountApi = {
  list: (agentName: string) =>
    invoke<UpstreamAccountOption[]>("list_agent_upstream_accounts", { agentName }),
  setActive: (agentName: string, accountRef: string) =>
    invoke<void>("set_active_upstream_account", { agentName, accountRef }),
  getActive: (agentName: string) =>
    invoke<string>("get_active_upstream_account", { agentName }),
};

// F-C: local model fit ranking
export interface HardwareInfo { cpu_cores: number; cpu_brand: string; ram_gb: number; }
export interface ModelRecommendation {
  name: string; family: string; params_b: number; best_quant: string;
  needed_gb: number; fit: "fits" | "tight" | "wont_run";
}
export const localModelApi = {
  detectHardware: () => invoke<HardwareInfo>("detect_hardware"),
  recommend: (budgetGb: number) =>
    invoke<ModelRecommendation[]>("recommend_local_models", { budgetGb }),
};

// ── Remote Dev (Labs) ──

export interface SshHost {
  id: string;
  name: string;
  host: string;
  port: number;
  user: string;
  key_path: string;
  default_workdir: string;
}
export interface SshTestResult {
  ok: boolean;
  latency_ms: number;
  uname: string;
  error: string;
}
export interface RemoteHardware {
  gpu: string;
  ram_mb: number;
  cpu_cores: number;
}
export interface RemoteAgentStatus {
  agent: string;
  bin: string;
  installed: boolean;
  path: string;
  version: string;
}
export interface RemoteModelHostTest {
  ok: boolean;
  latency_ms: number;
  models: string[];
  error: string;
}
export const remoteDevApi = {
  listHosts: () => invoke<SshHost[]>("list_ssh_hosts"),
  saveHost: (host: SshHost) => invoke<SshHost>("save_ssh_host", { host }),
  deleteHost: (id: string) => invoke<void>("delete_ssh_host", { id }),
  testHost: (id: string) => invoke<SshTestResult>("test_ssh_host", { id }),
  probeHardware: (id: string) => invoke<RemoteHardware>("probe_remote_hardware", { id }),
  detectAgents: (id: string) => invoke<RemoteAgentStatus[]>("detect_remote_agents", { id }),
  installAgent: (id: string, agent: string) =>
    invoke<string>("install_remote_agent", { id, agent }),
  testModelHost: (url: string) =>
    invoke<RemoteModelHostTest>("test_remote_model_host", { url }),
  startRun: (hostId: string, agent: string, workdir: string, prompt: string, useGateway: boolean) =>
    invoke<{ run_id: string }>("start_remote_run", { hostId, agent, workdir, prompt, useGateway }),
  stopRun: (runId: string) => invoke<void>("stop_remote_run", { runId }),
};

// ── Storage locations (R1 存储位置中心) ──

export interface StorageLocation {
  key: string;
  label: string;
  path: string;
  default_path: string;
  is_default: boolean;
}
export interface SkillsMigrationReport {
  moved: number;
  new_dir: string;
  old_dir: string;
  errors: string[];
}
export const storageApi = {
  getConfig: () => invoke<StorageLocation[]>("get_storage_config"),
  setDir: (key: string, path: string) =>
    invoke<void>("set_storage_dir", { key, path }),
  migrateSkillsStore: (newDir: string) =>
    invoke<SkillsMigrationReport>("migrate_skills_store", { newDir }),
};

// ── Agent installation management (R3 统一安装) ──

export interface AgentInstallation {
  path: string;
  version: string;
  kind: "managed" | "npm_global" | "other" | string;
  is_active: boolean;
}
export interface AgentInstallGroup {
  agent: string;
  managed_root: string;
  installations: AgentInstallation[];
}
export const agentInstallApi = {
  scan: () => invoke<AgentInstallGroup[]>("scan_agent_installations"),
  remove: (agent: string, kind: string) =>
    invoke<void>("remove_agent_installation", { agent, kind }),
};

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
  applyFusion: (name: string, description: string, content: string) =>
    invoke<void>("apply_pool_fusion", { name, description, content }),
  remove: (name: string) => invoke<void>("delete_pool_skill", { name }),
};

// ── Presentations / PPT panel (结构化幻灯模型，preview == export) ──

export type SlideLayout =
  | "cover" | "section" | "bullets" | "content"
  | "two-column" | "quote" | "image" | "image-left";

export interface SlideColumn {
  title?: string;
  bullets?: string[];
  body?: string;
}
export interface Slide {
  layout: SlideLayout | string;
  title?: string;
  subtitle?: string;
  bullets?: string[];
  body?: string;
  columns?: SlideColumn[];
  image?: string;
  notes?: string;
}
export interface Brand {
  name: string;
  primary: string;
  accent: string;
  background: string;
  text: string;
  font: string;
  logo: string;
  footer: string;
}
export interface Deck {
  id: string;
  title: string;
  theme: string;
  slides: Slide[];
  brand?: Brand | null;
}
export interface OutlineItem {
  layout: string;
  title: string;
  points: string[];
}
export interface Outline {
  title: string;
  theme: string;
  items: OutlineItem[];
}
export interface DeckMeta {
  id: string;
  title: string;
  theme: string;
  slide_count: number;
  updated_at: string;
}
export interface DeckRecord {
  id: string;
  title: string;
  theme: string;
  model_json: string;
}
export interface DeckVersion {
  id: number;
  label: string;
  created_at: string;
}
export const DECK_THEMES = ["midnight", "minimal", "corporate", "sunset"] as const;

export const slidesApi = {
  list: () => invoke<DeckMeta[]>("list_decks"),
  get: (id: string) => invoke<DeckRecord>("get_deck", { id }),
  create: (title: string, theme: string) =>
    invoke<DeckRecord>("create_deck", { title, theme }),
  save: (id: string, modelJson: string) =>
    invoke<DeckRecord>("save_deck", { id, modelJson }),
  remove: (id: string) => invoke<void>("delete_deck", { id }),
  render: (modelJson: string, slideIndex?: number | null, print = false) =>
    invoke<string>("render_deck", {
      modelJson,
      slideIndex: slideIndex ?? null,
      print,
    }),
  generate: (topic: string, chatModel: string, slideCount?: number) =>
    invoke<DeckRecord>("generate_deck", {
      topic,
      chatModel,
      slideCount: slideCount ?? null,
    }),
  editAi: (id: string, instruction: string, chatModel: string) =>
    invoke<DeckRecord>("edit_deck_ai", { id, instruction, chatModel }),
  exportHtml: (modelJson: string) =>
    invoke<string>("export_deck_html", { modelJson }),
  exportPdf: (modelJson: string) =>
    invoke<string>("export_deck_pdf", { modelJson }),
  // E: real PowerPoint from the same JSON model, QA'd by OfficeCLI on the way out
  exportPptx: (modelJson: string) =>
    invoke<PptxExportResult>("export_deck_pptx", { modelJson }),
  // A: two-stage generation (outline → expand)
  generateOutline: (topic: string, chatModel: string, slideCount?: number) =>
    invoke<Outline>("generate_outline", { topic, chatModel, slideCount: slideCount ?? null }),
  expandOutline: (outline: Outline, chatModel: string) =>
    invoke<DeckRecord>("expand_outline", { outline, chatModel }),
  // B: single-slide diff edit
  editSlide: (id: string, slideIndex: number, instruction: string, chatModel: string) =>
    invoke<DeckRecord>("edit_slide_ai", { id, slideIndex, instruction, chatModel }),
  // C: auto illustration
  suggestImagePrompt: (modelJson: string, slideIndex: number) =>
    invoke<string>("suggest_slide_image_prompt", { modelJson, slideIndex }),
  generateImage: (
    id: string,
    slideIndex: number,
    platformId: string,
    model: string,
    prompt: string,
    size?: string,
  ) =>
    invoke<DeckRecord>("generate_slide_image", {
      id, slideIndex, platformId, model, prompt, size: size ?? null,
    }),
  // Version history — every AI mutation is undoable
  listVersions: (id: string) => invoke<DeckVersion[]>("list_deck_versions", { id }),
  restoreVersion: (id: string, versionId?: number) =>
    invoke<DeckRecord>("restore_deck_version", { id, versionId: versionId ?? null }),
  // D: reusable brand masters
  listBrands: () => invoke<Brand[]>("list_brands"),
  saveBrand: (brand: Brand) => invoke<void>("save_brand", { brand }),
  deleteBrand: (name: string) => invoke<void>("delete_brand", { name }),
};

// ── Write (Markdown writing workspace)──

export interface WriteSpace {
  name: string;
  path: string;
  is_default: boolean;
}
export interface WriteFile {
  name: string;
  relative_path: string;
  updated_at: string;
}
export const writeApi = {
  listSpaces: () => invoke<WriteSpace[]>("write_list_spaces"),
  addSpace: (path: string) => invoke<WriteSpace>("write_add_space", { path }),
  removeSpace: (path: string) => invoke("write_remove_space", { path }),
  listFiles: (spacePath: string) => invoke<WriteFile[]>("write_list_files", { spacePath }),
  readFile: (spacePath: string, relativePath: string) =>
    invoke<string>("write_read_file", { spacePath, relativePath }),
  saveFile: (spacePath: string, relativePath: string, content: string) =>
    invoke("write_save_file", { spacePath, relativePath, content }),
  createFile: (spacePath: string, name: string) =>
    invoke<string>("write_create_file", { spacePath, name }),
  renameFile: (spacePath: string, relativePath: string, newName: string) =>
    invoke<string>("write_rename_file", { spacePath, relativePath, newName }),
  deleteFile: (spacePath: string, relativePath: string) =>
    invoke("write_delete_file", { spacePath, relativePath }),
  exportHtml: (spacePath: string, relativePath: string, html: string) =>
    invoke<string>("write_export_html", { spacePath, relativePath, html }),
};

// ── Autopilots (scheduled agent work)──

export interface Autopilot {
  id: string;
  title: string;
  prompt: string;
  agent_name: string;
  workspace_path: string;
  schedule: string;
  permission: string;
  work_mode: string;
  enabled: boolean;
  last_run: string | null;
  created_at: string;
}
export interface QueuedAutopilotRun {
  run_id: string;
  autopilot_id: string;
  title: string;
  conversation_id: string;
  prompt: string;
  agent_name: string;
  workspace_path: string;
  permission: string;
  work_mode: string;
}
export interface AutopilotRunInfo {
  id: string;
  autopilot_id: string;
  conversation_id: string;
  status: string;
  trigger_source: string;
  created_at: string;
}
export const autopilotApi = {
  list: () => invoke<Autopilot[]>("autopilot_list"),
  create: (p: { title: string; prompt: string; agentName: string; workspacePath: string; schedule: string; permission: string; workMode: string }) =>
    invoke<Autopilot>("autopilot_create", p),
  update: (p: { id: string; title: string; prompt: string; agentName: string; workspacePath: string; schedule: string; permission: string; workMode: string }) =>
    invoke<Autopilot>("autopilot_update", p),
  setEnabled: (id: string, enabled: boolean) => invoke("autopilot_set_enabled", { id, enabled }),
  delete: (id: string) => invoke("autopilot_delete", { id }),
  runNow: (id: string) => invoke<string>("autopilot_run_now", { id }),
  takeQueuedRuns: () => invoke<QueuedAutopilotRun[]>("autopilot_take_queued_runs"),
  markRun: (runId: string, status: "done" | "failed") => invoke("autopilot_mark_run", { runId, status }),
  listRuns: (autopilotId: string) => invoke<AutopilotRunInfo[]>("autopilot_list_runs", { autopilotId }),
};

// ── SDD (requirement → plan)──

export interface PlanTodo {
  line_index: number;
  done: boolean;
  text: string;
}
export interface PlanFile {
  relative_path: string;
  title: string;
  updated_at: string;
  todo_total: number;
  todo_done: number;
}
export const sddApi = {
  reservePlanPath: (workspacePath: string, title: string) =>
    invoke<string>("sdd_reserve_plan_path", { workspacePath, title }),
  writePlan: (workspacePath: string, title: string, markdown: string) =>
    invoke<string>("sdd_write_plan", { workspacePath, title, markdown }),
  listPlans: (workspacePath: string) =>
    invoke<PlanFile[]>("sdd_list_plans", { workspacePath }),
  readPlan: (workspacePath: string, relativePath: string) =>
    invoke<[string, PlanTodo[]]>("sdd_read_plan", { workspacePath, relativePath }),
  toggleTodo: (workspacePath: string, relativePath: string, lineIndex: number, done: boolean) =>
    invoke<PlanTodo[]>("sdd_toggle_plan_todo", { workspacePath, relativePath, lineIndex, done }),
  clarifyPrompt: (draft: string) => invoke<string>("sdd_clarify_prompt", { draft }),
  planPrompt: (draft: string, planRelativePath: string) =>
    invoke<string>("sdd_plan_prompt", { draft, planRelativePath }),
};

// ── Conversations ─────────────────────────────────────

export type ConversationGoalStatus = "active" | "paused" | "complete";
export interface ConversationGoal {
  conversation_id: string;
  objective: string;
  status: ConversationGoalStatus;
  created_at: string;
  updated_at: string;
}

export const conversationApi = {
  list: () => invoke<ConversationInfo[]>("get_all_conversations"),
  create: (params: { id: string; title: string; workspacePath: string; activeAgent: string; parentConversationId?: string }) =>
    invoke("create_conversation", params),
  delete: (id: string) => invoke("delete_conversation", { conversationId: id }),
  archive: (id: string) => invoke("archive_conversation", { conversationId: id }),
  // Long-term goal (/goal)
  getGoal: (conversationId: string) =>
    invoke<ConversationGoal | null>("get_conversation_goal", { conversationId }),
  setGoal: (conversationId: string, objective: string) =>
    invoke<ConversationGoal>("set_conversation_goal", { conversationId, objective }),
  setGoalStatus: (conversationId: string, status: ConversationGoalStatus) =>
    invoke<ConversationGoal>("set_conversation_goal_status", { conversationId, status }),
  clearGoal: (conversationId: string) =>
    invoke("clear_conversation_goal", { conversationId }),
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

export const runtimeApi = {
  getAgentCatalog: () => invoke<RuntimeAgentCatalogEntry[]>("runtime_get_agent_catalog"),
  getModelOptions: (agent: RuntimeAgentId) =>
    invoke<RuntimeModelOption[]>("runtime_get_model_options", { agent }),
  getAgentModelPreference: (agent: RuntimeAgentId) =>
    invoke<string>("runtime_get_agent_model_preference", { agent }),
  setAgentModelPreference: (agent: RuntimeAgentId, model: string) =>
    invoke<void>("runtime_set_agent_model_preference", { agent, model }),
  startSession: (request: {
    conversation_id: string;
    agent: RuntimeAgentId;
    workspace_path: string;
    model: RuntimeModelSelection;
    permission: RuntimePermissionPolicy;
    work_mode: WorkMode;
  }) => invoke<AgentSessionRecord>("runtime_start_session", { request }),
  sendMessage: (
    sessionId: string,
    prompt: string,
    displayText?: string,
    handoff?: boolean,
    images?: Array<{ mime: string; data: string }>,
  ) => invoke("runtime_send_message", { sessionId, prompt, displayText, handoff, images }),
  respondApproval: (params: {
    sessionId: string;
    requestId: string;
    approved: boolean;
    forSession: boolean;
    approvalMethod: string;
    requestedPermissions?: unknown;
  }) => invoke("runtime_respond_approval", params),
  setSessionModel: (sessionId: string, model: string) =>
    invoke("runtime_set_session_model", { sessionId, model }),
  stopSession: (sessionId: string) => invoke("runtime_stop_session", { sessionId }),
  resumeSession: (sessionId: string) =>
    invoke<AgentSessionRecord>("runtime_resume_session", { sessionId }),
  getSession: (sessionId: string) =>
    invoke<AgentSessionRecord>("runtime_get_session", { sessionId }),
  getEvents: (sessionId: string) =>
    invoke<RuntimeEvent[]>("runtime_get_events", { sessionId }),
  listConversationSessions: (conversationId: string) =>
    invoke<AgentSessionRecord[]>("runtime_list_conversation_sessions", { conversationId }),
};

// ── Agent Detection ───────────────────────────────────

export const agentApi = {
  detectInstalled: () => invoke<DetectedAgent[]>("detect_installed_agents"),
  install: (agentName: string) => invoke("install_agent_cli", { agentName }),
  update: (agentName: string) => invoke("repair_installed_agent", { agentName }),
  checkUpdates: () => invoke<AgentUpdateInfo[]>("check_agent_updates"),
};

export const profileApi = {
  getStats: () => invoke<ProfileStats>("get_profile_stats"),
};

export const mediaApi = {
  generateImage: (platformId: string, model: string, prompt: string, size: string) =>
    invoke<MediaTask>("media_generate_image", { platformId, model, prompt, size }),
  createVideoTask: (
    platformId: string,
    model: string,
    prompt: string,
    width: number,
    height: number,
    numFrames: number,
    frameRate: number,
    imageTaskId: string | null,
  ) =>
    invoke<MediaTask>("media_create_video_task", {
      platformId, model, prompt, width, height, numFrames, frameRate, imageTaskId,
    }),
  listTasks: () => invoke<MediaTask[]>("media_list_tasks"),
  deleteTask: (taskId: string) => invoke("media_delete_task", { taskId }),
  readFile: (taskId: string) => invoke<string>("media_read_file", { taskId }),
  readAttachment: (path: string) => invoke<string>("media_read_attachment", { path }),
  modelSuggestions: () => invoke<MediaModelSuggestions>("media_model_suggestions"),
};

// Team and workspace runs

export const teamRunApi = {
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
  generatePlan: (goal: string, workspacePath: string, managerAgent: string) =>
    invoke<TeamRunDetail>("team_generate_plan", { goal, workspacePath, managerAgent }),
  getDetail: (runId: string) =>
    invoke<TeamRunDetail>("team_get_run_detail", { runId }),
  startApproved: (runId: string, concurrency = 2) =>
    invoke<TeamRunDetail>("team_start_approved_run", { runId, concurrency }),
  stop: (runId: string) =>
    invoke<TeamRunDetail>("team_stop_run", { runId }),
  retryWorker: (workerId: string) =>
    invoke<TeamRunDetail>("team_retry_worker", { workerId }),
  respondWorkerApproval: (workerId: string, requestId: string, approved: boolean, requestedPermissions?: unknown) =>
    invoke<TeamRunDetail>("team_respond_worker_approval", { workerId, requestId, approved, requestedPermissions }),
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

export interface FilePreview {
  path: string;
  kind: "text" | "markdown" | "image" | "pdf" | "binary";
  language: string;
  content: string;
  size: number;
  truncated: boolean;
}
export const workspaceApi = {
  snapshot: (workspacePath: string) =>
    invoke<WorkspaceSnapshot>("get_workspace_snapshot", { workspacePath }),
  readFile: (workspacePath: string, relativePath: string) =>
    invoke<FilePreview>("read_workspace_file", { workspacePath, relativePath }),
};

// ── Environment Diagnostics ───────────────────────────

export const diagnosticsApi = {
  run: () => invoke<Record<string, string>>("run_env_diagnostics"),
  repair: (toolName: string) => invoke("repair_env_tool", { toolName }),
};

// ── Remote Access ─────────────────────────────────────

export const remoteApi = {
  getInfo: () => invoke<RemoteAccessInfo>("get_remote_access_info"),
  /** Enable/disable LAN binding for remote phone access; restarts the proxy. */
  setAccess: (enabled: boolean) => invoke<void>("set_remote_access", { enabled }),
};

// ── Knowledge Base ─────────────────────────────────────

export const knowledgeApi = {
  listBases: () => invoke<KnowledgeBase[]>("kb_list_bases"),
  createBase: (name: string, description = "") =>
    invoke<KnowledgeBase>("kb_create_base", { name, description }),
  updateBase: (knowledgeBaseId: string, name: string, description = "") =>
    invoke("kb_update_base", { knowledgeBaseId, name, description }),
  deleteBase: (knowledgeBaseId: string) => invoke("kb_delete_base", { knowledgeBaseId }),
  exportBase: (knowledgeBaseId: string) => invoke<string>("kb_export_base", { knowledgeBaseId }),
  importBase: (data: string) => invoke<KnowledgeBase>("kb_import_base", { data }),
  listDocuments: (knowledgeBaseId?: string) =>
    invoke<KbDocument[]>("kb_list_documents", { knowledgeBaseId }),
  importDocument: (params: { knowledgeBaseId?: string; title: string; sourcePath: string; fileType: string; content: string; chunkConfig?: ChunkConfig }) =>
    invoke<KbDocument>("kb_import_document", params),
  importFile: (params: { filePath: string; knowledgeBaseId?: string; chunkConfig?: ChunkConfig }) =>
    invoke<KbDocument>("kb_import_file", params),
  importDirectory: (params: { directoryPath: string; extensions?: string; knowledgeBaseId?: string }) =>
    invoke<KbDocument[]>("kb_import_directory", params),
  deleteDocument: (documentId: string) => invoke("kb_delete_document", { documentId }),
  getChunks: (documentId: string) => invoke<KbChunk[]>("kb_get_chunks", { documentId }),
  generateEmbeddings: (params: { documentId: string; modelName: string }) =>
    invoke<EmbeddingProgress>("kb_generate_embeddings", params),
  hybridSearch: (params: { query: string; embeddingModel: string; limit?: number; knowledgeBaseIds?: string[] }) =>
    invoke<SearchResult[]>("kb_hybrid_search", params),
  ragQuery: (params: { query: string; embeddingModel: string; chatModel: string; topK?: number; knowledgeBaseIds?: string[] }) =>
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

// ── Workspace checkpoints + diff review ──
export interface Checkpoint {
  id: string; workspace_path: string; session_id: string; label: string;
  vcs: string; ref_name: string; created_at: string; skipped: boolean;
}
export interface FileDiff {
  path: string; status: string; additions: number; deletions: number; unified_diff: string;
}
export const checkpointApi = {
  create: (workspacePath: string, sessionId: string, label: string) =>
    invoke<Checkpoint>("create_checkpoint", { workspacePath, sessionId, label }),
  list: (workspacePath: string, sessionId?: string) =>
    invoke<Checkpoint[]>("list_checkpoints", { workspacePath, sessionId }),
  diff: (workspacePath: string, checkpointId?: string) =>
    invoke<FileDiff[]>("get_workspace_diff", { workspacePath, checkpointId }),
  restore: (checkpointId: string) =>
    invoke<Checkpoint>("restore_checkpoint", { checkpointId }),
  revertFile: (checkpointId: string, path: string) =>
    invoke<void>("revert_file", { checkpointId, path }),
};

// ── Parallel sessions via Git worktrees ──
export interface Worktree {
  id: string; repo_path: string; worktree_path: string; branch: string;
  session_id: string; label: string; created_at: string;
  is_main: boolean; exists: boolean; dirty: boolean; ahead: number;
}
export interface MergeResult { merged: boolean; conflict: boolean; message: string; }
export const worktreeApi = {
  create: (workspacePath: string, sessionId: string, label: string, branch?: string) =>
    invoke<Worktree>("create_worktree", { workspacePath, sessionId, label, branch }),
  list: (workspacePath: string) =>
    invoke<Worktree[]>("list_worktrees", { workspacePath }),
  remove: (worktreeId: string, deleteBranch: boolean, force: boolean) =>
    invoke<void>("remove_worktree", { worktreeId, deleteBranch, force }),
  merge: (worktreeId: string) =>
    invoke<MergeResult>("merge_worktree", { worktreeId }),
};

// ── User-state hooks: event → action rules ──
export interface Hook {
  id: string; name: string; event: string; matcher: string;
  action_type: string; action_payload: string; enabled: boolean;
  created_at: string; fire_count: number; last_fired_at: string | null;
}
export interface HookRun {
  id: number; hook_id: string; hook_name: string; session_id: string;
  event: string; fired_at: string; ok: boolean; detail: string;
}
export const hooksApi = {
  list: () => invoke<Hook[]>("list_hooks"),
  save: (h: { id?: string; name: string; event: string; matcher: string; action_type: string; action_payload: string; enabled: boolean }) =>
    invoke<Hook>("save_hook", h),
  toggle: (id: string, enabled: boolean) => invoke<void>("toggle_hook", { id, enabled }),
  remove: (id: string) => invoke<void>("delete_hook", { id }),
  test: (id: string) => invoke<string>("test_hook", { id }),
  runs: (limit?: number) => invoke<HookRun[]>("get_hook_runs", { limit }),
  clearRuns: () => invoke<void>("clear_hook_runs"),
};

// ── Custom Quick Assistant actions (划词助手深挖) ──
export interface QuickAction {
  id: string; label: string; emoji: string; prompt_template: string;
  enabled: boolean; order_num: number; created_at: string;
}
export const quickActionApi = {
  list: () => invoke<QuickAction[]>("list_quick_actions"),
  save: (a: { id?: string; label: string; emoji: string; promptTemplate: string; enabled: boolean; orderNum: number }) =>
    invoke<QuickAction>("save_quick_action", { id: a.id, label: a.label, emoji: a.emoji, promptTemplate: a.promptTemplate, enabled: a.enabled, orderNum: a.orderNum }),
  remove: (id: string) => invoke<void>("delete_quick_action", { id }),
};

// ── Notes (笔记) ──
export interface Note {
  id: string; title: string; content: string; tags: string;
  source: string; created_at: string; updated_at: string;
}
export const notesApi = {
  list: (query?: string) => invoke<Note[]>("list_notes", { query }),
  save: (n: { id?: string; title: string; content: string; tags?: string; source?: string }) =>
    invoke<Note>("save_note", n),
  remove: (id: string) => invoke<void>("delete_note", { id }),
  dir: () => invoke<string>("get_notes_dir"),
  openFolder: () => invoke<void>("open_notes_folder"),
};

// ── In-session background tasks / sub-agents (own worktree, concurrent session) ──
export interface SubAgent {
  id: string; parent_conversation_id: string; title: string; prompt: string;
  agent: string; child_conversation_id: string; child_session_id: string;
  worktree_id: string; worktree_path: string; status: string;
  created_at: string; updated_at: string;
}
export const subAgentApi = {
  create: (r: { parentConversationId: string; title: string; prompt: string; agent: string; childConversationId: string; childSessionId: string; worktreeId: string; worktreePath: string }) =>
    invoke<SubAgent>("create_subagent", r),
  list: (parentConversationId: string) =>
    invoke<SubAgent[]>("list_subagents", { parentConversationId }),
  updateStatus: (id: string, status: string) =>
    invoke<void>("update_subagent_status", { id, status }),
  remove: (id: string) => invoke<void>("delete_subagent", { id }),
};

// ── MCP sync to Agent native config ──
export interface McpSyncReport { agent: string; synced: string[]; skipped: string[]; backup_path: string | null; }
export interface AgentMcpState { agent: string; config_path: string; config_exists: boolean; server_names: string[]; }
export const mcpSyncApi = {
  getAgentStates: () => invoke<AgentMcpState[]>("mcp_get_agent_states"),
  syncToAgents: (agents: string[], serverIds: string[]) =>
    invoke<McpSyncReport[]>("mcp_sync_to_agents", { agents, serverIds }),
  removeFromAgent: (agent: string, serverName: string) =>
    invoke<string | null>("mcp_remove_from_agent", { agent, serverName }),
  importFromAgent: (agent: string) =>
    invoke<string[]>("mcp_import_from_agent", { agent }),
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

// ── Skills Lock File ──────────────

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

// ── Agent Execution Environment ───

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

// ── Autopilot ─────────────────────

export interface AutopilotConfig {
  task_id: string;
  agent_name: string | null;
  prompt_template: string | null;
  trigger_type: string;     // "cron" | "webhook"
  webhook_secret: string | null;
  webhook_url: string | null;
}

// NOTE: legacy config-on-cron-task autopilot (never surfaced in the UI). The
// active, standalone Autopilot feature is `autopilotApi` above. Kept only so the
// registered backend commands remain reachable; rename avoids the name clash.
export const autopilotConfigApi = {
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

// ── Workspace GC ──────────────────

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

// ── Skill Compound Interest ───────

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

// Config Backup
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

// API Provider Preset
export const apiPresetApi = {
  apply: (presetId: string, apiKey: string) =>
    invoke<string>("apply_api_preset", { presetId, apiKey }),
};

// Architecture Graph
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

// Skill Library Features
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

// Circuit Breaker & Session Usage
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

// OAuth Auth Center — use your subscriptions in agents
export type OAuthProvider = "anthropic_claude" | "openai_codex" | "google_gemini";
export interface OAuthStartResult {
  authorize_url: string; state: string; manual_paste: boolean; redirect_uri: string;
}
export interface OAuthAccountView {
  id: string; provider: OAuthProvider; provider_name: string; label: string;
  scope: string | null; expires_at: string | null; has_refresh: boolean;
  expired: boolean; created_at: string;
}
export const oauthApi = {
  start: (provider: OAuthProvider) => invoke<OAuthStartResult>("oauth_start", { provider }),
  complete: (provider: OAuthProvider, callbackInput: string, label: string) =>
    invoke<OAuthAccountView>("oauth_complete", { provider, callbackInput, label }),
  listAccounts: () => invoke<OAuthAccountView[]>("oauth_list_accounts"),
  deleteAccount: (id: string) => invoke<void>("oauth_delete_account", { id }),
  refreshAccount: (id: string) => invoke<void>("oauth_refresh_account", { id }),
};

// Office 底座 — OfficeCLI managed install + pptx QA/import; skill auto-update.
export interface PptxQa {
  ran: boolean;
  schema_ok: boolean;
  issue_count: number;
  detail: string[];
}
export interface PptxExportResult { path: string; qa: PptxQa; }
export interface OfficeStatus {
  installed: boolean;
  path: string | null;
  kind: "managed" | "system" | null;
  version: string | null;
  pinned_version: string;
  update_available: boolean;
  skill_pool: string | null;
  skill_reviewed: boolean;
}
export const officeApi = {
  status: () => invoke<OfficeStatus>("office_status"),
  install: () => invoke<string>("office_install"),
  importPptx: (filePath: string) => invoke<DeckRecord>("import_pptx_deck", { filePath }),
};

export interface SkillUpdated { name: string; from_tool: string; backup_dir: string; needs_re_review: boolean; }
export interface SkillConflict { name: string; source_path: string; from_tool: string; }
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
export interface GrokAuthStatus {
  cli_installed: boolean;
  cli_path: string | null;
  signed_in: boolean;
  auth_file: string;
  api_key_env: boolean;
  api_key_in_omnix: boolean;
}
export const grokAuthApi = {
  status: () => invoke<GrokAuthStatus>("grok_auth_status"),
  loginStart: () => invoke<void>("grok_login_start"),
  loginCancel: () => invoke<void>("grok_login_cancel"),
  logout: () => invoke<void>("grok_logout"),
};

// CLI 配置接管 — point native CLIs at a chosen target
export interface TakeoverTarget { kind: "gateway" | "platform" | "oauth"; ref_id?: string; model?: string; }
export interface TakeoverReport { agent: string; config_path: string; applied: boolean; backup_path: string | null; detail: string; }
export interface AgentTakeoverState { agent: string; config_path: string; config_exists: boolean; current_base_url: string | null; has_backup: boolean; }
export const cliTakeoverApi = {
  status: () => invoke<AgentTakeoverState[]>("cli_takeover_status"),
  apply: (agents: string[], target: TakeoverTarget) =>
    invoke<TakeoverReport[]>("cli_takeover_apply", { agents, target }),
  revert: (agent: string) => invoke<string>("cli_takeover_revert", { agent }),
};

// Skill DAG
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

// Async Agent Mailbox
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

// Enhanced Task Dependencies
export const taskDependencyApi = {
  setBlocks: (taskId: string, blocksIds: string[]) =>
    invoke("set_task_blocks", { taskId, blocksIds }),
  autoUnblock: (completedTaskId: string) =>
    invoke<string[]>("auto_unblock_tasks", { completedTaskId }),
};

// YOLO Full-Auto Mode
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

// Persistent Cron
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

// Skill Rule Generator
// Conversation Skills Indicator
export const conversationSkillsApi = {
  get: (conversationId: string) =>
    invoke<Array<{ name: string; description: string; category: string | null; usage_count: number; priority_score: number }>>("get_conversation_skills", { conversationId }),
};

// Tool Call Confirmation Queue
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

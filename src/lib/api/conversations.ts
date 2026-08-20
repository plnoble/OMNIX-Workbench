/** Auto-split from tauri-api.ts — domain: conversations. Import via "@/lib/tauri-api". */
import { invoke } from "@tauri-apps/api/core";
import type {
  AgentUpdateInfo,
  DetectedAgent,
  MediaModelSuggestions,
  MediaTask,
  ProfileStats,
  ConversationInfo,
  ConversationMessage,
  MessagesDelta,
  MessagePage,
  ConversationPage,
  CronTask,
  CronRun,
  WorkspaceRun,
  AgentRun,
  TeamAssignmentInput,
  TeamPlan,
  TeamRunDetail,
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
  list: (limit: number) => invoke<ConversationPage>("get_all_conversations", { limit }),
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
  listArchived: (limit: number) =>
    invoke<ConversationPage>("get_archived_conversations", { limit }),
  getMessages: (conversationId: string) =>
    invoke<ConversationMessage[]>("get_conversation_messages", { conversationId }),
  /**
   * 只取「我还没有的那几条」。
   *
   * `afterMessageId` 传手上最后一条的 id。后端找不到那条（比如它已被压缩删掉）
   * 就退回全量并把 `is_full` 置真——**调用方必须看这个标志**：该替换的时候当成
   * 追加，界面上会把一整段历史渲染两遍。
   */
  getMessagesSince: (conversationId: string, afterMessageId: string | null) =>
    invoke<MessagesDelta>("get_messages_since", { conversationId, afterMessageId }),
  /**
   * 取一页消息：不给 `beforeMessageId` 就是**最近** N 条（聊天从最新往回看），
   * 给了就是那条之前的 N 条。返回值里的 `older_remaining` 必须显示出来。
   */
  getMessagesPage: (conversationId: string, beforeMessageId: string | null, limit: number) =>
    invoke<MessagePage>("get_messages_page", { conversationId, beforeMessageId, limit }),
  /** 按标题搜索会话。搜索走后端——前端过滤需要全量在手，等于没分页。 */
  search: (query: string, archived: boolean, limit: number) =>
    invoke<ConversationInfo[]>("search_conversations", { query, archived, limit }),
  addMessage: (params: { id: string; conversationId: string; role: string; content: string }) =>
    invoke("add_conversation_message", params),
};

// ── PTY Sessions ──────────────────────────────────────

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
    invoke<string>("runtime_set_session_model", { sessionId, model }),
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
  // 编排预设（借鉴 paseo）：不经 AI 队长，直接构造 handoff / advisor 计划，仍进批准
  buildPreset: (preset: "handoff" | "advisor", task: string, workspacePath: string, plannerAgent: string, workerAgent: string) =>
    invoke<TeamRunDetail>("team_build_preset", { preset, task, workspacePath, plannerAgent, workerAgent }),
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

/** 后端 `get_previewable_files` 返回的条目。 */
export interface PreviewFileEntry {
  /** 绝对路径。 */
  path: string;
  /** 相对工作区根，正斜杠分隔——`workspaceApi.readFile` 要的就是这个。 */
  relative: string;
  name: string;
  ext: string;
  modified: number;
}

export const previewApi = {
  listFiles: (workspacePath: string) =>
    invoke<PreviewFileEntry[]>("get_previewable_files", { workspacePath }),
  // 文件内容一律走 `workspaceApi.readFile`（read_workspace_file）：它带
  // 目录穿越校验、认得 image/pdf/binary、有大小上限。
  //
  // 这里以前挂着 read_file_as_base64 / read_file_content_utf8 两个绑定，
  // 传的是 { workspacePath, fileName }，而后端签名是 `file_path`——参数名
  // 就对不上，每次读取都被驳回；错误被 usePreview 吞进 console，面板一片
  // 空白。而且那两个命令用 `validate_relative_path` 拒绝绝对路径，偏偏
  // listFiles 给的就是绝对路径，参数名修对了照样读不出来。
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


export interface MailMessage {
  id: string; from_agent: string; to_agent: string;
  subject: string; body: string; read: boolean; created_at: string;
}


// Tool Call Confirmation Queue

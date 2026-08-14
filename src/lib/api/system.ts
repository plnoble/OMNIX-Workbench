/** Auto-split from tauri-api.ts — domain: system. Import via "@/lib/tauri-api". */
import { invoke } from "@tauri-apps/api/core";

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

// NOTE: legacy config-on-cron-task autopilot (never surfaced in the UI). The
// active, standalone Autopilot feature is `autopilotApi` above. Kept only so the
// registered backend commands remain reachable; rename avoids the name clash.
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

// Skill Rule Generator
// Conversation Skills Indicator

export const statusDockApi = {
  isEnabled: () => invoke<boolean>("get_status_dock_enabled"),
  setEnabled: (enabled: boolean) => invoke<void>("set_status_dock_enabled", { enabled }),
};

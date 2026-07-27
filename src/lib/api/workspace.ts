/** Auto-split from tauri-api.ts — domain: workspace. Import via "@/lib/tauri-api". */
import { invoke } from "@tauri-apps/api/core";
import type {
  BackupTableInfo,
  ImportResult,
  PromptEntry,
  ActivityLogEntry,
} from "@/types";

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


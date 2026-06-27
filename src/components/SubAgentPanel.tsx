import { useCallback, useEffect, useState } from "react";
import { Bot, Plus, Loader2, GitMerge, Square, ExternalLink, Trash2, FolderOpen } from "lucide-react";
import { openPath } from "@tauri-apps/plugin-opener";

import { subAgentApi, worktreeApi, runtimeApi, conversationApi, type SubAgent } from "@/lib/tauri-api";
import type { RuntimeAgentId, RuntimeModelSelection, RuntimePermissionPolicy } from "@/types";
import { toast } from "@/components/ui/sonner";

/**
 * SubAgentPanel — in-session background tasks / sub-agents (R3 follow-up).
 * Spawns an independent child agent session in its OWN Git worktree so it runs
 * concurrently with the parent without sharing a working tree (real
 * session-level parallelism / process overlap). Each sub-agent can be opened
 * (to watch / approve / interact), merged back, stopped, or removed.
 *
 * Honest scope: this is NOT intra-session pipelining. A background sub-agent
 * that hits an approval prompt pauses until you open it and respond.
 */
interface Props {
  parentConversationId: string;
  workspacePath: string;
  /** Runtime agent id for startSession (Claude Code / Codex). */
  agent: RuntimeAgentId | null;
  /** Display agent name for the child conversation row. */
  agentDisplay: string;
  permission: RuntimePermissionPolicy;
  refreshSignal: number;
  onOpenConversation?: (conversationId: string) => void;
}

const STATUS_BADGE: Record<string, { label: string; cls: string }> = {
  running: { label: "运行中", cls: "bg-info/15 text-info" },
  awaiting_approval: { label: "待审批", cls: "bg-warning/15 text-warning" },
  completed: { label: "已完成", cls: "bg-success/15 text-success" },
  failed: { label: "失败", cls: "bg-destructive/15 text-destructive" },
  stopped: { label: "已停止", cls: "bg-muted/30 text-muted-foreground" },
};

/** Map the runtime session status onto a sub-agent status. */
function mapStatus(sessionStatus: string): string {
  switch (sessionStatus) {
    case "completed": return "completed";
    case "failed": return "failed";
    case "cancelled":
    case "stopping": return "stopped";
    case "awaiting_approval": return "awaiting_approval";
    default: return "running";
  }
}

export function SubAgentPanel({ parentConversationId, workspacePath, agent, agentDisplay, permission, refreshSignal, onOpenConversation }: Props) {
  const [subagents, setSubagents] = useState<SubAgent[]>([]);
  const [adding, setAdding] = useState(false);
  const [title, setTitle] = useState("");
  const [prompt, setPrompt] = useState("");
  const [spawning, setSpawning] = useState(false);
  const [busy, setBusy] = useState("");

  const load = useCallback(async () => {
    try {
      const list = await subAgentApi.list(parentConversationId);
      setSubagents(list);
      // Reconcile each child session's live status.
      for (const sub of list) {
        if (sub.status === "completed" || sub.status === "stopped") continue;
        const session = await runtimeApi.getSession(sub.child_session_id).catch(() => null);
        if (session) {
          const next = mapStatus(session.status);
          if (next !== sub.status) await subAgentApi.updateStatus(sub.id, next).catch(() => undefined);
        }
      }
      // Refetch if any status changed.
      setSubagents(await subAgentApi.list(parentConversationId));
    } catch {
      setSubagents([]);
    }
  }, [parentConversationId]);

  useEffect(() => {
    void load();
  }, [load, refreshSignal]);

  const spawn = async () => {
    if (!agent) {
      toast.error("当前 Agent 尚未适配运行时（请用 Claude Code 或 Codex）");
      return;
    }
    if (!prompt.trim()) {
      toast.error("请填写子代理任务");
      return;
    }
    const label = title.trim() || prompt.trim().slice(0, 24);
    setSpawning(true);
    try {
      // 1. Isolated worktree so the sub-agent never clobbers the parent tree.
      const wt = await worktreeApi.create(workspacePath, parentConversationId, label);
      // 2. Child conversation bound to the worktree.
      const childConvId = `sub_${Date.now()}`;
      await conversationApi.create({ id: childConvId, title: `🤖 ${label}`, workspacePath: wt.worktree_path, activeAgent: agentDisplay });
      // 3. Concurrent child session in the worktree, then send the task.
      const model: RuntimeModelSelection = { kind: "agent_default" };
      const session = await runtimeApi.startSession({
        conversation_id: childConvId,
        agent,
        workspace_path: wt.worktree_path,
        model,
        permission,
        work_mode: "direct",
      });
      await runtimeApi.sendMessage(session.id, prompt.trim());
      // 4. Persist the parent → child link.
      await subAgentApi.create({
        parentConversationId,
        title: label,
        prompt: prompt.trim(),
        agent: agentDisplay,
        childConversationId: childConvId,
        childSessionId: session.id,
        worktreeId: wt.id,
        worktreePath: wt.worktree_path,
      });
      toast.success("子代理已在独立 worktree 启动", { description: `${wt.branch}` });
      setTitle("");
      setPrompt("");
      setAdding(false);
      await load();
    } catch (e) {
      toast.error("启动子代理失败", { description: String(e) });
    } finally {
      setSpawning(false);
    }
  };

  const merge = async (sub: SubAgent) => {
    if (!window.confirm(`将子代理「${sub.title}」的改动合并回主工作区？`)) return;
    setBusy(sub.id);
    try {
      const result = await worktreeApi.merge(sub.worktree_id);
      if (result.merged) toast.success("已合并", { description: result.message });
      else toast.error(result.conflict ? "存在合并冲突" : "合并失败", { description: result.message });
    } catch (e) {
      toast.error("合并失败", { description: String(e) });
    } finally {
      setBusy("");
    }
  };

  const stop = async (sub: SubAgent) => {
    setBusy(sub.id);
    try {
      await runtimeApi.stopSession(sub.child_session_id).catch(() => undefined);
      await subAgentApi.updateStatus(sub.id, "stopped");
      await load();
    } finally {
      setBusy("");
    }
  };

  const remove = async (sub: SubAgent) => {
    if (!window.confirm(`移除子代理「${sub.title}」？将停止其会话并删除它的 worktree。`)) return;
    setBusy(sub.id);
    try {
      await runtimeApi.stopSession(sub.child_session_id).catch(() => undefined);
      await worktreeApi.remove(sub.worktree_id, true, true).catch(() => undefined);
      await subAgentApi.remove(sub.id);
      await load();
    } finally {
      setBusy("");
    }
  };

  return (
    <div>
      <div className="mb-2 flex items-center justify-between">
        <div className="flex items-center gap-1.5 text-xs font-semibold text-muted-foreground">
          <Bot className="h-3.5 w-3.5" /> 后台子代理 {subagents.length > 0 && `(${subagents.length})`}
        </div>
        {!adding && (
          <button onClick={() => setAdding(true)} className="flex items-center gap-1 rounded border border-border px-1.5 py-0.5 text-xs hover:bg-muted/30">
            <Plus className="h-3 w-3" /> 新建
          </button>
        )}
      </div>

      {adding && (
        <div className="mb-2 flex flex-col gap-2 rounded-lg border border-border bg-muted/10 p-2">
          <input
            className="rounded border border-border bg-background px-2 py-1.5 text-xs"
            placeholder="标题（可选）"
            value={title}
            onChange={(e) => setTitle(e.target.value)}
          />
          <textarea
            className="min-h-[56px] rounded border border-border bg-background px-2 py-1.5 text-xs"
            placeholder="要这个后台子代理做的任务…（会在独立 worktree 并行执行）"
            value={prompt}
            onChange={(e) => setPrompt(e.target.value)}
          />
          <div className="flex justify-end gap-2">
            <button onClick={() => { setAdding(false); setTitle(""); setPrompt(""); }} className="rounded px-2 py-1 text-xs text-muted-foreground hover:bg-muted/30">取消</button>
            <button onClick={() => void spawn()} disabled={spawning} className="flex items-center gap-1 rounded bg-primary/15 px-2 py-1 text-xs text-primary hover:bg-primary/25 disabled:opacity-50">
              {spawning ? <Loader2 className="h-3 w-3 animate-spin" /> : <Plus className="h-3 w-3" />} 启动
            </button>
          </div>
        </div>
      )}

      {subagents.length === 0 ? (
        !adding && (
          <div className="rounded border border-dashed border-border px-2 py-2 text-xs text-muted-foreground">
            暂无后台子代理。新建一个,让它在隔离 worktree 里和你并行干活;完成后可一键合并回来。
          </div>
        )
      ) : (
        <div className="space-y-1.5">
          {subagents.map((sub) => {
            const badge = STATUS_BADGE[sub.status] ?? STATUS_BADGE.running;
            return (
              <div key={sub.id} className="rounded border border-border px-2 py-1.5">
                <div className="flex items-center gap-2">
                  <Bot className="h-3.5 w-3.5 shrink-0 text-info" />
                  <span className="min-w-0 flex-1 truncate text-xs font-medium" title={sub.prompt}>{sub.title}</span>
                  <span className={`shrink-0 rounded px-1 text-[10px] ${badge.cls}`}>{badge.label}</span>
                </div>
                <div className="mt-1 flex flex-wrap items-center gap-1 pl-5">
                  {onOpenConversation && (
                    <button onClick={() => onOpenConversation(sub.child_conversation_id)} className="flex items-center gap-1 rounded px-1.5 py-0.5 text-[11px] text-muted-foreground hover:bg-muted/30 hover:text-foreground" title="打开子代理会话（查看/审批/交互）">
                      <ExternalLink className="h-3 w-3" /> 打开
                    </button>
                  )}
                  <button onClick={() => openPath(sub.worktree_path).catch((e) => toast.error(`无法打开：${e}`))} className="flex items-center gap-1 rounded px-1.5 py-0.5 text-[11px] text-muted-foreground hover:bg-muted/30 hover:text-foreground" title="打开 worktree 目录">
                    <FolderOpen className="h-3 w-3" /> 目录
                  </button>
                  <button onClick={() => void merge(sub)} disabled={busy === sub.id} className="flex items-center gap-1 rounded px-1.5 py-0.5 text-[11px] text-muted-foreground hover:bg-muted/30 hover:text-foreground disabled:opacity-40" title="合并回主工作区">
                    {busy === sub.id ? <Loader2 className="h-3 w-3 animate-spin" /> : <GitMerge className="h-3 w-3" />} 合并
                  </button>
                  {(sub.status === "running" || sub.status === "awaiting_approval") && (
                    <button onClick={() => void stop(sub)} disabled={busy === sub.id} className="flex items-center gap-1 rounded px-1.5 py-0.5 text-[11px] text-muted-foreground hover:bg-muted/30 hover:text-foreground disabled:opacity-40" title="停止子代理会话">
                      <Square className="h-3 w-3" /> 停止
                    </button>
                  )}
                  <button onClick={() => void remove(sub)} disabled={busy === sub.id} className="flex items-center gap-1 rounded px-1.5 py-0.5 text-[11px] text-muted-foreground hover:bg-destructive/10 hover:text-destructive disabled:opacity-40" title="移除子代理及其 worktree">
                    <Trash2 className="h-3 w-3" /> 移除
                  </button>
                </div>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}

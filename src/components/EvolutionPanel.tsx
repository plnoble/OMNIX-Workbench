/**
 * EvolutionPanel — 进化中枢
 *
 * One place to see the self-improvement loop land:
 *  - per-workspace project-protocol status + recorded events (viewer)
 *  - pending evolution proposals + protocol actions → review & apply/reject
 *  - the embedding model used for relevance/dedup (picker)
 */
import { useCallback, useEffect, useState } from "react";
import {
  Check,
  FileClock,
  FolderGit2,
  GitPullRequestArrow,
  ListChecks,
  Power,
  RefreshCw,
  Trash2,
  X,
} from "lucide-react";

import { Button } from "@/components/ui/button";
import { toast } from "@/components/ui/sonner";
import { cn } from "@/lib/utils";
import {
  modelApi,
  projectProtocolApi,
  settingsApi,
  type EvolutionProposal,
  type ProjectProtocolEvent,
  type ProjectProtocolStatus,
  type ProtocolActionDraft,
} from "@/lib/tauri-api";
import type { PlatformModel } from "@/types";

function shortPath(p: string): string {
  const parts = p.replace(/\\/g, "/").split("/").filter(Boolean);
  return parts.length <= 2 ? p : `…/${parts.slice(-2).join("/")}`;
}

function prettyJson(value: string): string {
  try {
    return JSON.stringify(JSON.parse(value), null, 2);
  } catch {
    return value;
  }
}

export function EvolutionPanel() {
  const [runs, setRuns] = useState<ProjectProtocolStatus[]>([]);
  const [selected, setSelected] = useState<string>("");
  const [events, setEvents] = useState<ProjectProtocolEvent[]>([]);
  const [proposals, setProposals] = useState<EvolutionProposal[]>([]);
  const [actions, setActions] = useState<ProtocolActionDraft[]>([]);
  const [busyId, setBusyId] = useState("");
  const [pendingRemove, setPendingRemove] = useState<ProjectProtocolStatus | null>(null);

  // Embedding model (used for relevance/dedup of experience memories)
  const [embedModels, setEmbedModels] = useState<PlatformModel[]>([]);
  const [embedModel, setEmbedModel] = useState<string>("");

  const loadRuns = useCallback(async () => {
    try {
      setRuns(await projectProtocolApi.listRuns());
    } catch (error) {
      toast.error(`读取协议工作区失败：${error}`);
    }
  }, []);

  const loadWorkspace = useCallback(async (ws: string) => {
    if (!ws) return;
    try {
      const [ev, props, acts] = await Promise.all([
        projectProtocolApi.listEvents(ws, 100),
        projectProtocolApi.listEvolutionProposals(ws, "pending"),
        projectProtocolApi.listActions(ws, "pending"),
      ]);
      setEvents(ev);
      setProposals(props);
      setActions(acts);
    } catch (error) {
      toast.error(`读取工作区协议数据失败：${error}`);
    }
  }, []);

  useEffect(() => {
    void loadRuns();
    // Embedding model picker data
    modelApi
      .getActive()
      .then((models) =>
        setEmbedModels(
          models.filter((m) => m.has_embedding && !m.model_name.toLowerCase().includes("rerank")),
        ),
      )
      .catch(() => {});
    settingsApi
      .get("embedding_model")
      .then((v) => setEmbedModel(v || ""))
      .catch(() => {});
  }, [loadRuns]);

  useEffect(() => {
    void loadWorkspace(selected);
  }, [selected, loadWorkspace]);

  const saveEmbedModel = async (value: string) => {
    const previous = embedModel;
    setEmbedModel(value);
    try {
      await settingsApi.set("embedding_model", value);
      toast.success(value ? `经验向量将使用：${value}` : "已清除 embedding 模型选择");
    } catch (error) {
      // Roll back so the dropdown never shows a value that didn't persist.
      setEmbedModel(previous);
      toast.error(`保存失败：${error}`);
    }
  };

  const applyProposal = async (id: string, approved: boolean) => {
    setBusyId(id);
    try {
      await projectProtocolApi.applyEvolutionProposal(id, approved);
      await Promise.all([loadWorkspace(selected), loadRuns()]);
      toast.success(approved ? "提案已应用" : "提案已拒绝");
    } catch (error) {
      toast.error(`处理提案失败：${error}`);
    } finally {
      setBusyId("");
    }
  };

  const applyAction = async (id: string, approved: boolean) => {
    setBusyId(id);
    try {
      await projectProtocolApi.applyAction(id, approved);
      await Promise.all([loadWorkspace(selected), loadRuns()]);
      toast.success(approved ? "动作已应用" : "动作已拒绝");
    } catch (error) {
      toast.error(`处理动作失败：${error}`);
    } finally {
      setBusyId("");
    }
  };

  // Enable/disable a workspace's protocol without touching any disk files.
  const toggleEnabled = async (r: ProjectProtocolStatus) => {
    setBusyId(r.workspace_path);
    try {
      await projectProtocolApi.setEnabled(r.workspace_path, !r.enabled);
      await loadRuns();
      toast.success(r.enabled ? `已停用「${r.project_name}」的项目协议` : `已启用「${r.project_name}」的项目协议`);
    } catch (error) {
      toast.error(`切换失败：${error}`);
    } finally {
      setBusyId("");
    }
  };

  // Remove a workspace from the Evolution Hub (DB records only — never disk files).
  const removeWorkspace = async (r: ProjectProtocolStatus) => {
    setBusyId(r.workspace_path);
    try {
      await projectProtocolApi.removeWorkspace(r.workspace_path);
      if (selected === r.workspace_path) setSelected("");
      await loadRuns();
      toast.success(`已从进化中枢移除「${r.project_name}」`);
    } catch (error) {
      toast.error(`移除失败：${error}`);
    } finally {
      setBusyId("");
      setPendingRemove(null);
    }
  };

  return (
    <section className="py-5">
      {/* Embedding model picker */}
      <div className="rounded-lg border border-border bg-muted/20 p-4">
        <div className="flex flex-wrap items-center gap-3">
          <span className="text-sm font-medium">经验向量模型</span>
          <select
            className="h-9 min-w-64 rounded-md border border-border bg-background px-3 text-sm"
            value={embedModel}
            onChange={(e) => void saveEmbedModel(e.target.value)}
          >
            <option value="">（自动选择第一个可用的 embedding 模型）</option>
            {embedModels.map((m) => (
              <option key={m.id} value={m.model_name}>
                {m.model_name} · {m.platform_id}
              </option>
            ))}
          </select>
          <span className="text-xs text-muted-foreground">
            用于「相关性注入」和「去重」；记忆与工作区画像必须用同一个模型。
          </span>
        </div>
        {embedModels.length === 0 && (
          <p className="mt-2 text-xs text-amber-500">
            未检测到已启用的 embedding 模型 —— 请先到「模型」页启用一个（如 Qwen/Qwen3-Embedding-8B）。
          </p>
        )}
      </div>

      <div className="mt-5 grid gap-4 lg:grid-cols-[minmax(240px,0.8fr)_minmax(0,1.6fr)]">
        {/* Workspace list */}
        <div>
          <div className="mb-2 flex items-center justify-between">
            <h3 className="flex items-center gap-1.5 text-sm font-semibold">
              <FolderGit2 className="h-4 w-4" /> 协议工作区
            </h3>
            <button
              type="button"
              className="rounded p-1 text-muted-foreground hover:text-foreground"
              title="刷新"
              onClick={() => void loadRuns()}
            >
              <RefreshCw className="h-4 w-4" />
            </button>
          </div>
          <div className="space-y-2">
            {runs.map((r) => {
              const pending = r.pending_proposals + r.pending_actions;
              const busy = busyId === r.workspace_path;
              return (
                <div
                  key={r.workspace_path}
                  role="button"
                  tabIndex={0}
                  onClick={() => setSelected(r.workspace_path)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter" || e.key === " ") {
                      e.preventDefault();
                      setSelected(r.workspace_path);
                    }
                  }}
                  className={cn(
                    "group w-full cursor-pointer rounded-lg border p-3 text-left transition-colors",
                    selected === r.workspace_path
                      ? "border-primary bg-primary/5"
                      : "border-border hover:bg-muted/30",
                  )}
                >
                  <div className="flex items-center justify-between gap-2">
                    <span className="truncate text-sm font-medium">{r.project_name}</span>
                    <div className="flex shrink-0 items-center gap-1">
                      {pending > 0 && (
                        <span className="rounded-full bg-primary px-2 py-0.5 text-xs text-primary-foreground">
                          {pending} 待办
                        </span>
                      )}
                      <button
                        type="button"
                        title={r.enabled ? "停用项目协议" : "启用项目协议"}
                        aria-label={r.enabled ? "停用项目协议" : "启用项目协议"}
                        disabled={busy}
                        onClick={(e) => {
                          e.stopPropagation();
                          void toggleEnabled(r);
                        }}
                        className={cn(
                          "rounded p-1 opacity-0 transition-opacity hover:bg-muted/40 group-hover:opacity-100 disabled:opacity-40",
                          r.enabled ? "text-emerald-500" : "text-muted-foreground",
                        )}
                      >
                        <Power className="h-3.5 w-3.5" />
                      </button>
                      <button
                        type="button"
                        title="从进化中枢移除（不删除磁盘文件）"
                        aria-label="移除工作区"
                        disabled={busy}
                        onClick={(e) => {
                          e.stopPropagation();
                          setPendingRemove(r);
                        }}
                        className="rounded p-1 text-muted-foreground opacity-0 transition-opacity hover:bg-destructive/10 hover:text-destructive group-hover:opacity-100 disabled:opacity-40"
                      >
                        <Trash2 className="h-3.5 w-3.5" />
                      </button>
                    </div>
                  </div>
                  <div className="mt-1 truncate text-xs text-muted-foreground" title={r.workspace_path}>
                    {shortPath(r.workspace_path)}
                  </div>
                  <div className="mt-1.5 flex flex-wrap gap-1.5 text-xs text-muted-foreground">
                    <span className={r.enabled ? "text-emerald-500" : ""}>
                      {r.enabled ? "● 已启用" : "○ 未启用"}
                    </span>
                    {r.last_event_at && <span>· 最近事件 {r.last_event_at.slice(0, 16)}</span>}
                  </div>
                </div>
              );
            })}
            {runs.length === 0 && (
              <div className="rounded-lg border border-dashed border-border py-10 text-center text-sm text-muted-foreground">
                还没有启用项目协议的工作区
              </div>
            )}
          </div>
        </div>

        {/* Selected workspace detail */}
        <div>
          {!selected ? (
            <div className="flex h-full items-center justify-center rounded-lg border border-dashed border-border py-16 text-sm text-muted-foreground">
              选择左侧一个工作区，查看事件与待审提案
            </div>
          ) : (
            <div className="space-y-6">
              {/* Evolution proposals */}
              <div>
                <h3 className="mb-2 flex items-center gap-1.5 text-sm font-semibold">
                  <GitPullRequestArrow className="h-4 w-4" /> 进化提案（{proposals.length}）
                </h3>
                <div className="space-y-2">
                  {proposals.map((p) => (
                    <article key={p.id} className="rounded-lg border border-border p-3">
                      <div className="flex items-start justify-between gap-2">
                        <div>
                          <div className="text-xs text-muted-foreground">{p.proposal_type}</div>
                          <h4 className="text-sm font-medium">{p.title}</h4>
                        </div>
                        <span className="rounded-full border border-border px-2 py-0.5 text-xs">{p.status}</span>
                      </div>
                      {p.rationale && <p className="mt-1.5 text-sm text-muted-foreground">{p.rationale}</p>}
                      {p.diff_json && p.diff_json !== "{}" && (
                        <pre className="mt-2 max-h-40 overflow-auto rounded-md bg-muted p-2 text-xs whitespace-pre-wrap">
                          {prettyJson(p.diff_json)}
                        </pre>
                      )}
                      <div className="mt-3 flex justify-end gap-2">
                        <Button variant="outline" size="sm" disabled={busyId === p.id} onClick={() => applyProposal(p.id, false)}>
                          <X className="h-4 w-4" /> 拒绝
                        </Button>
                        <Button size="sm" disabled={busyId === p.id} onClick={() => applyProposal(p.id, true)}>
                          <Check className="h-4 w-4" /> 应用
                        </Button>
                      </div>
                    </article>
                  ))}
                  {proposals.length === 0 && (
                    <div className="rounded-lg border border-dashed border-border py-6 text-center text-xs text-muted-foreground">
                      没有待审提案
                    </div>
                  )}
                </div>
              </div>

              {/* Protocol actions */}
              <div>
                <h3 className="mb-2 flex items-center gap-1.5 text-sm font-semibold">
                  <ListChecks className="h-4 w-4" /> 协议动作（{actions.length}）
                </h3>
                <div className="space-y-2">
                  {actions.map((a) => (
                    <article key={a.id} className="rounded-lg border border-border p-3">
                      <div className="flex items-start justify-between gap-2">
                        <div>
                          <div className="text-xs text-muted-foreground">{a.action_type}</div>
                          <h4 className="text-sm font-medium">{a.title}</h4>
                        </div>
                        <span className="rounded-full border border-border px-2 py-0.5 text-xs">{a.status}</span>
                      </div>
                      {a.content && <p className="mt-1.5 whitespace-pre-wrap text-sm text-muted-foreground">{a.content}</p>}
                      <div className="mt-3 flex justify-end gap-2">
                        <Button variant="outline" size="sm" disabled={busyId === a.id} onClick={() => applyAction(a.id, false)}>
                          <X className="h-4 w-4" /> 拒绝
                        </Button>
                        <Button size="sm" disabled={busyId === a.id} onClick={() => applyAction(a.id, true)}>
                          <Check className="h-4 w-4" /> 应用
                        </Button>
                      </div>
                    </article>
                  ))}
                  {actions.length === 0 && (
                    <div className="rounded-lg border border-dashed border-border py-6 text-center text-xs text-muted-foreground">
                      没有待处理动作
                    </div>
                  )}
                </div>
              </div>

              {/* Event viewer */}
              <div>
                <h3 className="mb-2 flex items-center gap-1.5 text-sm font-semibold">
                  <FileClock className="h-4 w-4" /> 协议事件（{events.length}）
                </h3>
                <div className="max-h-72 space-y-1.5 overflow-y-auto">
                  {events.map((e) => (
                    <div key={e.id} className="flex items-start gap-2 rounded-md border border-border/60 px-2.5 py-1.5 text-xs">
                      <span
                        className={cn(
                          "mt-0.5 shrink-0 rounded px-1.5 py-0.5 font-medium",
                          e.event_type === "error"
                            ? "bg-destructive/15 text-destructive"
                            : "bg-muted text-muted-foreground",
                        )}
                      >
                        {e.event_type}
                      </span>
                      <span className="min-w-0 flex-1 break-words text-foreground/80">{e.summary}</span>
                      <span className="shrink-0 text-muted-foreground">{e.created_at.slice(5, 16)}</span>
                    </div>
                  ))}
                  {events.length === 0 && (
                    <div className="rounded-lg border border-dashed border-border py-6 text-center text-xs text-muted-foreground">
                      还没有记录事件
                    </div>
                  )}
                </div>
              </div>
            </div>
          )}
        </div>
      </div>

      {/* Confirm remove-from-hub modal (DB records only, never disk files) */}
      {pendingRemove && (
        <div className="fixed inset-0 z-[1000] flex items-center justify-center bg-black/60 p-4 backdrop-blur-sm">
          <div className="w-full max-w-md rounded-lg border border-border bg-card p-5 shadow-xl">
            <h3 className="m-0 mb-2 text-base font-semibold text-foreground">从进化中枢移除工作区？</h3>
            <p className="mb-1 truncate text-sm text-muted-foreground" title={pendingRemove.workspace_path}>
              「{pendingRemove.project_name}」· {shortPath(pendingRemove.workspace_path)}
            </p>
            <p className="mb-4 text-xs leading-5 text-muted-foreground">
              仅从进化中枢的列表中移除，并清除 OMNIX 数据库里该工作区的协议事件、进化提案与动作。
              <span className="text-foreground/80">不会删除磁盘上的任何文件</span>
              （包括工作区里的 <code>.omx/</code> 记录和代码）。如只是暂时不用，建议改用「停用」。
            </p>
            <div className="flex justify-end gap-2">
              <button
                className="rounded-md border border-border bg-muted/10 px-3 py-1.5 text-sm text-foreground hover:bg-muted/30"
                onClick={() => setPendingRemove(null)}
              >
                取消
              </button>
              <button
                className="rounded-md bg-destructive px-3 py-1.5 text-sm text-destructive-foreground hover:bg-destructive/90 disabled:opacity-50"
                disabled={busyId === pendingRemove.workspace_path}
                onClick={() => void removeWorkspace(pendingRemove)}
              >
                确认移除
              </button>
            </div>
          </div>
        </div>
      )}
    </section>
  );
}

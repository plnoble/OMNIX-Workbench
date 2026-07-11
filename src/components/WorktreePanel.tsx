import { useCallback, useEffect, useState } from "react";
import { GitBranch, Plus, FolderOpen, GitMerge, Trash2, Loader2, RefreshCw } from "lucide-react";
import { openPath } from "@tauri-apps/plugin-opener";

import { worktreeApi, type Worktree } from "@/lib/tauri-api";
import { toast } from "@/components/ui/sonner";
import { cn } from "@/lib/utils";

/**
 * WorktreePanel — parallel sessions via Git worktrees. Each worktree is an isolated checkout on its own branch so
 * several agents can work the same repo without clobbering each other. Lists the
 * repo's worktrees (main + OMNIX-created), and lets the user create / open /
 * merge / remove them. Merge surfaces conflicts honestly instead of resolving.
 */
interface Props {
  workspacePath: string;
  conversationId: string;
  /** Bump to refetch (e.g. after a tool_completed runtime event). */
  refreshSignal: number;
}

export function WorktreePanel({ workspacePath, conversationId, refreshSignal }: Props) {
  const [worktrees, setWorktrees] = useState<Worktree[]>([]);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState("");
  const [creating, setCreating] = useState(false);

  const load = useCallback(async () => {
    try {
      setWorktrees(await worktreeApi.list(workspacePath));
      setError("");
    } catch (e) {
      setError(String(e));
      setWorktrees([]);
    }
  }, [workspacePath]);

  useEffect(() => {
    void load();
  }, [load, refreshSignal]);

  const create = async () => {
    setCreating(true);
    try {
      const label = `会话 ${new Date().toLocaleString("zh-CN", { hour: "2-digit", minute: "2-digit", month: "numeric", day: "numeric" })}`;
      const wt = await worktreeApi.create(workspacePath, conversationId, label);
      toast.success("已创建并行 worktree", { description: `${wt.branch} → ${wt.worktree_path}` });
      await load();
    } catch (e) {
      toast.error("创建 worktree 失败", { description: String(e) });
    } finally {
      setCreating(false);
    }
  };

  const merge = async (wt: Worktree) => {
    if (!window.confirm(`将 ${wt.branch} 合并到主工作区当前分支？`)) return;
    setBusy(wt.id);
    try {
      const result = await worktreeApi.merge(wt.id);
      if (result.merged) {
        toast.success("已合并", { description: result.message });
      } else {
        toast.error(result.conflict ? "存在合并冲突" : "合并失败", { description: result.message });
      }
      await load();
    } catch (e) {
      toast.error("合并失败", { description: String(e) });
    } finally {
      setBusy("");
    }
  };

  const remove = async (wt: Worktree) => {
    const force = wt.dirty;
    const msg = wt.dirty
      ? `${wt.branch} 有未提交改动。强制移除该 worktree 及其分支？改动将丢失。`
      : `移除 worktree ${wt.branch} 及其分支？`;
    if (!window.confirm(msg)) return;
    setBusy(wt.id);
    try {
      await worktreeApi.remove(wt.id, true, force);
      toast.success("已移除 worktree");
      await load();
    } catch (e) {
      toast.error("移除失败", { description: String(e) });
    } finally {
      setBusy("");
    }
  };

  const parallel = worktrees.filter((w) => !w.is_main);

  return (
    <div>
      <div className="mb-2 flex items-center justify-between">
        <div className="flex items-center gap-1.5 text-xs font-semibold text-muted-foreground">
          <GitBranch className="h-3.5 w-3.5" /> 并行 worktree {parallel.length > 0 && `(${parallel.length})`}
        </div>
        <div className="flex items-center gap-1">
          <button onClick={() => void load()} title="刷新" className="rounded p-1 text-muted-foreground hover:bg-muted/30 hover:text-foreground">
            <RefreshCw className="h-3.5 w-3.5" />
          </button>
          <button
            onClick={() => void create()}
            disabled={creating}
            title="为当前会话新建隔离的并行 worktree"
            className="flex items-center gap-1 rounded border border-border px-1.5 py-0.5 text-xs hover:bg-muted/30 disabled:opacity-50"
          >
            {creating ? <Loader2 className="h-3 w-3 animate-spin" /> : <Plus className="h-3 w-3" />} 新建
          </button>
        </div>
      </div>

      {error ? (
        <div className="rounded border border-dashed border-border px-2 py-1.5 text-xs text-muted-foreground">{error}</div>
      ) : parallel.length === 0 ? (
        <div className="rounded border border-dashed border-border px-2 py-2 text-xs text-muted-foreground">
          暂无并行 worktree。新建后可让另一个 Agent 在隔离的分支/目录里同时开发，互不干扰。
        </div>
      ) : (
        <div className="space-y-1.5">
          {parallel.map((wt) => (
            <div key={wt.id || wt.worktree_path} className="rounded border border-border px-2 py-1.5">
              <div className="flex items-center gap-2">
                <GitBranch className="h-3.5 w-3.5 shrink-0 text-info" />
                <span className="min-w-0 flex-1 truncate text-xs font-medium" title={wt.worktree_path}>{wt.branch}</span>
                {wt.dirty && <span className="shrink-0 rounded bg-warning/15 px-1 text-[10px] text-warning">未提交</span>}
                {wt.ahead > 0 && <span className="shrink-0 rounded bg-success/15 px-1 text-[10px] text-success">↑{wt.ahead}</span>}
                {!wt.exists && <span className="shrink-0 rounded bg-destructive/15 px-1 text-[10px] text-destructive">已失效</span>}
              </div>
              {wt.label && <div className="mt-0.5 truncate pl-5 text-[10px] text-muted-foreground">{wt.label}</div>}
              <div className="mt-1 flex items-center gap-1 pl-5">
                <button
                  onClick={() => openPath(wt.worktree_path).catch((e) => toast.error(`无法打开：${e}`))}
                  disabled={!wt.exists}
                  className="flex items-center gap-1 rounded px-1.5 py-0.5 text-[11px] text-muted-foreground hover:bg-muted/30 hover:text-foreground disabled:opacity-40"
                  title="在文件管理器中打开"
                >
                  <FolderOpen className="h-3 w-3" /> 打开
                </button>
                <button
                  onClick={() => void merge(wt)}
                  disabled={busy === wt.id || wt.ahead === 0}
                  className="flex items-center gap-1 rounded px-1.5 py-0.5 text-[11px] text-muted-foreground hover:bg-muted/30 hover:text-foreground disabled:opacity-40"
                  title={wt.ahead === 0 ? "没有领先提交可合并" : "合并到主工作区"}
                >
                  {busy === wt.id ? <Loader2 className="h-3 w-3 animate-spin" /> : <GitMerge className="h-3 w-3" />} 合并
                </button>
                <button
                  onClick={() => void remove(wt)}
                  disabled={busy === wt.id}
                  className={cn(
                    "flex items-center gap-1 rounded px-1.5 py-0.5 text-[11px] hover:bg-destructive/10",
                    "text-muted-foreground hover:text-destructive disabled:opacity-40",
                  )}
                  title="移除 worktree 及其分支"
                >
                  <Trash2 className="h-3 w-3" /> 移除
                </button>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

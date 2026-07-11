import { useCallback, useEffect, useState } from "react";
import { History, RotateCcw, Undo2, ChevronDown, ChevronRight } from "lucide-react";

import { checkpointApi, type Checkpoint, type FileDiff } from "@/lib/tauri-api";
import { toast } from "@/components/ui/sonner";
import { cn } from "@/lib/utils";

/**
 * WorkspaceCheckpoints — checkpoint timeline + per-file diff review for the Work
 * surface. Shows the agent's changes
 * against the latest checkpoint, lets the user reject a single file or rewind
 * the whole workspace.
 */
interface Props {
  workspacePath: string;
  conversationId: string;
  /** Bump to refetch (e.g. after a tool_completed runtime event). */
  refreshSignal: number;
}

const STATUS_STYLE: Record<string, { label: string; cls: string }> = {
  A: { label: "新增", cls: "text-success border-success/40" },
  M: { label: "修改", cls: "text-warning border-warning/40" },
  D: { label: "删除", cls: "text-destructive border-destructive/40" },
  R: { label: "重命名", cls: "text-info border-info/40" },
};

export function WorkspaceCheckpoints({ workspacePath, conversationId, refreshSignal }: Props) {
  const [checkpoints, setCheckpoints] = useState<Checkpoint[]>([]);
  const [diffs, setDiffs] = useState<FileDiff[]>([]);
  const [expanded, setExpanded] = useState<string | null>(null);
  const [busy, setBusy] = useState("");
  const [error, setError] = useState("");

  const baseCheckpointId = checkpoints[0]?.id;

  const load = useCallback(async () => {
    try {
      const list = await checkpointApi.list(workspacePath, conversationId);
      setCheckpoints(list);
      const fileDiffs = await checkpointApi.diff(workspacePath, list[0]?.id);
      setDiffs(fileDiffs);
      setError("");
    } catch (e) {
      setError(String(e));
      setDiffs([]);
    }
  }, [workspacePath, conversationId]);

  useEffect(() => {
    void load();
  }, [load, refreshSignal]);

  const rejectFile = async (path: string) => {
    if (!baseCheckpointId) return;
    if (!window.confirm(`还原「${path}」到检查点版本？该文件的本次改动将丢失。`)) return;
    setBusy(path);
    try {
      await checkpointApi.revertFile(baseCheckpointId, path);
      await load();
      toast.success(`已还原 ${path}`);
    } catch (e) {
      toast.error(`还原失败：${e}`);
    } finally {
      setBusy("");
    }
  };

  const restore = async (cp: Checkpoint) => {
    if (!window.confirm(`回退整个工作区到此检查点？\n「${cp.label || cp.created_at}」\n回退前会自动再建一个备份点。`)) return;
    setBusy(cp.id);
    try {
      await checkpointApi.restore(cp.id);
      await load();
      toast.success("已回退到该检查点");
    } catch (e) {
      toast.error(`回退失败：${e}`);
    } finally {
      setBusy("");
    }
  };

  return (
    <div className="flex flex-col gap-3 text-sm">
      {/* Changes / diff */}
      <div>
        <div className="mb-1.5 flex items-center gap-1.5 text-xs font-semibold text-muted-foreground">
          <History className="h-3.5 w-3.5" />
          本次改动（对比最近检查点）
          {diffs.length > 0 && <span className="rounded bg-muted/40 px-1.5">{diffs.length}</span>}
        </div>
        {error ? (
          <p className="text-xs text-muted-foreground">{error}</p>
        ) : diffs.length === 0 ? (
          <p className="text-xs text-muted-foreground">暂无改动</p>
        ) : (
          <div className="flex flex-col gap-1">
            {diffs.map((d) => {
              const style = STATUS_STYLE[d.status] ?? { label: d.status, cls: "text-muted-foreground border-border" };
              const open = expanded === d.path;
              return (
                <div key={d.path} className="rounded-md border border-border">
                  <div className="flex items-center gap-1.5 px-2 py-1.5">
                    <button onClick={() => setExpanded(open ? null : d.path)} className="text-muted-foreground hover:text-foreground">
                      {open ? <ChevronDown className="h-3.5 w-3.5" /> : <ChevronRight className="h-3.5 w-3.5" />}
                    </button>
                    <span className={cn("rounded border px-1 text-[10px]", style.cls)}>{style.label}</span>
                    <span className="min-w-0 flex-1 truncate text-xs" title={d.path}>{d.path}</span>
                    <span className="shrink-0 text-[10px] text-muted-foreground">+{d.additions} -{d.deletions}</span>
                    {baseCheckpointId && (
                      <button
                        onClick={() => rejectFile(d.path)}
                        disabled={busy === d.path}
                        title="拒绝：还原此文件到检查点版本"
                        className="shrink-0 text-muted-foreground hover:text-destructive"
                      >
                        <Undo2 className="h-3.5 w-3.5" />
                      </button>
                    )}
                  </div>
                  {open && (
                    <pre className="max-h-64 overflow-auto border-t border-border bg-muted/30 p-2 text-[11px] leading-5">
                      {d.unified_diff.split("\n").map((line, i) => (
                        <div
                          key={i}
                          className={cn(
                            "whitespace-pre-wrap break-all",
                            line.startsWith("+") && !line.startsWith("+++") && "text-success",
                            line.startsWith("-") && !line.startsWith("---") && "text-destructive",
                            line.startsWith("@@") && "text-info",
                          )}
                        >
                          {line || " "}
                        </div>
                      ))}
                    </pre>
                  )}
                </div>
              );
            })}
          </div>
        )}
      </div>

      {/* Checkpoint timeline */}
      <div>
        <div className="mb-1.5 flex items-center gap-1.5 text-xs font-semibold text-muted-foreground">
          <RotateCcw className="h-3.5 w-3.5" />
          检查点
        </div>
        {checkpoints.length === 0 ? (
          <p className="text-xs text-muted-foreground">改动前会自动创建检查点</p>
        ) : (
          <div className="flex flex-col gap-1">
            {checkpoints.map((cp) => (
              <div key={cp.id} className="flex items-center gap-2 rounded-md border border-border px-2 py-1.5">
                <span className="min-w-0 flex-1">
                  <span className="block truncate text-xs">{cp.label || "检查点"}</span>
                  <span className="block text-[10px] text-muted-foreground">{new Date(cp.created_at).toLocaleString()}</span>
                </span>
                <button
                  onClick={() => restore(cp)}
                  disabled={busy === cp.id}
                  className="shrink-0 rounded border border-border px-2 py-0.5 text-[11px] hover:bg-muted/20"
                >
                  回退到此
                </button>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

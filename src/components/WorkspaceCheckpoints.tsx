import { useCallback, useEffect, useState } from "react";
import { History, RotateCcw, Undo2, ChevronDown, ChevronRight, FileCode } from "lucide-react";

import { checkpointApi, codeAnalysisApi, type Checkpoint, type CodebaseAnalysis, type FileDiff } from "@/lib/tauri-api";
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
      <CodebaseStats workspacePath={workspacePath} />

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

/**
 * 这个工作区有多大：文件数、行数、语言分布、最大的几个文件。
 *
 * **要人按才扫**。它遍历整棵目录树并逐个文件数行数——挂在「选了工作区就自动跑」
 * 上会变成用户没要求的后台开销，而这个数字并不是每次都需要看。
 *
 * 结果不缓存：库随时在变，缓存一份旧数字比不显示更容易误导。
 */
function CodebaseStats({ workspacePath }: { workspacePath: string }) {
  const [stats, setStats] = useState<CodebaseAnalysis | null>(null);
  const [scanning, setScanning] = useState(false);

  // 换了工作区，上一个的统计立刻作废——留着会张冠李戴。
  useEffect(() => { setStats(null); }, [workspacePath]);

  const scan = async () => {
    setScanning(true);
    try {
      setStats(await codeAnalysisApi.analyze(workspacePath));
    } catch (e) {
      toast.error(`统计失败：${e}`);
    } finally {
      setScanning(false);
    }
  };

  // 语言按文件数降序，只显示前 5 种——尾巴上那些各一两个文件的没有信息量。
  const topLanguages = stats
    ? Object.entries(stats.languages).sort((a, b) => b[1] - a[1]).slice(0, 5)
    : [];

  return (
    <div>
      <div className="mb-1.5 flex items-center gap-1.5 text-xs font-semibold text-muted-foreground">
        <FileCode className="h-3.5 w-3.5" />
        代码库规模
        <button
          className="ml-auto rounded border border-border px-1.5 py-0.5 text-[10px] font-normal hover:bg-muted/40 disabled:opacity-50"
          disabled={scanning || !workspacePath}
          onClick={() => void scan()}
          title="遍历工作区统计文件数、行数和语言分布。跳过 node_modules / target / .venv 这类构建产物和依赖树。"
        >
          {scanning ? "统计中…" : stats ? "重新统计" : "统计"}
        </button>
      </div>
      {stats === null ? (
        <p className="text-xs text-muted-foreground">
          {scanning ? "正在遍历目录…" : "未统计。"}
        </p>
      ) : (
        <div className="flex flex-col gap-1.5 rounded-md border border-border/60 px-2.5 py-2">
          <div className="text-xs">
            {stats.total_files.toLocaleString()} 个文件 · {stats.total_lines.toLocaleString()} 行
          </div>
          {topLanguages.length > 0 && (
            <div className="flex flex-wrap gap-1">
              {topLanguages.map(([lang, count]) => (
                <span key={lang} className="rounded bg-muted/40 px-1.5 py-0.5 text-[10px]">
                  {lang} {count}
                </span>
              ))}
            </div>
          )}
          {stats.largest_files.length > 0 && (
            <div className="text-[11px] text-muted-foreground">
              最大：
              {stats.largest_files.slice(0, 3).map((f) => (
                <span key={f.name} className="ml-1" title={`${f.size_bytes.toLocaleString()} 字节`}>
                  {f.name} ({Math.round(f.size_bytes / 1024)}KB)
                </span>
              ))}
            </div>
          )}
        </div>
      )}
    </div>
  );
}

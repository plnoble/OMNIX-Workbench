/**
 * SkillSyncPanel — 工具同步（把正式池技能分发到不走网关的 agent 工具目录）。
 * 从旧版 SkillHub 拆出的精简版：工具状态 + 全盘扫描 + 一键统一。
 */
import { useEffect, useState } from "react";
import { ArrowRightLeft, HardDrive, Loader2, RefreshCw, X } from "lucide-react";
import { toast } from "sonner";

import { cn } from "@/lib/utils";
import { skillSyncApi, type ScanReport, type ToolStatus } from "@/lib/tauri-api";

export function SkillSyncPanel({ onClose }: { onClose: () => void }) {
  const [tools, setTools] = useState<ToolStatus[]>([]);
  const [report, setReport] = useState<ScanReport | null>(null);
  const [scanning, setScanning] = useState(false);
  const [unifying, setUnifying] = useState(false);

  useEffect(() => {
    skillSyncApi.getToolStatus().then(setTools).catch(() => {});
  }, []);

  const scan = async () => {
    setScanning(true);
    try {
      setReport(await skillSyncApi.scanDiskSkills());
    } catch (e) {
      toast.error(`扫描失败：${e}`);
    } finally {
      setScanning(false);
    }
  };

  /** 一键统一：未纳管的导入中央库 → 已纳管+漂移的全部按中央库覆盖分发。 */
  const unifyAll = async () => {
    if (!report) return;
    if (!window.confirm("一键统一：把未纳管技能导入中央库（进待定池），并把中央库技能按覆盖策略分发到所有已安装工具。继续？")) return;
    setUnifying(true);
    try {
      if (report.unmanaged.length > 0) {
        await skillSyncApi.importUnmanaged(report.unmanaged);
      }
      const names = [...new Set([...report.managed, ...report.drifted].map((s) => s.name))];
      if (names.length > 0) {
        const results = await skillSyncApi.syncBatch(names, "symlink", "overwrite");
        const ok = results.reduce((n, r) => n + r.succeeded, 0);
        const total = results.reduce((n, r) => n + r.total, 0);
        toast.success(`统一完成：导入 ${report.unmanaged.length}，分发 ${ok}/${total}`);
      } else {
        toast.success(`统一完成：导入 ${report.unmanaged.length} 个技能`);
      }
      await scan();
    } catch (e) {
      toast.error(`统一失败：${e}`);
    } finally {
      setUnifying(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-8" onClick={onClose}>
      <div
        className="flex max-h-full w-full max-w-2xl flex-col gap-3 rounded-xl border border-border bg-background p-4 shadow-2xl"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center justify-between">
          <span className="text-sm font-semibold">工具同步</span>
          <button onClick={onClose}>
            <X className="h-4 w-4 text-muted-foreground hover:text-foreground" />
          </button>
        </div>
        <p className="text-xs leading-5 text-muted-foreground">
          走网关的 agent 已由「网关注入直调」覆盖，无需分发；这里用于把技能物理分发到不走网关的工具目录（软链，Windows 自动回退复制）。
        </p>

        {/* tools */}
        <div className="flex flex-wrap gap-1.5">
          {tools.map((t) => (
            <span
              key={t.tool_id}
              className={cn(
                "rounded border px-2 py-0.5 text-[11px]",
                t.is_installed
                  ? "border-success/40 bg-success/10 text-success"
                  : "border-border bg-muted/20 text-muted-foreground",
              )}
              title={t.skill_base_path}
            >
              <HardDrive className="mr-1 inline h-3 w-3" />
              {t.display_name}
              {t.is_installed ? "" : "（未装）"}
            </span>
          ))}
        </div>

        <div className="flex gap-2">
          <button
            onClick={() => void scan()}
            disabled={scanning}
            className="inline-flex h-9 items-center gap-1.5 rounded-lg border border-border px-4 text-sm hover:bg-muted/40 disabled:opacity-50"
          >
            {scanning ? <Loader2 className="h-4 w-4 animate-spin" /> : <RefreshCw className="h-4 w-4" />}
            扫描全盘
          </button>
          <button
            onClick={() => void unifyAll()}
            disabled={unifying || !report}
            className="inline-flex h-9 items-center gap-1.5 rounded-lg bg-primary px-4 text-sm font-medium text-primary-foreground hover:opacity-90 disabled:opacity-50"
          >
            {unifying ? <Loader2 className="h-4 w-4 animate-spin" /> : <ArrowRightLeft className="h-4 w-4" />}
            一键统一
          </button>
        </div>

        {report && (
          <div className="grid grid-cols-4 gap-2 text-center">
            {(
              [
                ["已纳管", report.managed.length, "text-success"],
                ["未纳管", report.unmanaged.length, "text-warning"],
                ["漂移", report.drifted.length, "text-destructive"],
                ["孤儿", report.orphaned.length, "text-muted-foreground"],
              ] as [string, number, string][]
            ).map(([label, n, cls]) => (
              <div key={label} className="rounded-lg border border-border bg-card/40 p-2">
                <div className={cn("text-lg font-semibold", cls)}>{n}</div>
                <div className="text-[10px] text-muted-foreground">{label}</div>
              </div>
            ))}
          </div>
        )}
        {report && report.unmanaged.length > 0 && (
          <div className="max-h-40 overflow-y-auto rounded-lg border border-border p-2 text-xs text-muted-foreground">
            未纳管：{report.unmanaged.map((s) => s.name).join("、")}
          </div>
        )}
      </div>
    </div>
  );
}

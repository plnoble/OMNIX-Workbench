import { Check, FileCode2, RotateCcw, X } from "lucide-react";

import { Button } from "@/components/ui/button";
import {
  applyReview,
  evidenceShelfRows,
  panelKindLabel,
  restoredProcessLabel,
  type TaskDesktopLayout,
  type TaskEvidenceBundle,
  type TaskPanelKind,
} from "@/lib/taskDesktop";

export function TaskEvidenceShelf({
  layout,
  evidence,
  running,
  onOpenPanel,
  onClosePanel,
  onEvidenceChange,
}: {
  layout: TaskDesktopLayout | null;
  evidence: TaskEvidenceBundle | null;
  running: boolean;
  onOpenPanel: (kind: TaskPanelKind, resource: string, stealFocus?: boolean) => void;
  onClosePanel: (panelId: string) => void;
  onEvidenceChange: (next: TaskEvidenceBundle) => void;
}) {
  const rows = evidence ? evidenceShelfRows(evidence) : [];
  const panels = layout?.panels ?? [];
  if (!layout && rows.length === 0) return null;

  return (
    <section className="space-y-3">
      <div className="text-xs font-semibold text-muted-foreground">任务现场</div>
      {panels.length === 0 ? (
        <div className="text-xs text-muted-foreground">还没有打开的文件 / Diff / 预览</div>
      ) : (
        <div className="space-y-1">
          {panels.map((panel) => (
            <div key={panel.id} className="flex items-center gap-1">
              <button
                type="button"
                className="flex min-w-0 flex-1 items-center gap-1.5 rounded px-1.5 py-1 text-left text-xs hover:bg-muted/25"
                onClick={() => onOpenPanel(panel.kind, panel.resource, true)}
                title={panel.resource}
              >
                <FileCode2 className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
                <span className="shrink-0 text-muted-foreground">{panelKindLabel(panel.kind)}</span>
                <span className="truncate">{panel.resource.split(/[\\/]/).pop()}</span>
                {panel.kind === "logs" && !running && (
                  <span className="ml-auto shrink-0 text-muted-foreground">{restoredProcessLabel()}</span>
                )}
              </button>
              <button
                type="button"
                className="rounded p-1 text-muted-foreground hover:bg-muted/30"
                title="关闭面板，不会停止后台任务"
                onClick={() => onClosePanel(panel.id)}
              >
                <X className="h-3 w-3" />
              </button>
            </div>
          ))}
        </div>
      )}

      {rows.length > 0 && (
        <div className="space-y-2">
          <div className="text-xs font-semibold text-muted-foreground">交付与验证</div>
          {rows.map((row) => (
            <div key={row.artifact.id} className="rounded-md border border-border px-2 py-2 text-xs">
              <div className="font-medium">{row.artifact.path}</div>
              <div className="mt-1 text-muted-foreground">
                {row.artifact.source.agent} · {row.artifact.source.stepId}
              </div>
              <div className="mt-1 text-muted-foreground">
                验证：{row.verification?.status ?? "无"}
                {row.verification?.command ? ` · ${row.verification.command}` : ""}
              </div>
              {row.acceptedByTestsOnly && (
                <div className="mt-1 text-warning">测试通过不等于已接受</div>
              )}
              <div className="mt-2 flex flex-wrap gap-1">
                <Button
                  size="sm"
                  variant="outline"
                  onClick={() => onEvidenceChange(applyReview(evidence!, { artifactId: row.artifact.id, decision: "accepted" }))}
                >
                  <Check className="h-3 w-3" /> 接受
                </Button>
                <Button
                  size="sm"
                  variant="outline"
                  onClick={() => onEvidenceChange(applyReview(evidence!, { artifactId: row.artifact.id, decision: "rejected" }))}
                >
                  <X className="h-3 w-3" /> 拒绝
                </Button>
                {row.review && (
                  <span className="ml-1 self-center text-muted-foreground">
                    已{row.review.decision === "accepted" ? "接受" : "拒绝"}
                  </span>
                )}
              </div>
            </div>
          ))}
        </div>
      )}

      {panels.some((panel) => panel.kind === "logs") && running && (
        <div className="flex items-center gap-1 text-xs text-muted-foreground">
          <RotateCcw className="h-3 w-3" />
          关闭日志面板不会停止正在运行的任务
        </div>
      )}
    </section>
  );
}

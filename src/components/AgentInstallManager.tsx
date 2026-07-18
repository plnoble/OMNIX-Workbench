/**
 * AgentInstallManager — Agent 安装管理 (R3 统一安装).
 *
 * 扫描每个 agent 在电脑上的全部安装副本（npm 全局 / OMNIX 托管 / 其它），
 * 标出 OMNIX 实际在用的那份，删掉多余的，并把新安装统一收进托管目录
 * （目录可在 设置→数据备份→存储位置 里指到 D 盘）。
 */
import { useCallback, useState } from "react";
import { CheckCircle2, HardDrive, Loader2, ScanSearch, Trash2 } from "lucide-react";
import { toast } from "sonner";

import { cn } from "@/lib/utils";
import { agentApi, agentInstallApi, type AgentInstallGroup } from "@/lib/tauri-api";

const KIND_META: Record<string, { label: string; cls: string }> = {
  managed: { label: "OMNIX 托管", cls: "bg-success/15 text-success" },
  npm_global: { label: "npm 全局", cls: "bg-primary/15 text-primary" },
  other: { label: "其它来源", cls: "bg-muted/60 text-muted-foreground" },
};

export function AgentInstallManager() {
  const [groups, setGroups] = useState<AgentInstallGroup[] | null>(null);
  const [scanning, setScanning] = useState(false);
  const [busy, setBusy] = useState<string | null>(null);

  const scan = useCallback(async () => {
    setScanning(true);
    try {
      setGroups(await agentInstallApi.scan());
    } catch (e) {
      toast.error(`扫描失败：${e}`);
    } finally {
      setScanning(false);
    }
  }, []);

  const handleRemove = async (agent: string, kind: string, path: string) => {
    if (
      !window.confirm(
        `删除「${agent}」的这个安装副本？\n${path}\n\n${
          kind === "npm_global" ? "将执行 npm uninstall -g" : "将从 OMNIX 托管目录移除"
        }`,
      )
    )
      return;
    setBusy(path);
    try {
      await agentInstallApi.remove(agent, kind);
      toast.success(`已删除 ${agent} 的${KIND_META[kind]?.label ?? ""}副本`);
      await scan();
    } catch (e) {
      toast.error(String(e));
    } finally {
      setBusy(null);
    }
  };

  const handleInstallManaged = async (agent: string) => {
    setBusy(`install:${agent}`);
    try {
      await agentApi.install(agent);
      toast.success(`${agent} 已安装到托管目录`);
      await scan();
    } catch (e) {
      toast.error(`安装失败：${e}`);
    } finally {
      setBusy(null);
    }
  };

  return (
    <div className="rounded-xl border border-border glass-surface p-4">
      <div className="flex flex-wrap items-center gap-2">
        <div className="flex items-center gap-2 text-sm font-semibold">
          <HardDrive className="h-4 w-4 text-primary" /> 安装管理
        </div>
        <p className="text-xs text-muted-foreground">
          查出每个 Agent 装在哪几处、删掉多余的、统一装进托管目录（位置见 设置→数据备份→存储位置）
        </p>
        <button
          onClick={() => void scan()}
          disabled={scanning}
          className="ml-auto inline-flex h-8 items-center gap-1.5 rounded-lg border border-border px-3 text-sm hover:bg-muted/40 disabled:opacity-50"
        >
          {scanning ? <Loader2 className="h-4 w-4 animate-spin" /> : <ScanSearch className="h-4 w-4" />}
          {groups ? "重新扫描" : "扫描全部安装"}
        </button>
      </div>

      {groups && (
        <div className="mt-3 flex flex-col gap-3">
          {groups.map((g) => {
            const hasManaged = g.installations.some((i) => i.kind === "managed");
            return (
              <div key={g.agent} className="rounded-lg border border-border bg-background/60 p-3">
                <div className="flex flex-wrap items-center gap-2">
                  <span className="text-sm font-medium">{g.agent}</span>
                  <span className="text-[10px] text-muted-foreground">
                    {g.installations.length === 0
                      ? "未安装"
                      : `${g.installations.length} 处安装`}
                  </span>
                  {!hasManaged && (
                    <button
                      onClick={() => void handleInstallManaged(g.agent)}
                      disabled={busy !== null}
                      className="ml-auto inline-flex h-7 items-center gap-1 rounded-md border border-success/40 bg-success/10 px-2 text-xs text-success hover:bg-success/20 disabled:opacity-50"
                    >
                      {busy === `install:${g.agent}` ? (
                        <Loader2 className="h-3 w-3 animate-spin" />
                      ) : (
                        <HardDrive className="h-3 w-3" />
                      )}
                      装进托管目录
                    </button>
                  )}
                </div>
                {g.installations.map((inst) => (
                  <div
                    key={inst.path}
                    className="mt-1.5 flex flex-wrap items-center gap-2 rounded-md border border-border/60 px-2 py-1.5"
                  >
                    <span
                      className={cn(
                        "shrink-0 rounded px-1.5 py-0.5 text-[10px]",
                        KIND_META[inst.kind]?.cls ?? KIND_META.other.cls,
                      )}
                    >
                      {KIND_META[inst.kind]?.label ?? inst.kind}
                    </span>
                    {inst.is_active && (
                      <span className="inline-flex shrink-0 items-center gap-0.5 rounded bg-primary/15 px-1.5 py-0.5 text-[10px] text-primary">
                        <CheckCircle2 className="h-2.5 w-2.5" /> 使用中
                      </span>
                    )}
                    <span
                      className="min-w-0 flex-1 truncate font-mono text-[11px] text-muted-foreground"
                      title={inst.path}
                    >
                      {inst.path}
                    </span>
                    {inst.version && (
                      <span className="shrink-0 text-[10px] text-muted-foreground">{inst.version}</span>
                    )}
                    {inst.kind !== "other" && (
                      <button
                        onClick={() => void handleRemove(g.agent, inst.kind, inst.path)}
                        disabled={busy !== null}
                        title="删除这个副本"
                        className="shrink-0 rounded p-1 text-muted-foreground hover:text-destructive disabled:opacity-50"
                      >
                        {busy === inst.path ? (
                          <Loader2 className="h-3 w-3 animate-spin" />
                        ) : (
                          <Trash2 className="h-3 w-3" />
                        )}
                      </button>
                    )}
                  </div>
                ))}
                <p className="mt-1.5 text-[10px] text-muted-foreground">
                  提示：系统 PATH 里的安装优先于托管副本——想统一用托管目录，就把 npm 全局等多余副本删掉。
                </p>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}

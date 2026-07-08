/**
 * GatewayHealthCard — per-platform circuit breaker status (cc-switch inspired).
 *
 * Reads the live circuit state the proxy maintains: a platform that keeps
 * failing trips to Open (skipped by the router) and auto-probes back to
 * HalfOpen after a cooldown. Healthy platforms collapse into a one-line
 * summary; only degraded/tripped ones are shown in detail, each resettable.
 */
import { useCallback, useEffect, useState } from "react";
import { Activity, RotateCcw, ShieldAlert, ShieldCheck } from "lucide-react";

import { toast } from "@/components/ui/sonner";
import { cn } from "@/lib/utils";
import { circuitBreakerApi, type CircuitBreakerStatus, type CircuitState } from "@/lib/tauri-api";

const STATE_META: Record<CircuitState, { label: string; cls: string; dot: string }> = {
  Closed: { label: "正常", cls: "text-success", dot: "bg-success" },
  HalfOpen: { label: "探测中", cls: "text-warning", dot: "bg-warning" },
  Open: { label: "已熔断", cls: "text-destructive", dot: "bg-destructive" },
};

export function GatewayHealthCard({ className }: { className?: string }) {
  const [rows, setRows] = useState<CircuitBreakerStatus[]>([]);
  const [busy, setBusy] = useState("");

  const load = useCallback(async () => {
    try {
      setRows(await circuitBreakerApi.getStatus());
    } catch {
      /* transient — keep last known */
    }
  }, []);

  useEffect(() => {
    void load();
    const timer = setInterval(() => void load(), 10_000);
    return () => clearInterval(timer);
  }, [load]);

  const reset = async (platformId: string) => {
    setBusy(platformId);
    try {
      await circuitBreakerApi.reset(platformId);
      await load();
      toast.success(`已重置 ${platformId} 的熔断状态`);
    } catch (error) {
      toast.error(`重置失败：${String(error)}`);
    } finally {
      setBusy("");
    }
  };

  const degraded = rows.filter((r) => r.state !== "Closed");
  const healthy = rows.length - degraded.length;

  return (
    <div className={cn("rounded-lg border border-border bg-card/40 p-4", className)}>
      <div className="mb-3 flex items-center justify-between">
        <div className="flex items-center gap-2 text-sm font-semibold">
          <Activity className="h-4 w-4 text-primary" /> 网关健康
        </div>
        <span className="flex items-center gap-1.5 text-xs text-muted-foreground">
          {degraded.length === 0 ? (
            <>
              <ShieldCheck className="h-3.5 w-3.5 text-success" />
              {rows.length > 0 ? `全部 ${rows.length} 个平台正常` : "暂无启用的平台"}
            </>
          ) : (
            <>
              <ShieldAlert className="h-3.5 w-3.5 text-warning" />
              {degraded.length} 个异常 · {healthy} 个正常
            </>
          )}
        </span>
      </div>

      {degraded.length === 0 ? (
        <p className="text-xs text-muted-foreground">
          连续失败达阈值的平台会被自动熔断并从路由中摘除，冷却后自动探测恢复。
        </p>
      ) : (
        <div className="space-y-2">
          {degraded.map((r) => {
            const meta = STATE_META[r.state];
            return (
              <div
                key={r.platform_id}
                className="flex items-center gap-3 rounded-md border border-border/60 px-3 py-2"
              >
                <span className={cn("h-2 w-2 shrink-0 rounded-full", meta.dot)} />
                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-2">
                    <span className="truncate text-sm font-medium">{r.platform_id}</span>
                    <span className={cn("text-xs font-medium", meta.cls)}>{meta.label}</span>
                    <span className="text-xs text-muted-foreground">
                      连续失败 {r.consecutive_failures}
                    </span>
                  </div>
                  {r.last_error && (
                    <div className="mt-0.5 truncate text-xs text-muted-foreground" title={r.last_error}>
                      {r.last_error}
                    </div>
                  )}
                </div>
                <button
                  type="button"
                  disabled={busy === r.platform_id}
                  onClick={() => void reset(r.platform_id)}
                  className="flex shrink-0 items-center gap-1 rounded border border-border px-2 py-1 text-xs text-muted-foreground hover:bg-muted/30 hover:text-foreground disabled:opacity-50"
                >
                  <RotateCcw className="h-3 w-3" /> 重置
                </button>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}

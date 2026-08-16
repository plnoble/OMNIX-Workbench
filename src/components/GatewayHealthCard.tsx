/**
 * GatewayHealthCard — per-platform circuit breaker status + on-demand probe.
 *
 * Reads the live circuit state the proxy maintains: a platform that keeps
 * failing trips to Open (skipped by the router) and auto-probes back to
 * HalfOpen after a cooldown. Healthy platforms collapse into a one-line
 * summary; only degraded/tripped ones are shown in detail, each resettable.
 *
 * 熔断器只能**被动**观察真实流量——没跑过请求的平台，它什么都不知道。所以这里
 * 还挂了一次性的主动探测（`check_all_platform_health`）：逐个平台发探针，回报
 * 可达性、延迟、模型数。两者互补，合在一张卡里而不是新开一张——它们讲的是同一
 * 件事的两面，分成两张卡只会让人问「这俩有什么区别」。
 *
 * 模型中心的头部本来就写着「批量健康检查」，但那条命令一直没有调用方——接上它
 * 之前，那句话是空头承诺。
 */
import { useCallback, useState } from "react";
import { usePolling } from "@/hooks/usePolling";
import { Activity, RotateCcw, ShieldAlert, ShieldCheck, Stethoscope } from "lucide-react";

import { toast } from "@/components/ui/sonner";
import { cn } from "@/lib/utils";
import {
  circuitBreakerApi,
  healthCheckApi,
  type CircuitBreakerStatus,
  type CircuitState,
  type HealthCheckResult,
} from "@/lib/tauri-api";

const STATE_META: Record<CircuitState, { label: string; cls: string; dot: string }> = {
  Closed: { label: "正常", cls: "text-success", dot: "bg-success" },
  HalfOpen: { label: "探测中", cls: "text-warning", dot: "bg-warning" },
  Open: { label: "已熔断", cls: "text-destructive", dot: "bg-destructive" },
};

export function GatewayHealthCard({ className }: { className?: string }) {
  const [rows, setRows] = useState<CircuitBreakerStatus[]>([]);
  const [busy, setBusy] = useState("");
  /** 上一次主动探测的结果。null = 还没探测过，和「探测结果全正常」是两回事。 */
  const [probe, setProbe] = useState<HealthCheckResult[] | null>(null);
  const [probing, setProbing] = useState(false);

  const load = useCallback(async () => {
    try {
      setRows(await circuitBreakerApi.getStatus());
    } catch {
      /* transient — keep last known */
    }
  }, []);

  usePolling(load, 10_000);

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

  /**
   * 主动探测一遍所有启用的平台。
   *
   * 结果**不写回熔断器**——探针通不通和真实请求成不成功是两件事（探针可能只打
   * `/models`，而真实请求要过鉴权、配额、模型可用性）。拿探测结果去重置熔断状态，
   * 等于用一个弱信号覆盖强信号。这里只把结果摆出来给人看。
   */
  const runProbe = async () => {
    setProbing(true);
    try {
      const result = await healthCheckApi.checkAll();
      setProbe(result);
      const bad = result.filter((r) => !r.is_reachable).length;
      if (bad === 0) {
        toast.success(`${result.length} 个平台全部可达`);
      } else {
        toast.warning(`${bad} / ${result.length} 个平台不可达`);
      }
    } catch (error) {
      toast.error(`探测失败：${String(error)}`);
    } finally {
      setProbing(false);
    }
  };

  const degraded = rows.filter((r) => r.state !== "Closed");
  const healthy = rows.length - degraded.length;
  // 和熔断器同一个取向：只列不可达的。全通就一行汇总。
  const unreachable = probe?.filter((r) => !r.is_reachable) ?? [];

  return (
    <div className={cn("rounded-lg border border-border glass-surface p-4", className)}>
      <div className="mb-3 flex items-center justify-between">
        <div className="flex items-center gap-2 text-sm font-semibold">
          <Activity className="h-4 w-4 text-primary" /> 网关健康
        </div>
        <div className="flex items-center gap-3">
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
          <button
            type="button"
            disabled={probing}
            onClick={() => void runProbe()}
            title="主动给每个启用的平台发一次探针，报告可达性与延迟。熔断器只看真实流量，没跑过请求的平台它不知道状态。"
            className="flex shrink-0 items-center gap-1 rounded border border-border px-2 py-1 text-xs text-muted-foreground hover:bg-muted/30 hover:text-foreground disabled:opacity-50"
          >
            <Stethoscope className={cn("h-3 w-3", probing && "animate-pulse")} />
            {probing ? "探测中…" : "全部检测"}
          </button>
        </div>
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

      {probe !== null && (
        <div className="mt-3 border-t border-border/60 pt-3">
          {unreachable.length === 0 ? (
            <p className="text-xs text-muted-foreground">
              上次探测：{probe.length} 个平台全部可达
              {probe.length > 0 &&
                `，最慢 ${Math.max(...probe.map((r) => r.latency_ms))}ms`}
            </p>
          ) : (
            <div className="space-y-2">
              <p className="text-xs text-muted-foreground">
                上次探测：{unreachable.length} / {probe.length} 个不可达
              </p>
              {unreachable.map((r) => (
                <div
                  key={r.platform_id}
                  className="flex items-center gap-3 rounded-md border border-destructive/40 bg-destructive/5 px-3 py-2"
                >
                  <span className="h-2 w-2 shrink-0 rounded-full bg-destructive" />
                  <div className="min-w-0 flex-1">
                    <span className="truncate text-sm font-medium">{r.platform_name}</span>
                    {r.error && (
                      <div className="mt-0.5 truncate text-xs text-muted-foreground" title={r.error}>
                        {r.error}
                      </div>
                    )}
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>
      )}
    </div>
  );
}

/**
 * UsageDashboardTab — 用量成本看板.
 *
 * A first-class destination for gateway operations: circuit health, the
 * existing token/cost activity panel, a per-platform cost breakdown, and a
 * recent-request stream. All read-only over the already-collected
 * `request_logs` + circuit state — no new telemetry.
 */
import { useCallback, useEffect, useState } from "react";
import { BarChart3, RefreshCw, Server } from "lucide-react";

import { GatewayHealthCard } from "@/components/GatewayHealthCard";
import { TokenActivityPanel } from "@/components/TokenActivityPanel";
import { cn } from "@/lib/utils";
import { requestLogApi, type PlatformUsage, type RequestLogEntry } from "@/lib/tauri-api";

function fmtTokens(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(2)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return `${n}`;
}
function fmtCost(n: number): string {
  if (n === 0) return "$0";
  if (n < 0.01) return "<$0.01";
  return `$${n.toFixed(2)}`;
}
function fmtTime(iso: string): string {
  try {
    return new Date(iso.endsWith("Z") ? iso : `${iso}Z`).toLocaleString("zh-CN", {
      month: "2-digit", day: "2-digit", hour: "2-digit", minute: "2-digit", second: "2-digit",
    });
  } catch {
    return iso;
  }
}

export function UsageDashboardTab() {
  const [platforms, setPlatforms] = useState<PlatformUsage[]>([]);
  const [logs, setLogs] = useState<RequestLogEntry[]>([]);
  const [loading, setLoading] = useState(false);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const [p, l] = await Promise.all([
        requestLogApi.platformUsage(),
        requestLogApi.getLogs(1, 40),
      ]);
      setPlatforms(p);
      setLogs(l);
    } catch {
      /* transient */
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  const maxCost = Math.max(...platforms.map((p) => p.cost_usd), 0.0001);

  return (
    <div className="flex h-full flex-1 flex-col overflow-hidden bg-background">
      <div className="flex items-center justify-between border-b border-border px-6 py-4">
        <div>
          <div className="flex items-center gap-2 text-lg font-semibold">
            <BarChart3 className="h-5 w-5 text-primary" /> 用量成本看板
          </div>
          <p className="mt-1 text-sm text-muted-foreground">
            网关健康、token 与成本活动、按平台开销、最近请求 —— 全部基于已采集的 request_logs（费用为按模型定价的估算）。
          </p>
        </div>
        <button
          onClick={() => void load()}
          className="flex items-center gap-1.5 rounded-md border border-border px-3 py-1.5 text-sm text-muted-foreground hover:bg-muted/20 hover:text-foreground"
        >
          <RefreshCw className={cn("h-4 w-4", loading && "animate-spin")} /> 刷新
        </button>
      </div>

      <div className="flex flex-col gap-5 overflow-y-auto p-6">
        <GatewayHealthCard />

        <TokenActivityPanel />

        {/* Per-platform cost breakdown */}
        <div className="rounded-lg border border-border bg-card/40 p-4">
          <div className="mb-3 flex items-center gap-2 text-sm font-semibold">
            <Server className="h-4 w-4 text-primary" /> 按平台开销
          </div>
          {platforms.length === 0 ? (
            <p className="text-xs text-muted-foreground">暂无请求记录。挂上 OMNIX 网关跑几次请求后这里会出现分平台的 token 与成本。</p>
          ) : (
            <div className="space-y-2">
              {platforms.map((p) => (
                <div key={p.platform} className="flex items-center gap-3">
                  <div className="w-40 shrink-0 truncate text-sm" title={p.platform}>{p.platform}</div>
                  <div className="relative h-6 flex-1 overflow-hidden rounded bg-muted/20">
                    <div
                      className="h-full rounded bg-primary/25"
                      style={{ width: `${Math.max((p.cost_usd / maxCost) * 100, 2)}%` }}
                    />
                  </div>
                  <div className="w-20 shrink-0 text-right text-sm font-medium text-success">{fmtCost(p.cost_usd)}</div>
                  <div className="w-16 shrink-0 text-right text-xs text-muted-foreground">{fmtTokens(p.total_tokens)}</div>
                  <div className="w-24 shrink-0 text-right text-xs text-muted-foreground">
                    {p.request_count} 次{p.error_count > 0 && <span className="text-destructive"> · {p.error_count} 错</span>}
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>

        {/* Recent request stream */}
        <div className="rounded-lg border border-border bg-card/40 p-4">
          <div className="mb-3 text-sm font-semibold">最近请求</div>
          {logs.length === 0 ? (
            <p className="text-xs text-muted-foreground">还没有请求记录。</p>
          ) : (
            <div className="overflow-x-auto">
              <table className="w-full text-xs">
                <thead className="text-muted-foreground">
                  <tr className="border-b border-border/60 text-left">
                    <th className="py-1.5 pr-3 font-medium">时间</th>
                    <th className="py-1.5 pr-3 font-medium">模型</th>
                    <th className="py-1.5 pr-3 font-medium">平台</th>
                    <th className="py-1.5 pr-3 text-right font-medium">tokens</th>
                    <th className="py-1.5 pr-3 text-right font-medium">延迟</th>
                    <th className="py-1.5 font-medium">状态</th>
                  </tr>
                </thead>
                <tbody>
                  {logs.map((log) => (
                    <tr key={log.id} className="border-b border-border/30">
                      <td className="py-1.5 pr-3 whitespace-nowrap text-muted-foreground">{fmtTime(log.timestamp)}</td>
                      <td className="py-1.5 pr-3 max-w-[160px] truncate" title={log.model}>{log.model}</td>
                      <td className="py-1.5 pr-3 text-muted-foreground">{log.platform || "—"}</td>
                      <td className="py-1.5 pr-3 text-right">{fmtTokens(log.total_tokens)}</td>
                      <td className="py-1.5 pr-3 text-right text-muted-foreground">{log.latency_ms}ms</td>
                      <td className="py-1.5">
                        <span className={cn(
                          "rounded px-1.5 py-0.5 font-medium",
                          log.is_error ? "bg-destructive/15 text-destructive" : "bg-success/15 text-success",
                        )}>
                          {log.status_code || (log.is_error ? "ERR" : "OK")}
                        </span>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

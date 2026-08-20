/**
 * UsageDashboardTab — 用量成本看板.
 *
 * 只回答一个问题：花了多少。token/成本活动、按平台开销、最近请求都读已采集的
 * `request_logs`；「Auto 路由」那一格读 `router_decisions`——那是唯一一处新增的
 * 采集，每次 Auto 选型一行，表里没有任何自由文本（建表处有隐私契约说明）。
 *
 * 网关健康（各平台熔断状态）不在这里——它的唯一去处是「模型」页，那里它就
 * 贴在平台列表上方，看到谁熔断了往下一格就能改 key / 停用。
 */
import { useCallback, useEffect, useState } from "react";
import { BarChart3, RefreshCw, Route, Server } from "lucide-react";

import { TokenActivityPanel } from "@/components/TokenActivityPanel";
import { cn } from "@/lib/utils";
import {
  requestLogApi,
  routerDecisionApi,
  type PlatformUsage,
  type RequestLogEntry,
  type RouterDecisionReport,
} from "@/lib/tauri-api";

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
/** 费率：$/百万 token（输入+输出合计）。和 fmtCost 不是一个口径，别混用。 */
function fmtRate(n: number): string {
  if (n === 0) return "—";
  return `$${n.toFixed(2)}/M`;
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
  const [router, setRouter] = useState<RouterDecisionReport | null>(null);
  const [loading, setLoading] = useState(false);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const [p, l, r] = await Promise.all([
        requestLogApi.platformUsage(),
        requestLogApi.getLogs(1, 40),
        routerDecisionApi.get(),
      ]);
      setPlatforms(p);
      setLogs(l);
      setRouter(r);
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
            token 与成本活动、按平台开销、最近请求 —— 全部基于已采集的 request_logs（费用为按模型定价的估算）。网关健康在「模型」页。
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
        <TokenActivityPanel />

        {/* Auto 路由：选了谁、比不比价前便宜、防降档拦了几次 */}
        <div className="rounded-lg border border-border glass-surface p-4">
          <div className="mb-1 flex items-center gap-2 text-sm font-semibold">
            <Route className="h-4 w-4 text-primary" /> Auto 路由
          </div>
          <p className="mb-3 text-xs text-muted-foreground">
            基线 = 不比价、不粘性时会选的那个模型。费率是 $/百万 token（输入+输出合计），
            不是实际花销——选型发生在请求之前，那时还不知道会用掉多少 token。
          </p>
          {!router || router.total === 0 ? (
            <p className="text-xs text-muted-foreground">
              还没有 Auto 选型记录。把「设置 → 内置功能默认模型」设成 Auto 之后，这里会记下每一次选型的依据。
            </p>
          ) : (
            <>
              <div className="mb-3 grid grid-cols-3 gap-3">
                <div className="rounded border border-border/60 p-2">
                  <div className="text-xs text-muted-foreground">选型次数</div>
                  <div className="text-lg font-semibold">{router.total}</div>
                </div>
                <div className="rounded border border-border/60 p-2">
                  <div className="text-xs text-muted-foreground">比基线便宜</div>
                  <div className="text-lg font-semibold text-success">
                    {router.cheaper_than_baseline}
                    {router.avg_rate_cut > 0 && (
                      <span className="ml-1 text-xs font-normal text-muted-foreground">
                        均降 {(router.avg_rate_cut * 100).toFixed(0)}%
                      </span>
                    )}
                  </div>
                </div>
                <div className="rounded border border-border/60 p-2">
                  <div className="text-xs text-muted-foreground">防降档拦下</div>
                  <div className="text-lg font-semibold">{router.anti_downgrade_count}</div>
                </div>
              </div>
              <div className="overflow-x-auto">
                <table className="w-full text-xs">
                  <thead className="text-muted-foreground">
                    <tr className="border-b border-border/60 text-left">
                      <th className="py-1.5 pr-3 font-medium">时间</th>
                      <th className="py-1.5 pr-3 font-medium">这轮需要</th>
                      <th className="py-1.5 pr-3 font-medium">选中</th>
                      <th className="py-1.5 pr-3 text-right font-medium">费率</th>
                      <th className="py-1.5 pr-3 font-medium">基线</th>
                      <th className="py-1.5 font-medium">备注</th>
                    </tr>
                  </thead>
                  <tbody>
                    {router.recent.map((d) => (
                      <tr key={d.id} className="border-b border-border/30">
                        <td className="py-1.5 pr-3 whitespace-nowrap text-muted-foreground">{fmtTime(d.created_at)}</td>
                        <td className="py-1.5 pr-3 text-muted-foreground">{d.needs || "—"}</td>
                        <td className="py-1.5 pr-3 max-w-[180px] truncate" title={d.chosen_model}>{d.chosen_model}</td>
                        <td className="py-1.5 pr-3 text-right whitespace-nowrap">{fmtRate(d.chosen_price)}</td>
                        <td
                          className={cn(
                            "py-1.5 pr-3 max-w-[180px] truncate",
                            d.chosen_model === d.baseline_model ? "text-muted-foreground" : "",
                          )}
                          title={d.baseline_model}
                        >
                          {d.chosen_model === d.baseline_model ? "同上" : d.baseline_model}
                        </td>
                        <td className="py-1.5">
                          {d.anti_downgrade && (
                            <span className="rounded bg-primary/15 px-1.5 py-0.5 font-medium text-primary">防降档</span>
                          )}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </>
          )}
        </div>

        {/* Per-platform cost breakdown */}
        <div className="rounded-lg border border-border glass-surface p-4">
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
        <div className="rounded-lg border border-border glass-surface p-4">
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

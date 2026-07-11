import { useCallback, useEffect, useState } from "react";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Activity, RefreshCw, Coins, Hash, Timer } from "lucide-react";

import { requestLogApi, type UsageStats, type DailyUsage } from "@/lib/tauri-api";

/**
 * TokenActivityPanel — token & cost activity. Surfaces the already-collected `request_logs` usage with estimated
 * cost and a daily activity chart. Cost is estimated from a per-model pricing
 * table (unknown models fall back to a default rate), so it is an approximation.
 */
function fmtTokens(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(2)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return `${n}`;
}
function fmtCost(n: number): string {
  if (n === 0) return "$0";
  if (n < 0.01) return `<$0.01`;
  return `$${n.toFixed(2)}`;
}

type Metric = "tokens" | "cost";

export function TokenActivityPanel() {
  const [stats, setStats] = useState<UsageStats | null>(null);
  const [series, setSeries] = useState<DailyUsage[]>([]);
  const [metric, setMetric] = useState<Metric>("tokens");
  const [days, setDays] = useState(14);
  const [loading, setLoading] = useState(false);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const [s, t] = await Promise.all([requestLogApi.getStats(), requestLogApi.timeseries(days)]);
      setStats(s);
      setSeries(t);
    } catch {
      /* logs may be empty before the gateway has handled any request */
    } finally {
      setLoading(false);
    }
  }, [days]);

  useEffect(() => {
    void load();
  }, [load]);

  // Build a gap-filled day axis so the chart shows empty days too.
  const today = new Date();
  const axis: DailyUsage[] = [];
  const byDate = new Map(series.map((d) => [d.date, d]));
  for (let i = days - 1; i >= 0; i--) {
    const d = new Date(today);
    d.setDate(today.getDate() - i);
    const key = d.toISOString().slice(0, 10);
    axis.push(byDate.get(key) ?? { date: key, requests: 0, tokens: 0, cost_usd: 0 });
  }
  const value = (d: DailyUsage) => (metric === "tokens" ? d.tokens : d.cost_usd);
  const maxVal = Math.max(1, ...axis.map(value));
  const topModels = stats?.top_models.slice(0, 6) ?? [];
  const maxModelTokens = Math.max(1, ...topModels.map((m) => m.total_tokens));

  return (
    <Card>
      <CardHeader className="mb-2 flex-row items-center justify-between">
        <CardTitle className="flex items-center gap-2 text-sm">
          <Activity className="h-4 w-4" /> Token 活动消耗与费用
        </CardTitle>
        <div className="flex items-center gap-1">
          {([7, 14, 30] as const).map((d) => (
            <button
              key={d}
              onClick={() => setDays(d)}
              className={`rounded px-1.5 py-0.5 text-[11px] ${days === d ? "bg-primary/15 text-primary" : "text-muted-foreground hover:bg-muted/30"}`}
            >
              {d}天
            </button>
          ))}
          <button onClick={() => void load()} title="刷新" className="ml-1 rounded p-1 text-muted-foreground hover:bg-muted/30 hover:text-foreground">
            <RefreshCw className={`h-3.5 w-3.5 ${loading ? "animate-spin" : ""}`} />
          </button>
        </div>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        {/* Metric cards */}
        <div className="grid grid-cols-2 gap-3 sm:grid-cols-4">
          <Metric2 icon={<Hash className="h-3.5 w-3.5" />} label="今日 tokens" value={fmtTokens(stats?.tokens_today ?? 0)} sub={`累计 ${fmtTokens(stats?.total_tokens ?? 0)}`} />
          <Metric2 icon={<Coins className="h-3.5 w-3.5" />} label="今日费用" value={fmtCost(stats?.cost_today_usd ?? 0)} sub={`累计 ${fmtCost(stats?.total_cost_usd ?? 0)}`} accent="text-success" />
          <Metric2 icon={<Activity className="h-3.5 w-3.5" />} label="今日请求" value={`${stats?.requests_today ?? 0}`} sub={`累计 ${stats?.total_requests ?? 0}`} />
          <Metric2 icon={<Timer className="h-3.5 w-3.5" />} label="平均延迟" value={`${Math.round(stats?.avg_latency_ms ?? 0)}ms`} sub={(stats?.total_errors ?? 0) > 0 ? `错误 ${stats?.total_errors}` : "无错误"} accent={(stats?.total_errors ?? 0) > 0 ? "text-destructive" : undefined} />
        </div>

        {/* Daily activity chart */}
        <div>
          <div className="mb-1.5 flex items-center justify-between">
            <span className="text-xs font-semibold text-muted-foreground">每日活动（近 {days} 天）</span>
            <div className="flex items-center gap-1">
              <button onClick={() => setMetric("tokens")} className={`rounded px-1.5 py-0.5 text-[11px] ${metric === "tokens" ? "bg-primary/15 text-primary" : "text-muted-foreground hover:bg-muted/30"}`}>Tokens</button>
              <button onClick={() => setMetric("cost")} className={`rounded px-1.5 py-0.5 text-[11px] ${metric === "cost" ? "bg-primary/15 text-primary" : "text-muted-foreground hover:bg-muted/30"}`}>费用</button>
            </div>
          </div>
          <div className="flex h-28 items-end gap-0.5">
            {axis.map((d) => {
              const v = value(d);
              const h = Math.max(2, (v / maxVal) * 100);
              return (
                <div
                  key={d.date}
                  className="group relative flex-1 rounded-t bg-gradient-to-t from-cyan-500/70 to-blue-500/70 hover:from-cyan-400 hover:to-blue-400"
                  style={{ height: `${h}%` }}
                  title={`${d.date}\n${fmtTokens(d.tokens)} tokens · ${fmtCost(d.cost_usd)} · ${d.requests} 次`}
                />
              );
            })}
          </div>
          <div className="mt-1 flex justify-between text-[10px] text-muted-foreground">
            <span>{axis[0]?.date.slice(5)}</span>
            <span>{axis[axis.length - 1]?.date.slice(5)}</span>
          </div>
        </div>

        {/* Top models with cost */}
        {topModels.length > 0 && (
          <div>
            <div className="mb-1.5 text-xs font-semibold text-muted-foreground">按模型消耗</div>
            <div className="flex flex-col gap-2">
              {topModels.map((m) => (
                <div key={m.model} className="flex items-center gap-3">
                  <div className="min-w-0 flex-1">
                    <div className="mb-1 flex items-center justify-between gap-2">
                      <span className="truncate text-xs font-medium" title={m.model}>{m.model}</span>
                      <span className="shrink-0 text-[11px] text-muted-foreground">
                        {m.request_count} 次 · {fmtTokens(m.total_tokens)} · <span className="text-success">{fmtCost(m.cost_usd)}</span>
                      </span>
                    </div>
                    <div className="h-1.5 overflow-hidden rounded-full bg-muted/20">
                      <div className="h-full rounded-full bg-gradient-to-r from-cyan-500 to-blue-500" style={{ width: `${(m.total_tokens / maxModelTokens) * 100}%` }} />
                    </div>
                  </div>
                </div>
              ))}
            </div>
          </div>
        )}

        <p className="text-[10px] text-muted-foreground">
          费用为估算值（按模型定价表，未知模型用默认费率），仅供参考。统计来自 OMNIX 网关经手的请求；直接走 CLI 的 Agent 会话由其自身计费。
        </p>
      </CardContent>
    </Card>
  );
}

function Metric2({ icon, label, value, sub, accent }: { icon: React.ReactNode; label: string; value: string; sub?: string; accent?: string }) {
  return (
    <div className="rounded-lg border border-border bg-muted/10 px-3 py-2">
      <div className="mb-1 flex items-center gap-1.5 text-[11px] text-muted-foreground">{icon} {label}</div>
      <div className={`text-lg font-semibold ${accent ?? ""}`}>{value}</div>
      {sub && <div className="text-[10px] text-muted-foreground">{sub}</div>}
    </div>
  );
}

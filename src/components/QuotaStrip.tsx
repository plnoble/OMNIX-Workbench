/**
 * QuotaStrip — 订阅额度条（监督台顶部）。
 *
 * 数据全部读本机 CLI 日志、零联网（后端 quota.rs）：
 * - Codex：官方回传的 rate_limits，used_percent 是精确值，直接画进度条。
 * - Claude Code：Anthropic 不在本地回传限额百分比，所以只显示「本 5 小时块
 *   / 本周的消耗与重置时间」，绝不编造百分比。
 */
import { useCallback, useEffect, useState } from "react";
import { Gauge, RefreshCw } from "lucide-react";

import { cn } from "@/lib/utils";
import { quotaApi, type ClaudeQuota, type CodexQuota } from "@/lib/tauri-api";

function fmtTokens(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return `${n}`;
}

function fmtReset(iso: string | null): string {
  if (!iso) return "";
  const d = new Date(iso);
  const mins = Math.round((d.getTime() - Date.now()) / 60000);
  if (mins <= 0) return "即将重置";
  if (mins < 60) return `${mins} 分钟后重置`;
  const hours = Math.floor(mins / 60);
  if (hours < 24) return `${hours} 小时 ${mins % 60} 分后重置`;
  return `${Math.floor(hours / 24)} 天后重置`;
}

function fmtWindow(minutes: number): string {
  if (minutes >= 10080) return "周额度";
  if (minutes >= 1440) return `${Math.round(minutes / 1440)} 天窗口`;
  if (minutes >= 60) return `${Math.round(minutes / 60)} 小时窗口`;
  return `${minutes} 分钟窗口`;
}

function Bar({ pct }: { pct: number }) {
  const clamped = Math.max(0, Math.min(100, pct));
  const tone = clamped >= 90 ? "bg-destructive" : clamped >= 70 ? "bg-warning" : "bg-success";
  return (
    <div className="h-1.5 w-full overflow-hidden rounded-full bg-muted/30">
      <div className={cn("h-full rounded-full transition-[width] duration-500", tone)} style={{ width: `${clamped}%` }} />
    </div>
  );
}

export function QuotaStrip() {
  const [claude, setClaude] = useState<ClaudeQuota | null>(null);
  const [codex, setCodex] = useState<CodexQuota | null>(null);
  const [loaded, setLoaded] = useState(false);

  const load = useCallback(async () => {
    try {
      const o = await quotaApi.overview();
      setClaude(o.claude);
      setCodex(o.codex);
    } catch {
      /* transient */
    } finally {
      setLoaded(true);
    }
  }, []);

  useEffect(() => {
    void load();
    const t = setInterval(() => void load(), 60000);
    return () => clearInterval(t);
  }, [load]);

  if (loaded && !claude && !codex) return null;

  return (
    <section className="rounded-xl border border-border glass-surface p-3">
      <div className="mb-2 flex items-center gap-1.5 text-sm font-semibold">
        <Gauge className="h-4 w-4 text-primary" />
        订阅额度
        <span className="text-xs font-normal text-muted-foreground">读本机 CLI 用量日志 · 零联网 · 每分钟刷新</span>
        <button className="ml-auto text-muted-foreground hover:text-foreground" onClick={() => void load()} title="立即刷新">
          <RefreshCw className="h-3.5 w-3.5" />
        </button>
      </div>
      <div className="grid grid-cols-1 gap-3 md:grid-cols-2">
        {/* Codex — official percentage windows */}
        {codex && (codex.primary || codex.secondary) && (
          <div className="rounded-lg border border-border bg-background/40 p-3">
            <div className="mb-2 flex items-center gap-2 text-xs font-medium">
              Codex
              <span className="rounded border border-border px-1.5 py-0.5 text-[10px] text-muted-foreground">{codex.plan_type}</span>
              <span className="ml-auto text-[10px] text-muted-foreground">官方口径</span>
            </div>
            {[codex.primary, codex.secondary].filter(Boolean).map((w, i) => (
              <div key={i} className="mb-2 last:mb-0">
                <div className="mb-1 flex items-baseline justify-between text-xs">
                  <span className="text-muted-foreground">{fmtWindow(w!.window_minutes)}</span>
                  <span className={cn("font-medium", w!.used_percent >= 90 && "text-destructive")}>
                    {w!.used_percent.toFixed(0)}%
                    <span className="ml-1.5 text-[10px] font-normal text-muted-foreground">{fmtReset(w!.resets_at)}</span>
                  </span>
                </div>
                <Bar pct={w!.used_percent} />
              </div>
            ))}
          </div>
        )}

        {/* Claude Code — consumption + reset (no local percentage to show) */}
        {claude && (
          <div className="rounded-lg border border-border bg-background/40 p-3">
            <div className="mb-2 flex items-center gap-2 text-xs font-medium">
              Claude Code
              <span className="ml-auto text-[10px] text-muted-foreground">
                {claude.window_resets_at ? fmtReset(claude.window_resets_at) : "本 5 小时块无活动"}
              </span>
            </div>
            <div className="grid grid-cols-2 gap-2 text-xs">
              <div>
                <div className="text-[10px] text-muted-foreground">本 5 小时块</div>
                <div className="font-medium">
                  {fmtTokens(claude.window.input + claude.window.output)} tok · {claude.window.requests} 次
                </div>
              </div>
              <div>
                <div className="text-[10px] text-muted-foreground">近 7 天</div>
                <div className="font-medium">
                  {fmtTokens(claude.week.input + claude.week.output)} tok · {claude.week.requests} 次
                </div>
              </div>
            </div>
            {claude.window_models.length > 0 && (
              <div className="mt-1.5 truncate text-[10px] text-muted-foreground">
                本块模型：{claude.window_models.map(([m, out]) => `${m}(${fmtTokens(out)})`).join("、")}
              </div>
            )}
          </div>
        )}
      </div>
      {/* 诚实说明：为什么只有 Codex/Claude——不为 Grok/Gemini 编造假额度 */}
      <p className="mt-2 text-[10px] leading-4 text-muted-foreground/70">
        仅 Codex 在本地留有官方限额百分比；Claude Code 只留消耗（Anthropic 不回传本地百分比，故不显示）。
        Grok CLI 不记录限额、Gemini 走 OAuth 服务端配额，本地读不到，故未列出。
      </p>
    </section>
  );
}

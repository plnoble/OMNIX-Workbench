/**
 * ProfileTab — personal AI-coding activity profile.
 *
 * Borrowed from Synara's Profile/stats feature (MIT): a GitHub-style
 * contribution heatmap, streak stats, per-agent breakdown, and a shareable
 * card. Adapted to OMNIX design tokens; the card is drawn on a <canvas> so
 * export needs no extra dependency. All data comes from get_profile_stats.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Loader2, Flame, MessageSquare, Bot, Coins, Download, RefreshCw } from "lucide-react";
import { toast } from "sonner";

import { profileApi } from "@/lib/tauri-api";
import type { ProfileStats } from "@/types";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

const HANDLE_KEY = "omnix_profile_handle";
const WEEKDAY_LABELS = ["日", "一", "二", "三", "四", "五", "六"];

/** Buckets a daily count into a 0–4 intensity level (fixed thresholds). */
function intensityLevel(count: number): number {
  if (count <= 0) return 0;
  if (count <= 2) return 1;
  if (count <= 5) return 2;
  if (count <= 9) return 3;
  return 4;
}

const LEVEL_CLASSES = [
  "bg-muted/60",
  "bg-[color-mix(in_srgb,var(--color-info)_24%,transparent)]",
  "bg-[color-mix(in_srgb,var(--color-info)_46%,transparent)]",
  "bg-[color-mix(in_srgb,var(--color-info)_72%,transparent)]",
  "bg-[var(--color-info)]",
];

function formatCompact(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return `${n}`;
}

interface HeatCell {
  day: string;
  count: number;
  level: number;
}

/** Splits the flat daily series into GitHub-style week columns (Sun→Sat). */
function toWeeks(daily: ProfileStats["daily"]): HeatCell[][] {
  const cells: (HeatCell | null)[] = daily.map((d) => ({
    day: d.day,
    count: d.count,
    level: intensityLevel(d.count),
  }));
  // Pad the front so the first column starts on Sunday.
  const first = daily[0] ? new Date(`${daily[0].day}T00:00:00`).getDay() : 0;
  const padded: (HeatCell | null)[] = [...Array(first).fill(null), ...cells];
  const weeks: HeatCell[][] = [];
  for (let i = 0; i < padded.length; i += 7) {
    weeks.push(padded.slice(i, i + 7).map((c) => c ?? { day: "", count: -1, level: 0 }));
  }
  return weeks;
}

export function ProfileTab() {
  const [stats, setStats] = useState<ProfileStats | null>(null);
  const [loading, setLoading] = useState(true);
  const [handle, setHandle] = useState<string>(() => localStorage.getItem(HANDLE_KEY) || "");
  const cardRef = useRef<HTMLDivElement>(null);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      setStats(await profileApi.getStats());
    } catch (error) {
      toast.error(`读取画像失败：${error}`);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  const saveHandle = (value: string) => {
    const clean = value.replace(/^@+/, "").trim().slice(0, 24);
    setHandle(clean);
    localStorage.setItem(HANDLE_KEY, clean);
  };

  const weeks = useMemo(() => (stats ? toWeeks(stats.daily) : []), [stats]);
  const maxAgent = useMemo(
    () => Math.max(1, ...(stats?.per_agent.map((a) => a.count) ?? [1])),
    [stats]
  );
  const displayHandle = handle || "coder";

  const exportCard = useCallback(() => {
    if (!stats) return;
    try {
      drawShareCard(stats, displayHandle);
      toast.success("战绩卡已导出为 PNG");
    } catch (error) {
      toast.error(`导出失败：${error}`);
    }
  }, [stats, displayHandle]);

  if (loading && !stats) {
    return (
      <div className="flex h-full items-center justify-center text-muted-foreground">
        <Loader2 className="mr-2 h-5 w-5 animate-spin" /> 正在统计你的编程画像…
      </div>
    );
  }
  if (!stats) return null;

  const statCards = [
    { icon: MessageSquare, label: "总提示", value: formatCompact(stats.total_prompts) },
    { icon: Bot, label: "Agent 会话", value: formatCompact(stats.total_sessions) },
    { icon: Coins, label: "累计 tokens", value: formatCompact(stats.total_tokens) },
    { icon: Flame, label: "当前连续", value: `${stats.current_streak} 天` },
  ];

  return (
    <div className="h-full overflow-y-auto p-6">
      <div className="mx-auto max-w-3xl space-y-6">
        {/* Header */}
        <div ref={cardRef} className="flex items-center gap-4 rounded-lg border border-border bg-card/40 p-5">
          <div className="flex h-14 w-14 shrink-0 items-center justify-center rounded-full accent-gradient text-xl font-bold text-primary-foreground">
            {(displayHandle[0] || "O").toUpperCase()}
          </div>
          <div className="min-w-0 flex-1">
            <div className="flex items-center gap-1 text-lg font-semibold">
              <span className="text-muted-foreground">@</span>
              <input
                value={handle}
                onChange={(e) => saveHandle(e.target.value)}
                placeholder="coder"
                className="w-40 bg-transparent outline-none placeholder:text-muted-foreground/50"
                aria-label="你的昵称"
              />
            </div>
            <div className="mt-0.5 text-xs text-muted-foreground">
              OMNIX 编程画像 · {stats.active_days} 活跃天 · 最长连续 {stats.longest_streak} 天
              {stats.first_active ? ` · 始于 ${stats.first_active}` : ""}
            </div>
          </div>
          <div className="flex gap-2">
            <Button size="sm" variant="outline" onClick={() => void load()} title="刷新">
              <RefreshCw className="h-3.5 w-3.5" />
            </Button>
            <Button size="sm" onClick={exportCard}>
              <Download className="h-3.5 w-3.5" /> 导出战绩卡
            </Button>
          </div>
        </div>

        {/* Stat cards */}
        <div className="grid grid-cols-2 gap-3 sm:grid-cols-4">
          {statCards.map((card) => (
            <div key={card.label} className="rounded-lg border border-border bg-card/40 p-4">
              <card.icon className="h-4 w-4 text-primary" />
              <div className="mt-2 text-2xl font-bold">{card.value}</div>
              <div className="text-xs text-muted-foreground">{card.label}</div>
            </div>
          ))}
        </div>

        {/* Activity heatmap */}
        <div className="rounded-lg border border-border bg-card/40 p-5">
          <div className="mb-3 flex items-center justify-between">
            <div className="text-sm font-medium">活动热力图 · 近 26 周</div>
            <div className="flex items-center gap-1 text-xs text-muted-foreground">
              少
              {LEVEL_CLASSES.map((c, i) => (
                <span key={i} className={cn("h-2.5 w-2.5 rounded-[2px]", c)} />
              ))}
              多
            </div>
          </div>
          <div className="flex gap-[3px] overflow-x-auto pb-1">
            <div className="mr-1 flex flex-col gap-[3px] pt-[2px] text-[9px] text-muted-foreground">
              {WEEKDAY_LABELS.map((d, i) => (
                <span key={d} className="h-2.5 leading-[10px]">{i % 2 === 1 ? d : ""}</span>
              ))}
            </div>
            {weeks.map((week, wi) => (
              <div key={wi} className="flex flex-col gap-[3px]">
                {week.map((cell, di) => (
                  <div
                    key={di}
                    className={cn(
                      "h-2.5 w-2.5 rounded-[2px]",
                      cell.count < 0 ? "bg-transparent" : LEVEL_CLASSES[cell.level]
                    )}
                    title={cell.count >= 0 ? `${cell.day}: ${cell.count} 次提示` : ""}
                  />
                ))}
              </div>
            ))}
          </div>
        </div>

        {/* Per-agent breakdown */}
        {stats.per_agent.length > 0 && (
          <div className="rounded-lg border border-border bg-card/40 p-5">
            <div className="mb-3 text-sm font-medium">各 Agent 会话占比</div>
            <div className="space-y-2">
              {stats.per_agent.map((agent) => (
                <div key={agent.agent} className="flex items-center gap-3">
                  <div className="w-32 shrink-0 truncate text-sm">{agent.agent}</div>
                  <div className="h-2 flex-1 overflow-hidden rounded-full bg-muted/40">
                    <div
                      className="h-full rounded-full bg-primary"
                      style={{ width: `${(agent.count / maxAgent) * 100}%` }}
                    />
                  </div>
                  <div className="w-10 shrink-0 text-right text-xs text-muted-foreground">
                    {agent.count}
                  </div>
                </div>
              ))}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

/** Draws the shareable stats card on an offscreen canvas and triggers a PNG download. */
function drawShareCard(stats: ProfileStats, handle: string) {
  const W = 1000;
  const H = 560;
  const canvas = document.createElement("canvas");
  canvas.width = W;
  canvas.height = H;
  const ctx = canvas.getContext("2d");
  if (!ctx) throw new Error("canvas 不可用");

  // Background gradient (OMNIX dark palette).
  const bg = ctx.createLinearGradient(0, 0, W, H);
  bg.addColorStop(0, "#0a0b10");
  bg.addColorStop(1, "#12141f");
  ctx.fillStyle = bg;
  ctx.fillRect(0, 0, W, H);

  // Title.
  ctx.fillStyle = "#00f2fe";
  ctx.font = "bold 42px system-ui, sans-serif";
  ctx.fillText("OMNIX 编程画像", 60, 90);
  ctx.fillStyle = "#94a3b8";
  ctx.font = "24px system-ui, sans-serif";
  ctx.fillText(`@${handle}`, 60, 130);

  // Stats grid.
  const items: Array<[string, string]> = [
    [formatCompact(stats.total_prompts), "总提示"],
    [formatCompact(stats.total_sessions), "Agent 会话"],
    [formatCompact(stats.total_tokens), "累计 tokens"],
    [`${stats.current_streak}`, "当前连续(天)"],
    [`${stats.longest_streak}`, "最长连续(天)"],
    [`${stats.active_days}`, "活跃天数"],
  ];
  items.forEach(([value, label], i) => {
    const x = 60 + (i % 3) * 300;
    const y = 200 + Math.floor(i / 3) * 110;
    ctx.fillStyle = "#f8fafc";
    ctx.font = "bold 44px system-ui, sans-serif";
    ctx.fillText(value, x, y);
    ctx.fillStyle = "#94a3b8";
    ctx.font = "20px system-ui, sans-serif";
    ctx.fillText(label, x, y + 30);
  });

  // Mini heatmap (last ~18 weeks) along the bottom.
  const recent = stats.daily.slice(-18 * 7);
  const cell = 12;
  const gap = 3;
  const startX = 60;
  const startY = 450;
  const firstDay = recent[0] ? new Date(`${recent[0].day}T00:00:00`).getDay() : 0;
  const ramp = ["#1c2030", "#0e7490", "#0891b2", "#22d3ee", "#00f2fe"];
  recent.forEach((d, idx) => {
    const pos = idx + firstDay;
    const col = Math.floor(pos / 7);
    const rowIdx = pos % 7;
    ctx.fillStyle = ramp[intensityLevel(d.count)];
    ctx.fillRect(startX + col * (cell + gap), startY + rowIdx * (cell + gap), cell, cell);
  });

  const url = canvas.toDataURL("image/png");
  const a = document.createElement("a");
  a.href = url;
  a.download = `omnix-profile-${handle}.png`;
  a.click();
}

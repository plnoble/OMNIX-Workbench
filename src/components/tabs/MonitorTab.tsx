/**
 * MonitorTab — 监控中心（合并项）。
 *
 * 把三个本质都是「监控/统计」的页面收进一个入口，用一条极简模式栏切换：
 * - 总控：所有在跑的 agent 会话、审批、订阅额度（实时）
 * - 用量：网关健康、token / 成本活动（已采集的 request_logs）
 * - 画像：AI 编程活动热力图、连续天数、战绩卡（历史）
 *
 * 启动器只暴露这一个入口；旧的 profile / usage id 别名到这里，不断链。
 */
import { useState } from "react";
import { Activity, BarChart3, LayoutDashboard, UserRound } from "lucide-react";

import { cn } from "@/lib/utils";
import { SupervisionConsole } from "@/components/tabs/SupervisionTab";
import { UsageDashboardTab } from "@/components/tabs/UsageDashboardTab";
import { ProfileTab } from "@/components/tabs/ProfileTab";

type MonitorMode = "console" | "usage" | "profile";

const MODES: { id: MonitorMode; label: string; icon: typeof Activity }[] = [
  { id: "console", label: "总控", icon: LayoutDashboard },
  { id: "usage", label: "用量", icon: BarChart3 },
  { id: "profile", label: "画像", icon: UserRound },
];

const LAST_MODE_KEY = "omnix_monitor_mode";

export function MonitorTab({ defaultMode }: { defaultMode?: MonitorMode } = {}) {
  const [mode, setMode] = useState<MonitorMode>(() => {
    if (defaultMode) return defaultMode;
    const saved = localStorage.getItem(LAST_MODE_KEY) as MonitorMode | null;
    return saved && MODES.some((m) => m.id === saved) ? saved : "console";
  });

  const switchMode = (next: MonitorMode) => {
    setMode(next);
    localStorage.setItem(LAST_MODE_KEY, next);
  };

  return (
    <div className="flex h-full flex-1 flex-col overflow-hidden bg-background">
      <div className="flex h-11 shrink-0 items-center gap-1 border-b border-border px-4">
        <Activity className="mr-1 h-4 w-4 text-primary" />
        <span className="mr-2 text-sm font-semibold">监控</span>
        {MODES.map(({ id, label, icon: Icon }) => (
          <button
            key={id}
            onClick={() => switchMode(id)}
            className={cn(
              "inline-flex h-8 items-center gap-1.5 rounded-md px-3 text-sm transition",
              mode === id
                ? "bg-accent text-accent-foreground"
                : "text-muted-foreground hover:bg-muted/40 hover:text-foreground",
            )}
          >
            <Icon className="h-4 w-4" /> {label}
          </button>
        ))}
      </div>
      <div className="min-h-0 flex-1">
        {mode === "console" && <SupervisionConsole />}
        {mode === "usage" && <UsageDashboardTab />}
        {mode === "profile" && <ProfileTab />}
      </div>
    </div>
  );
}

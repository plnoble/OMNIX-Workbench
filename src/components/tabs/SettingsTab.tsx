/**
 * SettingsTab — 应用设置（壳）。子页拆分在 ./settings/ 下：
 * Platform / System / Mcp / Backup 各自独立文件，props 类型在 settings/types。
 * 旧公共导出（PlatformSubTab/McpSubTab/两个 Props 类型）在此保留 re-export。
 */
import { Database, Plug, Server, Settings, Wrench } from "lucide-react";
import { cn } from "@/lib/utils";
import type { SettingsSubTab } from "@/types";
import type { SettingsTabProps } from "./settings/types";
import { PlatformSubTab } from "./settings/PlatformSubTab";
import { SystemSubTab } from "./settings/SystemSubTab";
import { McpSubTab } from "./settings/McpSubTab";
import { BackupSubTab } from "./settings/BackupSubTab";

export type { SettingsTabProps } from "./settings/types";
export { PlatformSubTab } from "./settings/PlatformSubTab";
export { McpSubTab } from "./settings/McpSubTab";

const SETTINGS_TABS: { id: SettingsSubTab; label: string; icon: React.ReactNode }[] = [
  { id: "platform", label: "大模型平台", icon: <Plug className="h-3.5 w-3.5" /> },
  { id: "system", label: "系统设置", icon: <Settings className="h-3.5 w-3.5" /> },
  { id: "diagnostics", label: "诊断", icon: <Wrench className="h-3.5 w-3.5" /> },
  { id: "mcp", label: "MCP 服务器", icon: <Server className="h-3.5 w-3.5" /> },
  { id: "backup", label: "数据备份", icon: <Database className="h-3.5 w-3.5" /> },
];


export function SettingsTab(props: SettingsTabProps) {
  return (
    <div className="flex flex-col h-full overflow-hidden flex-1">
      {/* Top horizontal Tab bar */}
      <div className="flex items-center gap-1 px-5 pt-4 pb-2 border-b border-border bg-[rgba(10,10,14,0.1)]">
        {/* platform → focused 模型中心; mcp → focused MCP page. Settings keeps only system + backup. */}
        {SETTINGS_TABS.filter((tab) => tab.id !== "platform" && tab.id !== "mcp").map((tab) => (
          <button
            key={tab.id}
            onClick={() => props.setSettingsSubTab(tab.id)}
            className={cn(
              "flex items-center gap-1.5 px-3 py-1.5 rounded-md text-sm font-medium transition-all cursor-pointer",
              props.settingsSubTab === tab.id
                ? "bg-accent/10 text-accent border border-accent/30"
                : "text-muted-foreground hover:text-foreground hover:bg-muted/20 border border-transparent"
            )}
          >
            {tab.icon}
            {tab.label}
          </button>
        ))}
      </div>

      {/* Content panel */}
      <div className="flex-1 overflow-y-auto p-5">
        {props.settingsSubTab === "platform" && <PlatformSubTab />}
        {props.settingsSubTab === "system" && <SystemSubTab />}
        {props.settingsSubTab === "diagnostics" && props.diagnosticsPanel}
        {props.settingsSubTab === "mcp" && <McpSubTab />}
        {props.settingsSubTab === "backup" && <BackupSubTab />}
      </div>
    </div>
  );
}

// ── Platform Sub-Tab ─────────────────────────────────


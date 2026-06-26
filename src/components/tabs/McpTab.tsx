import { Plug } from "lucide-react";

import { McpSubTab, type SettingsTabProps } from "@/components/tabs/SettingsTab";

/**
 * McpTab — a focused MCP page (mirrors ModelsTab). The title-bar "MCP" entry
 * used to render the full Settings tabbed view (System + Backup tabs visible),
 * which looked identical to opening Settings. This shows only the MCP surface.
 */
export function McpTab(props: SettingsTabProps) {
  return (
    <div className="flex h-full flex-1 flex-col overflow-hidden bg-background">
      <div className="border-b border-border px-6 py-4">
        <div className="flex items-center gap-2 text-lg font-semibold">
          <Plug className="h-5 w-5 text-primary" />
          MCP / 工具
        </div>
        <p className="mt-1 max-w-3xl text-sm leading-6 text-muted-foreground">
          管理 MCP 工具服务，并一键同步到 Claude Code 和 Codex 的原生配置。系统设置与数据备份在右上角「设置」里。
        </p>
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto p-5">
        <McpSubTab {...props} />
      </div>
    </div>
  );
}

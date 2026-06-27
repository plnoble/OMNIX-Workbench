/**
 * HooksTab — focused page for user-state hooks (event → action automation).
 * Wraps the self-contained HooksPanel in a scrollable full-page layout, the
 * same way McpTab gives MCP its own page instead of burying it.
 */
import { Webhook } from "lucide-react";

import { HooksPanel } from "@/components/HooksPanel";

export function HooksTab() {
  return (
    <div className="flex h-full w-full flex-col overflow-hidden">
      <div className="flex items-center gap-2 border-b border-border px-4 py-4">
        <Webhook className="h-4 w-4" />
        <span className="text-sm font-semibold">事件 Hooks · Agent 事件触发的自动化规则</span>
      </div>
      <div className="flex-1 overflow-y-auto p-4">
        <HooksPanel />
      </div>
    </div>
  );
}

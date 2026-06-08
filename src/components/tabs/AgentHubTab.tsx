/**
 * AgentHubTab — Agent 仓库 & 管理管理器
 *
 * Shows installed agents grid and account management
 */

import { Card, CardContent } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Bot, Plus, Edit, Trash2, ToggleLeft } from "lucide-react";
import { cn } from "@/lib/utils";
import type { DetectedAgent, AgentAccount, PlatformModel } from "@/types";

interface AgentHubTabProps {
  detectedAgents: DetectedAgent[];
  activeAgent: string;
  accounts: AgentAccount[];
  activeModels: PlatformModel[];
  onSwitchAgent: (name: string) => void;
  onAddAccount: () => void;
  onEditAccount: (acc: AgentAccount) => void;
  onDeleteAccount: (id: string) => void;
  onSwitchAccount: (id: string) => void;
}

export function AgentHubTab({
  detectedAgents,
  activeAgent,
  accounts,
  onSwitchAgent,
  onAddAccount,
  onEditAccount,
  onDeleteAccount,
  onSwitchAccount,
}: AgentHubTabProps) {
  return (
    <div className="p-6 overflow-y-auto flex flex-col gap-6">
      {/* Installed Agents Grid */}
      <div>
        <h3 className="text-sm font-semibold mb-3 flex items-center gap-2">
          <Bot className="h-4 w-4" /> 已检测的智能体 CLI
        </h3>
        <div className="grid grid-cols-2 gap-3">
          {detectedAgents.map((agent) => {
            const isActive = agent.name === activeAgent;
            const isInstalled = agent.status === "installed";

            return (
              <Card
                key={agent.name}
                className={cn(
                  "cursor-pointer transition-all",
                  isActive && "border-accent/40 glow-border",
                  !isInstalled && "opacity-70"
                )}
                onClick={() => isInstalled && onSwitchAgent(agent.name)}
              >
                <CardContent className="p-4">
                  <div className="flex items-center justify-between mb-2">
                    <span className="font-semibold text-sm">{agent.name}</span>
                    <Badge variant={isInstalled ? "success" : "secondary"}>
                      {isInstalled ? "已安装" : "未检测到"}
                    </Badge>
                  </div>
                  {isInstalled && (
                    <div className="text-xs text-muted-foreground space-y-0.5">
                      <div>版本: <code>{agent.version}</code></div>
                      <div className="truncate">路径: <code className="text-[10px]">{agent.path}</code></div>
                    </div>
                  )}
                </CardContent>
              </Card>
            );
          })}
        </div>
      </div>

      {/* Account Management */}
      <div>
        <div className="flex justify-between items-center mb-3">
          <h3 className="text-sm font-semibold flex items-center gap-2">
            <ToggleLeft className="h-4 w-4" /> 智能体云端账户授权
          </h3>
          <Button size="sm" variant="outline" onClick={() => onAddAccount()}>
            <Plus className="h-3 w-3" /> 新增账户
          </Button>
        </div>

        {accounts.length === 0 ? (
          <Card>
            <CardContent className="p-4 text-center text-muted-foreground text-xs">
              暂无账户凭证
            </CardContent>
          </Card>
        ) : (
          <div className="flex flex-col gap-2">
            {accounts.map((acc) => (
              <Card
                key={acc.id}
                className={cn(acc.is_active && "border-accent/30 bg-accent/[0.04]")}
              >
                <CardContent className="p-3 flex justify-between items-center">
                  <div>
                    <span className="font-semibold text-sm">{acc.account_name}</span>
                    <span className="text-xs text-muted-foreground ml-2.5">
                      Endpoint: <code>{acc.api_host}</code> | Model: <code>{acc.target_model}</code>
                    </span>
                  </div>
                  <div className="flex gap-1.5">
                    {!acc.is_active && (
                      <Button size="sm" variant="outline" onClick={() => onSwitchAccount(acc.id)}>
                        启用
                      </Button>
                    )}
                    <Button size="sm" variant="outline" onClick={() => onEditAccount(acc)}>
                      <Edit className="h-3 w-3" />
                    </Button>
                    <Button size="sm" variant="outline" onClick={() => onDeleteAccount(acc.id)}>
                      <Trash2 className="h-3 w-3 text-destructive" />
                    </Button>
                  </div>
                </CardContent>
              </Card>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

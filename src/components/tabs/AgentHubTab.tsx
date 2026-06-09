/**
 * AgentHubTab — Agent 仓库 & 管理管理器
 *
 * Shows installed agents grid, account management, and agent template library
 */

import { useState, useEffect } from "react";
import { Card, CardContent } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Bot, Plus, Edit, Trash2, ToggleLeft, BookOpen, Bug, Search, Layout, Palette, GitCommit, GitPullRequest, FileText, AlertTriangle, HelpCircle, TestTube, Target, Users, Lightbulb, Languages, Mail, Briefcase, Presentation, GraduationCap, Type, MessageSquare, ChevronDown, ChevronRight } from "lucide-react";
import { cn } from "@/lib/utils";
import { agentTemplateApi } from "@/lib/tauri-api";
import type { DetectedAgent, AgentAccount, PlatformModel } from "@/types";
import type { AgentTemplate } from "@/lib/tauri-api";

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
  onUseTemplate?: (template: AgentTemplate) => void;
}

/** Map icon string to Lucide component */
function getTemplateIcon(icon: string) {
  const map: Record<string, React.ComponentType<{ className?: string }>> = {
    Bug, Search, Layout, Palette, GitCommit, GitPullRequest, FileText,
    AlertTriangle, HelpCircle, TestTube, Target, Users, Lightbulb,
    Languages, Mail, MessageSquare, Edit, Briefcase, Presentation,
    GraduationCap, Type, BookOpen,
  };
  return map[icon] || Bot;
}

/** Map accent to badge color */
function getAccentClass(accent: string) {
  switch (accent) {
    case "warning": return "bg-amber-500/12 text-amber-400 border-amber-500/30";
    case "info": return "bg-cyan-500/12 text-cyan-400 border-cyan-500/30";
    case "success": return "bg-emerald-500/12 text-emerald-400 border-emerald-500/30";
    case "error": return "bg-red-500/12 text-red-400 border-red-500/30";
    default: return "bg-white/5 text-muted-foreground border-border";
  }
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
  onUseTemplate,
}: AgentHubTabProps) {
  const [templates, setTemplates] = useState<AgentTemplate[]>([]);
  const [expandedCategory, setExpandedCategory] = useState<string | null>("Engineering");
  const [expandedTemplate, setExpandedTemplate] = useState<string | null>(null);

  useEffect(() => {
    agentTemplateApi.getAll().then(setTemplates).catch(console.error);
  }, []);

  // Group templates by category
  const categories = templates.reduce<Record<string, AgentTemplate[]>>((acc, t) => {
    (acc[t.category] ||= []).push(t);
    return acc;
  }, {});

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

      {/* Agent Template Library (Multica-inspired) */}
      <div>
        <h3 className="text-sm font-semibold mb-3 flex items-center gap-2">
          <BookOpen className="h-4 w-4" /> Agent 角色模板库 ({templates.length})
        </h3>
        <p className="text-xs text-muted-foreground mb-3">
          从预设角色模板快速创建 Agent 会话，每个模板包含专业的系统提示和关联技能。
        </p>

        <div className="flex flex-col gap-3">
          {Object.entries(categories).map(([category, categoryTemplates]) => (
            <div key={category}>
              <button
                className="flex items-center gap-2 w-full text-left py-1.5 px-2 rounded-lg hover:bg-white/5 transition-colors"
                onClick={() => setExpandedCategory(expandedCategory === category ? null : category)}
              >
                {expandedCategory === category ? <ChevronDown className="h-3.5 w-3.5" /> : <ChevronRight className="h-3.5 w-3.5" />}
                <span className="text-xs font-medium text-secondary-foreground">{category}</span>
                <span className="text-[10px] text-muted-foreground ml-auto">{categoryTemplates.length}</span>
              </button>

              {expandedCategory === category && (
                <div className="grid grid-cols-1 gap-2 ml-5 mt-1">
                  {categoryTemplates.map((tmpl) => {
                    const Icon = getTemplateIcon(tmpl.icon);
                    const isExpanded = expandedTemplate === tmpl.slug;

                    return (
                      <Card key={tmpl.slug} className="overflow-hidden">
                        <CardContent className="p-3">
                          <div
                            className="flex items-start gap-3 cursor-pointer"
                            onClick={() => setExpandedTemplate(isExpanded ? null : tmpl.slug)}
                          >
                            <div className={cn("p-1.5 rounded-lg border", getAccentClass(tmpl.accent))}>
                              <Icon className="h-3.5 w-3.5" />
                            </div>
                            <div className="flex-1 min-w-0">
                              <div className="flex items-center gap-2">
                                <span className="font-medium text-sm">{tmpl.name}</span>
                                <Badge variant="outline" className={cn("text-[9px] py-0", getAccentClass(tmpl.accent))}>
                                  {tmpl.category}
                                </Badge>
                              </div>
                              <p className="text-xs text-muted-foreground mt-0.5 line-clamp-2">
                                {tmpl.description}
                              </p>
                            </div>
                            <Button
                              size="sm"
                              variant="outline"
                              className="shrink-0 text-[10px] h-6"
                              onClick={(e) => {
                                e.stopPropagation();
                                onUseTemplate?.(tmpl);
                              }}
                            >
                              使用
                            </Button>
                          </div>

                          {/* Expanded details */}
                          {isExpanded && (
                            <div className="mt-3 pt-3 border-t border-border">
                              <div className="text-[10px] font-medium text-secondary-foreground mb-1.5">系统提示预览:</div>
                              <pre className="text-[10px] text-muted-foreground bg-black/20 rounded p-2 max-h-[200px] overflow-y-auto whitespace-pre-wrap font-mono">
                                {tmpl.instructions.slice(0, 500)}{tmpl.instructions.length > 500 ? "..." : ""}
                              </pre>
                              {tmpl.skills.length > 0 && (
                                <div className="mt-2">
                                  <span className="text-[10px] font-medium text-secondary-foreground">关联技能: </span>
                                  {tmpl.skills.map((s, i) => (
                                    <span key={i} className="text-[10px] text-cyan-400">
                                      {s.name}{i < tmpl.skills.length - 1 ? ", " : ""}
                                    </span>
                                  ))}
                                </div>
                              )}
                            </div>
                          )}
                        </CardContent>
                      </Card>
                    );
                  })}
                </div>
              )}
            </div>
          ))}
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

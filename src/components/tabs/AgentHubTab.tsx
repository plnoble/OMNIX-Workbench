/**
 * AgentHubTab — Agent 仓库 & 管理管理器
 *
 * Shows installed agents grid, account management, and agent template library
 */

import { useState, useEffect } from "react";
import { Card, CardContent } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Bot, Plus, Edit, Trash2, ToggleLeft, BookOpen, Bug, Search, Layout, Palette, GitCommit, GitPullRequest, FileText, AlertTriangle, HelpCircle, TestTube, Target, Users, Lightbulb, Languages, Mail, Briefcase, Presentation, GraduationCap, Type, MessageSquare, ChevronDown, ChevronRight, Link, Unlink, Check, Loader2 } from "lucide-react";
import { cn } from "@/lib/utils";
import { toast } from "sonner";
import { agentTemplateApi, agentBindingApi, platformHealthApi } from "@/lib/tauri-api";
import type { DetectedAgent, AgentAccount, PlatformModel } from "@/types";
import type { AgentTemplate, AgentPlatformBinding, PlatformHealth } from "@/lib/tauri-api";

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
    default: return "bg-muted/20 text-muted-foreground border-border";
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
  const [isLoadingTemplates, setIsLoadingTemplates] = useState(true);

  // Per-agent binding state (CC Switch inspired)
  const [bindings, setBindings] = useState<AgentPlatformBinding[]>([]);
  const [platformHealth, setPlatformHealth] = useState<PlatformHealth[]>([]);
  const [editingAgent, setEditingAgent] = useState<string | null>(null);
  const [isLoadingBindings, setIsLoadingBindings] = useState(true);

  useEffect(() => {
    setIsLoadingTemplates(true);
    agentTemplateApi.getAll()
      .then(setTemplates)
      .catch((e) => { console.error("Failed to load templates:", e); toast.error("加载模板库失败"); })
      .finally(() => setIsLoadingTemplates(false));
    loadBindings();
    loadPlatformHealth();
  }, []);

  const loadBindings = async () => {
    setIsLoadingBindings(true);
    try {
      const list = await agentBindingApi.getAll();
      setBindings(list);
    } catch (e) {
      console.error("Failed to load bindings:", e);
      toast.error("加载 Agent 绑定失败");
    } finally {
      setIsLoadingBindings(false);
    }
  };

  const loadPlatformHealth = async () => {
    try {
      const health = await platformHealthApi.getAll();
      setPlatformHealth(health);
    } catch (e) {
      console.error("Failed to load platform health:", e);
      toast.error("加载平台健康状态失败");
    }
  };

  const handleBindAgent = async (agentName: string, platformId: string) => {
    try {
      await agentBindingApi.set(agentName, platformId);
      await loadBindings();
      setEditingAgent(null);
    } catch (e) {
      console.error("Failed to bind agent:", e);
      toast.error("绑定 Agent 失败");
    }
  };

  const handleUnbindAgent = async (agentName: string) => {
    try {
      await agentBindingApi.remove(agentName);
      await loadBindings();
    } catch (e) {
      console.error("Failed to unbind agent:", e);
      toast.error("解除绑定失败");
    }
  };

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
        <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
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
                      <div className="truncate">路径: <code className="text-xs">{agent.path}</code></div>
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
          {isLoadingTemplates ? (
            <div className="flex items-center justify-center py-8 text-muted-foreground">
              <Loader2 className="h-5 w-5 animate-spin mr-2" /> 加载模板库中...
            </div>
          ) : Object.entries(categories).map(([category, categoryTemplates]) => (
            <div key={category}>
              <button
                className="flex items-center gap-2 w-full text-left py-1.5 px-2 rounded-lg hover:bg-muted/20 transition-colors"
                onClick={() => setExpandedCategory(expandedCategory === category ? null : category)}
              >
                {expandedCategory === category ? <ChevronDown className="h-3.5 w-3.5" /> : <ChevronRight className="h-3.5 w-3.5" />}
                <span className="text-xs font-medium text-secondary-foreground">{category}</span>
                <span className="text-xs text-muted-foreground ml-auto">{categoryTemplates.length}</span>
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
                                <Badge variant="outline" className={cn("text-xs py-0", getAccentClass(tmpl.accent))}>
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
                              className="shrink-0 text-xs h-6"
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
                              <div className="text-xs font-medium text-secondary-foreground mb-1.5">系统提示预览:</div>
                              <pre className="text-xs text-muted-foreground bg-black/20 rounded p-2 max-h-[200px] overflow-y-auto whitespace-pre-wrap font-mono">
                                {tmpl.instructions.slice(0, 500)}{tmpl.instructions.length > 500 ? "..." : ""}
                              </pre>
                              {tmpl.skills.length > 0 && (
                                <div className="mt-2">
                                  <span className="text-xs font-medium text-secondary-foreground">关联技能: </span>
                                  {tmpl.skills.map((s, i) => (
                                    <span key={i} className="text-xs text-cyan-400">
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

      {/* Agent-Platform Binding Panel (CC Switch inspired) */}
      <div>
        <h3 className="text-sm font-semibold mb-3 flex items-center gap-2">
          <Link className="h-4 w-4" /> Agent API 供应商绑定
        </h3>
        <p className="text-xs text-muted-foreground mb-3">
          为每个 Agent 绑定不同的 API 供应商。Agent 发起请求时自动路由到绑定的平台。
        </p>

        <div className="flex flex-col gap-2">
          {isLoadingBindings ? (
            <div className="flex items-center justify-center py-4 text-muted-foreground">
              <Loader2 className="h-4 w-4 animate-spin mr-2" /> 加载绑定状态...
            </div>
          ) : detectedAgents.filter(a => a.status === "installed").map((agent) => {
            const binding = bindings.find(b => b.agent_name === agent.name);
            const isEditing = editingAgent === agent.name;
            const boundPlatform = platformHealth.find(p => p.id === binding?.platform_id);

            return (
              <Card key={agent.name} className={cn(binding && "border-cyan-500/20")}>
                <CardContent className="p-3">
                  <div className="flex items-center justify-between">
                    <div className="flex items-center gap-2">
                      <Bot className="h-4 w-4" />
                      <span className="text-sm font-medium">{agent.name}</span>
                      {binding ? (
                        <Badge variant="outline" className="text-xs bg-cyan-500/10 text-cyan-400 border-cyan-500/30">
                          → {binding.platform_name}
                        </Badge>
                      ) : (
                        <Badge variant="outline" className="text-xs text-muted-foreground">
                          默认路由
                        </Badge>
                      )}
                      {boundPlatform && (
                        <span className={cn(
                          "inline-block w-2 h-2 rounded-full",
                          boundPlatform.is_healthy ? "bg-emerald-500" : "bg-red-500"
                        )} />
                      )}
                    </div>
                    <div className="flex gap-1.5">
                      <Button
                        size="sm"
                        variant="outline"
                        className="text-xs h-6"
                        onClick={() => setEditingAgent(isEditing ? null : agent.name)}
                      >
                        {isEditing ? "取消" : binding ? "切换" : "绑定"}
                      </Button>
                      {binding && (
                        <Button
                          size="sm"
                          variant="outline"
                          className="text-xs h-6 text-red-400"
                          onClick={() => handleUnbindAgent(agent.name)}
                          aria-label="解除绑定"
                        >
                          <Unlink className="h-3 w-3" />
                        </Button>
                      )}
                    </div>
                  </div>

                  {/* Platform selector (expanded) */}
                  {isEditing && (
                    <div className="mt-3 pt-3 border-t border-border">
                      <div className="grid grid-cols-1 sm:grid-cols-2 gap-2">
                        {platformHealth.map((platform) => (
                          <button
                            key={platform.id}
                            className={cn(
                              "flex items-center gap-2 p-2 rounded-lg border text-left text-xs transition-all",
                              binding?.platform_id === platform.id
                                ? "bg-cyan-500/12 border-cyan-500/40 text-cyan-400"
                                : platform.is_healthy
                                  ? "bg-muted/10 border-border hover:bg-muted/20"
                                  : "bg-red-500/5 border-red-500/20 text-red-400 opacity-60"
                            )}
                            onClick={() => handleBindAgent(agent.name, platform.id)}
                          >
                            <span className={cn(
                              "inline-block w-2 h-2 rounded-full",
                              platform.is_healthy ? "bg-emerald-500" : "bg-red-500"
                            )} />
                            <div className="flex-1 min-w-0">
                              <div className="font-medium truncate">{platform.name}</div>
                              <div className="text-xs text-muted-foreground">{platform.api_type}</div>
                            </div>
                            {binding?.platform_id === platform.id && (
                              <Check className="h-3 w-3 text-cyan-400 shrink-0" />
                            )}
                          </button>
                        ))}
                      </div>
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

/**
 * AppSidebar — 品牌头 + 10 Tab 导航 + 对话列表抽屉（含归档/删除/全屏切换）
 */

import { useState } from "react";
import { Separator } from "@/components/ui/separator";
import {
  LayoutDashboard, MessageSquare, Bot, GitCompare, Users,
  Brain, Sparkles, BookOpen, Clock, Settings, Plus, FolderOpen, Trash2, Archive, Maximize2,
} from "lucide-react";
import { cn } from "@/lib/utils";
import type { ConversationInfo, GatewayStatus } from "@/types";

interface NavItem {
  id: string;
  label: string;
  icon: React.ReactNode;
}

const NAV_ITEMS: NavItem[] = [
  { id: "dashboard", label: "控制面板", icon: <LayoutDashboard className="h-4 w-4" /> },
  { id: "chat", label: "智能体对话", icon: <MessageSquare className="h-4 w-4" /> },
  { id: "agents", label: "Agent 仓库", icon: <Bot className="h-4 w-4" /> },
  { id: "compare", label: "比对中枢", icon: <GitCompare className="h-4 w-4" /> },
  { id: "team", label: "团队协同", icon: <Users className="h-4 w-4" /> },
  { id: "memories", label: "长期记忆", icon: <Brain className="h-4 w-4" /> },
  { id: "skills", label: "自进化技能", icon: <Sparkles className="h-4 w-4" /> },
  { id: "knowledge", label: "知识库 RAG", icon: <BookOpen className="h-4 w-4" /> },
  { id: "cron", label: "定时任务", icon: <Clock className="h-4 w-4" /> },
  { id: "settings", label: "中转与设置", icon: <Settings className="h-4 w-4" /> },
];

interface AppSidebarProps {
  activeTab: string;
  onTabChange: (tab: string) => void;
  gatewayStatus: GatewayStatus;
  showConversations: boolean;
  conversations: ConversationInfo[];
  currentConvId: string;
  activeSessions: string[];
  onSelectConversation: (id: string) => void;
  onDeleteConversation: (id: string, e: React.MouseEvent) => void;
  onArchiveConversation?: (id: string, e: React.MouseEvent) => void;
  onOpenHistoryFullscreen?: () => void;
  onNewConversation: () => void;
  onOpenWorkspaceModal: () => void;
}

export function AppSidebar({
  activeTab,
  onTabChange,
  gatewayStatus,
  showConversations,
  conversations,
  currentConvId,
  activeSessions,
  onSelectConversation,
  onDeleteConversation,
  onArchiveConversation,
  onOpenHistoryFullscreen,
  onNewConversation,
  onOpenWorkspaceModal,
}: AppSidebarProps) {
  const [pendingDelete, setPendingDelete] = useState<{ id: string; title: string } | null>(null);

  return (
    <aside className="flex flex-col w-56 shrink-0 border-r border-border glass-panel">
      {/* Brand Header */}
      <div className="p-5 border-b border-border flex items-center gap-2.5">
        <div
          className={cn(
            "w-2 h-2 rounded-full",
            gatewayStatus === "idle" && "bg-emerald-500 animate-pulse-green",
            gatewayStatus === "busy" && "bg-amber-500 animate-pulse-amber",
            gatewayStatus === "error" && "bg-red-500 animate-pulse-red"
          )}
        />
        <div>
          <h1 className="text-base font-bold m-0">OMNIX DevFlow</h1>
          <span className="text-xs text-muted-foreground">智能跨模型网关桌面端 v0.1.0</span>
        </div>
      </div>

      {/* Navigation */}
      <nav className="p-2.5 flex flex-col gap-1">
        {NAV_ITEMS.map((item) => (
          <button
            key={item.id}
            onClick={() => onTabChange(item.id)}
            className={cn(
              "w-full flex items-center gap-2.5 px-3 py-2 rounded-lg text-sm cursor-pointer transition-all",
              activeTab === item.id
                ? "bg-accent/10 text-accent"
                : "text-foreground hover:bg-muted/20"
            )}
          >
            {item.icon}
            <span>{item.label}</span>
          </button>
        ))}
      </nav>

      {/* Conversation Drawer */}
      {showConversations && (
        <>
          <Separator />
          <div className="flex-1 flex flex-col overflow-hidden">
            <div className="px-4 py-3 flex justify-between items-center border-b border-border">
              <span className="text-xs font-semibold text-muted-foreground uppercase">
                💬 智能体对话
              </span>
              <div className="flex gap-1.5">
                {onOpenHistoryFullscreen && (
                  <button
                    title="在大窗口查看历史"
                    aria-label="在大窗口查看历史"
                    onClick={onOpenHistoryFullscreen}
                    className="bg-transparent border-none text-muted-foreground cursor-pointer text-sm hover:text-foreground"
                  >
                    <Maximize2 className="h-3.5 w-3.5" />
                  </button>
                )}
                <button
                  title="载入项目工作区"
                  aria-label="载入项目工作区"
                  onClick={onOpenWorkspaceModal}
                  className="bg-transparent border-none text-muted-foreground cursor-pointer text-sm hover:text-foreground"
                >
                  <FolderOpen className="h-3.5 w-3.5" />
                </button>
                <button
                  title="新建对话"
                  aria-label="新建对话"
                  onClick={onNewConversation}
                  className="bg-transparent border-none text-muted-foreground cursor-pointer text-sm hover:text-foreground"
                >
                  <Plus className="h-3.5 w-3.5" />
                </button>
              </div>
            </div>

            <div className="flex-1 overflow-y-auto p-2.5 flex flex-col gap-0.5">
              {conversations.length === 0 ? (
                <div className="py-5 text-center text-muted-foreground text-xs">
                  无历史会话
                  <div className="text-xs mt-1">点 + 开始新对话</div>
                </div>
              ) : (
                conversations.map((conv) => {
                  const isActive = currentConvId === conv.id;
                  const isRunning = activeSessions.includes(conv.id);
                  const isProject = conv.workspace_path && conv.workspace_path !== "direct";
                  const folderName = isProject ? conv.workspace_path.split(/[\\/]/).pop() : "";

                  return (
                    <div
                      key={conv.id}
                      className={cn(
                        "group w-full relative pr-14 flex justify-between items-center px-3 py-2 rounded-lg cursor-pointer transition-all",
                        isActive ? "bg-accent/10 text-accent" : "hover:bg-muted/20 text-foreground"
                      )}
                      onClick={() => onSelectConversation(conv.id)}
                    >
                      <div className="flex flex-col gap-0.5 overflow-hidden">
                        <span className="text-sm font-medium truncate">{conv.title}</span>
                        <div className="flex items-center gap-1.5">
                          {isRunning && (
                            <span className="w-1.5 h-1.5 rounded-full bg-emerald-500 animate-pulse-green" />
                          )}
                          {isProject ? (
                            <span className="text-xs text-muted-foreground">📁 {folderName}</span>
                          ) : (
                            <span className="text-xs text-muted-foreground">💬 {conv.active_agent}</span>
                          )}
                        </div>
                      </div>

                      {/* Action buttons (hover-only on desktop, always visible on touch) */}
                      <div className="absolute right-1 top-1/2 -translate-y-1/2 flex items-center gap-0.5 opacity-60 group-hover:opacity-100 transition-opacity">
                        {onArchiveConversation && (
                          <button
                            type="button"
                            title="归档对话"
                            aria-label="归档对话"
                            onClick={(e) => { e.stopPropagation(); onArchiveConversation(conv.id, e); }}
                            className="p-1 rounded text-muted-foreground hover:text-amber-500 hover:bg-amber-500/10 cursor-pointer"
                          >
                            <Archive className="h-3 w-3" />
                          </button>
                        )}
                        <button
                          type="button"
                          title="删除对话"
                          aria-label="删除对话"
                          onClick={(e) => { e.stopPropagation(); setPendingDelete({ id: conv.id, title: conv.title }); }}
                          className="p-1 rounded text-muted-foreground hover:text-destructive hover:bg-destructive/10 cursor-pointer"
                        >
                          <Trash2 className="h-3 w-3" />
                        </button>
                      </div>
                    </div>
                  );
                })
              )}
            </div>
          </div>
        </>
      )}

      {/* Confirm delete modal */}
      {pendingDelete && (
        <div className="fixed inset-0 bg-black/60 backdrop-blur-sm flex items-center justify-center z-[1000]">
          <div className="bg-card border border-border rounded-lg p-5 max-w-md mx-4 shadow-xl">
            <h3 className="text-base font-semibold m-0 mb-2 text-foreground">确认删除对话?</h3>
            <p className="text-sm text-muted-foreground mb-1 truncate">"{pendingDelete.title}"</p>
            <p className="text-xs text-muted-foreground mb-4">删除后将无法恢复（消息记录也会被一并清除）。如想保留请使用「归档」。</p>
            <div className="flex gap-2 justify-end">
              <button
                className="px-3 py-1.5 text-sm rounded-md border border-border bg-muted/10 hover:bg-muted/30 text-foreground"
                onClick={() => setPendingDelete(null)}
              >
                取消
              </button>
              <button
                className="px-3 py-1.5 text-sm rounded-md bg-destructive text-destructive-foreground hover:bg-destructive/90"
                onClick={(e) => {
                  if (pendingDelete) {
                    onDeleteConversation(pendingDelete.id, e as unknown as React.MouseEvent);
                  }
                  setPendingDelete(null);
                }}
              >
                确认删除
              </button>
            </div>
          </div>
        </div>
      )}
    </aside>
  );
}

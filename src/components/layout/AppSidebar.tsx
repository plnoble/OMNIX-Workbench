/**
 * AppSidebar — 品牌头 + 9 Tab 导航 + 对话列表抽屉
 */

import { Separator } from "@/components/ui/separator";
import {
  LayoutDashboard, MessageSquare, Bot, GitCompare, Users,
  Brain, Sparkles, BookOpen, Clock, Settings, Plus, FolderOpen, Trash2,
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
  onNewConversation,
  onOpenWorkspaceModal,
}: AppSidebarProps) {
  return (
    <aside className="flex flex-col w-72 border-r border-border glass-panel">
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
          <span className="text-[11px] text-muted-foreground">智能跨模型网关桌面端 v0.1.0</span>
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
                : "text-foreground hover:bg-white/5"
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
            <div className="px-4 py-3 flex justify-between items-center border-b border-white/[0.02]">
              <span className="text-[11px] font-semibold text-muted-foreground uppercase">
                💬 智能体对话
              </span>
              <div className="flex gap-1.5">
                <button
                  title="载入项目工作区"
                  onClick={onOpenWorkspaceModal}
                  className="bg-transparent border-none text-muted-foreground cursor-pointer text-sm hover:text-foreground"
                >
                  <FolderOpen className="h-3.5 w-3.5" />
                </button>
                <button
                  title="新建对话"
                  onClick={onNewConversation}
                  className="bg-transparent border-none text-muted-foreground cursor-pointer text-sm hover:text-foreground"
                >
                  <Plus className="h-3.5 w-3.5" />
                </button>
              </div>
            </div>

            <div className="flex-1 overflow-y-auto p-2.5 flex flex-col gap-0.5">
              {conversations.length === 0 ? (
                <div className="py-5 text-center text-muted-foreground text-xs">无历史会话</div>
              ) : (
                conversations.map((conv) => {
                  const isActive = currentConvId === conv.id;
                  const isRunning = activeSessions.includes(conv.id);
                  const isProject = conv.workspace_path && conv.workspace_path !== "direct";
                  const folderName = isProject ? conv.workspace_path.split(/[\\/]/).pop() : "";

                  return (
                    <button
                      key={conv.id}
                      onClick={() => onSelectConversation(conv.id)}
                      className={cn(
                        "w-full relative pr-10 flex justify-between items-center px-3 py-2 rounded-lg text-left cursor-pointer transition-all",
                        isActive ? "bg-accent/10 text-accent" : "hover:bg-white/5 text-foreground"
                      )}
                    >
                      <div className="flex flex-col gap-0.5 overflow-hidden">
                        <span className="text-sm font-medium truncate">{conv.title}</span>
                        <div className="flex items-center gap-1.5">
                          {isRunning && (
                            <span className="w-1.5 h-1.5 rounded-full bg-emerald-500 animate-pulse-green" />
                          )}
                          {isProject ? (
                            <span className="text-[10px] text-muted-foreground">📁 {folderName}</span>
                          ) : (
                            <span className="text-[10px] text-muted-foreground">💬 {conv.active_agent}</span>
                          )}
                        </div>
                      </div>
                      <span
                        onClick={(e) => onDeleteConversation(conv.id, e)}
                        className="absolute right-2 top-1/2 -translate-y-1/2 text-muted-foreground hover:text-destructive cursor-pointer"
                      >
                        <Trash2 className="h-3 w-3" />
                      </span>
                    </button>
                  );
                })
              )}
            </div>
          </div>
        </>
      )}
    </aside>
  );
}

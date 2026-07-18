import { useMemo, useState, type MouseEvent, type ReactNode } from "react";
import { createPortal } from "react-dom";
import {
  Archive,
  CalendarClock,
  FolderOpen,
  History,
  MessageSquare,
  Plus,
  Search,
  Trash2,
  Users,
} from "lucide-react";

import { Separator } from "@/components/ui/separator";
import { cn } from "@/lib/utils";
import type { ConversationInfo, GatewayStatus } from "@/types";

interface AppSidebarProps {
  activeTab: string;
  onTabChange: (tab: string) => void;
  gatewayStatus: GatewayStatus;
  showConversations: boolean;
  conversations: ConversationInfo[];
  activeAgent: string;
  currentConvId: string;
  activeSessions: string[];
  onSelectConversation: (id: string) => void;
  onDeleteConversation: (id: string, e: MouseEvent) => void;
  onArchiveConversation?: (id: string, title: string) => void;
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
  activeAgent,
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
  const isWorkSurface = showConversations && (activeTab === "chat" || activeTab === "work" || activeTab === "team");
  const showWorkspaceList = activeTab === "work" || activeTab === "team";
  const showChatList = activeTab === "chat" || activeTab === "team";

  // Each Agent keeps an independent chat history, so the work context only
  // lists conversations belonging to the currently active Agent.
  const grouped = useMemo(() => {
    const direct: ConversationInfo[] = [];
    const workspace: ConversationInfo[] = [];
    for (const conv of conversations) {
      if (conv.active_agent !== activeAgent) continue;
      if (conv.workspace_path && conv.workspace_path !== "direct") workspace.push(conv);
      else direct.push(conv);
    }
    return { direct, workspace };
  }, [conversations, activeAgent]);

  const statusText = {
    idle: "空闲",
    busy: "执行中",
    error: "异常",
  }[gatewayStatus];

  if (!isWorkSurface) {
    return null;
  }

  return (
    <aside className="glass-chrome flex w-44 shrink-0 flex-col border-r md:w-52 min-[1500px]:w-72">
      <div className="border-b border-border p-4">
        <div className="flex items-center justify-between gap-3">
          <div className="min-w-0">
            <div className="truncate text-sm font-semibold">{activeTab === "team" ? "团队上下文" : activeTab === "chat" ? "对话" : "工作上下文"}</div>
            <div className="mt-1 flex items-center gap-1.5 text-xs text-muted-foreground">
              <span className={cn("h-1.5 w-1.5 rounded-full", gatewayStatus === "idle" && "bg-success", gatewayStatus === "busy" && "bg-warning", gatewayStatus === "error" && "bg-destructive")} />
              {statusText}
            </div>
          </div>
          {onOpenHistoryFullscreen && (
            <button
              title="历史与归档"
              aria-label="历史与归档"
              onClick={onOpenHistoryFullscreen}
              className="rounded-md border border-border p-2 text-muted-foreground hover:bg-muted/20 hover:text-foreground"
            >
              <History className="h-4 w-4" />
            </button>
          )}
        </div>
      </div>

      {isWorkSurface ? (
        <>
          <div className="grid grid-cols-1 gap-1.5 p-3">
            <button
              className="flex items-center gap-3 rounded-md px-3 py-2 text-left text-sm hover:bg-muted/20"
              onClick={() => (activeTab === "work" ? onOpenWorkspaceModal() : onNewConversation())}
            >
              <span className="flex h-8 w-8 items-center justify-center rounded-md bg-muted/30">
                <Plus className="h-4 w-4" />
              </span>
              {activeTab === "chat" ? "新对话" : activeTab === "work" ? "新工作会话" : "新会话"}
            </button>
            <button className="flex items-center gap-3 rounded-md px-3 py-2 text-left text-sm hover:bg-muted/20" onClick={onOpenHistoryFullscreen}>
              <span className="flex h-8 w-8 items-center justify-center rounded-md bg-muted/30">
                <Search className="h-4 w-4" />
              </span>
              搜索
            </button>
            <button className="flex items-center gap-3 rounded-md px-3 py-2 text-left text-sm hover:bg-muted/20" onClick={() => onTabChange("cron")}>
              <span className="flex h-8 w-8 items-center justify-center rounded-md bg-muted/30">
                <CalendarClock className="h-4 w-4" />
              </span>
              定时任务
            </button>
          </div>

          <Separator />

          <div className="flex min-h-0 flex-1 flex-col overflow-hidden">
            {showWorkspaceList && (
              <ConversationSection
                title="工作"
                icon={<FolderOpen className="h-3.5 w-3.5" />}
                emptyText="还没有工作区会话"
                conversations={grouped.workspace}
                currentConvId={currentConvId}
                activeSessions={activeSessions}
                onSelectConversation={onSelectConversation}
                onDelete={(conv) => setPendingDelete({ id: conv.id, title: conv.title })}
                onArchiveConversation={onArchiveConversation}
              />
            )}
            {showChatList && (
            <ConversationSection
              title={activeTab === "team" ? "团队" : "对话"}
              icon={activeTab === "team" ? <Users className="h-3.5 w-3.5" /> : <MessageSquare className="h-3.5 w-3.5" />}
              emptyText="还没有普通对话"
              conversations={grouped.direct}
              currentConvId={currentConvId}
              activeSessions={activeSessions}
              onSelectConversation={onSelectConversation}
              onDelete={(conv) => setPendingDelete({ id: conv.id, title: conv.title })}
              onArchiveConversation={onArchiveConversation}
            />
            )}
            {/* Explicit entry: archived conversations live in the fullscreen
                history view — an icon-only entry point proved undiscoverable. */}
            {onOpenHistoryFullscreen && (
              <button
                className="mx-3 mb-2 flex items-center gap-2 rounded-md px-3 py-2 text-left text-xs text-muted-foreground hover:bg-muted/20 hover:text-foreground"
                onClick={onOpenHistoryFullscreen}
              >
                <History className="h-3.5 w-3.5" />
                历史与归档…
              </button>
            )}
          </div>

          {activeTab !== "chat" && (
          <div className="border-t border-border p-3">
            <button
              className="flex w-full items-center justify-center gap-2 rounded-md border border-border px-3 py-2 text-sm hover:bg-muted/20"
              onClick={onOpenWorkspaceModal}
            >
              <FolderOpen className="h-4 w-4" />
              选择工作区
            </button>
          </div>
          )}
        </>
      ) : (
        <div className="flex flex-1 flex-col justify-between p-4">
          <div className="rounded-md border border-border bg-card/40 p-4">
            <div className="text-sm font-semibold">当前页面</div>
            <p className="mt-2 text-xs leading-5 text-muted-foreground">
              左侧栏只显示当前页面需要的上下文。资源和实验功能从顶栏应用宫格进入，避免主界面拥挤。
            </p>
          </div>
          <button
            className="flex items-center justify-center gap-2 rounded-md border border-border px-3 py-2 text-sm hover:bg-muted/20"
            onClick={() => onTabChange("work")}
          >
            <MessageSquare className="h-4 w-4" />
            返回工作
          </button>
        </div>
      )}

      {pendingDelete && createPortal(
        <div className="fixed inset-0 z-[1000] flex items-center justify-center bg-black/60 p-4 backdrop-blur-sm">
          <div className="w-full max-w-sm rounded-md border border-border bg-card p-5 shadow-xl">
            <h3 className="m-0 mb-2 text-base font-semibold text-foreground">确认删除会话？</h3>
            <p className="mb-1 break-words text-sm text-muted-foreground line-clamp-3">"{pendingDelete.title}"</p>
            <p className="mb-4 text-xs leading-5 text-muted-foreground">
              删除后无法恢复。需要保留记录时，请先归档。
            </p>
            <div className="flex justify-end gap-2">
              <button
                className="rounded-md border border-border bg-muted/10 px-3 py-1.5 text-sm text-foreground hover:bg-muted/30"
                onClick={() => setPendingDelete(null)}
              >
                取消
              </button>
              <button
                className="rounded-md bg-destructive px-3 py-1.5 text-sm text-destructive-foreground hover:bg-destructive/90"
                onClick={(e) => {
                  onDeleteConversation(pendingDelete.id, e as unknown as MouseEvent);
                  setPendingDelete(null);
                }}
              >
                确认删除
              </button>
            </div>
          </div>
        </div>,
        document.body
      )}
    </aside>
  );
}

function ConversationSection({
  title,
  icon,
  emptyText,
  conversations,
  currentConvId,
  activeSessions,
  onSelectConversation,
  onDelete,
  onArchiveConversation,
}: {
  title: string;
  icon: ReactNode;
  emptyText: string;
  conversations: ConversationInfo[];
  currentConvId: string;
  activeSessions: string[];
  onSelectConversation: (id: string) => void;
  onDelete: (conv: ConversationInfo) => void;
  onArchiveConversation?: (id: string, title: string) => void;
}) {
  return (
    <section className="min-h-0 flex-1 overflow-hidden border-b border-border last:border-b-0">
      <div className="flex items-center gap-2 px-4 py-3 text-xs font-semibold uppercase text-muted-foreground">
        {icon}
        {title}
        <span className="ml-auto rounded bg-muted/30 px-1.5 py-0.5">{conversations.length}</span>
      </div>
      <div className="max-h-[calc(50vh-7rem)] overflow-y-auto px-2 pb-3">
        {conversations.length === 0 ? (
          <div className="rounded-md border border-dashed border-border px-3 py-5 text-center text-xs text-muted-foreground">
            {emptyText}
          </div>
        ) : (
          conversations.map((conv) => {
            const isActive = currentConvId === conv.id;
            const isRunning = activeSessions.includes(conv.id);
            const label = conv.workspace_path && conv.workspace_path !== "direct"
              ? conv.workspace_path.split(/[\\/]/).pop()
              : conv.active_agent;

            return (
              <div
                key={conv.id}
                className={cn(
                  "group relative mb-1 cursor-pointer rounded-md px-3 py-2 pr-16 transition-colors",
                  isActive ? "bg-primary/12 text-primary" : "text-foreground hover:bg-muted/20"
                )}
                onClick={() => onSelectConversation(conv.id)}
              >
                <div className="truncate text-sm font-medium">{conv.title}</div>
                <div className="mt-0.5 flex items-center gap-1.5 text-xs text-muted-foreground">
                  {isRunning && <span className="h-1.5 w-1.5 rounded-full bg-success animate-pulse-green" />}
                  <span className="truncate">{label}</span>
                </div>

                <div className="absolute right-1 top-1/2 flex -translate-y-1/2 items-center gap-0.5 opacity-0 transition-opacity group-hover:opacity-100">
                  {onArchiveConversation && (
                    <button
                      type="button"
                      title="归档"
                      aria-label="归档"
                      onClick={(e) => {
                        e.stopPropagation();
                        onArchiveConversation(conv.id, conv.title);
                      }}
                      className="rounded p-1 text-muted-foreground hover:bg-warning/10 hover:text-warning"
                    >
                      <Archive className="h-3 w-3" />
                    </button>
                  )}
                  <button
                    type="button"
                    title="删除"
                    aria-label="删除"
                    onClick={(e) => {
                      e.stopPropagation();
                      onDelete(conv);
                    }}
                    className="rounded p-1 text-muted-foreground hover:bg-destructive/10 hover:text-destructive"
                  >
                    <Trash2 className="h-3 w-3" />
                  </button>
                </div>
              </div>
            );
          })
        )}
      </div>
    </section>
  );
}

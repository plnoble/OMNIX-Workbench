/**
 * ConversationHistoryView — 会话历史全屏页面
 *
 * 解决侧栏列表拥挤问题：以独立大窗口形式查看活跃 + 归档会话
 * 支持搜索、归档/取消归档、删除（带确认）
 */

import { useState, useMemo, useEffect } from "react";
import { Search, MessageSquare, FolderOpen, Archive, ArchiveRestore, Trash2, X, Plus, Clock } from "lucide-react";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import type { ConversationInfo } from "@/types";

interface ConversationHistoryViewProps {
  conversations: ConversationInfo[];
  archivedConversations: ConversationInfo[];
  currentConvId: string;
  activeSessions: string[];
  onSelectConversation: (id: string) => void;
  onDeleteConversation: (id: string, e: React.MouseEvent) => void;
  onArchiveConversation: (id: string, title: string) => void;
  onUnarchiveConversation: (id: string) => void;
  onNewConversation: () => void;
  onLoadArchived: () => void;
  onClose: () => void;
}

export function ConversationHistoryView({
  conversations,
  archivedConversations,
  currentConvId,
  activeSessions,
  onSelectConversation,
  onDeleteConversation,
  onArchiveConversation,
  onUnarchiveConversation,
  onNewConversation,
  onLoadArchived,
  onClose,
}: ConversationHistoryViewProps) {
  const [tab, setTab] = useState<"active" | "archived">("active");
  const [query, setQuery] = useState("");
  const [pendingDelete, setPendingDelete] = useState<{ id: string; title: string } | null>(null);

  useEffect(() => {
    if (tab === "archived") {
      onLoadArchived();
    }
  }, [tab, onLoadArchived]);

  const source = tab === "active" ? conversations : archivedConversations;

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return source;
    return source.filter(
      (c) =>
        c.title.toLowerCase().includes(q) ||
        c.active_agent.toLowerCase().includes(q) ||
        (c.workspace_path && c.workspace_path.toLowerCase().includes(q))
    );
  }, [source, query]);

  const formatDate = (iso: string) => {
    try {
      const d = new Date(iso);
      return d.toLocaleString("zh-CN", {
        year: "numeric", month: "2-digit", day: "2-digit",
        hour: "2-digit", minute: "2-digit",
      });
    } catch {
      return iso;
    }
  };

  return (
    <div className="fixed inset-0 z-50 bg-background/95 backdrop-blur-sm flex flex-col">
      {/* Header */}
      <div className="flex items-center justify-between px-6 py-4 border-b border-border">
        <div className="flex items-center gap-3">
          <MessageSquare className="h-5 w-5 text-accent" />
          <div>
            <h2 className="text-lg font-semibold m-0 text-foreground">会话历史</h2>
            <p className="text-xs text-muted-foreground m-0">浏览、归档、检索全部对话记录</p>
          </div>
        </div>
        <div className="flex items-center gap-2">
          <Button size="sm" onClick={() => { onNewConversation(); onClose(); }}>
            <Plus className="h-4 w-4" /> 新建对话
          </Button>
          <Button size="sm" variant="outline" onClick={onClose}>
            <X className="h-4 w-4" /> 关闭
          </Button>
        </div>
      </div>

      {/* Tabs + Search */}
      <div className="flex items-center justify-between px-6 py-3 border-b border-border gap-4">
        <div className="flex gap-1 bg-muted/10 p-1 rounded-lg border border-border">
          <button
            onClick={() => setTab("active")}
            className={cn(
              "px-4 py-1.5 text-sm rounded-md cursor-pointer transition-all",
              tab === "active"
                ? "bg-accent text-accent-foreground font-medium"
                : "text-muted-foreground hover:text-foreground"
            )}
          >
            🗨️ 活跃对话 ({conversations.length})
          </button>
          <button
            onClick={() => setTab("archived")}
            className={cn(
              "px-4 py-1.5 text-sm rounded-md cursor-pointer transition-all",
              tab === "archived"
                ? "bg-accent text-accent-foreground font-medium"
                : "text-muted-foreground hover:text-foreground"
            )}
          >
            📦 归档 ({archivedConversations.length})
          </button>
        </div>

        <div className="relative flex-1 max-w-md">
          <Search className="h-4 w-4 absolute left-3 top-1/2 -translate-y-1/2 text-muted-foreground" />
          <input
            type="text"
            placeholder="搜索标题、Agent 名称、工作区..."
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            className="w-full pl-9 pr-3 py-2 text-sm bg-muted/10 border border-border rounded-lg text-foreground placeholder:text-muted-foreground focus:outline-none focus:border-accent"
          />
        </div>
      </div>

      {/* List */}
      <div className="flex-1 overflow-y-auto p-6">
        {filtered.length === 0 ? (
          <div className="flex flex-col items-center justify-center h-full text-muted-foreground gap-2">
            <MessageSquare className="h-12 w-12 opacity-30" />
            <p className="text-sm">
              {query.trim()
                ? "没有匹配的会话"
                : tab === "active"
                  ? "暂无活跃对话"
                  : "暂无归档对话"}
            </p>
          </div>
        ) : (
          <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-3 max-w-6xl mx-auto">
            {filtered.map((conv) => {
              const isActive = currentConvId === conv.id;
              const isRunning = activeSessions.includes(conv.id);
              const isProject = conv.workspace_path && conv.workspace_path !== "direct";
              const folderName = isProject ? conv.workspace_path.split(/[\\/]/).pop() : "";

              return (
                <div
                  key={conv.id}
                  className={cn(
                    "group relative p-4 rounded-lg border bg-card transition-all cursor-pointer hover:border-accent/50 hover:shadow-md",
                    isActive ? "border-accent ring-1 ring-accent" : "border-border"
                  )}
                  onClick={() => {
                    onSelectConversation(conv.id);
                    onClose();
                  }}
                >
                  {/* Title row */}
                  <div className="flex items-start justify-between gap-2 mb-2">
                    <h3 className="text-sm font-semibold text-foreground m-0 flex-1 truncate">
                      {conv.title}
                    </h3>
                    {isRunning && (
                      <span className="shrink-0 w-2 h-2 rounded-full bg-emerald-500 animate-pulse-green" title="运行中" />
                    )}
                  </div>

                  {/* Meta */}
                  <div className="flex flex-col gap-1 text-xs text-muted-foreground mb-3">
                    <div className="flex items-center gap-1.5">
                      {isProject ? (
                        <>
                          <FolderOpen className="h-3 w-3" />
                          <span className="truncate">{folderName}</span>
                        </>
                      ) : (
                        <>
                          <MessageSquare className="h-3 w-3" />
                          <span>{conv.active_agent}</span>
                        </>
                      )}
                    </div>
                    <div className="flex items-center gap-1.5">
                      <Clock className="h-3 w-3" />
                      <span>{formatDate(conv.created_at)}</span>
                    </div>
                  </div>

                  {/* Actions */}
                  <div className="flex gap-2 justify-end" onClick={(e) => e.stopPropagation()}>
                    {tab === "active" ? (
                      <button
                        type="button"
                        onClick={() => onArchiveConversation(conv.id, conv.title)}
                        className="text-xs px-2.5 py-1 rounded border border-border bg-muted/10 hover:bg-amber-500/10 hover:border-amber-500/30 hover:text-amber-500 text-muted-foreground transition-colors cursor-pointer flex items-center gap-1"
                      >
                        <Archive className="h-3 w-3" /> 归档
                      </button>
                    ) : (
                      <button
                        type="button"
                        onClick={() => onUnarchiveConversation(conv.id)}
                        className="text-xs px-2.5 py-1 rounded border border-border bg-muted/10 hover:bg-emerald-500/10 hover:border-emerald-500/30 hover:text-emerald-500 text-muted-foreground transition-colors cursor-pointer flex items-center gap-1"
                      >
                        <ArchiveRestore className="h-3 w-3" /> 还原
                      </button>
                    )}
                    <button
                      type="button"
                      onClick={() => setPendingDelete({ id: conv.id, title: conv.title })}
                      className="text-xs px-2.5 py-1 rounded border border-border bg-muted/10 hover:bg-destructive/10 hover:border-destructive/30 hover:text-destructive text-muted-foreground transition-colors cursor-pointer flex items-center gap-1"
                    >
                      <Trash2 className="h-3 w-3" /> 删除
                    </button>
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </div>

      {/* Confirm delete modal */}
      {pendingDelete && (
        <div className="fixed inset-0 bg-black/60 backdrop-blur-sm flex items-center justify-center z-[60]">
          <div className="bg-card border border-border rounded-lg p-5 max-w-md mx-4 shadow-xl">
            <h3 className="text-base font-semibold m-0 mb-2 text-foreground">确认删除对话?</h3>
            <p className="text-sm text-muted-foreground mb-1 truncate">"{pendingDelete.title}"</p>
            <p className="text-xs text-muted-foreground mb-4">删除后将无法恢复，消息记录也会被一并清除。如想保留请使用「归档」。</p>
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
    </div>
  );
}

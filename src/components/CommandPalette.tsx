/**
 * CommandPalette — Ctrl+K 全局命令面板
 *
 * 快速搜索 tab、设置项、快捷操作。
 * 居中弹窗，glass-card 风格，搜索输入框 + 列表。
 */

import { useState, useEffect, useRef, useCallback } from "react";
import {
  LayoutDashboard, MessageSquare, Bot, GitCompare, Users,
  Brain, Sparkles, BookOpen, Clock, Settings,
  Search, ArrowRight, Zap,
} from "lucide-react";
import { cn } from "@/lib/utils";

interface CommandItem {
  id: string;
  label: string;
  description?: string;
  icon: React.ReactNode;
  category: "navigation" | "action";
  action: () => void;
}

interface CommandPaletteProps {
  open: boolean;
  onClose: () => void;
  onNavigate: (tab: string) => void;
  onToggleTheme: () => void;
}

const NAV_COMMANDS = [
  { id: "nav-dashboard", label: "控制面板", icon: <LayoutDashboard className="h-4 w-4" />, tab: "dashboard" },
  { id: "nav-chat", label: "智能体对话", icon: <MessageSquare className="h-4 w-4" />, tab: "chat" },
  { id: "nav-agents", label: "Agent 仓库", icon: <Bot className="h-4 w-4" />, tab: "agents" },
  { id: "nav-compare", label: "比对中枢", icon: <GitCompare className="h-4 w-4" />, tab: "compare" },
  { id: "nav-team", label: "团队协同", icon: <Users className="h-4 w-4" />, tab: "team" },
  { id: "nav-memories", label: "长期记忆", icon: <Brain className="h-4 w-4" />, tab: "memories" },
  { id: "nav-skills", label: "自进化技能", icon: <Sparkles className="h-4 w-4" />, tab: "skills" },
  { id: "nav-knowledge", label: "知识库 RAG", icon: <BookOpen className="h-4 w-4" />, tab: "knowledge" },
  { id: "nav-cron", label: "定时任务", icon: <Clock className="h-4 w-4" />, tab: "cron" },
  { id: "nav-settings", label: "中转与设置", icon: <Settings className="h-4 w-4" />, tab: "settings" },
];

export function CommandPalette({ open, onClose, onNavigate, onToggleTheme }: CommandPaletteProps) {
  const [query, setQuery] = useState("");
  const [selectedIndex, setSelectedIndex] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);

  const commands: CommandItem[] = [
    ...NAV_COMMANDS.map((c) => ({
      ...c,
      description: `导航到 ${c.label}`,
      category: "navigation" as const,
      action: () => { onNavigate(c.tab); onClose(); },
    })),
    {
      id: "action-theme",
      label: "切换主题",
      description: "切换深色/浅色/自动主题",
      icon: <Zap className="h-4 w-4" />,
      category: "action" as const,
      action: () => { onToggleTheme(); onClose(); },
    },
  ];

  const filtered = query.trim()
    ? commands.filter((c) =>
        c.label.toLowerCase().includes(query.toLowerCase()) ||
        (c.description && c.description.toLowerCase().includes(query.toLowerCase()))
      )
    : commands;

  useEffect(() => {
    setSelectedIndex(0);
  }, [query]);

  useEffect(() => {
    if (open) {
      setQuery("");
      setTimeout(() => inputRef.current?.focus(), 50);
    }
  }, [open]);

  const handleKeyDown = useCallback((e: React.KeyboardEvent) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setSelectedIndex((prev) => Math.min(prev + 1, filtered.length - 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setSelectedIndex((prev) => Math.max(prev - 1, 0));
    } else if (e.key === "Enter") {
      e.preventDefault();
      if (filtered[selectedIndex]) {
        filtered[selectedIndex].action();
      }
    } else if (e.key === "Escape") {
      onClose();
    }
  }, [filtered, selectedIndex, onClose]);

  if (!open) return null;

  return (
    <div className="fixed inset-0 z-[100] flex items-start justify-center pt-[20vh]">
      {/* Backdrop */}
      <div className="absolute inset-0 bg-black/40" onClick={onClose} />

      {/* Palette */}
      <div className="relative w-[520px] glass-card p-0 overflow-hidden animate-fade-in">
        {/* Search input */}
        <div className="flex items-center gap-3 px-4 py-3 border-b border-border">
          <Search className="h-4 w-4 text-muted-foreground" />
          <input
            ref={inputRef}
            type="text"
            className="flex-1 bg-transparent text-sm outline-none placeholder:text-muted-foreground"
            placeholder="搜索命令、导航、操作…"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={handleKeyDown}
          />
          <kbd className="text-xs px-1.5 py-0.5 rounded border border-border text-muted-foreground">ESC</kbd>
        </div>

        {/* Results */}
        <div className="max-h-[300px] overflow-y-auto py-1">
          {filtered.length === 0 ? (
            <div className="py-6 text-center text-sm text-muted-foreground">
              无匹配结果
            </div>
          ) : (
            filtered.map((item, i) => (
              <button
                key={item.id}
                className={cn(
                  "w-full flex items-center gap-3 px-4 py-2 text-left text-sm transition-colors",
                  i === selectedIndex
                    ? "bg-primary/10 text-primary"
                    : "hover:bg-muted/50"
                )}
                onClick={() => item.action()}
                onMouseEnter={() => setSelectedIndex(i)}
              >
                <span className="text-muted-foreground">{item.icon}</span>
                <span className="flex-1">{item.label}</span>
                {item.description && (
                  <span className="text-xs text-muted-foreground">{item.description}</span>
                )}
                <ArrowRight className="h-3 w-3 text-muted-foreground opacity-0 group-hover:opacity-100" />
              </button>
            ))
          )}
        </div>

        {/* Footer */}
        <div className="flex items-center gap-4 px-4 py-2 border-t border-border text-xs text-muted-foreground">
          <span>↑↓ 导航</span>
          <span>↵ 执行</span>
          <span>ESC 关闭</span>
        </div>
      </div>
    </div>
  );
}

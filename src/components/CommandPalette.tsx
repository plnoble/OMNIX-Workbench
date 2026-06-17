import { useCallback, useEffect, useRef, useState, type KeyboardEvent, type ReactNode } from "react";
import {
  ArrowRight,
  Bot,
  Brain,
  BookOpen,
  Clock,
  Database,
  FlaskConical,
  GitCompare,
  MessageSquare,
  Plug,
  Search,
  Settings,
  Sparkles,
  Users,
  Zap,
} from "lucide-react";

import { cn } from "@/lib/utils";

interface CommandItem {
  id: string;
  label: string;
  description?: string;
  icon: ReactNode;
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
  { id: "nav-work", label: "工作", description: "选择 Agent 后直接开始输入", icon: <MessageSquare className="h-4 w-4" />, tab: "work" },
  { id: "nav-team", label: "团队", description: "队长生成计划，确认后启动 Worker", icon: <Users className="h-4 w-4" />, tab: "team" },
  { id: "nav-agents", label: "智能体", description: "检测、安装、更新和模型绑定", icon: <Bot className="h-4 w-4" />, tab: "agents" },
  { id: "nav-skills", label: "技能", description: "管理技能包和同步目标", icon: <Sparkles className="h-4 w-4" />, tab: "skills" },
  { id: "nav-models", label: "模型中心", description: "供应商、API Key、模型列表和健康检查", icon: <Database className="h-4 w-4" />, tab: "models" },
  { id: "nav-knowledge", label: "知识库", description: "普通对话可手动启用的 RAG 资料源", icon: <BookOpen className="h-4 w-4" />, tab: "knowledge" },
  { id: "nav-search", label: "搜索", description: "搜索供应商和搜索调试", icon: <Search className="h-4 w-4" />, tab: "search" },
  { id: "nav-mcp", label: "MCP", description: "工具服务和 MCP Server", icon: <Plug className="h-4 w-4" />, tab: "mcp" },
  { id: "nav-memory", label: "Memory", description: "长期记忆和经验复用", icon: <Brain className="h-4 w-4" />, tab: "memories" },
  { id: "nav-labs", label: "Labs", description: "实验功能总览", icon: <FlaskConical className="h-4 w-4" />, tab: "labs" },
  { id: "nav-compare", label: "Compare", description: "模型对比实验", icon: <GitCompare className="h-4 w-4" />, tab: "compare" },
  { id: "nav-cron", label: "Cron", description: "定时任务", icon: <Clock className="h-4 w-4" />, tab: "cron" },
  { id: "nav-settings", label: "设置", description: "系统设置和数据备份", icon: <Settings className="h-4 w-4" />, tab: "settings" },
];

export function CommandPalette({ open, onClose, onNavigate, onToggleTheme }: CommandPaletteProps) {
  const [query, setQuery] = useState("");
  const [selectedIndex, setSelectedIndex] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);

  const commands: CommandItem[] = [
    ...NAV_COMMANDS.map((command) => ({
      ...command,
      category: "navigation" as const,
      action: () => {
        onNavigate(command.tab);
        onClose();
      },
    })),
    {
      id: "action-theme",
      label: "切换主题",
      description: "深色 / 浅色 / 跟随系统",
      icon: <Zap className="h-4 w-4" />,
      category: "action" as const,
      action: () => {
        onToggleTheme();
        onClose();
      },
    },
  ];

  const filtered = query.trim()
    ? commands.filter((command) =>
        command.label.toLowerCase().includes(query.toLowerCase()) ||
        (command.description && command.description.toLowerCase().includes(query.toLowerCase()))
      )
    : commands;

  useEffect(() => {
    setSelectedIndex(0);
  }, [query]);

  useEffect(() => {
    if (!open) return;
    setQuery("");
    const focusTimer = setTimeout(() => inputRef.current?.focus(), 50);
    return () => clearTimeout(focusTimer);
  }, [open]);

  const handleKeyDown = useCallback((event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "ArrowDown") {
      event.preventDefault();
      setSelectedIndex((prev) => Math.min(prev + 1, filtered.length - 1));
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      setSelectedIndex((prev) => Math.max(prev - 1, 0));
    } else if (event.key === "Enter") {
      event.preventDefault();
      filtered[selectedIndex]?.action();
    } else if (event.key === "Escape") {
      onClose();
    }
  }, [filtered, selectedIndex, onClose]);

  if (!open) return null;

  return (
    <div className="fixed inset-0 z-[100] flex items-start justify-center pt-[20vh]">
      <div className="absolute inset-0 bg-black/40" onClick={onClose} />

      <div className="relative w-[560px] overflow-hidden rounded-md border border-border bg-card shadow-xl animate-fade-in">
        <div className="flex items-center gap-3 border-b border-border px-4 py-3">
          <Search className="h-4 w-4 text-muted-foreground" />
          <input
            ref={inputRef}
            type="text"
            className="flex-1 bg-transparent text-sm outline-none placeholder:text-muted-foreground"
            placeholder="搜索页面或操作..."
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            onKeyDown={handleKeyDown}
          />
          <kbd className="rounded border border-border px-1.5 py-0.5 text-xs text-muted-foreground">ESC</kbd>
        </div>

        <div className="max-h-[340px] overflow-y-auto py-1">
          {filtered.length === 0 ? (
            <div className="py-6 text-center text-sm text-muted-foreground">没有匹配结果</div>
          ) : (
            filtered.map((item, index) => (
              <button
                key={item.id}
                className={cn(
                  "flex w-full items-center gap-3 px-4 py-2 text-left text-sm transition-colors",
                  index === selectedIndex ? "bg-primary/10 text-primary" : "hover:bg-muted/30"
                )}
                onClick={() => item.action()}
                onMouseEnter={() => setSelectedIndex(index)}
              >
                <span className="text-muted-foreground">{item.icon}</span>
                <span className="min-w-0 flex-1 truncate">{item.label}</span>
                {item.description && (
                  <span className="max-w-[220px] truncate text-xs text-muted-foreground">{item.description}</span>
                )}
                <ArrowRight className="h-3 w-3 text-muted-foreground" />
              </button>
            ))
          )}
        </div>

        <div className="flex items-center gap-4 border-t border-border px-4 py-2 text-xs text-muted-foreground">
          <span>上下键导航</span>
          <span>Enter 执行</span>
          <span>Esc 关闭</span>
        </div>
      </div>
    </div>
  );
}

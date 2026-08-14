import { useCallback, useEffect, useRef, useState, type KeyboardEvent, type ReactNode } from "react";
import {
  ArrowRight,
  Database,
  FlaskConical,
  MessageSquare,
  Search,
  Settings,
  Sparkles,
  Zap,
} from "lucide-react";

import { cn } from "@/lib/utils";
import { APP_ENTRIES } from "@/lib/appRegistry";

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

/**
 * 导航项**由 `APP_ENTRIES` 生成**，不再手写一份。
 *
 * 上一版是手抄清单，于是它和真实页面漂开了：有一条 `tab: "labs"`——`labs` 在
 * appRegistry 里只是**分组名**不是页面 id，点进去主区一片空白；同时漏掉了
 * 「对话」「办公」「监控」这些每天都用的页。Ctrl+K 本该是熟手入口，结果是一张
 * 过期站点地图。
 *
 * 图标按分组给：`AppEntry` 里没有图标字段，与其为此再手抄一份 id→图标的映射
 * （那就是同一个错误换个地方），不如按 group 分五种。
 */
const GROUP_ICONS: Record<string, ReactNode> = {
  core: <MessageSquare className="h-4 w-4" />,
  resource: <Database className="h-4 w-4" />,
  assistant: <Sparkles className="h-4 w-4" />,
  labs: <FlaskConical className="h-4 w-4" />,
  system: <Settings className="h-4 w-4" />,
};

const NAV_COMMANDS = APP_ENTRIES.filter((entry) => entry.placement !== "hidden").map((entry) => ({
  id: `nav-${entry.id}`,
  label: entry.label,
  description: entry.description,
  icon: GROUP_ICONS[entry.group] ?? <Settings className="h-4 w-4" />,
  tab: entry.id,
}));

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

      <div className="relative w-[560px] overflow-hidden rounded-md border border-border glass-surface shadow-xl animate-fade-in">
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

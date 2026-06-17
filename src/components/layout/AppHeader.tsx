import { useMemo, useState } from "react";
import {
  ChevronDown,
  ChevronLeft,
  ChevronRight,
  Eye,
  EyeOff,
  Grid3X3,
  Monitor,
  Moon,
  Pin,
  RotateCcw,
  Settings,
  Sun,
} from "lucide-react";

import { Button } from "@/components/ui/button";
import { APP_ICON_MAP } from "@/lib/appRegistry";
import { cn } from "@/lib/utils";
import type { AppEntry, GatewayStatus, NavigationPlacement } from "@/types";

type NavigationDirection = "left" | "right";

interface AppHeaderProps {
  activeTab: string;
  activeAgent: string;
  chatWorkspace: string;
  gatewayStatus: GatewayStatus;
  pinnedEntries: AppEntry[];
  launcherEntries: AppEntry[];
  hiddenEntries: AppEntry[];
  themeMode: "dark" | "light" | "auto";
  showPreviewButton: boolean;
  isPreviewOpen: boolean;
  onNavigate: (tab: string) => void;
  onMoveEntry: (id: string, placement: NavigationPlacement) => void;
  onReorderEntry: (id: string, direction: NavigationDirection) => void;
  onResetNavigation: () => void;
  onToggleTheme: () => void;
  onTogglePreview: () => void;
}

const GROUP_LABELS: Record<AppEntry["group"], string> = {
  core: "核心",
  resource: "资源",
  assistant: "助手",
  labs: "Labs",
  system: "系统",
};

function AppIcon({ id, className }: { id: string; className?: string }) {
  const Icon = APP_ICON_MAP[id as keyof typeof APP_ICON_MAP] ?? Grid3X3;
  return <Icon className={className ?? "h-4 w-4"} />;
}

function ThemeIcon({ mode }: { mode: "dark" | "light" | "auto" }) {
  if (mode === "light") return <Sun className="h-4 w-4" />;
  if (mode === "dark") return <Moon className="h-4 w-4" />;
  return <Monitor className="h-4 w-4" />;
}

export function AppHeader({
  activeTab,
  activeAgent,
  chatWorkspace,
  gatewayStatus,
  pinnedEntries,
  launcherEntries,
  hiddenEntries,
  themeMode,
  showPreviewButton,
  isPreviewOpen,
  onNavigate,
  onMoveEntry,
  onReorderEntry,
  onResetNavigation,
  onToggleTheme,
  onTogglePreview,
}: AppHeaderProps) {
  const [launcherOpen, setLauncherOpen] = useState(false);
  const activeEntry = [...pinnedEntries, ...launcherEntries, ...hiddenEntries].find((entry) => entry.id === activeTab);
  const workspaceLabel = chatWorkspace === "direct" ? "对话" : chatWorkspace.split(/[\\/]/).pop() || "工作区";

  const launcherGroups = useMemo(() => {
    return launcherEntries.reduce<Record<string, AppEntry[]>>((acc, entry) => {
      const label = GROUP_LABELS[entry.group];
      (acc[label] ||= []).push(entry);
      return acc;
    }, {});
  }, [launcherEntries]);

  const statusClass = {
    idle: "bg-success",
    busy: "bg-warning",
    error: "bg-destructive",
  }[gatewayStatus];

  return (
    <header className="relative z-40 border-b border-border bg-background/92 backdrop-blur-xl">
      <div className="flex h-14 items-center gap-3 px-3">
        <button
          className="flex h-10 min-w-0 items-center gap-2 rounded-md px-2 text-left hover:bg-muted/20"
          onClick={() => onNavigate("work")}
          title="回到工作"
        >
          <span className={cn("h-2 w-2 rounded-full", statusClass)} />
          <div className="min-w-0">
            <div className="truncate text-sm font-semibold">OMNIX</div>
            <div className="truncate text-[11px] text-muted-foreground">
              {workspaceLabel} · {activeAgent}
            </div>
          </div>
        </button>

        <nav className="flex min-w-0 flex-1 items-center gap-1 overflow-x-auto">
          {pinnedEntries.map((entry) => (
            <button
              key={entry.id}
              className={cn(
                "flex h-9 shrink-0 items-center gap-2 rounded-md border px-3 text-sm transition-colors",
                activeTab === entry.id
                  ? "border-primary/30 bg-primary/12 text-primary"
                  : "border-transparent text-muted-foreground hover:bg-muted/20 hover:text-foreground"
              )}
              onClick={() => onNavigate(entry.id)}
              title={entry.description}
            >
              <AppIcon id={entry.id} />
              <span>{entry.label}</span>
            </button>
          ))}

          <button
            className={cn(
              "flex h-10 w-10 shrink-0 items-center justify-center rounded-md border transition-colors",
              launcherOpen ? "border-primary/30 bg-primary/12 text-primary" : "border-border bg-card/40 hover:bg-muted/20"
            )}
            onClick={() => setLauncherOpen((open) => !open)}
            title="应用宫格"
            aria-label="打开应用宫格"
          >
            <Grid3X3 className="h-4 w-4" />
          </button>
        </nav>

        <div className="hidden min-w-0 items-center gap-2 lg:flex">
          {activeEntry && (
            <div className="max-w-64 truncate text-right">
              <div className="truncate text-sm font-medium">{activeEntry.title}</div>
              <div className="truncate text-[11px] text-muted-foreground">{activeEntry.description}</div>
            </div>
          )}
        </div>

        <div className="ml-auto flex shrink-0 items-center gap-1">
        {showPreviewButton && (
          <Button size="sm" variant="outline" onClick={onTogglePreview}>
            <Eye className="h-3.5 w-3.5" />
            {isPreviewOpen ? "关闭预览" : "预览"}
          </Button>
        )}

        <Button size="sm" variant="ghost" className="h-9 w-9 p-0" onClick={() => onNavigate("dashboard")} title="诊断">
          <Monitor className="h-4 w-4" />
        </Button>
        <Button size="sm" variant="ghost" className="h-9 w-9 p-0" onClick={onToggleTheme} title="切换主题">
          <ThemeIcon mode={themeMode} />
        </Button>
        <Button size="sm" variant="ghost" className="h-9 w-9 p-0" onClick={() => onNavigate("settings")} title="设置">
          <Settings className="h-4 w-4" />
        </Button>
        </div>
      </div>

      {launcherOpen && (
        <div className="absolute left-3 right-3 top-16 z-50 max-h-[calc(100vh-5rem)] overflow-y-auto rounded-md border border-border bg-popover p-4 shadow-2xl">
          <div className="mb-4 flex items-center justify-between gap-3">
            <div>
              <div className="text-sm font-semibold">应用宫格</div>
              <div className="text-xs text-muted-foreground">
                固定会显示在标题栏；收纳会留在这个宫格；隐藏只在下方隐藏区恢复。
              </div>
            </div>
            <Button size="sm" variant="outline" onClick={onResetNavigation}>
              <RotateCcw className="h-3.5 w-3.5" />
              恢复默认
            </Button>
          </div>

          <section className="mb-5">
            <div className="mb-2 flex items-center gap-2 text-xs font-semibold text-muted-foreground">
              <Pin className="h-3.5 w-3.5" />
              已固定到标题栏
            </div>
            <div className="grid grid-cols-1 gap-2 md:grid-cols-2 xl:grid-cols-4">
              {pinnedEntries.map((entry, index) => (
                <LauncherItem
                  key={entry.id}
                  entry={entry}
                  active={activeTab === entry.id}
                  onOpen={() => {
                    onNavigate(entry.id);
                    setLauncherOpen(false);
                  }}
                  onMove={onMoveEntry}
                  onReorder={onReorderEntry}
                  canMoveLeft={index > 0}
                  canMoveRight={index < pinnedEntries.length - 1}
                  actions={entry.id === "work" ? [] : ["launcher", "hidden"]}
                  launcherLabel="收纳到宫格"
                />
              ))}
            </div>
          </section>

          {Object.entries(launcherGroups).map(([group, entries]) => (
            <section key={group} className="mb-5">
              <div className="mb-2 flex items-center gap-2 text-xs font-semibold text-muted-foreground">
                <ChevronDown className="h-3.5 w-3.5" />
                {group}
              </div>
              <div className="grid grid-cols-1 gap-2 md:grid-cols-2 xl:grid-cols-4">
                {entries.map((entry) => (
                  <LauncherItem
                    key={entry.id}
                    entry={entry}
                    active={activeTab === entry.id}
                    onOpen={() => {
                      onNavigate(entry.id);
                      setLauncherOpen(false);
                    }}
                    onMove={onMoveEntry}
                    actions={["pinned", "hidden"]}
                  />
                ))}
              </div>
            </section>
          ))}

          {hiddenEntries.length > 0 && (
            <section>
              <div className="mb-2 flex items-center gap-2 text-xs font-semibold text-muted-foreground">
                <EyeOff className="h-3.5 w-3.5" />
                已隐藏
              </div>
              <div className="grid grid-cols-1 gap-2 md:grid-cols-2 xl:grid-cols-4">
                {hiddenEntries.map((entry) => (
                  <LauncherItem
                    key={entry.id}
                    entry={entry}
                    active={activeTab === entry.id}
                    onOpen={() => {
                      onNavigate(entry.id);
                      setLauncherOpen(false);
                    }}
                    onMove={onMoveEntry}
                    actions={["pinned", "launcher"]}
                    launcherLabel="恢复到宫格"
                  />
                ))}
              </div>
            </section>
          )}
        </div>
      )}
    </header>
  );
}

function LauncherItem({
  entry,
  active,
  onOpen,
  onMove,
  onReorder,
  actions,
  launcherLabel = "收纳",
  canMoveLeft = false,
  canMoveRight = false,
}: {
  entry: AppEntry;
  active: boolean;
  onOpen: () => void;
  onMove: (id: string, placement: NavigationPlacement) => void;
  onReorder?: (id: string, direction: NavigationDirection) => void;
  actions: NavigationPlacement[];
  launcherLabel?: string;
  canMoveLeft?: boolean;
  canMoveRight?: boolean;
}) {
  return (
    <div className={cn("rounded-md border p-3", active ? "border-primary/40 bg-primary/10" : "border-border bg-card/40")}>
      <button className="flex w-full items-start gap-3 text-left" onClick={onOpen}>
        <div className="flex h-10 w-10 shrink-0 items-center justify-center rounded-md bg-muted/30">
          <AppIcon id={entry.id} className="h-5 w-5" />
        </div>
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            <span className="truncate text-sm font-semibold">{entry.label}</span>
            {entry.is_experimental && (
              <span className="rounded border border-warning/30 px-1.5 py-0.5 text-[10px] text-warning">Labs</span>
            )}
            {entry.is_incomplete && (
              <span className="rounded border border-muted-foreground/30 px-1.5 py-0.5 text-[10px] text-muted-foreground">
                Incomplete
              </span>
            )}
          </div>
          <p className="mt-1 line-clamp-2 text-xs leading-5 text-muted-foreground">{entry.description}</p>
        </div>
      </button>

      <div className="mt-3 flex flex-wrap gap-1.5">
        {onReorder && (
          <>
            <Button
              size="sm"
              variant="ghost"
              className="h-7 px-2 text-xs"
              onClick={() => onReorder(entry.id, "left")}
              disabled={!canMoveLeft}
              title="在标题栏前移"
            >
              <ChevronLeft className="h-3 w-3" />
              前移
            </Button>
            <Button
              size="sm"
              variant="ghost"
              className="h-7 px-2 text-xs"
              onClick={() => onReorder(entry.id, "right")}
              disabled={!canMoveRight}
              title="在标题栏后移"
            >
              后移
              <ChevronRight className="h-3 w-3" />
            </Button>
          </>
        )}
        {actions.includes("pinned") && (
          <Button size="sm" variant="ghost" className="h-7 px-2 text-xs" onClick={() => onMove(entry.id, "pinned")}>
            <Pin className="h-3 w-3" />
            固定
          </Button>
        )}
        {actions.includes("launcher") && (
          <Button size="sm" variant="ghost" className="h-7 px-2 text-xs" onClick={() => onMove(entry.id, "launcher")}>
            <Grid3X3 className="h-3 w-3" />
            {launcherLabel}
          </Button>
        )}
        {actions.includes("hidden") && (
          <Button size="sm" variant="ghost" className="h-7 px-2 text-xs" onClick={() => onMove(entry.id, "hidden")}>
            <EyeOff className="h-3 w-3" />
            隐藏
          </Button>
        )}
      </div>
    </div>
  );
}

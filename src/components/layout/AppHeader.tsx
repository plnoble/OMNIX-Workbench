import { useEffect, useMemo, useRef, useState } from "react";
import { usePolling } from "@/hooks/usePolling";
import {
  ChevronDown,
  ChevronLeft,
  ChevronRight,
  Eye,
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
import { circuitBreakerApi } from "@/lib/tauri-api";
import type { AppEntry, NavigationPlacement } from "@/types";

type NavigationDirection = "left" | "right";

interface AppHeaderProps {
  activeTab: string;
  activeAgent: string;
  chatWorkspace: string;
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
  labs: "实验室",
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
  // 宫格两态：默认「使用」只有搜索+分类点开；「自定义」才显示固定/收纳/排序按钮。
  const [launcherEditMode, setLauncherEditMode] = useState(false);
  const [launcherQuery, setLauncherQuery] = useState("");
  const launcherToggleRef = useRef<HTMLButtonElement>(null);
  const launcherPanelRef = useRef<HTMLDivElement>(null);
  const activeEntry = [...pinnedEntries, ...launcherEntries, ...hiddenEntries].find((entry) => entry.id === activeTab);
  const workspaceLabel = chatWorkspace === "direct" ? "对话" : chatWorkspace.split(/[\\/]/).pop() || "工作区";

  // Close the launcher on outside click or Escape.
  useEffect(() => {
    if (!launcherOpen) return;
    const handlePointerDown = (event: MouseEvent) => {
      const target = event.target as Node | null;
      if (
        !launcherPanelRef.current?.contains(target) &&
        !launcherToggleRef.current?.contains(target)
      ) {
        setLauncherOpen(false);
      }
    };
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") setLauncherOpen(false);
    };
    document.addEventListener("mousedown", handlePointerDown);
    document.addEventListener("keydown", handleKeyDown);
    return () => {
      document.removeEventListener("mousedown", handlePointerDown);
      document.removeEventListener("keydown", handleKeyDown);
    };
  }, [launcherOpen]);

  const launcherGroups = useMemo(() => {
    return launcherEntries.reduce<Record<string, AppEntry[]>>((acc, entry) => {
      const label = GROUP_LABELS[entry.group];
      (acc[label] ||= []).push(entry);
      return acc;
    }, {});
  }, [launcherEntries]);

  // Search across every entry (pinned + grid) by label/title/description.
  const launcherMatches = useMemo(() => {
    const q = launcherQuery.trim().toLowerCase();
    if (!q) return null;
    return [...pinnedEntries, ...launcherEntries].filter((entry) =>
      [entry.label, entry.title, entry.description].some((text) => text.toLowerCase().includes(q))
    );
  }, [launcherQuery, pinnedEntries, launcherEntries]);

  // 这个点以前显示的是 `gatewayStatus`——而那个值由 `saveSettings` 置 busy/idle，
  // 也就是「设置正在保存吗」。它长在网关图标上、几乎永远是绿的，日常最显眼的
  // 状态灯却和网关无关；真实健康在另一页 10 秒轮询一次。
  //
  // 现在接真的：读熔断器状态，有平台被熔断（Open）就红，半开就黄。保存设置的
  // 反馈已经由 SystemSubTab 的 toast 承担，不需要这个点兼任。
  const [gatewayHealth, setGatewayHealth] = useState<"ok" | "degraded" | "down">("ok");
  usePolling(async () => {
    try {
      const rows = await circuitBreakerApi.getStatus();
      if (rows.some((r) => r.state === "Open")) setGatewayHealth("down");
      else if (rows.some((r) => r.state === "HalfOpen")) setGatewayHealth("degraded");
      else setGatewayHealth("ok");
    } catch {
      // 拿不到状态就别乱报警——保持上一次的结论。
    }
  }, 30_000);

  const statusClass = {
    ok: "bg-success",
    degraded: "bg-warning",
    down: "bg-destructive",
  }[gatewayHealth];
  const statusTitle = {
    ok: "网关正常：没有平台被熔断",
    degraded: "网关半开：有平台正在试探性恢复",
    down: "网关异常：有平台已被熔断，请到模型中心查看",
  }[gatewayHealth];

  return (
    <header className="glass-chrome relative z-40 border-b">
      <div className="flex h-14 items-center gap-3 px-3">
        <button
          className="flex h-10 min-w-0 items-center gap-2 rounded-md px-2 text-left hover:bg-muted/20"
          onClick={() => onNavigate("work")}
          title="回到工作"
        >
          <span className="relative h-7 w-7 shrink-0">
            <img
              src="/omnix-workbench-icon.png"
              alt=""
              aria-hidden="true"
              className="h-7 w-7 rounded-md"
            />
            <span
              className={cn(
                "absolute -bottom-0.5 -right-0.5 h-2.5 w-2.5 rounded-full border-2 border-background",
                statusClass
              )}
              title={statusTitle}
            />
          </span>
          <div className="hidden min-w-0 min-[1500px]:block">
            <div className="truncate text-sm font-semibold">OMNIX</div>
            <div className="hidden truncate text-[11px] text-muted-foreground min-[1120px]:block">
              {workspaceLabel} · {activeAgent}
            </div>
          </div>
        </button>

        <nav className="flex min-w-0 flex-1 items-center gap-1 overflow-x-auto">
          {pinnedEntries.map((entry) => (
            <button
              key={entry.id}
              className={cn(
                "flex h-9 shrink-0 items-center gap-2 rounded-md border px-2 text-sm transition-colors min-[1500px]:px-3",
                activeTab === entry.id
                  ? "border-primary/30 bg-primary/12 text-primary"
                  : "border-transparent text-muted-foreground hover:bg-muted/20 hover:text-foreground"
              )}
              onClick={() => onNavigate(entry.id)}
              title={entry.description}
              aria-label={entry.label}
            >
              <AppIcon id={entry.id} />
              <span className="hidden min-[1024px]:inline">{entry.label}</span>
            </button>
          ))}

          <button
            ref={launcherToggleRef}
            className={cn(
              "flex h-10 w-10 shrink-0 items-center justify-center rounded-md border transition-colors",
              launcherOpen ? "border-primary/30 bg-primary/12 text-primary" : "border-border glass-surface hover:bg-muted/20"
            )}
            onClick={() => setLauncherOpen((open) => !open)}
            title="应用宫格"
            aria-label="打开应用宫格"
            aria-expanded={launcherOpen}
          >
            <Grid3X3 className="h-4 w-4" />
          </button>
        </nav>

        <div className="hidden min-w-0 items-center gap-2 min-[1600px]:flex">
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

        <Button size="sm" variant="ghost" className="h-9 w-9 p-0" onClick={onToggleTheme} title="切换主题">
          <ThemeIcon mode={themeMode} />
        </Button>
        <Button size="sm" variant="ghost" className="h-9 w-9 p-0" onClick={() => onNavigate("settings")} title="设置">
          <Settings className="h-4 w-4" />
        </Button>
        </div>
      </div>

      {launcherOpen && (
        <div
          ref={launcherPanelRef}
          className="absolute left-3 right-3 top-16 z-50 max-h-[calc(100vh-5rem)] overflow-y-auto rounded-md border border-border bg-popover p-4 shadow-2xl"
        >
          <div className="mb-3 flex items-center justify-between gap-3">
            <div>
              <div className="text-sm font-semibold">应用宫格</div>
              <div className="text-xs text-muted-foreground">
                {launcherEditMode ? "调整固定、收纳与排序；完成后回到使用模式。" : "点击打开应用；「自定义」可调整导航。"}
              </div>
            </div>
            <div className="flex items-center gap-2">
              {launcherEditMode && (
                <Button size="sm" variant="outline" onClick={onResetNavigation}>
                  <RotateCcw className="h-3.5 w-3.5" />
                  恢复默认
                </Button>
              )}
              <Button size="sm" variant={launcherEditMode ? "default" : "outline"} onClick={() => setLauncherEditMode((v) => !v)}>
                {launcherEditMode ? "完成" : "自定义"}
              </Button>
            </div>
          </div>

          {!launcherEditMode && (
            <input
              value={launcherQuery}
              onChange={(event) => setLauncherQuery(event.target.value)}
              placeholder="搜索应用…"
              className="mb-4 h-9 w-full rounded-md border border-border bg-background px-3 text-sm outline-none placeholder:text-muted-foreground/60 focus:border-primary/50"
              aria-label="搜索应用"
            />
          )}

          {/* Search results replace the sections while a query is active. */}
          {!launcherEditMode && launcherMatches ? (
            <div className="grid grid-cols-1 gap-2 md:grid-cols-2 xl:grid-cols-4">
              {launcherMatches.length === 0 ? (
                <div className="col-span-full py-6 text-center text-xs text-muted-foreground">没有匹配「{launcherQuery}」的应用</div>
              ) : (
                launcherMatches.map((entry) => (
                  <LauncherItem
                    key={entry.id}
                    entry={entry}
                    active={activeTab === entry.id}
                    editable={false}
                    onOpen={() => {
                      onNavigate(entry.id);
                      setLauncherOpen(false);
                    }}
                    onMove={onMoveEntry}
                    actions={[]}
                  />
                ))
              )}
            </div>
          ) : (
            <>
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
                      editable={launcherEditMode}
                      onOpen={() => {
                        onNavigate(entry.id);
                        setLauncherOpen(false);
                      }}
                      onMove={onMoveEntry}
                      onReorder={onReorderEntry}
                      canMoveLeft={index > 0}
                      canMoveRight={index < pinnedEntries.length - 1}
                      actions={entry.id === "work" ? [] : ["launcher"]}
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
                        editable={launcherEditMode}
                        onOpen={() => {
                          onNavigate(entry.id);
                          setLauncherOpen(false);
                        }}
                        onMove={onMoveEntry}
                        actions={["pinned"]}
                      />
                    ))}
                  </div>
                </section>
              ))}
            </>
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
  editable = true,
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
  /** 使用模式下隐藏移动/固定/收纳按钮，卡片只负责打开。 */
  editable?: boolean;
}) {
  return (
    <div className={cn("rounded-md border p-3", active ? "border-primary/40 bg-primary/10" : "border-border glass-surface")}>
      <button className="flex w-full items-start gap-3 text-left" onClick={onOpen}>
        <div className="flex h-10 w-10 shrink-0 items-center justify-center rounded-md bg-muted/30">
          <AppIcon id={entry.id} className="h-5 w-5" />
        </div>
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            <span className="truncate text-sm font-semibold">{entry.label}</span>
            {entry.is_experimental && (
              <span className="rounded border border-warning/30 px-1.5 py-0.5 text-[10px] text-warning">实验</span>
            )}
            {entry.is_incomplete && (
              <span className="rounded border border-muted-foreground/30 px-1.5 py-0.5 text-[10px] text-muted-foreground">
                未完成
              </span>
            )}
          </div>
          <p className="mt-1 line-clamp-2 text-xs leading-5 text-muted-foreground">{entry.description}</p>
        </div>
      </button>

      {editable && (
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
      </div>
      )}
    </div>
  );
}

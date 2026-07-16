/**
 * OfficeTab — Office 工作台（合并项，不是加法）。
 *
 * 演示 / 文档 / 表格三个子工作台收进一个入口：顶部一条极简模式栏切换，
 * 子工作台首次进入后保活（切换不丢编辑状态）；「概览」是共用首页——
 * 三个模块的近期项 + 共用底座（OfficeCLI 引擎状态、质检/导入/母版/批量
 * 这些贯穿能力的一句话说明）。启动器只暴露这一个入口。
 */
import { useCallback, useEffect, useState } from "react";
import {
  FileText,
  LayoutGrid,
  Loader2,
  PenLine,
  Presentation,
  Table2,
  Wrench,
} from "lucide-react";
import { toast } from "sonner";

import { cn } from "@/lib/utils";
import {
  officeApi,
  settingsApi,
  slidesApi,
  writeApi,
  type DeckMeta,
  type OfficeStatus,
  type WriteFile,
} from "@/lib/tauri-api";
import { SlidesTab } from "@/components/tabs/SlidesTab";
import { WriteTab } from "@/components/tabs/WriteTab";
import { ExcelTab } from "@/components/tabs/ExcelTab";

type OfficeMode = "home" | "slides" | "write" | "excel";

const MODES: { id: OfficeMode; label: string; icon: typeof LayoutGrid }[] = [
  { id: "home", label: "概览", icon: LayoutGrid },
  { id: "slides", label: "演示", icon: Presentation },
  { id: "write", label: "文档", icon: PenLine },
  { id: "excel", label: "表格", icon: Table2 },
];

const LAST_MODE_KEY = "omnix_office_mode";

export function OfficeTab({ defaultMode }: { defaultMode?: OfficeMode } = {}) {
  const [mode, setMode] = useState<OfficeMode>(() => {
    if (defaultMode) return defaultMode;
    const saved = localStorage.getItem(LAST_MODE_KEY) as OfficeMode | null;
    return saved && MODES.some((m) => m.id === saved) ? saved : "home";
  });
  // 保活：进过的子工作台保持挂载，切换不丢状态。
  const [visited, setVisited] = useState<Set<OfficeMode>>(() => new Set([mode]));

  const switchMode = (next: OfficeMode) => {
    setMode(next);
    setVisited((prev) => (prev.has(next) ? prev : new Set(prev).add(next)));
    localStorage.setItem(LAST_MODE_KEY, next);
  };

  return (
    <div className="flex h-full flex-1 flex-col overflow-hidden bg-background">
      {/* 极简模式栏 */}
      <div className="flex h-11 shrink-0 items-center gap-1 border-b border-border px-4">
        <FileText className="mr-1 h-4 w-4 text-primary" />
        <span className="mr-2 text-sm font-semibold">Office</span>
        {MODES.map(({ id, label, icon: Icon }) => (
          <button
            key={id}
            onClick={() => switchMode(id)}
            className={cn(
              "inline-flex h-8 items-center gap-1.5 rounded-md px-3 text-sm transition",
              mode === id
                ? "bg-accent text-accent-foreground"
                : "text-muted-foreground hover:bg-muted/40 hover:text-foreground",
            )}
          >
            <Icon className="h-4 w-4" /> {label}
          </button>
        ))}
      </div>

      <div className="relative min-h-0 flex-1">
        {mode === "home" && <OfficeHome onEnter={switchMode} />}
        {visited.has("slides") && (
          <div className={cn("absolute inset-0", mode !== "slides" && "hidden")}>
            <SlidesTab />
          </div>
        )}
        {visited.has("write") && (
          <div className={cn("absolute inset-0", mode !== "write" && "hidden")}>
            <WriteTab />
          </div>
        )}
        {visited.has("excel") && (
          <div className={cn("absolute inset-0", mode !== "excel" && "hidden")}>
            <ExcelTab />
          </div>
        )}
      </div>
    </div>
  );
}

/** 概览首页：三模块近期项 + 共用底座。 */
function OfficeHome({ onEnter }: { onEnter: (mode: OfficeMode) => void }) {
  const [decks, setDecks] = useState<DeckMeta[]>([]);
  const [docs, setDocs] = useState<WriteFile[]>([]);
  const [sheets, setSheets] = useState<string[]>([]);
  const [engine, setEngine] = useState<OfficeStatus | null>(null);
  const [installing, setInstalling] = useState(false);

  const load = useCallback(async () => {
    slidesApi.list().then((list) => setDecks(list.slice(0, 5))).catch(() => {});
    writeApi
      .listSpaces()
      .then((spaces) => (spaces[0] ? writeApi.listFiles(spaces[0].path) : []))
      .then((files) => setDocs(files.slice(0, 5)))
      .catch(() => {});
    settingsApi
      .get("office_recent_files")
      .then((raw) => {
        const list = raw ? (JSON.parse(raw) as string[]) : [];
        setSheets(list.slice(0, 5));
      })
      .catch(() => {});
    officeApi.status().then(setEngine).catch(() => {});
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  const installEngine = async () => {
    setInstalling(true);
    try {
      const path = await officeApi.install();
      toast.success("OfficeCLI 已就绪", { description: path });
      await load();
    } catch (e) {
      toast.error(`安装失败：${e}`);
    } finally {
      setInstalling(false);
    }
  };

  const moduleCard = (
    modeId: OfficeMode,
    icon: React.ReactNode,
    title: string,
    tagline: string,
    items: { key: string; label: string; hint?: string }[],
    empty: string,
  ) => (
    <button
      onClick={() => onEnter(modeId)}
      className="flex min-h-52 flex-col rounded-xl border border-border bg-card/40 p-4 text-left transition hover:border-primary/60"
    >
      <div className="flex items-center gap-2">
        {icon}
        <span className="text-sm font-semibold">{title}</span>
        <span className="ml-auto text-xs text-primary">进入 →</span>
      </div>
      <p className="mt-1 text-xs leading-5 text-muted-foreground">{tagline}</p>
      <div className="mt-3 flex flex-col gap-1.5">
        {items.length === 0 ? (
          <span className="text-xs text-muted-foreground/70">{empty}</span>
        ) : (
          items.map((item) => (
            <div key={item.key} className="flex items-baseline gap-2 text-xs">
              <span className="truncate">{item.label}</span>
              {item.hint && <span className="ml-auto shrink-0 text-muted-foreground/70">{item.hint}</span>}
            </div>
          ))
        )}
      </div>
    </button>
  );

  return (
    <div className="flex h-full flex-col gap-4 overflow-y-auto p-6">
      <div className="grid grid-cols-1 gap-4 lg:grid-cols-3">
        {moduleCard(
          "slides",
          <Presentation className="h-4 w-4 text-primary" />,
          "演示",
          "结构化模型：大纲两阶段 → 单页精修 → 配图/母版 → 放映；导出 PDF / HTML / PPTX（带质检门），可导入现有 PPTX。",
          decks.map((d) => ({ key: d.id, label: d.title, hint: `${d.slide_count} 页` })),
          "还没有演示——进入后用 AI 生成或导入 PPTX。",
        )}
        {moduleCard(
          "write",
          <PenLine className="h-4 w-4 text-primary" />,
          "文档",
          "Markdown 为源：AI 长文两阶段、润色/续写；导出带样式 Word（品牌母版）、导入 docx、模板批量生成。",
          docs.map((f) => ({ key: f.relative_path, label: f.name })),
          "还没有文档——进入后新建或导入 docx。",
        )}
        {moduleCard(
          "excel",
          <Table2 className="h-4 w-4 text-primary" />,
          "表格",
          "真实 .xlsx 为源：AI 指令 → 受校验批量操作 → 公式即时求值；CSV 导入；docx / pptx 只读预览。",
          sheets.map((p) => ({ key: p, label: p.split(/[\\/]/).pop() ?? p })),
          "还没有近期表格——进入后新建或打开。",
        )}
      </div>

      {/* 共用底座 */}
      <div className="rounded-xl border border-border bg-card/40 p-4">
        <div className="flex items-center gap-2">
          <Wrench className="h-4 w-4 text-primary" />
          <span className="text-sm font-semibold">共用底座 · OfficeCLI 引擎</span>
          {engine &&
            (engine.installed ? (
              <span className="rounded border border-success/40 bg-success/10 px-1.5 py-0.5 text-[11px] text-success">
                已就绪 {engine.version ?? ""}
                {engine.kind === "system" ? "（系统副本）" : ""}
              </span>
            ) : (
              <span className="rounded border border-warning/40 bg-warning/10 px-1.5 py-0.5 text-[11px] text-warning">
                未安装
              </span>
            ))}
          {engine && (!engine.installed || engine.update_available) && (
            <button
              onClick={() => void installEngine()}
              disabled={installing}
              className="ml-auto inline-flex h-7 items-center gap-1.5 rounded-md bg-primary px-3 text-xs font-medium text-primary-foreground hover:opacity-90 disabled:opacity-50"
            >
              {installing ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : null}
              {engine.installed ? `更新到托管版 ${engine.pinned_version}` : "一键安装"}
            </button>
          )}
        </div>
        <p className="mt-2 text-xs leading-5 text-muted-foreground">
          Word / 表格的读写、PPTX 导出质检与导入、docx / pptx 预览都由它驱动（单文件，免装 Office）。
          {engine?.skill_pool === "official"
            ? " officecli 技能已在正式池：所有 agent 聊天里可直接做 Office 活。"
            : engine?.skill_pool === "pending"
              ? " officecli 技能在待定池——到「技能中心」审核转正后，所有 agent 聊天里都能直接做 Office 活。"
              : ""}
        </p>
      </div>
    </div>
  );
}

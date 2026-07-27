/** Split from ChatTab.tsx — pure move, no behavior change. */
import { useEffect, useRef, useState } from "react";
import { AlertTriangle, Brain, Check, ChevronDown, Shield, Sparkles } from "lucide-react";
import { Button } from "@/components/ui/button";
import { AGENT_NAMES } from "@/lib/constants";
import { cn } from "@/lib/utils";
import type { DetectedAgent, KnowledgeBase, RuntimeApprovalRequest, SearchResult } from "@/types";

export function AgentStrip({
  activeAgent,
  detectedAgents,
  onSelectAgent,
}: {
  activeAgent: string;
  detectedAgents: DetectedAgent[];
  onSelectAgent: (name: string) => void;
}) {
  // 8 个横向药丸挤爆一行 → 只露 3 个（活跃优先、已安装其次），其余进「更多」。
  const [moreOpen, setMoreOpen] = useState(false);
  const isInstalled = (name: string) =>
    detectedAgents.find((item) => item.name === name)?.status === "installed";
  const rank = (name: string) => (name === activeAgent ? 0 : isInstalled(name) ? 1 : 2);
  const ordered = [...AGENT_NAMES].sort((a, b) => rank(a) - rank(b));
  const visible = ordered.slice(0, 3);
  const rest = ordered.slice(3);

  const pill = (name: string) => {
    const installed = isInstalled(name);
    const active = activeAgent === name;
    return (
      <button
        key={name}
        onClick={() => onSelectAgent(name)}
        className={cn(
          "flex h-9 shrink-0 items-center gap-2 rounded-full border px-3 text-sm",
          active ? "border-primary/40 bg-primary/12 text-primary" : "border-border glass-surface text-muted-foreground hover:text-foreground"
        )}
        title={installed ? `${name} · 已就绪` : `${name} · 未安装`}
      >
        <span aria-hidden="true" className={cn("h-2 w-2 rounded-full", installed ? "bg-success" : "bg-muted-foreground")} />
        {name}
      </button>
    );
  };

  return (
    <div className="flex min-w-0 flex-1 items-center gap-2">
      {visible.map(pill)}
      {rest.length > 0 && (
        <div className="relative">
          <button
            type="button"
            onClick={() => setMoreOpen((open) => !open)}
            className={cn(
              "flex h-9 shrink-0 items-center gap-1 rounded-full border px-3 text-sm",
              moreOpen ? "border-primary/40 bg-primary/12 text-primary" : "border-border glass-surface text-muted-foreground hover:text-foreground"
            )}
            aria-expanded={moreOpen}
          >
            更多
            <ChevronDown className="h-3.5 w-3.5" />
          </button>
          {moreOpen && (
            <div className="absolute left-0 top-full z-50 mt-1 w-56 rounded-md border border-border bg-popover py-1 shadow-lg">
              {rest.map((name) => {
                const installed = isInstalled(name);
                return (
                  <button
                    key={name}
                    type="button"
                    onClick={() => {
                      onSelectAgent(name);
                      setMoreOpen(false);
                    }}
                    className="flex w-full items-center gap-2 px-3 py-2 text-left text-sm hover:bg-muted/30"
                  >
                    <span aria-hidden="true" className={cn("h-2 w-2 rounded-full", installed ? "bg-success" : "bg-muted-foreground")} />
                    <span className="min-w-0 flex-1 truncate">{name}</span>
                    <span className={cn("shrink-0 text-xs", installed ? "text-success" : "text-muted-foreground")}>
                      {installed ? "已就绪" : "未安装"}
                    </span>
                  </button>
                );
              })}
            </div>
          )}
        </div>
      )}
    </div>
  );
}

export function FirstScreen({ activeAgent, installed, onPrompt, onRedetect }: { activeAgent: string; installed: boolean; onPrompt: (prompt: string) => void; onRedetect?: () => Promise<void> }) {
  const [redetecting, setRedetecting] = useState(false);
  return (
    <div className="first-screen mx-auto flex min-h-full max-w-4xl flex-col items-center justify-center px-6 py-6 text-center">
      <div className="first-screen-icon mb-5 flex h-16 w-16 items-center justify-center rounded-md border border-border glass-surface">
        <Sparkles className="h-8 w-8 text-primary" />
      </div>
      <h2 className="first-screen-title m-0 text-3xl font-semibold">今天让 {activeAgent} 做什么？</h2>
      <p className="first-screen-description mt-3 max-w-2xl text-sm leading-6 text-muted-foreground">
        先选择 Agent，再直接输入任务。复杂任务可以转团队；普通问答可以手动接入知识库。
      </p>
      {!installed && (
        <div className="mt-4 flex items-center gap-2 rounded-md border border-warning/30 bg-warning/10 px-3 py-2 text-sm text-warning">
          <AlertTriangle className="h-4 w-4" />
          当前 Agent 未检测到——如果你刚在智能体页装好，点一下重新检测。
          {onRedetect && (
            <button
              className="ml-1 inline-flex items-center gap-1 rounded border border-warning/40 px-2 py-0.5 text-xs hover:bg-warning/20"
              disabled={redetecting}
              onClick={() => {
                setRedetecting(true);
                void onRedetect().finally(() => setRedetecting(false));
              }}
            >
              {redetecting ? "检测中…" : "重新检测"}
            </button>
          )}
        </div>
      )}
      <div className="first-screen-suggestions mt-7 grid w-full grid-cols-1 gap-2 md:grid-cols-3">
        {[
          ["盘点项目结构", "读取当前工作区，给我总结项目结构、关键模块和下一步重构建议。"],
          ["修复一个问题", "帮我定位并修复一个具体 bug，先说明原因，再给出最小改动。"],
          ["做一个计划", "先不要改文件，帮我把这个目标拆成可确认的开发计划。"],
        ].map(([label, prompt]) => (
          <button key={label} className="rounded-md border border-border glass-surface p-4 text-left hover:bg-muted/20" onClick={() => onPrompt(prompt)}>
            <div className="text-sm font-semibold">{label}</div>
            <div className="mt-2 line-clamp-2 text-xs leading-5 text-muted-foreground">{prompt}</div>
          </button>
        ))}
      </div>
    </div>
  );
}

export function KnowledgePicker({
  knowledgeBases,
  selectedIds,
  disabled,
  onToggle,
}: {
  knowledgeBases: KnowledgeBase[];
  selectedIds: string[];
  disabled: boolean;
  onToggle: (id: string) => void;
}) {
  const [open, setOpen] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);

  // Close on outside click or Escape.
  useEffect(() => {
    if (!open) return;
    const handlePointerDown = (event: MouseEvent) => {
      if (!containerRef.current?.contains(event.target as Node | null)) {
        setOpen(false);
      }
    };
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") setOpen(false);
    };
    document.addEventListener("mousedown", handlePointerDown);
    document.addEventListener("keydown", handleKeyDown);
    return () => {
      document.removeEventListener("mousedown", handlePointerDown);
      document.removeEventListener("keydown", handleKeyDown);
    };
  }, [open]);

  return (
    <div className="relative" ref={containerRef}>
      <button
        type="button"
        className={cn(
          "flex h-8 items-center gap-1.5 rounded-md border px-2 text-sm",
          selectedIds.length > 0 ? "border-primary/40 bg-primary/10 text-primary" : "border-border text-muted-foreground hover:text-foreground",
          disabled && "opacity-60"
        )}
        onClick={() => setOpen((value) => !value)}
        title={disabled ? "请先配置可用的 embedding 模型" : "选择知识库"}
        aria-expanded={open}
      >
        <Brain className="h-3.5 w-3.5" />
        知识库 {selectedIds.length > 0 ? selectedIds.length : ""}
      </button>

      {open && (
        <div className="absolute bottom-10 left-0 z-40 w-80 rounded-md border border-border bg-popover p-3 shadow-xl">
          <div className="mb-2 text-sm font-semibold">选择知识库</div>
          <p className="mb-3 text-xs leading-5 text-muted-foreground">
            仅普通对话启用。工作区和团队任务默认不使用知识库。
          </p>
          {disabled ? (
            <div className="rounded-md border border-dashed border-border p-3 text-xs text-muted-foreground">没有可用 embedding 模型。</div>
          ) : knowledgeBases.length === 0 ? (
            <div className="rounded-md border border-dashed border-border p-3 text-xs text-muted-foreground">还没有知识库。</div>
          ) : (
            <div className="max-h-64 overflow-y-auto">
              {knowledgeBases.map((base) => (
                <button
                  key={base.id}
                  type="button"
                  className="flex w-full items-center gap-2 rounded-md px-2 py-2 text-left hover:bg-muted/20"
                  onClick={() => onToggle(base.id)}
                >
                  <span className={cn("flex h-4 w-4 items-center justify-center rounded border", selectedIds.includes(base.id) ? "border-primary bg-primary/20 text-primary" : "border-border")}>
                    {selectedIds.includes(base.id) && <Check className="h-3 w-3" />}
                  </span>
                  <span className="min-w-0 flex-1">
                    <span className="block truncate text-sm">{base.name}</span>
                    <span className="block text-xs text-muted-foreground">{base.document_count} 个文档</span>
                  </span>
                </button>
              ))}
            </div>
          )}
        </div>
      )}
    </div>
  );
}

export function formatKnowledgeContext(results: SearchResult[], selectedBases: KnowledgeBase[]) {
  const baseNames = selectedBases.map((base) => base.name).join(", ") || "已选择知识库";
  return [
    `[知识库检索结果: ${baseNames}]`,
    ...results.map((result, index) =>
      `[${index + 1}] ${result.content}\n来源：${result.knowledge_base_name} / ${result.document_title}`
    ),
  ].join("\n\n");
}

export function ApprovalCard({
  approval,
  onRespond,
}: {
  approval: RuntimeApprovalRequest;
  onRespond: (approved: boolean, forSession?: boolean) => void;
}) {
  return (
    <div className="mx-auto w-full max-w-2xl rounded-md border border-warning/35 bg-warning/8 p-4">
      <div className="flex items-center gap-2 text-sm font-semibold">
        <Shield className="h-4 w-4 text-warning" />
        请求审批
      </div>
      <div className="mt-3 break-words text-sm leading-6">{approval.title}</div>
      <details className="mt-2 text-xs text-muted-foreground">
        <summary className="cursor-pointer">查看请求详情</summary>
        <pre className="mt-2 max-h-48 overflow-auto whitespace-pre-wrap break-all rounded bg-background/60 p-2">{approval.detail}</pre>
      </details>
      <div className="mt-4 flex flex-wrap justify-end gap-2">
        <Button variant="outline" size="sm" onClick={() => onRespond(false)}>拒绝</Button>
        <Button variant="outline" size="sm" onClick={() => onRespond(true, true)}>本会话允许</Button>
        <Button size="sm" onClick={() => onRespond(true)}>允许一次</Button>
      </div>
    </div>
  );
}

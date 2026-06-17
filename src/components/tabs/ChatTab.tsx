import { useEffect, useMemo, useRef, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  AlertTriangle,
  Brain,
  Check,
  ChevronDown,
  ChevronRight,
  Globe,
  Loader2,
  PanelRightClose,
  PanelRightOpen,
  Send,
  Shield,
  Sparkles,
  Square,
  Users,
} from "lucide-react";

import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";
import { AGENT_NAMES } from "@/lib/constants";
import { cn } from "@/lib/utils";
import { knowledgeApi, searchApi } from "@/lib/tauri-api";
import type {
  ConversationMessage,
  DetectedAgent,
  EmbeddingModelInfo,
  KbDocument,
  PermissionPolicy,
  PlatformModel,
  PromptType,
  SearchResult,
  WorkMode,
} from "@/types";

export interface ChatTabProps {
  activeAgent: string;
  detectedAgents: DetectedAgent[];
  messages: ConversationMessage[];
  chatInput: string;
  chatWorkspace: string;
  currentConvId: string;
  activeSessions: string[];
  promptType: PromptType;
  targetModel: string;
  activeModels: PlatformModel[];
  setActiveAgent: (name: string) => void;
  setChatInput: (val: string) => void;
  setChatWorkspace: (val: string) => void;
  setTargetModel: (val: string) => void;
  onSendMessage: (e: React.FormEvent, searchContext?: string) => void;
  onSendStdinDirect: (input: string) => void;
  onStopSession: (id: string) => void;
  onSuggestTeam?: (prompt: string) => void;
}

const PERMISSION_OPTIONS: Array<{ id: PermissionPolicy; label: string; desc: string }> = [
  { id: "ask_every_time", label: "请求审批", desc: "每次操作都先问你" },
  { id: "ask_on_risk", label: "风险审批", desc: "低风险自动，风险操作询问" },
  { id: "full_access", label: "完全访问", desc: "尽量少打断" },
];

const WORK_MODE_OPTIONS: Array<{ id: WorkMode; label: string; desc: string }> = [
  { id: "chat", label: "直接执行", desc: "直接把任务交给 Agent" },
  { id: "plan_first", label: "计划模式", desc: "先做计划，不直接操作" },
  { id: "goal", label: "追求目标", desc: "自主且长效地推进目标" },
];

function MessageContent({ content }: { content: string }) {
  const parts: Array<{ type: "text" | "think"; content: string }> = [];
  let remaining = content;

  while (remaining.length > 0) {
    const thinkStart = remaining.indexOf("<think>");
    if (thinkStart === -1) {
      parts.push({ type: "text", content: remaining });
      break;
    }
    if (thinkStart > 0) parts.push({ type: "text", content: remaining.slice(0, thinkStart) });
    const thinkEnd = remaining.indexOf("</think>", thinkStart);
    if (thinkEnd === -1) {
      parts.push({ type: "think", content: remaining.slice(thinkStart + 7) });
      break;
    }
    parts.push({ type: "think", content: remaining.slice(thinkStart + 7, thinkEnd) });
    remaining = remaining.slice(thinkEnd + 8);
  }

  return (
    <>
      {parts.map((part, index) => (
        part.type === "think" ? <ThinkBlock key={index} content={part.content} /> : <span key={index}>{part.content}</span>
      ))}
    </>
  );
}

function ThinkBlock({ content }: { content: string }) {
  const [expanded, setExpanded] = useState(false);
  const trimmed = content.trim();
  if (!trimmed) return null;

  return (
    <div className="my-2 rounded-md border border-primary/20 bg-primary/5">
      <button
        className="flex w-full items-center gap-2 px-3 py-2 text-left text-xs text-primary"
        onClick={() => setExpanded((value) => !value)}
      >
        {expanded ? <ChevronDown className="h-3 w-3" /> : <ChevronRight className="h-3 w-3" />}
        <Brain className="h-3 w-3" />
        推理过程
        <span className="text-primary/60">{trimmed.length} 字符</span>
      </button>
      {expanded && <pre className="px-3 pb-3 text-xs whitespace-pre-wrap text-primary/80">{trimmed}</pre>}
    </div>
  );
}

export function ChatTab({
  activeAgent,
  detectedAgents,
  messages,
  chatInput,
  chatWorkspace,
  currentConvId,
  activeSessions,
  promptType,
  targetModel,
  activeModels,
  setActiveAgent,
  setChatInput,
  setChatWorkspace,
  setTargetModel,
  onSendMessage,
  onSendStdinDirect,
  onStopSession,
  onSuggestTeam,
}: ChatTabProps) {
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const [permissionPolicy, setPermissionPolicy] = useState<PermissionPolicy>("ask_on_risk");
  const [workMode, setWorkMode] = useState<WorkMode>("chat");
  const [webSearchEnabled, setWebSearchEnabled] = useState(false);
  const [isSearching, setIsSearching] = useState(false);
  const [workspacePanelOpen, setWorkspacePanelOpen] = useState(true);
  const [knowledgeDocs, setKnowledgeDocs] = useState<KbDocument[]>([]);
  const [embeddingModels, setEmbeddingModels] = useState<EmbeddingModelInfo[]>([]);
  const [selectedKnowledgeIds, setSelectedKnowledgeIds] = useState<string[]>([]);

  const modelOptions = useMemo(
    () => activeModels.map((model) => ({
      value: `${model.platform_id}:${model.model_name}`,
      label: `${model.model_name} · ${model.platform_id}`,
    })),
    [activeModels],
  );
  const modelValues = useMemo(() => modelOptions.map((model) => model.value), [modelOptions]);
  const isOrphanModel = !!targetModel && !modelValues.includes(targetModel);
  const isWorkspaceMode = chatWorkspace !== "direct";
  const isRunning = !!currentConvId && activeSessions.includes(currentConvId);

  useEffect(() => {
    if (modelOptions.length > 0 && (!targetModel || isOrphanModel)) {
      setTargetModel(modelOptions[0].value);
    }
  }, [modelOptions, targetModel, isOrphanModel, setTargetModel]);

  useEffect(() => {
    const el = textareaRef.current;
    if (!el) return;
    el.style.height = "auto";
    el.style.height = `${Math.min(el.scrollHeight, Math.floor(window.innerHeight * 0.5))}px`;
  }, [chatInput]);

  useEffect(() => {
    knowledgeApi.listDocuments().then(setKnowledgeDocs).catch(() => setKnowledgeDocs([]));
    knowledgeApi.getEmbeddingModels().then(setEmbeddingModels).catch(() => setEmbeddingModels([]));
  }, []);

  const selectedPermission = PERMISSION_OPTIONS.find((item) => item.id === permissionPolicy)!;
  const selectedWorkMode = WORK_MODE_OPTIONS.find((item) => item.id === workMode)!;
  const selectedKnowledgeDocs = knowledgeDocs.filter((doc) => selectedKnowledgeIds.includes(doc.id));
  const selectedEmbeddingModel = embeddingModels[0]?.model_name ?? "";

  const handleKnowledgeToggle = (id: string) => {
    setSelectedKnowledgeIds((prev) => prev.includes(id) ? prev.filter((item) => item !== id) : [...prev, id]);
  };

  const buildContext = async () => {
    const blocks: string[] = [];

    if (webSearchEnabled) {
      const results = await searchApi.search(chatInput, undefined, 5);
      if (results.length > 0) {
        blocks.push([
          "[联网搜索结果]",
          ...results.map((result, index) => `[${index + 1}] ${result.title}\n${result.snippet}\n${result.url}`),
        ].join("\n\n"));
      }
    }

    if (!isWorkspaceMode && selectedKnowledgeIds.length > 0 && selectedEmbeddingModel) {
      const results = await knowledgeApi.hybridSearch({
        query: chatInput,
        embeddingModel: selectedEmbeddingModel,
        limit: 8,
      });
      if (results.length > 0) {
        blocks.push(formatKnowledgeContext(results, selectedKnowledgeDocs));
      }
    }

    if (workMode === "plan_first") {
      blocks.push("[工作模式]\n请先给出计划、风险和需要用户确认的步骤，不要直接执行文件或系统操作。");
    }
    if (workMode === "goal") {
      blocks.push("[工作模式]\n请围绕用户目标持续推进，遇到关键风险或权限边界时暂停请求确认。");
    }
    blocks.push(`[权限模式]\n${selectedPermission.label}: ${selectedPermission.desc}`);

    return blocks.join("\n\n---\n\n");
  };

  const handleSubmit = async (event: React.FormEvent) => {
    event.preventDefault();
    if (!chatInput.trim()) return;

    setIsSearching(webSearchEnabled || selectedKnowledgeIds.length > 0);
    try {
      const context = await buildContext();
      onSendMessage(event, context || undefined);
    } finally {
      setIsSearching(false);
    }
  };

  const openWorkspace = async () => {
    if (!isWorkspaceMode) {
      setChatWorkspace("direct");
      return;
    }
    const normalized = chatWorkspace.replace(/\\/g, "/");
    await openUrl(`file:///${normalized}`);
  };

  return (
    <div className="flex h-full flex-1 overflow-hidden bg-background">
      <div className="flex min-w-0 flex-1 flex-col">
        <div className="flex items-center gap-2 border-b border-border px-5 py-3">
          <AgentStrip
            activeAgent={activeAgent}
            detectedAgents={detectedAgents}
            onSelectAgent={setActiveAgent}
          />
          <Button
            variant="outline"
            size="sm"
            className="ml-auto"
            onClick={() => onSuggestTeam?.(chatInput)}
            disabled={!chatInput.trim()}
            title="把当前任务转为团队计划"
          >
            <Users className="h-3.5 w-3.5" />
            转团队
          </Button>
          <Button
            variant="ghost"
            size="sm"
            className="h-8 w-8 p-0"
            onClick={() => setWorkspacePanelOpen((open) => !open)}
            title={workspacePanelOpen ? "收起工作区" : "展开工作区"}
          >
            {workspacePanelOpen ? <PanelRightClose className="h-4 w-4" /> : <PanelRightOpen className="h-4 w-4" />}
          </Button>
        </div>

        <div className="flex-1 overflow-y-auto px-6 py-5">
          {messages.length === 0 ? (
            <FirstScreen
              activeAgent={activeAgent}
              installed={detectedAgents.find((agent) => agent.name === activeAgent)?.status === "installed"}
              onPrompt={setChatInput}
            />
          ) : (
            <div className="mx-auto flex max-w-4xl flex-col gap-5">
              {messages.map((message) => (
                <div key={message.id} className={cn("flex", message.role === "user" ? "justify-end" : "justify-start")}>
                  <div
                    className={cn(
                      "max-w-[78%] rounded-md border px-4 py-3 text-sm leading-6",
                      message.role === "user"
                        ? "border-primary/20 bg-primary/12"
                        : "border-border bg-card/50"
                    )}
                  >
                    <div className="mb-1 text-xs text-muted-foreground">{message.role === "user" ? "你" : activeAgent}</div>
                    <div className="whitespace-pre-wrap break-words">
                      <MessageContent content={message.content} />
                    </div>
                  </div>
                </div>
              ))}
              {promptType !== "none" && <PromptCards promptType={promptType} onSendStdin={onSendStdinDirect} />}
            </div>
          )}
        </div>

        <form onSubmit={handleSubmit} className="border-t border-border bg-background/95 p-5">
          <div className="mx-auto max-w-5xl rounded-md border border-border bg-card/60 p-3 shadow-lg">
            <Textarea
              ref={textareaRef}
              value={chatInput}
              onChange={(event) => setChatInput(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter" && !event.shiftKey) {
                  event.preventDefault();
                  event.currentTarget.form?.requestSubmit();
                }
              }}
              placeholder={`${activeAgent}，输入你要做的事情... 支持长文本、换行、@引用文件`}
              className="min-h-28 resize-none border-0 bg-transparent text-base leading-7 focus-visible:ring-0 focus-visible:ring-offset-0"
              style={{ maxHeight: "50vh" }}
            />

            <div className="mt-3 flex flex-wrap items-center gap-2">
              <select
                value={modelValues.includes(targetModel) ? targetModel : modelOptions[0]?.value || ""}
                onChange={(event) => setTargetModel(event.target.value)}
                className="h-8 max-w-56 rounded-md border border-border bg-background px-2 text-sm"
                disabled={modelOptions.length === 0}
              >
                {modelOptions.length === 0 ? <option value="">请先配置模型</option> : modelOptions.map((model) => <option key={model.value} value={model.value}>{model.label}</option>)}
              </select>

              <select
                value={permissionPolicy}
                onChange={(event) => setPermissionPolicy(event.target.value as PermissionPolicy)}
                className="h-8 rounded-md border border-border bg-background px-2 text-sm"
                title={selectedPermission.desc}
              >
                {PERMISSION_OPTIONS.map((option) => <option key={option.id} value={option.id}>{option.label}</option>)}
              </select>

              <select
                value={workMode}
                onChange={(event) => setWorkMode(event.target.value as WorkMode)}
                className="h-8 rounded-md border border-border bg-background px-2 text-sm"
                title={selectedWorkMode.desc}
              >
                {WORK_MODE_OPTIONS.map((option) => (
                  <option key={option.id} value={option.id}>{option.label}</option>
                ))}
              </select>

              {!isWorkspaceMode && (
                <KnowledgePicker
                  documents={knowledgeDocs}
                  selectedIds={selectedKnowledgeIds}
                  disabled={!selectedEmbeddingModel}
                  onToggle={handleKnowledgeToggle}
                />
              )}

              <button
                type="button"
                className={cn(
                  "flex h-8 items-center gap-1.5 rounded-md border px-2 text-sm",
                  webSearchEnabled ? "border-success/40 bg-success/10 text-success" : "border-border text-muted-foreground hover:text-foreground"
                )}
                onClick={() => setWebSearchEnabled((enabled) => !enabled)}
              >
                <Globe className="h-3.5 w-3.5" />
                搜索
              </button>

              <div className="ml-auto flex items-center gap-2">
                {isOrphanModel && modelOptions.length > 0 && (
                  <span className="flex items-center gap-1 text-xs text-warning">
                    <AlertTriangle className="h-3.5 w-3.5" />
                    已切换可用模型
                  </span>
                )}
                {isRunning && (
                  <Button type="button" variant="destructive" size="sm" onClick={() => onStopSession(currentConvId)}>
                    <Square className="h-3.5 w-3.5" />
                    停止
                  </Button>
                )}
                <Button type="submit" disabled={isSearching || !chatInput.trim()}>
                  {isSearching ? <Loader2 className="h-4 w-4 animate-spin" /> : <Send className="h-4 w-4" />}
                  发送
                </Button>
              </div>
            </div>
          </div>
        </form>
      </div>

      {workspacePanelOpen && (
        <aside className="hidden w-72 shrink-0 border-l border-border bg-card/30 xl:flex xl:flex-col 2xl:w-80">
          <div className="flex items-center justify-between border-b border-border p-4">
            <div>
              <div className="text-base font-semibold">工作区</div>
              <button
                className="mt-1 max-w-56 truncate text-left text-xs text-muted-foreground hover:text-foreground"
                onClick={openWorkspace}
                disabled={!isWorkspaceMode}
                title={isWorkspaceMode ? chatWorkspace : "当前是普通对话"}
              >
                {isWorkspaceMode ? chatWorkspace.split(/[\\/]/).pop() : "未选择工作区"}
              </button>
            </div>
            <button className="rounded p-1 text-muted-foreground hover:bg-muted/20" onClick={() => setWorkspacePanelOpen(false)}>
              <ChevronRight className="h-4 w-4" />
            </button>
          </div>

          <div className="flex-1 overflow-y-auto p-4">
            {isWorkspaceMode ? (
              <div className="space-y-4">
                <InfoBlock label="分支" value="等待 Git 状态接入" />
                <InfoBlock label="最近变更" value="Agent 修改文件后会在这里集中显示。" />
                <InfoBlock label="文件" value="后续接入文件树和点击打开文件。" />
              </div>
            ) : (
              <div className="rounded-md border border-dashed border-border p-4 text-sm text-muted-foreground">
                普通对话不绑定工作区。需要开发项目时，从左侧选择工作区或创建工作会话。
              </div>
            )}
          </div>
        </aside>
      )}
    </div>
  );
}

function AgentStrip({
  activeAgent,
  detectedAgents,
  onSelectAgent,
}: {
  activeAgent: string;
  detectedAgents: DetectedAgent[];
  onSelectAgent: (name: string) => void;
}) {
  return (
    <div className="flex min-w-0 items-center gap-2 overflow-x-auto">
      {AGENT_NAMES.map((name) => {
        const agent = detectedAgents.find((item) => item.name === name);
        const installed = agent?.status === "installed";
        const active = activeAgent === name;

        return (
          <button
            key={name}
            onClick={() => onSelectAgent(name)}
            className={cn(
              "flex h-9 shrink-0 items-center gap-2 rounded-full border px-3 text-sm",
              active ? "border-primary/40 bg-primary/12 text-primary" : "border-border bg-card/40 text-muted-foreground hover:text-foreground"
            )}
          >
            <span className={cn("h-2 w-2 rounded-full", installed ? "bg-success" : "bg-muted-foreground")} />
            {name}
          </button>
        );
      })}
    </div>
  );
}

function FirstScreen({ activeAgent, installed, onPrompt }: { activeAgent: string; installed: boolean; onPrompt: (prompt: string) => void }) {
  return (
    <div className="mx-auto flex h-full max-w-4xl flex-col items-center justify-center px-6 text-center">
      <div className="mb-5 flex h-16 w-16 items-center justify-center rounded-md border border-border bg-card/70">
        <Sparkles className="h-8 w-8 text-primary" />
      </div>
      <h2 className="m-0 text-3xl font-semibold">今天让 {activeAgent} 做什么？</h2>
      <p className="mt-3 max-w-2xl text-sm leading-6 text-muted-foreground">
        先选择 Agent，再直接输入任务。复杂任务可以转团队；普通问答可以手动接入知识库。
      </p>
      {!installed && (
        <div className="mt-4 flex items-center gap-2 rounded-md border border-warning/30 bg-warning/10 px-3 py-2 text-sm text-warning">
          <AlertTriangle className="h-4 w-4" />
          当前 Agent 未检测到，仍可先整理任务，稍后到智能体页安装或配置。
        </div>
      )}
      <div className="mt-7 grid w-full grid-cols-1 gap-2 md:grid-cols-3">
        {[
          ["盘点项目结构", "读取当前工作区，给我总结项目结构、关键模块和下一步重构建议。"],
          ["修复一个问题", "帮我定位并修复一个具体 bug，先说明原因，再给出最小改动。"],
          ["做一个计划", "先不要改文件，帮我把这个目标拆成可确认的开发计划。"],
        ].map(([label, prompt]) => (
          <button key={label} className="rounded-md border border-border bg-card/40 p-4 text-left hover:bg-muted/20" onClick={() => onPrompt(prompt)}>
            <div className="text-sm font-semibold">{label}</div>
            <div className="mt-2 line-clamp-2 text-xs leading-5 text-muted-foreground">{prompt}</div>
          </button>
        ))}
      </div>
    </div>
  );
}

function KnowledgePicker({
  documents,
  selectedIds,
  disabled,
  onToggle,
}: {
  documents: KbDocument[];
  selectedIds: string[];
  disabled: boolean;
  onToggle: (id: string) => void;
}) {
  const [open, setOpen] = useState(false);

  return (
    <div className="relative">
      <button
        type="button"
        className={cn(
          "flex h-8 items-center gap-1.5 rounded-md border px-2 text-sm",
          selectedIds.length > 0 ? "border-primary/40 bg-primary/10 text-primary" : "border-border text-muted-foreground hover:text-foreground",
          disabled && "opacity-60"
        )}
        onClick={() => setOpen((value) => !value)}
        title={disabled ? "请先配置可用的 embedding 模型" : "选择知识库"}
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
          ) : documents.length === 0 ? (
            <div className="rounded-md border border-dashed border-border p-3 text-xs text-muted-foreground">还没有知识库文档。</div>
          ) : (
            <div className="max-h-64 overflow-y-auto">
              {documents.map((doc) => (
                <button
                  key={doc.id}
                  type="button"
                  className="flex w-full items-center gap-2 rounded-md px-2 py-2 text-left hover:bg-muted/20"
                  onClick={() => onToggle(doc.id)}
                >
                  <span className={cn("flex h-4 w-4 items-center justify-center rounded border", selectedIds.includes(doc.id) ? "border-primary bg-primary/20 text-primary" : "border-border")}>
                    {selectedIds.includes(doc.id) && <Check className="h-3 w-3" />}
                  </span>
                  <span className="min-w-0 flex-1">
                    <span className="block truncate text-sm">{doc.title}</span>
                    <span className="block text-xs text-muted-foreground">{doc.chunk_count} chunks · {doc.embedding_status}</span>
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

function formatKnowledgeContext(results: SearchResult[], selectedDocs: KbDocument[]) {
  const docNames = selectedDocs.map((doc) => doc.title).join(", ") || "已选择知识库";
  return [
    `[知识库检索结果: ${docNames}]`,
    ...results.map((result, index) => `[${index + 1}] ${result.content}\nsource=${result.document_id}`),
  ].join("\n\n");
}

function InfoBlock({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded-md border border-border bg-background/40 p-3">
      <div className="text-xs font-semibold text-muted-foreground">{label}</div>
      <div className="mt-2 text-sm leading-6">{value}</div>
    </div>
  );
}

function PromptCards({ promptType, onSendStdin }: { promptType: PromptType; onSendStdin: (input: string) => void }) {
  if (promptType === "trust") {
    return (
      <div className="mx-auto max-w-xl rounded-md border border-success/30 bg-success/10 p-4">
        <div className="mb-2 flex items-center gap-2 text-sm font-semibold text-success">
          <Shield className="h-4 w-4" />
          安全确认
        </div>
        <p className="mb-3 text-xs leading-5 text-muted-foreground">Agent 正在请求信任当前目录或继续执行。请确认后再放行。</p>
        <div className="flex gap-2">
          <Button size="sm" onClick={() => onSendStdin("1\n")}>确认</Button>
          <Button size="sm" variant="outline" onClick={() => onSendStdin("2\n")}>拒绝</Button>
        </div>
      </div>
    );
  }

  if (promptType === "update") {
    return (
      <div className="mx-auto max-w-xl rounded-md border border-info/30 bg-info/10 p-4">
        <div className="mb-2 text-sm font-semibold text-info">检测到 CLI 更新提示</div>
        <div className="flex gap-2">
          <Button size="sm" onClick={() => onSendStdin("\r")}>确认更新</Button>
          <Button size="sm" variant="outline" onClick={() => onSendStdin("\x1b")}>跳过</Button>
        </div>
      </div>
    );
  }

  return (
    <div className="mx-auto flex max-w-xl flex-wrap justify-center gap-2 rounded-md border border-border bg-card/50 p-3">
      <Button variant="outline" size="sm" onClick={() => onSendStdin("\t")}>Tab</Button>
      <Button variant="outline" size="sm" onClick={() => onSendStdin(" ")}>空格</Button>
      <Button size="sm" onClick={() => onSendStdin("\r")}>确认</Button>
      <Button variant="outline" size="sm" onClick={() => onSendStdin("\x1b")}>Esc</Button>
    </div>
  );
}

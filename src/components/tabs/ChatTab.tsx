import { useEffect, useRef, useState } from "react";
import { openPath } from "@tauri-apps/plugin-opener";
import { toast } from "sonner";
import {
  AlertTriangle,
  Brain,
  Check,
  ChevronDown,
  ChevronRight,
  FileCode2,
  Folder,
  FolderOpen,
  GitBranch,
  Globe,
  Loader2,
  PanelRightClose,
  PanelRightOpen,
  Send,
  Shield,
  Sparkles,
  Square,
  Users,
  RefreshCw,
  StickyNote,
} from "lucide-react";
import { WorkspaceCheckpoints } from "@/components/WorkspaceCheckpoints";
import { WorktreePanel } from "@/components/WorktreePanel";
import { FilePreviewPanel } from "@/components/FilePreviewPanel";
import { ContextMeter } from "@/components/ContextMeter";
import { SubAgentPanel } from "@/components/SubAgentPanel";

import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";
import { AGENT_NAMES } from "@/lib/constants";
import { cn } from "@/lib/utils";
import { knowledgeApi, runtimeApi, searchApi, workspaceApi, notesApi } from "@/lib/tauri-api";
import type {
  ConversationMessage,
  DetectedAgent,
  EmbeddingModelInfo,
  KnowledgeBase,
  PermissionPolicy,
  RuntimeAgentId,
  RuntimeApprovalRequest,
  RuntimeModelOption,
  RuntimePermissionPolicy,
  SearchResult,
  WorkMode,
  WorkspaceSnapshot,
} from "@/types";
import type { RuntimeSendConfig } from "@/hooks/useConversations";

export interface ChatTabProps {
  surface: "chat" | "work";
  activeAgent: string;
  detectedAgents: DetectedAgent[];
  messages: ConversationMessage[];
  chatInput: string;
  chatWorkspace: string;
  currentConvId: string;
  activeSessions: string[];
  pendingApproval: RuntimeApprovalRequest | null;
  isAwaitingResponse?: boolean;
  setActiveAgent: (name: string) => void;
  setChatInput: (val: string) => void;
  setChatWorkspace: (val: string) => void;
  onOpenWorkspaceModal?: () => void;
  onSendMessage: (e: React.FormEvent, config: RuntimeSendConfig, searchContext?: string) => void;
  onRespondApproval: (approved: boolean, forSession?: boolean) => void;
  onStopSession: (id: string) => void;
  onSuggestTeam?: (prompt: string) => void;
  onReloadMessages?: () => void;
  onSelectConversation?: (id: string) => void;
}

const PERMISSION_OPTIONS: Array<{ id: PermissionPolicy; label: string; desc: string }> = [
  { id: "ask_every_time", label: "请求审批", desc: "每次操作都先问你" },
  { id: "ask_on_risk", label: "风险审批", desc: "低风险自动，风险操作询问" },
  { id: "full_access", label: "完全访问", desc: "尽量少打断" },
];

const WORK_MODE_OPTIONS: Array<{ id: WorkMode; label: string; desc: string }> = [
  { id: "direct", label: "直接执行", desc: "按权限策略执行任务" },
  { id: "plan", label: "计划模式", desc: "只读分析并先给出计划" },
];

function getRuntimeAgentId(agentName: string): RuntimeAgentId | null {
  if (agentName === "Claude Code") return "claude_code";
  if (agentName === "Codex") return "codex";
  return null;
}

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
  surface,
  activeAgent,
  detectedAgents,
  messages,
  chatInput,
  chatWorkspace,
  currentConvId,
  activeSessions,
  pendingApproval,
  isAwaitingResponse,
  setActiveAgent,
  setChatInput,
  setChatWorkspace,
  onOpenWorkspaceModal,
  onSendMessage,
  onRespondApproval,
  onStopSession,
  onSuggestTeam,
  onReloadMessages,
  onSelectConversation,
}: ChatTabProps) {
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const [permissionPolicy, setPermissionPolicy] = useState<PermissionPolicy>("ask_on_risk");
  const [workMode, setWorkMode] = useState<WorkMode>("direct");
  const [runtimeModels, setRuntimeModels] = useState<RuntimeModelOption[]>([]);
  const [selectedModelId, setSelectedModelId] = useState("agent_default");
  const [fullAccessConfirmed, setFullAccessConfirmed] = useState(false);
  const [webSearchEnabled, setWebSearchEnabled] = useState(false);
  const [isSearching, setIsSearching] = useState(false);
  const [workspacePanelOpen, setWorkspacePanelOpen] = useState(
    chatWorkspace !== "direct" && window.innerWidth >= 1000
  );
  const [knowledgeBases, setKnowledgeBases] = useState<KnowledgeBase[]>([]);
  const [embeddingModels, setEmbeddingModels] = useState<EmbeddingModelInfo[]>([]);
  const [selectedKnowledgeIds, setSelectedKnowledgeIds] = useState<string[]>([]);
  const [workspaceSnapshot, setWorkspaceSnapshot] = useState<WorkspaceSnapshot | null>(null);
  const [previewPath, setPreviewPath] = useState<string | null>(null);
  const [workspaceLoading, setWorkspaceLoading] = useState(false);

  const runtimeAgentId = getRuntimeAgentId(activeAgent);
  const selectedModel = runtimeModels.find((model) => model.id === selectedModelId)
    ?? runtimeModels.find((model) => model.is_default && model.compatibility.selectable)
    ?? runtimeModels.find((model) => model.compatibility.selectable);
  const isWorkspaceMode = chatWorkspace !== "direct";
  // 工作 surface requires a workspace before any task can be sent.
  const needsWorkspace = surface === "work" && !isWorkspaceMode;
  const isRunning = !!currentConvId && activeSessions.includes(currentConvId);
  // OMNIX (custom) models carry a provider_name; Agent default / builtins do not.
  const customModels = runtimeModels.filter((model) => model.provider_name);
  const noSelectableCustomModel =
    !!runtimeAgentId &&
    customModels.length > 0 &&
    !customModels.some((model) => model.compatibility.selectable);

  useEffect(() => {
    setWorkspacePanelOpen(isWorkspaceMode && window.innerWidth >= 1000);
  }, [isWorkspaceMode]);

  useEffect(() => {
    setFullAccessConfirmed(false);
  }, [currentConvId, permissionPolicy]);

  useEffect(() => {
    if (!runtimeAgentId) {
      setRuntimeModels([]);
      return;
    }
    runtimeApi.getModelOptions(runtimeAgentId).then((models) => {
      setRuntimeModels(models);
      // On every Agent switch, pre-select that Agent's configured default
      // (binding / global default). Don't carry over the previous Agent's
      // selection — the shared "agent_default" option is valid for every Agent
      // and would otherwise mask the new Agent's default.
      const preferred =
        models.find((model) => model.is_default && model.compatibility.selectable)
        ?? models.find((model) => model.compatibility.selectable);
      setSelectedModelId(preferred?.id || "");
    }).catch((error) => {
      setRuntimeModels([]);
      toast.error("无法读取 Agent 模型兼容目录", { description: String(error) });
    });
  }, [runtimeAgentId]);

  useEffect(() => {
    const el = textareaRef.current;
    if (!el) return;
    el.style.height = "auto";
    el.style.height = `${Math.min(el.scrollHeight, Math.floor(window.innerHeight * 0.5))}px`;
  }, [chatInput]);

  useEffect(() => {
    knowledgeApi.listBases().then(setKnowledgeBases).catch(() => setKnowledgeBases([]));
    knowledgeApi.getEmbeddingModels().then(setEmbeddingModels).catch(() => setEmbeddingModels([]));
  }, []);

  const refreshWorkspace = async () => {
    if (!isWorkspaceMode) {
      setWorkspaceSnapshot(null);
      return;
    }
    setWorkspaceLoading(true);
    try {
      setWorkspaceSnapshot(await workspaceApi.snapshot(chatWorkspace));
    } catch (error) {
      setWorkspaceSnapshot(null);
      toast.error("无法读取工作区", { description: String(error) });
    } finally {
      setWorkspaceLoading(false);
    }
  };

  useEffect(() => {
    void refreshWorkspace();
    // Refresh after completed messages because an Agent turn may change files.
  }, [chatWorkspace, messages.length]); // eslint-disable-line react-hooks/exhaustive-deps

  const selectedPermission = PERMISSION_OPTIONS.find((item) => item.id === permissionPolicy)!;
  const selectedWorkMode = WORK_MODE_OPTIONS.find((item) => item.id === workMode)!;
  const selectedKnowledgeBases = knowledgeBases.filter((base) => selectedKnowledgeIds.includes(base.id));
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
        knowledgeBaseIds: selectedKnowledgeIds,
      });
      if (results.length > 0) {
        blocks.push(formatKnowledgeContext(results, selectedKnowledgeBases));
      }
    }

    return blocks.join("\n\n---\n\n");
  };

  // Connect agent output → notebook: save a plan / suggestion / deferred item
  // straight into Notes so it can be reviewed for missed work later.
  const saveMessageAsNote = async (content: string) => {
    const firstLine = content.trim().split("\n").find((l) => l.trim()) ?? "Agent 笔记";
    const title = firstLine.replace(/[#>*`]/g, "").trim().slice(0, 40) || "Agent 笔记";
    const ws = isWorkspaceMode ? chatWorkspace.split(/[\\/]/).pop() : "";
    try {
      await notesApi.save({
        title: `[${activeAgent}] ${title}`,
        content,
        tags: "agent",
        source: ws ? `${activeAgent} · ${ws}` : activeAgent,
      });
      toast.success("已存为笔记", { description: "可在「笔记」中查阅" });
    } catch (e) {
      toast.error("保存笔记失败", { description: String(e) });
    }
  };

  const handleSubmit = async (event: React.FormEvent) => {
    event.preventDefault();
    if (!chatInput.trim() || !runtimeAgentId || !selectedModel) return;
    if (needsWorkspace) {
      onOpenWorkspaceModal?.();
      return;
    }

    let confirmed = fullAccessConfirmed;
    if (permissionPolicy === "full_access" && !confirmed) {
      confirmed = window.confirm("完全访问允许 Agent 在当前会话中绕过审批并修改系统可访问内容。仅在你信任任务和工作区时继续。");
      if (!confirmed) return;
      setFullAccessConfirmed(true);
    }

    setIsSearching(webSearchEnabled || selectedKnowledgeIds.length > 0);
    let context: string | undefined;
    try {
      context = (await buildContext()) || undefined;
    } catch (err) {
      // Web search / knowledge retrieval failed — don't silently drop the user's
      // message. Notify and still send without the enriched context.
      toast.error("上下文获取失败，已忽略联网/知识库上下文发送", {
        description: err instanceof Error ? err.message : String(err),
      });
    } finally {
      setIsSearching(false);
    }
    const permission: RuntimePermissionPolicy = permissionPolicy === "full_access"
      ? { kind: "full_access", confirmed }
      : { kind: permissionPolicy };
    onSendMessage(event, {
      model: selectedModel.selection,
      permission,
      workMode,
    }, context);
  };

  const openWorkspace = async () => {
    if (!isWorkspaceMode) {
      setChatWorkspace("direct");
      return;
    }
    await openPath(chatWorkspace);
  };

  const openWorkspaceEntry = async (relativePath: string) => {
    if (!workspaceSnapshot) return;
    const separator = workspaceSnapshot.root_path.includes("\\") ? "\\" : "/";
    await openPath(`${workspaceSnapshot.root_path}${separator}${relativePath.replace(/[\\/]/g, separator)}`);
  };

  return (
    <div className="relative flex h-full flex-1 overflow-hidden bg-background">
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
            <span className="hidden md:inline">转团队</span>
          </Button>
          {surface === "work" && (
            <>
              <Button
                variant="outline"
                size="sm"
                onClick={() => onOpenWorkspaceModal?.()}
                title={isWorkspaceMode ? chatWorkspace : "选择工作区"}
              >
                <FolderOpen className="h-3.5 w-3.5" />
                <span className="hidden md:inline max-w-32 truncate">
                  {isWorkspaceMode ? chatWorkspace.split(/[\\/]/).pop() : "选择工作区"}
                </span>
              </Button>
              <Button
                variant="ghost"
                size="sm"
                className="h-8 w-8 p-0"
                onClick={() => setWorkspacePanelOpen((open) => !open)}
                title={workspacePanelOpen ? "收起工作区" : "展开工作区"}
                disabled={!isWorkspaceMode}
              >
                {workspacePanelOpen ? <PanelRightClose className="h-4 w-4" /> : <PanelRightOpen className="h-4 w-4" />}
              </Button>
            </>
          )}
        </div>

        <div className="flex-1 overflow-y-auto px-6 py-5">
          {messages.length === 0 && needsWorkspace ? (
            <div className="mx-auto flex h-full max-w-md flex-col items-center justify-center gap-4 text-center">
              <div className="flex h-14 w-14 items-center justify-center rounded-xl bg-muted/40">
                <FolderOpen className="h-7 w-7 text-muted-foreground" />
              </div>
              <div>
                <div className="text-lg font-semibold">工作模式需要一个工作区</div>
                <p className="mt-1 text-sm text-muted-foreground">
                  选择一个项目文件夹，{activeAgent} 才能读写文件、查看 Git 变更并处理开发任务。只想随便聊聊？切到「对话」。
                </p>
              </div>
              <Button onClick={() => onOpenWorkspaceModal?.()}>
                <FolderOpen className="h-4 w-4" />
                选择工作区
              </Button>
            </div>
          ) : messages.length === 0 ? (
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
                    <div className="mb-1 flex items-center gap-2 text-xs text-muted-foreground">
                      <span>{message.role === "user" ? "你" : activeAgent}</span>
                      {message.role !== "user" && message.content.trim() && (
                        <button
                          onClick={() => void saveMessageAsNote(message.content)}
                          title="存为笔记（计划/建议/待办，方便回查）"
                          className="ml-auto flex items-center gap-1 rounded px-1 py-0.5 opacity-60 hover:bg-muted/30 hover:text-foreground hover:opacity-100"
                        >
                          <StickyNote className="h-3 w-3" /> 存为笔记
                        </button>
                      )}
                    </div>
                    <div className="whitespace-pre-wrap break-words">
                      <MessageContent content={message.content} />
                    </div>
                  </div>
                </div>
              ))}
              {pendingApproval && (
                <ApprovalCard
                  approval={pendingApproval}
                  onRespond={onRespondApproval}
                />
              )}
              {isAwaitingResponse && (
                <div className="flex justify-start">
                  <div className="max-w-[78%] rounded-md border border-border bg-card/50 px-4 py-3 text-sm">
                    <div className="mb-1 text-xs text-muted-foreground">{activeAgent}</div>
                    <div className="flex items-center gap-2 text-muted-foreground">
                      <Loader2 className="h-4 w-4 animate-spin" />
                      <span>正在启动 {activeAgent} 并等待响应…</span>
                    </div>
                    <div className="mt-1 text-xs text-muted-foreground/70">
                      首次启动需要初始化（可能十几秒），请稍候
                    </div>
                  </div>
                </div>
              )}
            </div>
          )}
        </div>

        <form onSubmit={handleSubmit} className="shrink-0 border-t border-border bg-background/95 p-5">
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
              placeholder={`${activeAgent}，输入你要做的事情...`}
              className="min-h-28 resize-none border-0 bg-transparent text-base leading-7 focus-visible:ring-0 focus-visible:ring-offset-0"
              style={{ maxHeight: "50vh" }}
            />

            <div className="mt-3 flex flex-wrap items-center gap-2">
              <select
                value={selectedModel?.id || ""}
                onChange={(event) => setSelectedModelId(event.target.value)}
                className="h-8 max-w-56 rounded-md border border-border bg-background px-2 text-sm"
                disabled={!runtimeAgentId || runtimeModels.length === 0}
                title={selectedModel?.compatibility.reason}
              >
                {!runtimeAgentId ? (
                  <option value="">待适配</option>
                ) : runtimeModels.length === 0 ? (
                  <option value="">读取模型中...</option>
                ) : runtimeModels.map((model) => (
                  <option key={model.id} value={model.id} disabled={!model.compatibility.selectable}>
                    {model.label}{model.compatibility.selectable ? "" : ` · 不可用：${model.compatibility.reason}`}
                  </option>
                ))}
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
                  knowledgeBases={knowledgeBases}
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
                {currentConvId && (
                  <ContextMeter
                    conversationId={currentConvId}
                    modelName={selectedModel?.model_name}
                    refreshSignal={messages.length}
                    onCompacted={onReloadMessages}
                  />
                )}
                {selectedModel && selectedModel.compatibility.level !== "native" && (
                  <span className="flex items-center gap-1 text-xs text-warning">
                    <AlertTriangle className="h-3.5 w-3.5" />
                    {selectedModel.compatibility.level === "gateway" ? "OMNIX 网关" : selectedModel.compatibility.reason}
                  </span>
                )}
                {isRunning && (
                  <Button type="button" variant="destructive" size="sm" onClick={() => onStopSession(currentConvId)}>
                    <Square className="h-3.5 w-3.5" />
                    停止
                  </Button>
                )}
                <Button type="submit" disabled={isSearching || !chatInput.trim() || !runtimeAgentId || !selectedModel?.compatibility.selectable || needsWorkspace}>
                  {isSearching ? <Loader2 className="h-4 w-4 animate-spin" /> : <Send className="h-4 w-4" />}
                  发送
                </Button>
              </div>
            </div>
            {noSelectableCustomModel && (
              <p className="mt-2 flex items-start gap-1.5 text-xs text-warning">
                <AlertTriangle className="mt-0.5 h-3.5 w-3.5 shrink-0" />
                <span>
                  {activeAgent === "Codex"
                    ? "Codex 只能使用 OpenAI 协议供应商。当前已启用的供应商不兼容（例如 Anthropic 类型仅 Claude Code 可用）。请在「模型」中启用或新增一个 OpenAI 类型供应商，OMNIX 网关会自动把 Codex 的请求翻译过去。"
                    : `${activeAgent} 暂无可用的自定义模型，请在「模型」中启用一个兼容的供应商，或使用 Agent 官方默认。`}
                </span>
              </p>
            )}
          </div>
        </form>
      </div>

      {workspacePanelOpen && (
        <aside className="absolute inset-y-0 right-0 z-30 flex w-[min(22rem,88vw)] shrink-0 flex-col border-l border-border bg-background shadow-xl min-[1600px]:static min-[1600px]:w-72 min-[1600px]:bg-card/30 min-[1600px]:shadow-none min-[1800px]:w-80">
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
              workspaceLoading && !workspaceSnapshot ? (
                <div className="flex items-center gap-2 text-sm text-muted-foreground">
                  <Loader2 className="h-4 w-4 animate-spin" />
                  读取工作区
                </div>
              ) : workspaceSnapshot ? (
                <div className="space-y-5">
                  <section>
                    <div className="mb-2 flex items-center justify-between text-xs font-semibold text-muted-foreground">
                      <span className="flex items-center gap-1.5"><GitBranch className="h-3.5 w-3.5" />分支</span>
                      <button className="rounded p-1 hover:bg-muted/30" onClick={() => void refreshWorkspace()} title="刷新工作区">
                        <RefreshCw className={cn("h-3.5 w-3.5", workspaceLoading && "animate-spin")} />
                      </button>
                    </div>
                    <div className="truncate text-sm">{workspaceSnapshot.branch || "非 Git 工作区"}</div>
                  </section>

                  <section>
                    <div className="mb-2 text-xs font-semibold text-muted-foreground">变更 {workspaceSnapshot.changes.length}</div>
                    <div className="max-h-40 space-y-1 overflow-y-auto">
                      {workspaceSnapshot.changes.length === 0 ? (
                        <div className="text-xs text-muted-foreground">工作区干净</div>
                      ) : workspaceSnapshot.changes.map((change) => (
                        <button
                          key={`${change.status}:${change.path}`}
                          className="flex w-full min-w-0 items-center gap-2 rounded px-1.5 py-1 text-left text-xs hover:bg-muted/25"
                          onClick={() => setPreviewPath(change.path)}
                          title={change.path}
                        >
                          <span className="w-6 shrink-0 font-mono text-warning">{change.status || "M"}</span>
                          <span className="truncate">{change.path}</span>
                        </button>
                      ))}
                    </div>
                  </section>

                  <section>
                    <WorkspaceCheckpoints
                      workspacePath={chatWorkspace}
                      conversationId={currentConvId}
                      refreshSignal={messages.length}
                    />
                  </section>

                  <section>
                    <WorktreePanel
                      workspacePath={chatWorkspace}
                      conversationId={currentConvId}
                      refreshSignal={messages.length}
                    />
                  </section>

                  <section>
                    <SubAgentPanel
                      parentConversationId={currentConvId}
                      workspacePath={chatWorkspace}
                      agent={runtimeAgentId}
                      agentDisplay={activeAgent}
                      permission={permissionPolicy === "full_access" ? { kind: "full_access", confirmed: fullAccessConfirmed } : { kind: permissionPolicy }}
                      refreshSignal={messages.length}
                      onOpenConversation={onSelectConversation}
                    />
                  </section>

                  <section>
                    <div className="mb-2 text-xs font-semibold text-muted-foreground">文件</div>
                    <div className="max-h-[48vh] overflow-y-auto">
                      {workspaceSnapshot.files.map((entry) => (
                        <button
                          key={entry.path}
                          className="flex w-full min-w-0 items-center gap-1.5 rounded py-1 pr-1 text-left text-xs hover:bg-muted/25"
                          style={{ paddingLeft: `${entry.depth * 12 + 4}px` }}
                          onClick={() => entry.is_dir ? void openWorkspaceEntry(entry.path) : setPreviewPath(entry.path)}
                          title={entry.path}
                        >
                          {entry.is_dir ? <Folder className="h-3.5 w-3.5 shrink-0 text-warning" /> : <FileCode2 className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />}
                          <span className="truncate">{entry.name}</span>
                        </button>
                      ))}
                      {workspaceSnapshot.truncated && <div className="mt-2 text-xs text-muted-foreground">仅显示前 600 项</div>}
                    </div>
                  </section>
                </div>
              ) : (
                <div className="text-sm text-destructive">工作区不可访问</div>
              )
            ) : (
              <div className="rounded-md border border-dashed border-border p-4 text-sm text-muted-foreground">
                普通对话不绑定工作区。需要开发项目时，从左侧选择工作区或创建工作会话。
              </div>
            )}
          </div>
        </aside>
      )}

      {previewPath && isWorkspaceMode && (
        <FilePreviewPanel
          workspacePath={chatWorkspace}
          relativePath={previewPath}
          onClose={() => setPreviewPath(null)}
        />
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
    <div className="flex min-w-0 flex-1 items-center gap-2 overflow-x-auto">
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
    <div className="first-screen mx-auto flex min-h-full max-w-4xl flex-col items-center justify-center px-6 py-6 text-center">
      <div className="first-screen-icon mb-5 flex h-16 w-16 items-center justify-center rounded-md border border-border bg-card/70">
        <Sparkles className="h-8 w-8 text-primary" />
      </div>
      <h2 className="first-screen-title m-0 text-3xl font-semibold">今天让 {activeAgent} 做什么？</h2>
      <p className="first-screen-description mt-3 max-w-2xl text-sm leading-6 text-muted-foreground">
        先选择 Agent，再直接输入任务。复杂任务可以转团队；普通问答可以手动接入知识库。
      </p>
      {!installed && (
        <div className="mt-4 flex items-center gap-2 rounded-md border border-warning/30 bg-warning/10 px-3 py-2 text-sm text-warning">
          <AlertTriangle className="h-4 w-4" />
          当前 Agent 未检测到，仍可先整理任务，稍后到智能体页安装或配置。
        </div>
      )}
      <div className="first-screen-suggestions mt-7 grid w-full grid-cols-1 gap-2 md:grid-cols-3">
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

function formatKnowledgeContext(results: SearchResult[], selectedBases: KnowledgeBase[]) {
  const baseNames = selectedBases.map((base) => base.name).join(", ") || "已选择知识库";
  return [
    `[知识库检索结果: ${baseNames}]`,
    ...results.map((result, index) =>
      `[${index + 1}] ${result.content}\n来源：${result.knowledge_base_name} / ${result.document_title}`
    ),
  ].join("\n\n");
}

function ApprovalCard({
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

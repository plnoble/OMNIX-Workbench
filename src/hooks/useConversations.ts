/**
 * useConversations — Conversation, runtime session, and chat management
 *
 * This is the most complex hook, managing:
 * - Conversation list and CRUD
 * - Active agent selection and detection
 * - Chat message state and sending
 * - Runtime session lifecycle and its event stream (`agent-session-event`)
 *
 * PTY（「兼容终端」）那条链已整体删除：它的入口 `start_agent_session` 没有任何
 * 调用方，所以 PTY 会话根本建不出来，`agent-output` 事件也就永远不会发。
 */

import { useState, useEffect, useCallback, useRef } from "react";
import { toast } from "sonner";
import { listen, emit } from "@tauri-apps/api/event";
import { conversationApi, agentApi, runtimeApi, checkpointApi, modelApi, distillationApi, type ConversationGoal, type ConversationGoalStatus } from "@/lib/tauri-api";
import { getRuntimeAgentId, loadAgentRegistry } from "@/lib/agentRegistry";
import { parseGoalCommand, parseBtwCommand, parseProposalCommand, type GoalCommand } from "@/lib/slashCommands";
import { buildProposalPrompt } from "@/lib/decisionBlock";
import { AGENT_NAMES } from "@/lib/constants";
import type {
  AcpModelOption,
  ChatImageAttachment,
  ConversationInfo,
  ConversationMessage,
  DetectedAgent,
  DevStatus,
  GatewayStatus,
  StatusChangeEvent,
  RuntimeAgentId,
  RuntimeApprovalRequest,
  RuntimeModelSelection,
  RuntimePermissionPolicy,
  RuntimeSessionEvent,
  WorkMode,
} from "@/types";

export interface RuntimeSendConfig {
  model: RuntimeModelSelection;
  permission: RuntimePermissionPolicy;
  workMode: WorkMode;
}

// Single source: backend agent registry via src/lib/agentRegistry.
const runtimeAgentId = getRuntimeAgentId;

/** 一个会话是不是「工作」会话（绑了具体工作区）。 */
export function conversationIsWork(conv: { workspace_path?: string | null }): boolean {
  return !!conv.workspace_path && conv.workspace_path !== "direct";
}

/** `pickConversationForSurface` 的结论。 */
export type SurfacePick =
  | { kind: "keep" }                       // 当前会话就合适，什么都别动
  | { kind: "select"; id: string }         // 切到这一个
  | { kind: "blank" };                     // 清空，开一个新的空编辑器

/**
 * 切 Agent / 切「对话⇄工作」时该显示哪个会话。
 *
 * 抽成纯函数是因为这里出过一个很难查的 bug：**切个页签回来历史就没了**。
 * 原因是「当前会话 id 有值、但它还不在 `conversations` 列表里」被当成了
 * 「会话不存在」，一路走到清空。而那只说明列表还没刷新——刚发出第一条消息
 * 就切走页签，新会话尚未回填。
 *
 * 判断混在 setState 里时没法单独验，只能靠手点。现在能穷举。
 */
export function pickConversationForSurface(args: {
  agent: string;
  surface: "chat" | "work";
  conversations: { id: string; active_agent: string; workspace_path?: string | null; created_at: string }[];
  currentConvId: string;
}): SurfacePick {
  const wantWork = args.surface === "work";
  const current = args.conversations.find((conv) => conv.id === args.currentConvId);
  if (current && current.active_agent === args.agent && conversationIsWork(current) === wantWork) {
    return { kind: "keep" };
  }
  // 有 id 却不在列表里 = 列表没刷新，不是会话不存在。清空会吃掉用户的历史。
  if (!current && args.currentConvId) {
    return { kind: "keep" };
  }
  // 「对话」恢复该 Agent 最近一条普通会话；「工作」永远从干净的工作区选择开始，
  // 而不是悄悄重开上一个工作区。
  if (!wantWork) {
    const candidates = args.conversations
      .filter((conv) => conv.active_agent === args.agent && !conversationIsWork(conv))
      .sort((a, b) => b.created_at.localeCompare(a.created_at));
    if (candidates.length > 0) {
      return { kind: "select", id: candidates[0].id };
    }
  }
  return { kind: "blank" };
}

/**
 * 主窗口发给悬浮状态坞的那条状态。
 *
 * 抽成纯函数的理由和 `pickConversationForSurface` 一样——它此前是 useEffect 里
 * 的一段内联对象字面量，字段名和坞里读的那份**完全不一样**（发 active_agent /
 * session_id / gateway_status，读 status / text），坞里因此常年读到 undefined。
 * `emit` 的载荷类型是 unknown、`listen<T>` 只是断言，两端各写一份就没人拦得住。
 * 现在两端共用 `StatusChangeEvent`，并且这里能被单独验。
 */
export function buildDockStatus(args: {
  activeAgent: string;
  gatewayStatus: GatewayStatus;
  waitingForApproval: boolean;
  working: boolean;
}): StatusChangeEvent {
  const status: DevStatus = args.gatewayStatus === "error"
    ? "error"
    : args.waitingForApproval
      ? "pending"
      : args.working
        ? "busy"
        : "idle";
  const suffix = args.waitingForApproval ? "等待审批" : args.working ? "运行中" : "就绪";
  return { status, text: `${args.activeAgent} · ${suffix}` };
}

export interface UseConversationsReturn {
  // Conversation state
  conversations: ConversationInfo[];
  currentConvId: string;
  messages: ConversationMessage[];
  chatInput: string;
  chatWorkspace: string;
  detectedAgents: DetectedAgent[];
  activeAgent: string;
  activeSessions: string[];
  collabStdin: string;
  pendingApproval: RuntimeApprovalRequest | null;
  startingConversations: string[]; // conversations awaiting session start / first token

  // Workspace modal
  isWorkspaceModalOpen: boolean;
  workspaceFormPath: string;

  // Actions
  setChatInput: (v: string) => void;
  setChatWorkspace: (v: string) => void;
  setActiveAgent: (v: string) => void; // Accepts any agent name string
  selectAgent: (name: string) => void; // Switch Agent and load that Agent's conversation
  enterSurface: (surface: "chat" | "work") => void; // Switch between 对话 and 工作
  setCollabStdin: (v: string) => void;
  setIsWorkspaceModalOpen: (v: boolean) => void;
  setWorkspaceFormPath: (v: string) => void;

  loadConversations: () => Promise<void>;
  detectAgents: () => Promise<void>;
  selectConversation: (id: string) => Promise<void>;
  newConversation: () => void;
  saveWorkspaceChat: () => Promise<void>;
  deleteConversation: (id: string, event: React.MouseEvent) => Promise<void>;
  archiveConversation: (id: string, distill: boolean) => Promise<void>;
  unarchiveConversation: (id: string) => Promise<void>;
  loadArchivedConversations: () => Promise<void>;
  archivedConversations: ConversationInfo[];
  sendMessage: (e: React.FormEvent, config: RuntimeSendConfig, searchContext?: string, images?: ChatImageAttachment[]) => Promise<void>;
  // Long-term goal for the current conversation (/goal)
  activeGoal: ConversationGoal | null;
  setGoalStatus: (status: ConversationGoalStatus) => Promise<void>;
  clearActiveGoal: () => Promise<void>;
  // Send assembled text as a turn (SDD clarify / plan prompts)
  sendPreparedMessage: (agentText: string, displayText: string, config: RuntimeSendConfig) => Promise<void>;
  respondToApproval: (approved: boolean, forSession?: boolean) => Promise<void>;
  stopAgentSession: (sessionId: string) => Promise<void>;
  acpModelOptions: Record<string, AcpModelOption>;
  setSessionModel: (conversationId: string, model: string) => Promise<void>;
}

export function useConversations(
  gatewayStatus: GatewayStatus,
): UseConversationsReturn {
  const [conversations, setConversations] = useState<ConversationInfo[]>([]);
  const [archivedConversations, setArchivedConversations] = useState<ConversationInfo[]>([]);
  const [currentConvId, setCurrentConvId] = useState("");
  const [messages, setMessages] = useState<ConversationMessage[]>([]);
  const [activeGoal, setActiveGoal] = useState<ConversationGoal | null>(null);
  const [chatInput, setChatInput] = useState("");
  const [chatWorkspace, setChatWorkspace] = useState("direct");
  const [detectedAgents, setDetectedAgents] = useState<DetectedAgent[]>([]);
  const [activeAgent, setActiveAgent] = useState<string>(AGENT_NAMES[0]);
  const [runtimeActiveConversations, setRuntimeActiveConversations] = useState<string[]>([]);
  const [collabStdin, setCollabStdin] = useState("");
  const [pendingApproval, setPendingApproval] = useState<RuntimeApprovalRequest | null>(null);
  const [startingConversations, setStartingConversations] = useState<string[]>([]);
  // ACP agents expose their selectable model via the session start event; keyed
  // by conversation id so the composer can show a model picker for that agent.
  const [acpModelOptions, setAcpModelOptions] = useState<Record<string, AcpModelOption>>({});
  const [currentSurface, setCurrentSurface] = useState<"chat" | "work">("chat");
  // PTY 那一半没了：`ptySessions` 由一个从来没有发送方的 `active-sessions-update`
  // 事件喂养，恒为空数组。现在只剩 runtime 会话这一个真实来源。
  const activeSessions = Array.from(new Set(runtimeActiveConversations));

  // Workspace modal
  const [isWorkspaceModalOpen, setIsWorkspaceModalOpen] = useState(false);
  const [workspaceFormPath, setWorkspaceFormPath] = useState("");

  // Refs for cross-render access
  const currentConvIdRef = useRef(currentConvId);
  // enterSurface 的去重依据。用 ref 不用 state：它要在同一轮事件里立刻反映
  // 最新值，而 setCurrentSurface 要等下一次渲染。
  const currentSurfaceRef = useRef<"chat" | "work">("chat");
  const runtimeSessionByConversationRef = useRef<Record<string, string>>({});
  const conversationByRuntimeSessionRef = useRef<Record<string, string>>({});
  const activeRuntimeConversationsRef = useRef(runtimeActiveConversations);
  const sendInFlightRef = useRef(false);
  currentConvIdRef.current = currentConvId;
  activeRuntimeConversationsRef.current = runtimeActiveConversations;

  // ── Agent registry (backend-driven, mount once) ────
  useEffect(() => {
    void loadAgentRegistry();
  }, []);

  useEffect(() => {
    const unlistenRuntime = listen<RuntimeSessionEvent>("agent-session-event", (event) => {
      void (async () => {
        const { session_id: sessionId, event: runtimeEvent } = event.payload;
        let conversationId = conversationByRuntimeSessionRef.current[sessionId];
        if (!conversationId) {
          try {
            const session = await runtimeApi.getSession(sessionId);
            conversationId = session.config.conversation_id;
            conversationByRuntimeSessionRef.current[sessionId] = conversationId;
            runtimeSessionByConversationRef.current[conversationId] = sessionId;
          } catch {
            return;
          }
        }

        // Once any real runtime output arrives, the "starting" indicator is done.
        if (["assistant_delta", "assistant_message", "plan", "tool_completed", "tool_started", "approval_requested", "turn_completed", "error"].includes(runtimeEvent.kind)) {
          setStartingConversations((current) => current.filter((id) => id !== conversationId));
        }

        if (runtimeEvent.kind === "session_started") {
          setRuntimeActiveConversations((current) =>
            current.includes(conversationId) ? current : [...current, conversationId]
          );
          // Capture the ACP agent's selectable model, if it advertised one, so
          // the composer can offer a model picker for this conversation. A new
          // session WITHOUT options (agent switched to Claude/Codex/Gemini)
          // must clear the stale entry, or the previous agent's model dropdown
          // keeps rendering for this conversation.
          const modelOption = runtimeEvent.metadata?.acp_model_option as
            | AcpModelOption
            | undefined;
          setAcpModelOptions((current) => {
            const next = { ...current };
            if (modelOption && Array.isArray(modelOption.options) && modelOption.options.length > 0) {
              next[conversationId] = modelOption;
            } else {
              delete next[conversationId];
            }
            return next;
          });
        }
        if (runtimeEvent.kind === "error") {
          setRuntimeActiveConversations((current) => current.filter((id) => id !== conversationId));
          if (conversationId === currentConvIdRef.current) {
            setMessages((current) => [
              ...current.filter((message) => message.id !== `runtime_stream_${sessionId}`),
              {
                id: `runtime_error_${Date.now()}`,
                conversation_id: conversationId,
                role: "assistant",
                content: `运行失败：${runtimeEvent.text || "未知错误"}`,
                timestamp: new Date().toISOString(),
              },
            ]);
          }
        }

        // `raw_log` 曾在这里被拼进一个 256 KB 的内存缓冲，然后……没有任何组件
        // 渲染它。而这些事件早已由 `record_runtime_event` 全量写进 SQLite 的
        // `runtime_events`（不截断、重启还在，`runtime_get_events` 就能读）。
        // 也就是说那份内存拷贝是同一批数据里更差的一份，还让当前会话每收到一行
        // 未识别的协议消息就触发一次全应用重渲染。已删除。

        if (conversationId === currentConvIdRef.current && runtimeEvent.kind === "assistant_delta") {
          const delta = runtimeEvent.text || "";
          setMessages((current) => {
            const streamId = `runtime_stream_${sessionId}`;
            const existingIndex = current.findIndex((message) => message.id === streamId);
            if (existingIndex === -1) {
              return [
                ...current,
                {
                  id: streamId,
                  conversation_id: conversationId,
                  role: "assistant",
                  content: delta,
                  timestamp: new Date().toISOString(),
                },
              ];
            }
            const updated = [...current];
            updated[existingIndex] = {
              ...updated[existingIndex],
              content: `${updated[existingIndex].content}${delta}`,
            };
            return updated;
          });
        }

        if (
          conversationId === currentConvIdRef.current
          && ["assistant_message", "plan", "tool_completed"].includes(runtimeEvent.kind)
        ) {
          const persisted = await conversationApi.getMessages(conversationId);
          setMessages(persisted);
        }

        if (runtimeEvent.kind === "approval_requested" && runtimeEvent.request_id) {
          const approvalMethod = typeof runtimeEvent.metadata.method === "string"
            ? runtimeEvent.metadata.method
            : "item/commandExecution/requestApproval";
          const params = runtimeEvent.metadata.params as Record<string, unknown> | undefined;
          setPendingApproval({
            session_id: sessionId,
            request_id: runtimeEvent.request_id,
            approval_method: approvalMethod,
            requested_permissions: params?.permissions ?? null,
            title: runtimeEvent.text || "Agent 请求执行操作",
            detail: JSON.stringify(runtimeEvent.metadata, null, 2),
          });
        }
        if (["turn_completed", "error"].includes(runtimeEvent.kind)) {
          setPendingApproval((current) =>
            current?.session_id === sessionId ? null : current
          );
        }
      })();
    });

    return () => {
      unlistenRuntime.then((unlisten) => unlisten());
    };
  }, []);

  // ── Status Dock events bridge ──────────────────────

  useEffect(() => {
    const unlistenNewConv = listen("omnix-action-new-conversation", () => {
      newConversation();
    });
    const unlistenSettings = listen("omnix-action-open-settings", () => {
      // This will be handled by MainApp's handleTabChange
      // We emit a custom event that MainApp can listen to
      emit("omnix-navigate-settings", {}).catch(() => {});
    });

    return () => {
      unlistenNewConv.then((fn) => fn());
      unlistenSettings.then((fn) => fn());
    };
  }, []); // eslint-disable-line react-hooks/exhaustive-deps -- listeners registered once; callbacks accessed via ref

  // ── Persist status updates to StatusDock window ────

  useEffect(() => {
    const payload = buildDockStatus({
      activeAgent,
      gatewayStatus,
      waitingForApproval: !!pendingApproval,
      working: startingConversations.length > 0 || activeSessions.length > 0,
    });
    emit("omnix-dev-status-change", payload).catch((e) =>
      console.error("[useConversations] Emit error:", e)
    );
  }, [activeAgent, gatewayStatus, activeSessions, pendingApproval, startingConversations]);

  // ── Agent Detection ────────────────────────────────

  const detectAgents = useCallback(async () => {
    try {
      const list = await agentApi.detectInstalled();
      setDetectedAgents(list);
    } catch (e) {
      console.error("[useConversations] Failed to detect agents:", e);
    }
  }, []);

  // ── Conversation CRUD ──────────────────────────────

  const loadConversations = useCallback(async () => {
    try {
      const list = await conversationApi.list();
      setConversations(list);
    } catch (e) {
      console.error("[useConversations] Failed to load conversations:", e);
    }
  }, []);

  const selectConversation = useCallback(async (id: string) => {
    setCurrentConvId(id);
    try {
      const msgs = await conversationApi.getMessages(id);
      setMessages(msgs);
    } catch (e) {
      console.error("[useConversations] Failed to load messages:", id, e);
      setMessages([]);
    }

    // Load this conversation's long-term goal (/goal) so the badge
    // and the per-turn injection reflect it.
    try {
      setActiveGoal(await conversationApi.getGoal(id));
    } catch {
      setActiveGoal(null);
    }

    try {
      const runtimeSessions = await runtimeApi.listConversationSessions(id);
      const latestRuntimeSession = runtimeSessions[runtimeSessions.length - 1];
      if (latestRuntimeSession) {
        runtimeSessionByConversationRef.current[id] = latestRuntimeSession.id;
        conversationByRuntimeSessionRef.current[latestRuntimeSession.id] = id;
      }
    } catch (e) {
      console.error("[useConversations] Failed to load runtime session:", id, e);
    }

    const conv = conversations.find((c) => c.id === id);
    if (conv) {
      setActiveAgent(conv.active_agent);
      setChatWorkspace(conv.workspace_path);

    }
  }, [conversations]);

  const newConversation = useCallback(() => {
    setCurrentConvId("");
    setMessages([]);
    setActiveGoal(null);
    setChatInput("");
    setPendingApproval(null);
    // A fresh conversation is unbound; the 工作 surface will then prompt for a workspace.
    setChatWorkspace("direct");
  }, []);

  // A conversation belongs to the 工作 (workspace) surface when it is bound to a
  // real workspace; otherwise it is a plain 对话 conversation.

  // Show the active Agent's latest conversation for a surface (对话 / 工作), or a
  // fresh empty composer when that (Agent, surface) pair has no conversation yet.
  const showLatestConversation = useCallback((agent: string, surface: "chat" | "work") => {
    const pick = pickConversationForSurface({
      agent,
      surface,
      conversations,
      currentConvId: currentConvIdRef.current,
    });
    if (pick.kind === "keep") return;
    if (pick.kind === "select") {
      void selectConversation(pick.id);
      return;
    }
    setCurrentConvId("");
    setMessages([]);
    setChatInput("");
    setPendingApproval(null);
    setChatWorkspace("direct");
  }, [conversations, selectConversation]);

  // Switching the active Agent switches to that Agent's own latest conversation
  // within the current surface so each Agent keeps independent history.
  const selectAgent = useCallback((name: string) => {
    if (name === activeAgent) return;
    setActiveAgent(name);
    showLatestConversation(name, currentSurface);
  }, [activeAgent, currentSurface, showLatestConversation]);

  // Entering the 对话 / 工作 surface shows that surface's conversation (and pins
  // plain conversations to no workspace).
  const enterSurface = useCallback((surface: "chat" | "work") => {
    // App 在每次切到 chat/work 时都调这里，包括从「模型」「设置」切回来——
    // 那种情况表面根本没变，重新定位会话只会把用户正看着的对话换掉。
    // 只有真正在 对话 ⇄ 工作 之间切换时才需要重新选会话。
    if (currentSurfaceRef.current === surface) return;
    currentSurfaceRef.current = surface;
    setCurrentSurface(surface);
    if (surface === "chat") setChatWorkspace("direct");
    showLatestConversation(activeAgent, surface);
  }, [activeAgent, showLatestConversation]);

  const saveWorkspaceChat = useCallback(async () => {
    if (!workspaceFormPath.trim()) {
      throw new Error("请输入项目路径");
    }

    setIsWorkspaceModalOpen(false);
    const workspaceName = workspaceFormPath.split(/[\\/]/).pop() || "Workspace";
    const id = `conv_${Date.now()}`;

    const newConv: ConversationInfo = {
      id,
      title: `项目: ${workspaceName}`,
      workspace_path: workspaceFormPath,
      active_agent: activeAgent,
      created_at: new Date().toISOString(),
    };

    await conversationApi.create({
      id,
      title: newConv.title,
      workspacePath: newConv.workspace_path,
      activeAgent: newConv.active_agent,
    });

    await loadConversations();
    await selectConversation(id);
    setWorkspaceFormPath("");
  }, [workspaceFormPath, activeAgent, loadConversations, selectConversation]);

  const deleteConversation = useCallback(async (id: string, event: React.MouseEvent) => {
    event.stopPropagation();
    try {
      await conversationApi.delete(id);
    } catch (e) {
      console.error("[useConversations] Failed to delete conversation:", e);
      throw e;
    }
    if (currentConvId === id) {
      newConversation();
    }
    await loadConversations();
  }, [currentConvId, loadConversations, newConversation]);

  const archiveConversation = useCallback(async (id: string, distill: boolean) => {
    // Optionally distill the conversation into the evolution inbox before
    // archiving. Distillation is best-effort: a failure (or no model) never
    // blocks the archive, so a low-value chat can always just be archived.
    if (distill) {
      try {
        const models = await modelApi.getActive();
        const model = models[0];
        if (!model) {
          toast.warning("没有可用模型，已直接归档（未蒸馏）");
        } else {
          const modelId = `${model.platform_id}:${model.model_name}`;
          toast.info("正在蒸馏后归档…");
          const candidates = await distillationApi.generate(id, modelId);
          toast.success(candidates.length > 0
            ? `已蒸馏 ${candidates.length} 条候选（进化中枢待审），并归档`
            : "本次对话无可蒸馏内容，已归档");
        }
      } catch (e) {
        toast.error(`蒸馏失败，仍已归档：${e}`);
      }
    }
    try {
      await conversationApi.archive(id);
    } catch (e) {
      console.error("[useConversations] Failed to archive conversation:", e);
      throw e;
    }
    if (currentConvId === id) {
      newConversation();
    }
    await loadConversations();
  }, [currentConvId, loadConversations, newConversation]);

  const unarchiveConversation = useCallback(async (id: string) => {
    try {
      await conversationApi.unarchive(id);
    } catch (e) {
      console.error("[useConversations] Failed to unarchive conversation:", e);
      throw e;
    }
    await loadConversations();
    try {
      const list = await conversationApi.listArchived();
      setArchivedConversations(list);
    } catch (e) {
      console.error("[useConversations] Failed to reload archived list:", e);
    }
  }, [loadConversations]);

  const loadArchivedConversations = useCallback(async () => {
    try {
      const list = await conversationApi.listArchived();
      setArchivedConversations(list);
    } catch (e) {
      console.error("[useConversations] Failed to load archived conversations:", e);
    }
  }, []);

  // ── PTY Session Management ─────────────────────────

  // Switch the model of a running ACP session (opencode etc.). The choice is
  // applied live via `session/set_config_option` and remembered per-agent.
  const setSessionModel = useCallback(async (conversationId: string, model: string) => {
    const sessionId = runtimeSessionByConversationRef.current[conversationId];
    if (!sessionId) {
      toast.error("会话尚未启动，发一条消息后即可切换模型");
      return;
    }
    try {
      const outcome = await runtimeApi.setSessionModel(sessionId, model);
      setAcpModelOptions((current) => {
        const existing = current[conversationId];
        if (!existing) return current;
        return { ...current, [conversationId]: { ...existing, current: model } };
      });
      toast.success(outcome || `模型已切换：${model}`);
    } catch (error) {
      toast.error("切换模型失败", { description: String(error) });
    }
  }, []);

  // Core turn delivery: append the user bubble, ensure a runtime session, and
  // send. Shared by sendMessage and the /btw branch handler so a branched
  // conversation reuses the exact same session/handoff/resume logic.
  const deliverTurn = useCallback(async (
    convId: string,
    agent: RuntimeAgentId,
    displayContent: string,
    agentContent: string,
    config: RuntimeSendConfig,
    images?: ChatImageAttachment[],
  ) => {
    if (sendInFlightRef.current) return;
    sendInFlightRef.current = true;
    try {
    // Append user message immediately (display original question). Attachment
    // previews ride in metadata so the bubble shows thumbnails right away; the
    // persisted row later carries file paths instead.
    const userMsg: ConversationMessage = {
      id: `msg_u_${Date.now()}`,
      conversation_id: convId,
      role: "user",
      content: displayContent,
      timestamp: new Date().toISOString(),
      metadata_json: images && images.length > 0
        ? JSON.stringify({ attachment_previews: images.map((image) => image.preview) })
        : undefined,
    };
    setMessages((prev) => [...prev, userMsg]);
    // Show a waiting indicator until the session starts and the first token arrives.
    // First Codex start can take a while (it boots MCP servers during thread/start).
    setStartingConversations((current) => current.includes(convId) ? current : [...current, convId]);

    // Auto-checkpoint before a workspace-modifying turn (Direct mode + real
    // workspace), so the user can review the diff and rewind. No-op / skipped
    // for non-Git workspaces; never blocks the turn.
    if (config.workMode === "direct" && chatWorkspace && chatWorkspace !== "direct") {
      const snippet = displayContent.slice(0, 40);
      checkpointApi.create(chatWorkspace, convId, snippet || "改动前检查点").catch(() => undefined);
    }

    const inputMsg = agentContent.trim() || "请查看附带的图片。";

    const startRuntimeSession = async () => {
      const session = await runtimeApi.startSession({
        conversation_id: convId,
        agent,
        workspace_path: chatWorkspace,
        model: config.model,
        permission: config.permission,
        work_mode: config.workMode,
      });
      runtimeSessionByConversationRef.current[convId] = session.id;
      conversationByRuntimeSessionRef.current[session.id] = convId;
      setRuntimeActiveConversations((current) =>
        current.includes(convId) ? current : [...current, convId]
      );
      return session;
    };

    try {
      let sessionId: string | undefined = runtimeSessionByConversationRef.current[convId];
      let session = sessionId ? await runtimeApi.getSession(sessionId).catch(() => null) : null;
      if (!session) {
        const historical = await runtimeApi.listConversationSessions(convId);
        session = historical[historical.length - 1] ?? null;
        sessionId = session?.id;
      }

      const sessionDead = !!session && (
        session.status === "cancelled"
        || session.status === "failed"
        || session.status === "completed"
        || session.status === "stopping"
      );
      const configChanged = !!session && (
        session.config.agent !== agent
        || session.config.work_mode !== config.workMode
        || session.config.permission.kind !== config.permission.kind
        || JSON.stringify(session.config.model) !== JSON.stringify(config.model)
      );
      // Hand off the prior transcript when the user switched this conversation to
      // a DIFFERENT agent, so the new agent continues with context. Gated by a
      // persisted toggle; the backend no-ops if there's no prior transcript.
      const priorAgent = session?.config.agent;
      const handoffEnabled = (localStorage.getItem("omnix_agent_handoff") ?? "true") !== "false";
      const isHandoff = handoffEnabled && !!priorAgent && priorAgent !== agent;
      if (!session || sessionDead || configChanged || !sessionId) {
        if (configChanged && sessionId && activeRuntimeConversationsRef.current.includes(convId)) {
          await runtimeApi.stopSession(sessionId).catch((error) => {
            console.warn("[useConversations] Failed to stop superseded runtime session:", error);
          });
        }
        session = await startRuntimeSession();
        sessionId = session.id;
      } else {
        runtimeSessionByConversationRef.current[convId] = sessionId;
        conversationByRuntimeSessionRef.current[sessionId] = convId;
      }

      if (isHandoff) {
        toast.info(`已把此前对话的上下文交接给 ${activeAgent}`);
      }
      const wireImages = images?.map((image) => ({ mime: image.mime, data: image.data }));
      try {
        await runtimeApi.sendMessage(sessionId, inputMsg, displayContent, isHandoff, wireImages);
      } catch (error) {
        const canResume = !!session.external_session_id;
        if (!canResume || !String(error).includes("not running")) throw error;
        await runtimeApi.resumeSession(sessionId);
        setRuntimeActiveConversations((current) =>
          current.includes(convId) ? current : [...current, convId]
        );
        await runtimeApi.sendMessage(sessionId, inputMsg, displayContent, isHandoff, wireImages);
      }
    } catch (err) {
      console.error("[useConversations] Failed to send runtime message:", err);
      setStartingConversations((current) => current.filter((id) => id !== convId));
      setMessages((current) => [
        ...current,
        {
          id: `runtime_start_error_${Date.now()}`,
          conversation_id: convId,
          role: "assistant",
          content: `无法开始 Agent 会话：${String(err)}`,
          timestamp: new Date().toISOString(),
        },
      ]);
      throw err;
    }
    } finally {
      sendInFlightRef.current = false;
    }
  }, [activeAgent, chatWorkspace]);

  // ── /goal controls (exposed for the goal badge buttons) ──
  const setGoalStatus = useCallback(async (status: ConversationGoalStatus) => {
    if (!currentConvId) return;
    try {
      setActiveGoal(await conversationApi.setGoalStatus(currentConvId, status));
    } catch (error) {
      toast.error(`目标操作失败：${error}`);
    }
  }, [currentConvId]);

  const clearActiveGoal = useCallback(async () => {
    if (!currentConvId) return;
    try {
      await conversationApi.clearGoal(currentConvId);
      setActiveGoal(null);
      toast.success("已清除长期目标");
    } catch (error) {
      toast.error(`清除目标失败：${error}`);
    }
  }, [currentConvId]);

  // ── /goal slash command — never sent to the agent ──
  const handleGoalCommand = useCallback(async (cmd: GoalCommand) => {
    if (!currentConvId) {
      toast.error("先开始一段对话，再设定长期目标");
      return;
    }
    setChatInput("");
    try {
      if (cmd.action === "menu") {
        toast.info(
          activeGoal
            ? `当前目标（${activeGoal.status}）：${activeGoal.objective}`
            : "用法：/goal <目标>　·　/goal pause|resume|complete|clear",
        );
      } else if (cmd.action === "set") {
        setActiveGoal(await conversationApi.setGoal(currentConvId, cmd.objective));
        toast.success("已设定长期目标，之后每轮都会提醒 Agent 朝它推进");
      } else if (cmd.action === "clear") {
        await conversationApi.clearGoal(currentConvId);
        setActiveGoal(null);
        toast.success("已清除长期目标");
      } else {
        const status: ConversationGoalStatus =
          cmd.action === "pause" ? "paused" : cmd.action === "resume" ? "active" : "complete";
        setActiveGoal(await conversationApi.setGoalStatus(currentConvId, status));
        toast.success(
          status === "paused" ? "目标已暂停（暂不注入）"
            : status === "active" ? "目标已继续"
            : "目标已标记完成（不再注入）",
        );
      }
    } catch (error) {
      toast.error(`目标操作失败：${error}`);
    }
  }, [currentConvId, activeGoal]);

  // ── /btw slash command — open a side conversation that
  // inherits the current context, then send the question into it ──
  const handleBtwCommand = useCallback(async (question: string | null, config: RuntimeSendConfig) => {
    if (!currentConvId) {
      toast.error("先在一段对话里，才能开旁支");
      return;
    }
    if (!question) {
      toast.info("用法：/btw 你想岔开讨论的问题");
      return;
    }
    const agent = runtimeAgentId(activeAgent);
    if (!agent) {
      toast.error("当前 Agent 尚未完成真实运行适配");
      return;
    }
    const branchId = `conv_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`;
    const title = `↳ ${question.length > 14 ? question.slice(0, 14) + "…" : question}`;
    try {
      await conversationApi.create({
        id: branchId,
        title,
        workspacePath: chatWorkspace,
        activeAgent,
        parentConversationId: currentConvId,
      });
      await loadConversations();
      setCurrentConvId(branchId);
      currentConvIdRef.current = branchId;
      setMessages([]);      // the branch view starts fresh (parent context is agent-only)
      setActiveGoal(null);  // a fresh branch carries no goal
      setChatInput("");
      toast.info("已开旁支，带着主对话的上下文继续");
      // Backend seeds the parent's transcript into this first turn (parent link + empty branch).
      await deliverTurn(branchId, agent, question, question, config);
    } catch (error) {
      toast.error(`开旁支失败：${error}`);
    }
  }, [currentConvId, activeAgent, chatWorkspace, loadConversations, deliverTurn]);

  // Sends a prepared message (assembled text, not from the composer) as a turn.
  // Used by the SDD flow to send the clarify / plan-generation prompts while
  // showing a short summary in the bubble. Creates a conversation if needed.
  const sendPreparedMessage = useCallback(async (
    agentText: string,
    displayText: string,
    config: RuntimeSendConfig,
  ) => {
    const agent = runtimeAgentId(activeAgent);
    if (!agent) {
      toast.error(`${activeAgent} 尚未完成真实运行适配`);
      return;
    }
    let convId = currentConvId;
    if (!convId) {
      convId = await createConversationFromPrompt(displayText);
    }
    await deliverTurn(convId, agent, displayText, agentText, config);
  }, [activeAgent, currentConvId, deliverTurn]);

  const sendMessage = useCallback(async (
    e: React.FormEvent,
    config: RuntimeSendConfig,
    searchContext?: string,
    images?: ChatImageAttachment[],
  ) => {
    e.preventDefault();
    if (!chatInput.trim() && !(images && images.length > 0)) return;

    // Slash-command interception: /goal and /btw are handled
    // locally and never forwarded to the agent as a normal message.
    const goalCmd = parseGoalCommand(chatInput);
    if (goalCmd) {
      await handleGoalCommand(goalCmd);
      return;
    }
    const btwCmd = parseBtwCommand(chatInput);
    if (btwCmd) {
      await handleBtwCommand(btwCmd.question, config);
      return;
    }

    const agent = runtimeAgentId(activeAgent);
    if (!agent) {
      throw new Error(`${activeAgent} 尚未完成真实运行适配，请选择 Claude Code、Codex、Gemini CLI、Qwen Code、OpenCode 或 GitHub Copilot CLI`);
    }

    // 方案抉择框 (#2): `/方案 <需求>` wraps the requirement in a prompt that asks
    // the agent to reply with 2-4 schemes as an interactive omnix-decision block.
    const proposalCmd = parseProposalCommand(chatInput);
    if (proposalCmd) {
      if (!proposalCmd.requirement) {
        setChatInput("");
        toast.info("用法：/方案 <你的需求> —— 让 AI 提出几个方案供你单选/多选");
        return;
      }
      const displayText = `🧭 方案抉择：${proposalCmd.requirement}`;
      const agentText = buildProposalPrompt(proposalCmd.requirement);
      let proposalConvId = currentConvId;
      if (!proposalConvId) {
        proposalConvId = await createConversationFromPrompt(displayText);
      }
      setChatInput("");
      await deliverTurn(proposalConvId, agent, displayText, agentText, config, images);
      return;
    }

    let convId = currentConvId;
    if (!convId) {
      convId = await createConversationFromPrompt(chatInput);
    }

    // Build message content — inject extra context if provided (search results,
    // knowledge, cross-agent @ references). The caller already labels each block
    // ([联网搜索结果] / [引用…]), so we just append it under the user's text.
    const displayContent = chatInput.trim() || "（图片）";
    const agentContent = searchContext
      ? `${chatInput}\n\n---\n${searchContext}`
      : chatInput;
    setChatInput("");
    await deliverTurn(convId, agent, displayContent, agentContent, config, images);
  }, [activeAgent, chatInput, currentConvId, deliverTurn, handleGoalCommand, handleBtwCommand]);

  const respondToApproval = useCallback(async (approved: boolean, forSession = false) => {
    if (!pendingApproval) return;
    await runtimeApi.respondApproval({
      sessionId: pendingApproval.session_id,
      requestId: pendingApproval.request_id,
      approved,
      forSession,
      approvalMethod: pendingApproval.approval_method,
      requestedPermissions: pendingApproval.requested_permissions ?? undefined,
    });
    setPendingApproval(null);
  }, [pendingApproval]);

  const stopAgentSession = useCallback(async (sessionId: string) => {
    try {
      const runtimeSessionId = runtimeSessionByConversationRef.current[sessionId];
      if (runtimeSessionId) {
        await runtimeApi.stopSession(runtimeSessionId);
        setRuntimeActiveConversations((current) => current.filter((id) => id !== sessionId));
        setPendingApproval((current) =>
          current?.session_id === runtimeSessionId ? null : current
        );
      }
    } catch (e) {
      console.error("[useConversations] Failed to stop session:", e);
      throw e;
    }
  }, []);

  // ── Helper: Create conversation from first prompt ──

  async function createConversationFromPrompt(prompt: string): Promise<string> {
    const convId = `conv_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`;
    const title = prompt.length > 15 ? prompt.slice(0, 15) + "..." : prompt;

    await conversationApi.create({
      id: convId,
      title,
      workspacePath: chatWorkspace,
      activeAgent,
    });

    await loadConversations();
    setCurrentConvId(convId);
    currentConvIdRef.current = convId;
    return convId;
  }

  return {
    conversations, currentConvId, messages, chatInput, chatWorkspace,
    detectedAgents, activeAgent, activeSessions,
    collabStdin, pendingApproval, startingConversations,
    enterSurface,
    isWorkspaceModalOpen, workspaceFormPath,
    archivedConversations,
    setChatInput, setChatWorkspace, setActiveAgent, selectAgent,
    setCollabStdin,
    setIsWorkspaceModalOpen, setWorkspaceFormPath,
    loadConversations, detectAgents, selectConversation,
    newConversation, saveWorkspaceChat, deleteConversation,
    archiveConversation, unarchiveConversation, loadArchivedConversations,
    sendMessage, respondToApproval, stopAgentSession,
    acpModelOptions, setSessionModel,
    activeGoal, setGoalStatus, clearActiveGoal,
    sendPreparedMessage,
  };
}

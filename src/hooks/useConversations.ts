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
  MessagesDelta,
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

/// 乐观气泡的 id 前缀。
///
/// 用户消息在界面上先以本地 id 出现，而**后端落库时会自己生成 `msg_agent_*`**
/// ——前端这个 id 从不进库。所以它既不能当增量游标，增量里一旦出现真的 user
/// 消息，手上这条也就该清掉了。
const OPTIMISTIC_ID_PREFIX = "msg_u_";

/// 会话列表一次取多少。
///
/// **不做无限滚动**：侧栏按 agent 分组渲染，分页会打断分组语义（翻页可能整组消失）。
/// 所以是「最近 N + 搜索」，超出的靠搜索够到，界面上把总数说出来。
export const CONVERSATION_PAGE_SIZE = 100;

/// 消息一页多少条。60 条大约几屏，往回翻一次够看一阵。
export const MESSAGE_PAGE_SIZE = 60;

/// 手上最后一条**持久化**消息的 id，用作增量游标。
///
/// 乐观气泡从不入库，拿它当游标后端一定查不到，会退回全量——那就等于没做增量。
/// 所以要跳过它们往前找。全是乐观气泡（刚发出第一句）时返回 null，让后端给全量。
export function lastPersistedMessageId(messages: ConversationMessage[]): string | null {
  for (let i = messages.length - 1; i >= 0; i -= 1) {
    if (!messages[i].id.startsWith(OPTIMISTIC_ID_PREFIX)) return messages[i].id;
  }
  return null;
}

/// 会话接不上时，先试哪条路。
///
/// `sessionDead` 涵盖了两种完全不同的情况：用户主动停掉 / 应用重启后被收敛。
/// 后者在 CLI 那边往往还留着 session，`external_session_id` 就是用来拉回来的
/// ——直接起新会话等于把几天的上下文扔掉。
///
/// 配置变了（换模型/换权限/换工作模式）则不能 resume：resume 会按**旧**配置
/// 重新拉起。
export function shouldTryResume(input: {
  sessionDead: boolean;
  hasExternalSessionId: boolean;
  configChanged: boolean;
}): boolean {
  return input.sessionDead && input.hasExternalSessionId && !input.configChanged;
}

/// 这一轮要不要把此前的对话交接给新会话。
///
/// **两种情况都要交接，第二种曾经漏掉过**：
/// 1. 用户把会话换了另一个 agent —— 新 agent 没有任何上下文；
/// 2. 旧会话接不上、只能起新会话 —— 哪怕**还是同一个 agent**。
///
/// 漏掉第 2 种的后果是静默的：同一个 agent 不触发交接，新会话完全空白，而
/// 用户看到的还是那条连续的对话，只会觉得「它怎么把刚才说的都忘了」。原文一直
/// 在 OMNIX 自己库里，没有理由不注入。
export function shouldHandoff(input: {
  handoffEnabled: boolean;
  priorAgent?: string;
  agent: string;
  /// resume 成功了就不用交接——agent 自己的上下文还在。
  resumed: boolean;
  hadSession: boolean;
  sessionDead: boolean;
  configChanged: boolean;
}): boolean {
  if (!input.handoffEnabled) return false;
  const agentChanged = !!input.priorAgent && input.priorAgent !== input.agent;
  const rebuildingContext =
    !input.resumed && input.hadSession && input.sessionDead && !input.configChanged;
  return agentChanged || rebuildingContext;
}

/// 把往回翻到的一页拼在**顶部**。
///
/// 抽成纯函数和 `mergeMessagesDelta` 同理：去重错了只表现为「界面上多了几条重复
/// 的历史」，不报任何错。并发点两次「加载更早」就会拿到同一页。
/// 把往回翻的一页接在顶部。
///
/// `requestedAnchorId` 是**发起这次请求时**列表最前面那条持久化消息的 id。
/// 响应回来时如果最前面已经不是它了，这一页就是过期响应，整页丢掉。
///
/// 借鉴 paseo 的时间线契约（AGPL-3.0，只看思想没抄代码）：一个向后页只在与
/// 当前历史起点**相邻**时才接受。少了这条校验，两种情况会把历史撕成两段不
/// 连续的，而且都不报错：
///
/// 1. 等待期间用户切了会话——上一个会话的旧消息会被前置进新会话的视图里；
/// 2. 等待期间又前置过一页——这一页接上去就跳过了中间那段。
///
/// 参数是必填的，不是可选的：可选的守卫等于没有守卫，调用方一不传就静默失效。
export function prependOlderMessages(
  current: ConversationMessage[],
  older: ConversationMessage[],
  requestedAnchorId: string,
): ConversationMessage[] {
  if (older.length === 0) return current;
  // 锚点只能是持久化消息——乐观气泡永远在最新一端，不会是列表最前面那条。
  const oldest = current.find((m) => !m.id.startsWith(OPTIMISTIC_ID_PREFIX));
  if (oldest?.id !== requestedAnchorId) return current;
  const seen = new Set(current.map((m) => m.id));
  const fresh = older.filter((m) => !seen.has(m.id));
  if (fresh.length === 0) return current;
  return [...fresh, ...current];
}

/// 把增量并进当前列表。
///
/// 抽成纯函数是为了测得到：这里有三件容易错的事，而它们错了都只表现为「界面上
/// 消息重了一条」，不会报任何错。
export function mergeMessagesDelta(
  current: ConversationMessage[],
  delta: MessagesDelta,
): ConversationMessage[] {
  // ① 全量就是替换。当成追加会把整段历史渲染两遍。
  if (delta.is_full) return delta.messages;
  if (delta.messages.length === 0) return current;

  // ② 后端落用户消息时自己生成 id，前端那个乐观 id 从不入库。所以增量里一旦
  //    出现 user 角色的消息，手上的乐观气泡就已经被取代——不清掉同一句话显示两遍。
  const superseded = delta.messages.some((m) => m.role === "user");
  const base = superseded
    ? current.filter((m) => !m.id.startsWith(OPTIMISTIC_ID_PREFIX))
    : current;

  // ③ 按 id 去重：`INSERT OR REPLACE` 会让一条消息换个 rowid 重新排到游标之后，
  //    那时它是「已有的」而不是新的。
  const seen = new Set(base.map((m) => m.id));
  return [...base, ...delta.messages.filter((m) => !seen.has(m.id))];
}

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
  /// 已加载的最早一条**之上**还剩多少条。界面必须把它显示出来——只截断不说
  /// 剩余数，用户会以为历史丢了。
  olderRemaining: number;
  /// 往上翻一页历史。滚动位置补偿由调用方负责。
  loadOlderMessages: () => Promise<void>;
  /// 会话总数（含未加载的），用于「显示最近 100 个 / 共 N 个」。
  conversationTotal: number;
  /// 按标题搜索会话（走后端；前端过滤需要全量在手，等于没分页）。
  searchConversations: (query: string) => Promise<ConversationInfo[]>;
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
  /// 会话总数（含未加载的）。界面要写「显示最近 100 个 / 共 N 个」。
  const [conversationTotal, setConversationTotal] = useState(0);
  /// 当前会话里，已加载的最早一条**之上**还有多少条。界面要写「上面还有 X 条」。
  const [olderRemaining, setOlderRemaining] = useState(0);
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
  // 增量拉取要读「当前手上最后一条的 id」当游标。事件回调是在 effect 里注册的，
  // 直接闭包 `messages` 会读到注册那一刻的旧值，所以走 ref。
  const messagesRef = useRef(messages);
  // enterSurface 的去重依据。用 ref 不用 state：它要在同一轮事件里立刻反映
  // 最新值，而 setCurrentSurface 要等下一次渲染。
  const currentSurfaceRef = useRef<"chat" | "work">("chat");
  const runtimeSessionByConversationRef = useRef<Record<string, string>>({});
  const conversationByRuntimeSessionRef = useRef<Record<string, string>>({});
  const activeRuntimeConversationsRef = useRef(runtimeActiveConversations);
  const sendInFlightRef = useRef(false);
  currentConvIdRef.current = currentConvId;
  messagesRef.current = messages;
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
          // 只拉「还没有的那几条」。原来这里是把**整个会话**重新拉一遍并整体
          // 替换——而一轮对话会走到这里几十次，会话一长就是每个事件一次全量读
          // 加一次全量重渲染。
          //
          // 游标用手上最后一条持久化消息的 id。乐观气泡（`msg_u_*`）从不入库，
          // 不能当游标，所以要跳过它们往前找。
          const cursor = lastPersistedMessageId(messagesRef.current);
          const delta = await conversationApi.getMessagesSince(conversationId, cursor);
          setMessages((current) => mergeMessagesDelta(current, delta));
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
      const page = await conversationApi.list(CONVERSATION_PAGE_SIZE);
      setConversations(page.conversations);
      setConversationTotal(page.total);
    } catch (e) {
      console.error("[useConversations] Failed to load conversations:", e);
    }
  }, []);

  /// 往上翻一页历史。
  ///
  /// 保持滚动位置由调用方负责（记加载前的 `scrollHeight`，加载后补差值）——
  /// 不补的话视野会跳，用户刚读到的那段会被顶走。
  const loadOlderMessages = useCallback(async () => {
    const current = messagesRef.current;
    // 往回翻的锚点是**最早**一条持久化消息。乐观气泡永远在最新一端，不会是它。
    const oldest = current.find((m) => !m.id.startsWith(OPTIMISTIC_ID_PREFIX));
    if (!oldest) return;
    const anchorId = oldest.id;
    const convId = currentConvIdRef.current;
    try {
      const page = await conversationApi.getMessagesPage(
        convId,
        anchorId,
        MESSAGE_PAGE_SIZE,
      );
      // 等这一页的时候用户切走了：它属于**上一个**会话，连「还剩多少条」都不能用。
      if (currentConvIdRef.current !== convId) return;
      if (page.messages.length === 0) {
        setOlderRemaining(0);
        return;
      }
      setOlderRemaining(page.older_remaining);
      setMessages((prev) => prependOlderMessages(prev, page.messages, anchorId));
    } catch (e) {
      console.error("[useConversations] Failed to load older messages:", e);
      toast.error(`加载更早的消息失败：${e}`);
    }
  }, []);

  const searchConversations = useCallback(async (query: string) => {
    try {
      return await conversationApi.search(query, false, CONVERSATION_PAGE_SIZE);
    } catch (e) {
      console.error("[useConversations] Failed to search conversations:", e);
      return [];
    }
  }, []);

  const selectConversation = useCallback(async (id: string) => {
    setCurrentConvId(id);
    try {
      // 只取**最近**一页。聊天是从最新往回看的，不是从最早往后翻。
      const page = await conversationApi.getMessagesPage(id, null, MESSAGE_PAGE_SIZE);
      setMessages(page.messages);
      setOlderRemaining(page.older_remaining);
    } catch (e) {
      console.error("[useConversations] Failed to load messages:", id, e);
      setMessages([]);
      setOlderRemaining(0);
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
      const page = await conversationApi.listArchived(CONVERSATION_PAGE_SIZE);
      setArchivedConversations(page.conversations);
    } catch (e) {
      console.error("[useConversations] Failed to reload archived list:", e);
    }
  }, [loadConversations]);

  const loadArchivedConversations = useCallback(async () => {
    try {
      const page = await conversationApi.listArchived(CONVERSATION_PAGE_SIZE);
      setArchivedConversations(page.conversations);
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
      id: `${OPTIMISTIC_ID_PREFIX}${Date.now()}`,
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
      // 失败要说出来。Direct 模式下这一轮**照样会改文件**，用户以为「有检查点、
      // 能回滚」，真出事时才发现快照根本没建成——那时已经晚了。
      // 非 git 工作区不会走到这里：后端对它返回 `skipped: true` 而不是错误，
      // 所以进到 catch 的都是真失败（git 静默丢写、杀软锁库、仓库损坏）。
      // 仍然不阻塞这一轮：提示归提示，任务照跑。
      checkpointApi.create(chatWorkspace, convId, snippet || "改动前检查点").catch((e) => {
        toast.warning("改动前检查点未能创建", {
          description: `${e}。这一轮的文件改动将无法一键回滚，建议先手动提交或备份。`,
          duration: 10000,
        });
      });
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

      // 会话「死了」不等于「接不上了」。
      //
      // 应用重启（崩溃、强杀、正常退出）之后，启动收敛会把遗留会话标成 failed
      // ——但 Agent CLI 自己往往还留着那个 session，`external_session_id` 就是
      // 用来把它拉回来的。直接起新会话等于把几天的上下文扔掉，而且用户毫无察觉：
      // 同一个 agent 不触发交接，新会话是**完全空白**的。
      //
      // 所以顺序是：能 resume 就 resume；resume 不上（CLI 把 session 清了）
      // 才起新会话，并且**强制走交接**——OMNIX 自己库里存着全部原文，没有理由
      // 让新会话从零开始。
      const canResume = shouldTryResume({
        sessionDead,
        hasExternalSessionId: !!session?.external_session_id,
        configChanged,
      });
      let resumed = false;
      if (canResume && sessionId) {
        resumed = await runtimeApi
          .resumeSession(sessionId)
          .then(() => true)
          .catch(() => false);
        if (resumed) {
          runtimeSessionByConversationRef.current[convId] = sessionId;
          conversationByRuntimeSessionRef.current[sessionId] = convId;
          setRuntimeActiveConversations((current) =>
            current.includes(convId) ? current : [...current, convId]
          );
        }
      }

      const rebuildingContext = !resumed && !!session && sessionDead && !configChanged;
      const isHandoff = shouldHandoff({
        handoffEnabled,
        priorAgent,
        agent,
        resumed,
        hadSession: !!session,
        sessionDead,
        configChanged,
      });

      if (!resumed && (!session || sessionDead || configChanged || !sessionId)) {
        if (configChanged && sessionId && activeRuntimeConversationsRef.current.includes(convId)) {
          await runtimeApi.stopSession(sessionId).catch((error) => {
            console.warn("[useConversations] Failed to stop superseded runtime session:", error);
          });
        }
        if (rebuildingContext) {
          toast.info("此前的 Agent 会话已无法恢复，正在用 OMNIX 保存的记录重建上下文");
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
        // 复用的会话在库里看着是活的，实际进程已经没了（上次没走到收敛，
        // 或者被外部杀掉）。和上面同一套顺序：先 resume，接不上就新会话 + 交接。
        if (!String(error).includes("not running")) throw error;
        const recovered = session.external_session_id
          ? await runtimeApi
              .resumeSession(sessionId)
              .then(() => true)
              .catch(() => false)
          : false;
        if (recovered) {
          setRuntimeActiveConversations((current) =>
            current.includes(convId) ? current : [...current, convId]
          );
          await runtimeApi.sendMessage(sessionId, inputMsg, displayContent, isHandoff, wireImages);
        } else {
          toast.info("此前的 Agent 会话已无法恢复，正在用 OMNIX 保存的记录重建上下文");
          const fresh = await startRuntimeSession();
          // 强制交接：新会话是空白的，而原文就在 OMNIX 库里。
          await runtimeApi.sendMessage(fresh.id, inputMsg, displayContent, true, wireImages);
        }
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
    olderRemaining, loadOlderMessages, conversationTotal, searchConversations,
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

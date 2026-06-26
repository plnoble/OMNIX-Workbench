/**
 * useConversations — Conversation, PTY session, and chat management
 *
 * This is the most complex hook, managing:
 * - Conversation list and CRUD
 * - Active agent selection and detection
 * - Chat message state and sending
 * - PTY session lifecycle (start/stop/stdin)
 * - Terminal stream processing and interactive prompt detection
 * - Collab logs for Team tab
 */

import { useState, useEffect, useCallback, useRef } from "react";
import { listen, emit } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { conversationApi, ptyApi, agentApi, runtimeApi } from "@/lib/tauri-api";
import { processTerminalStream, detectInteractivePrompts, detectMistakes } from "@/lib/terminal";
import { AGENT_NAMES } from "@/lib/constants";
import type {
  ConversationInfo,
  ConversationMessage,
  DetectedAgent,
  PromptType,
  GatewayStatus,
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

function runtimeAgentId(agentName: string): RuntimeAgentId | null {
  if (agentName === "Claude Code") return "claude_code";
  if (agentName === "Codex") return "codex";
  return null;
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
  promptType: PromptType;
  collabLogs: string;
  collabStdin: string;
  pendingApproval: RuntimeApprovalRequest | null;
  startingConversations: string[]; // conversations awaiting session start / first token

  // Workspace modal
  isWorkspaceModalOpen: boolean;
  workspaceFormPath: string;

  // Refs (exposed for Team tab and status dock bridge)
  terminalLogsRef: React.MutableRefObject<Record<string, string>>;
  currentConvIdRef: React.MutableRefObject<string>;

  // Actions
  setChatInput: (v: string) => void;
  setChatWorkspace: (v: string) => void;
  setActiveAgent: (v: string) => void; // Accepts any agent name string
  selectAgent: (name: string) => void; // Switch Agent and load that Agent's conversation
  currentSurface: "chat" | "work";
  enterSurface: (surface: "chat" | "work") => void; // Switch between 对话 and 工作
  setCollabLogs: (v: string) => void;
  setCollabStdin: (v: string) => void;
  setIsWorkspaceModalOpen: (v: boolean) => void;
  setWorkspaceFormPath: (v: string) => void;
  setCurrentConvId: (v: string) => void;

  loadConversations: () => Promise<void>;
  detectAgents: () => Promise<void>;
  selectConversation: (id: string) => Promise<void>;
  newConversation: () => void;
  saveWorkspaceChat: () => Promise<void>;
  deleteConversation: (id: string, event: React.MouseEvent) => Promise<void>;
  archiveConversation: (id: string, event: React.MouseEvent) => Promise<void>;
  unarchiveConversation: (id: string) => Promise<void>;
  loadArchivedConversations: () => Promise<void>;
  archivedConversations: ConversationInfo[];
  sendMessage: (e: React.FormEvent, config: RuntimeSendConfig, searchContext?: string) => Promise<void>;
  respondToApproval: (approved: boolean, forSession?: boolean) => Promise<void>;
  sendStdinDirect: (input: string) => Promise<void>;
  stopAgentSession: (sessionId: string) => Promise<void>;
  startAgentSession: (sessionId: string) => Promise<void>;
}

export function useConversations(
  gatewayStatus: GatewayStatus,
): UseConversationsReturn {
  const [conversations, setConversations] = useState<ConversationInfo[]>([]);
  const [archivedConversations, setArchivedConversations] = useState<ConversationInfo[]>([]);
  const [currentConvId, setCurrentConvId] = useState("");
  const [messages, setMessages] = useState<ConversationMessage[]>([]);
  const [chatInput, setChatInput] = useState("");
  const [chatWorkspace, setChatWorkspace] = useState("direct");
  const [detectedAgents, setDetectedAgents] = useState<DetectedAgent[]>([]);
  const [activeAgent, setActiveAgent] = useState<string>(AGENT_NAMES[0]);
  const [ptySessions, setPtySessions] = useState<string[]>([]);
  const [runtimeActiveConversations, setRuntimeActiveConversations] = useState<string[]>([]);
  const [promptType, setPromptType] = useState<PromptType>("none");
  const [collabLogs, setCollabLogs] = useState("");
  const [collabStdin, setCollabStdin] = useState("");
  const [pendingApproval, setPendingApproval] = useState<RuntimeApprovalRequest | null>(null);
  const [startingConversations, setStartingConversations] = useState<string[]>([]);
  const [currentSurface, setCurrentSurface] = useState<"chat" | "work">("chat");
  const activeSessions = Array.from(new Set([...ptySessions, ...runtimeActiveConversations]));

  // Workspace modal
  const [isWorkspaceModalOpen, setIsWorkspaceModalOpen] = useState(false);
  const [workspaceFormPath, setWorkspaceFormPath] = useState("");

  // Refs for cross-render access
  const terminalLogsRef = useRef<Record<string, string>>({});
  const currentConvIdRef = useRef(currentConvId);
  const loggedMistakesRef = useRef<Set<string>>(new Set()); // dedup per raw_line
  const runtimeSessionByConversationRef = useRef<Record<string, string>>({});
  const conversationByRuntimeSessionRef = useRef<Record<string, string>>({});
  const activeRuntimeConversationsRef = useRef(runtimeActiveConversations);
  currentConvIdRef.current = currentConvId;
  activeRuntimeConversationsRef.current = runtimeActiveConversations;

  // ── PTY Event Listener (mount once) ────────────────

  useEffect(() => {
    const unlistenOutput = listen<{
      session_id: string;
      stream_type: string;
      text: string;
    }>("agent-output", (event) => {
      const { session_id, text } = event.payload;
      const cleanText = processTerminalStream(text);
      if (!cleanText) return;

      // Update terminal logs ref
      const currentLogs = terminalLogsRef.current[session_id] || "";
      const updatedLogs = currentLogs + cleanText;
      terminalLogsRef.current[session_id] = updatedLogs;

      // Detect interactive prompts
      const detected = detectInteractivePrompts(updatedLogs);
      setPromptType(detected);

      // Detect development mistakes and log to activity_log
      const mistakes = detectMistakes(updatedLogs);
      if (mistakes.length > 0) {
        const newMistakes = mistakes.filter(m => !loggedMistakesRef.current.has(m.raw_line));
        if (newMistakes.length > 0) {
          newMistakes.forEach(m => loggedMistakesRef.current.add(m.raw_line));
          invoke("log_activity", {
            action: "mistake_detected",
            target: session_id,
            details: JSON.stringify(newMistakes),
          }).catch(() => {});
        }
      }

      // If this is the active conversation, update UI state
      if (session_id === currentConvIdRef.current) {
        setCollabLogs((prev) => prev + cleanText);
        setMessages((prev) => {
          if (prev.length === 0) return prev;
          const last = prev[prev.length - 1];
          if (last.role === "assistant") {
            const updated = [...prev];
            updated[updated.length - 1] = {
              ...last,
              content: last.content + cleanText,
            };
            return updated;
          }
          return [
            ...prev,
            {
              id: `msg_pt_${Date.now()}`,
              conversation_id: session_id,
              role: "assistant" as const,
              content: cleanText,
              timestamp: new Date().toISOString(),
            },
          ];
        });
      }
    });

    const unlistenActiveSessions = listen<string[]>(
      "active-sessions-update",
      (event) => {
        setPtySessions(event.payload);
      }
    );

    return () => {
      unlistenOutput.then((fn) => fn());
      unlistenActiveSessions.then((fn) => fn());
    };
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

        if (runtimeEvent.kind === "raw_log") {
          const text = runtimeEvent.text || "";
          terminalLogsRef.current[sessionId] = `${terminalLogsRef.current[sessionId] || ""}${text}\n`;
          if (conversationId === currentConvIdRef.current) {
            setCollabLogs((current) => `${current}${text}\n`);
          }
        }

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
  // Use a ref so the listeners always call the latest sendStdinDirect without re-registering.
  // The ref is initialized as a no-op and updated after sendStdinDirect is defined.
  const sendStdinRef = useRef<(input: string) => void>(() => {});

  useEffect(() => {
    const unlistenApproval = listen("omnix-action-toggle-approval", () => {
      sendStdinRef.current("\r");
    });
    const unlistenNewConv = listen("omnix-action-new-conversation", () => {
      newConversation();
    });
    const unlistenSettings = listen("omnix-action-open-settings", () => {
      // This will be handled by MainApp's handleTabChange
      // We emit a custom event that MainApp can listen to
      emit("omnix-navigate-settings", {}).catch(() => {});
    });

    return () => {
      unlistenApproval.then((fn) => fn());
      unlistenNewConv.then((fn) => fn());
      unlistenSettings.then((fn) => fn());
    };
  }, []); // eslint-disable-line react-hooks/exhaustive-deps -- listeners registered once; callbacks accessed via ref

  // ── Persist status updates to StatusDock window ────

  useEffect(() => {
    const statusPayload = {
      active_agent: activeAgent,
      session_id: currentConvId,
      gateway_status: gatewayStatus,
      active_sessions_count: activeSessions.length,
      db_status: "已连接",
    };
    emit("omnix-dev-status-change", statusPayload).catch((e) =>
      console.error("[useConversations] Emit error:", e)
    );
  }, [activeAgent, currentConvId, gatewayStatus, activeSessions]);

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
    setPromptType("none");
    try {
      const msgs = await conversationApi.getMessages(id);
      setMessages(msgs);
    } catch (e) {
      console.error("[useConversations] Failed to load messages:", id, e);
      setMessages([]);
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

      const logs = terminalLogsRef.current[id] || "";
      setCollabLogs(logs);
    }
  }, [conversations]);

  const newConversation = useCallback(() => {
    setCurrentConvId("");
    setMessages([]);
    setChatInput("");
    setPromptType("none");
    setPendingApproval(null);
    // A fresh conversation is unbound; the 工作 surface will then prompt for a workspace.
    setChatWorkspace("direct");
  }, []);

  // A conversation belongs to the 工作 (workspace) surface when it is bound to a
  // real workspace; otherwise it is a plain 对话 conversation.
  const conversationIsWork = (conv: ConversationInfo) =>
    !!conv.workspace_path && conv.workspace_path !== "direct";

  // Show the active Agent's latest conversation for a surface (对话 / 工作), or a
  // fresh empty composer when that (Agent, surface) pair has no conversation yet.
  const showLatestConversation = useCallback((agent: string, surface: "chat" | "work") => {
    const wantWork = surface === "work";
    const current = conversations.find((conv) => conv.id === currentConvIdRef.current);
    if (current && current.active_agent === agent && conversationIsWork(current) === wantWork) {
      return; // current conversation already fits this surface
    }
    // 对话: resume the Agent's latest plain conversation. 工作: always start from a
    // clean workspace choice rather than silently reopening a previous workspace.
    if (!wantWork) {
      const candidates = conversations
        .filter((conv) => conv.active_agent === agent && !conversationIsWork(conv))
        .sort((a, b) => b.created_at.localeCompare(a.created_at));
      if (candidates.length > 0) {
        void selectConversation(candidates[0].id);
        return;
      }
    }
    setCurrentConvId("");
    setMessages([]);
    setChatInput("");
    setPromptType("none");
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

  const archiveConversation = useCallback(async (id: string, event: React.MouseEvent) => {
    event.stopPropagation();
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

  const startAgentSession = useCallback(async (sessionId: string) => {
    const agent = detectedAgents.find((a) => a.name === activeAgent);
    const exePath = agent ? agent.path : "";

    setCollabLogs((prev) => prev + `\n--- 正在启动 ${activeAgent} 进程... ---\n`);
    try {
      await ptyApi.start({
        sessionId,
        agentName: activeAgent,
        exePath,
        args: [],
        workspaceDir: chatWorkspace,
      });
    } catch (err) {
      console.error("[useConversations] Failed to start agent session:", err);
      setCollabLogs((prev) => prev + `\n[错误] 启动进程失败: ${err}\n`);
      throw err;
    }
  }, [detectedAgents, activeAgent, chatWorkspace]);

  const sendMessage = useCallback(async (
    e: React.FormEvent,
    config: RuntimeSendConfig,
    searchContext?: string,
  ) => {
    e.preventDefault();
    if (!chatInput.trim()) return;

    const agent = runtimeAgentId(activeAgent);
    if (!agent) {
      throw new Error(`${activeAgent} 尚未完成真实运行适配，请先选择 Claude Code 或 Codex`);
    }

    let convId = currentConvId;
    if (!convId) {
      convId = await createConversationFromPrompt(chatInput);
    }

    // Build message content — inject search context if provided (AingDesk inspired)
    const displayContent = chatInput;
    const agentContent = searchContext
      ? `${chatInput}\n\n---\n[联网搜索结果]\n${searchContext}`
      : chatInput;

    // Append user message immediately (display original question)
    const userMsg: ConversationMessage = {
      id: `msg_u_${Date.now()}`,
      conversation_id: convId,
      role: "user",
      content: displayContent,
      timestamp: new Date().toISOString(),
    };
    setMessages((prev) => [...prev, userMsg]);
    // Show a waiting indicator until the session starts and the first token arrives.
    // First Codex start can take a while (it boots MCP servers during thread/start).
    setStartingConversations((current) => current.includes(convId) ? current : [...current, convId]);

    const inputMsg = agentContent;
    setChatInput("");

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

      const configChanged = !!session && (
        session.config.agent !== agent
        || session.config.work_mode !== config.workMode
        || session.config.permission.kind !== config.permission.kind
        || JSON.stringify(session.config.model) !== JSON.stringify(config.model)
      );
      if (!session || configChanged || !sessionId) {
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

      try {
        await runtimeApi.sendMessage(sessionId, inputMsg, displayContent);
      } catch (error) {
        const canResume = !!session.external_session_id;
        if (!canResume || !String(error).includes("not running")) throw error;
        await runtimeApi.resumeSession(sessionId);
        setRuntimeActiveConversations((current) =>
          current.includes(convId) ? current : [...current, convId]
        );
        await runtimeApi.sendMessage(sessionId, inputMsg, displayContent);
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
  }, [activeAgent, chatInput, chatWorkspace, currentConvId]);

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

  const sendStdinDirect = useCallback(async (inputStr: string) => {
    if (!currentConvId) return;
    try {
      await ptyApi.sendStdin({ sessionId: currentConvId, input: inputStr });
    } catch (err) {
      console.error("[useConversations] Failed to send direct stdin:", err);
    }
  }, [currentConvId]);

  // Keep the ref in sync so the event listener always calls the latest version
  sendStdinRef.current = sendStdinDirect;

  const stopAgentSession = useCallback(async (sessionId: string) => {
    try {
      const runtimeSessionId = runtimeSessionByConversationRef.current[sessionId];
      if (runtimeSessionId) {
        await runtimeApi.stopSession(runtimeSessionId);
        setRuntimeActiveConversations((current) => current.filter((id) => id !== sessionId));
        setPendingApproval((current) =>
          current?.session_id === runtimeSessionId ? null : current
        );
      } else {
        await ptyApi.stop(sessionId);
        setCollabLogs((prev) => prev + "\n--- 兼容终端进程已被手动终止 ---\n");
      }
    } catch (e) {
      console.error("[useConversations] Failed to stop session:", e);
      throw e;
    }
  }, []);

  // ── Helper: Create conversation from first prompt ──

  async function createConversationFromPrompt(prompt: string): Promise<string> {
    const convId = `conv_${Date.now()}`;
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
    detectedAgents, activeAgent, activeSessions, promptType,
    collabLogs, collabStdin, pendingApproval, startingConversations,
    currentSurface, enterSurface,
    isWorkspaceModalOpen, workspaceFormPath,
    terminalLogsRef, currentConvIdRef,
    archivedConversations,
    setChatInput, setChatWorkspace, setActiveAgent, selectAgent,
    setCollabLogs, setCollabStdin,
    setIsWorkspaceModalOpen, setWorkspaceFormPath,
    setCurrentConvId,
    loadConversations, detectAgents, selectConversation,
    newConversation, saveWorkspaceChat, deleteConversation,
    archiveConversation, unarchiveConversation, loadArchivedConversations,
    sendMessage, respondToApproval, sendStdinDirect, stopAgentSession, startAgentSession,
  };
}

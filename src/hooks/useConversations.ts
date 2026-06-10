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
import { conversationApi, ptyApi, agentApi } from "@/lib/tauri-api";
import { processTerminalStream, detectInteractivePrompts, detectMistakes } from "@/lib/terminal";
import { AGENT_NAMES } from "@/lib/constants";
import type {
  ConversationInfo,
  ConversationMessage,
  DetectedAgent,
  PromptType,
  GatewayStatus,
} from "@/types";

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
  sendMessage: (e: React.FormEvent, searchContext?: string) => Promise<void>;
  sendStdinDirect: (input: string) => Promise<void>;
  stopAgentSession: (sessionId: string) => Promise<void>;
  startAgentSession: (sessionId: string) => Promise<void>;
}

export function useConversations(
  gatewayStatus: GatewayStatus,
): UseConversationsReturn {
  const [conversations, setConversations] = useState<ConversationInfo[]>([]);
  const [currentConvId, setCurrentConvId] = useState("");
  const [messages, setMessages] = useState<ConversationMessage[]>([]);
  const [chatInput, setChatInput] = useState("");
  const [chatWorkspace, setChatWorkspace] = useState("direct");
  const [detectedAgents, setDetectedAgents] = useState<DetectedAgent[]>([]);
  const [activeAgent, setActiveAgent] = useState<string>(AGENT_NAMES[0]);
  const [activeSessions, setActiveSessions] = useState<string[]>([]);
  const [promptType, setPromptType] = useState<PromptType>("none");
  const [collabLogs, setCollabLogs] = useState("");
  const [collabStdin, setCollabStdin] = useState("");

  // Workspace modal
  const [isWorkspaceModalOpen, setIsWorkspaceModalOpen] = useState(false);
  const [workspaceFormPath, setWorkspaceFormPath] = useState("");

  // Refs for cross-render access
  const terminalLogsRef = useRef<Record<string, string>>({});
  const currentConvIdRef = useRef(currentConvId);
  const loggedMistakesRef = useRef<Set<string>>(new Set()); // dedup per raw_line
  currentConvIdRef.current = currentConvId;

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
        setActiveSessions(event.payload);
      }
    );

    return () => {
      unlistenOutput.then((fn) => fn());
      unlistenActiveSessions.then((fn) => fn());
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

      const conv = conversations.find((c) => c.id === id);
      if (conv) {
        setActiveAgent(conv.active_agent);
        setChatWorkspace(conv.workspace_path);

        const logs = terminalLogsRef.current[id] || "";
        setCollabLogs(logs);
      }
    } catch (e) {
      console.error("[useConversations] Failed to load messages:", id, e);
    }
  }, [conversations]);

  const newConversation = useCallback(() => {
    setCurrentConvId("");
    setMessages([]);
    setChatInput("");
    setPromptType("none");
  }, []);

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
    await conversationApi.delete(id);
    if (currentConvId === id) {
      newConversation();
    }
    await loadConversations();
  }, [currentConvId, loadConversations, newConversation]);

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

  const sendMessage = useCallback(async (e: React.FormEvent, searchContext?: string) => {
    e.preventDefault();
    if (!chatInput.trim()) return;

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

    // Save user message to database
    try {
      await conversationApi.addMessage({
        id: userMsg.id,
        conversationId: convId,
        role: "user",
        content: displayContent,
      });
    } catch (err) {
      console.error("[useConversations] Failed to save user message:", err);
    }

    const inputMsg = agentContent;
    setChatInput("");

    // Start session if not active
    if (!activeSessions.includes(convId)) {
      try {
        await startAgentSession(convId);
      } catch (err) {
        return; // Error already logged in startAgentSession
      }
    }

    // Route message to PTY stdin
    try {
      await ptyApi.sendStdin({ sessionId: convId, input: inputMsg + "\n" });
    } catch (err) {
      console.error("[useConversations] Failed to send stdin:", err);
      throw err;
    }
  }, [chatInput, currentConvId, activeSessions, startAgentSession]);

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
      await ptyApi.stop(sessionId);
      setCollabLogs((prev) => prev + "\n--- 进程已被手动终止 ---\n");
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
    collabLogs, collabStdin,
    isWorkspaceModalOpen, workspaceFormPath,
    terminalLogsRef, currentConvIdRef,
    setChatInput, setChatWorkspace, setActiveAgent,
    setCollabLogs, setCollabStdin,
    setIsWorkspaceModalOpen, setWorkspaceFormPath,
    setCurrentConvId,
    loadConversations, detectAgents, selectConversation,
    newConversation, saveWorkspaceChat, deleteConversation,
    sendMessage, sendStdinDirect, stopAgentSession, startAgentSession,
  };
}

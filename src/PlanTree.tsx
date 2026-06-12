import React, { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { cn } from "@/lib/utils";
import { toast } from "@/components/ui/sonner";

interface TaskNode {
  id: string;
  conversation_id: string;
  title: string;
  status: string; // "todo", "in_progress", "done"
  order_num: number;
  dependencies: string[];
}

interface MailboxMessage {
  filename: string;
  sender: string;
  receiver: string;
  command: string;
  params: Record<string, unknown>;
  status: string;
  timestamp: string;
}

interface PlanTreeProps {
  conversationId: string;
  containerWidth: number;
}

export const PlanTree: React.FC<PlanTreeProps> = ({ conversationId, containerWidth }) => {
  const [tasks, setTasks] = useState<TaskNode[]>([]);
  const [mailboxMsgs, setMailboxMsgs] = useState<MailboxMessage[]>([]);
  const [loading, setLoading] = useState(false);
  const [leader, setLeader] = useState("Claude Code");
  const [teammate, setTeammate] = useState("Google Antigravity");
  const [expandedMsg, setExpandedMsg] = useState<string | null>(null);

  // Fetch tasks and mailbox files
  const fetchData = async () => {
    if (!conversationId) return;
    setLoading(true);
    try {
      const taskList = await invoke<TaskNode[]>("get_conversation_tasks", {
        conversationId,
      });
      setTasks(taskList);

      const msgs = await invoke<MailboxMessage[]>("get_mailbox_messages");
      setMailboxMsgs(msgs);
    } catch (err) {
      console.error("Failed to fetch plan data:", err);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchData();

    // Listen for live ACP plan updates from Tauri
    const unlisten = listen<{ session_id: string; payload: { method: string; params: { tasks: TaskNode[] } } }>(
      "agent-task-update",
      (event) => {
        const { session_id, payload } = event.payload;
        if (session_id === conversationId && payload && payload.method === "task/plan") {
          const rawTasks: TaskNode[] = Array.isArray(payload.params?.tasks) ? payload.params.tasks : [];
          const formattedTasks: TaskNode[] = rawTasks.map((t, idx: number) => ({
            id: String(t.id ?? ""),
            conversation_id: session_id,
            title: String(t.title ?? ""),
            status: String(t.status ?? "todo"),
            order_num: idx,
            dependencies: Array.isArray(t.dependencies) ? t.dependencies : [],
          }));
          setTasks(formattedTasks);

          // Re-fetch mailbox messages as they might have updated
          invoke<MailboxMessage[]>("get_mailbox_messages")
            .then(setMailboxMsgs)
            .catch(console.error);
        }
      }
    );

    return () => {
      unlisten.then((fn) => fn());
    };
  }, [conversationId]);

  // Run the simulation script
  const handleIgniteSimulation = async () => {
    if (!conversationId) return;
    setLoading(true);
    try {
      await invoke("simulate_team_task_dispatch", {
        conversationId,
        leader,
        teammate,
      });
      // Wait a moment for files to write and SQLite to commit
      setTimeout(() => {
        fetchData();
      }, 500);
    } catch (err) {
      toast.error(`Simulation failed: ${err}`);
      setLoading(false);
    }
  };

  // Helper to calculate progress percentage
  const completedCount = tasks.filter((t) => t.status === "done").length;
  const progressPercent = tasks.length > 0 ? Math.round((completedCount / tasks.length) * 100) : 0;

  return (
    <div className="card animate-fade-in h-full flex flex-col gap-5 overflow-hidden">
      <div className="flex justify-between items-center border-b border-border pb-3">
        <div>
          <h3 className="m-0 text-base flex items-center gap-2">
            👥 Team Mode 协同任务树与计划看板
          </h3>
          <p className="mt-1 text-secondary-foreground text-xs">
            可视化的 Agent 协作计划树，追踪 Leader 分发至辅 Agent 的子任务。
          </p>
        </div>
        <button
          className={cn("btn btn-secondary", "px-2 py-1 text-xs")}
          onClick={fetchData}
          disabled={loading}
        >
          {loading ? "更新中..." : "🔄 刷新"}
        </button>
      </div>

      {/* Progress Section */}
      {tasks.length > 0 && (
        <div className="bg-muted/5 p-3 rounded-lg border border-border">
          <div className="flex justify-between text-xs mb-1.5">
            <span className="text-secondary-foreground">任务完成度: {completedCount}/{tasks.length}</span>
            <span className="text-[var(--color-primary)] font-semibold">{progressPercent}%</span>
          </div>
          <div className="w-full h-1.5 bg-muted/20 rounded-sm overflow-hidden">
            {/* TODO: migrate to Tailwind — dynamic width and CSS-variable gradient */}
            <div
              style={{
                width: `${progressPercent}%`,
                height: "100%",
                background: "linear-gradient(90deg, var(--color-primary), var(--color-secondary))",
                transition: "width 0.4s ease"
              }}
            />
          </div>
        </div>
      )}

      {/* Layout Grid: Tasks Tree on Left, Simulation/Mailbox on Right */}
      <div className={cn(
        containerWidth < 520 ? "flex flex-col overflow-y-auto" : "grid grid-cols-[1.2fr_1fr] overflow-y-hidden",
        "gap-5 grow overflow-x-hidden"
      )}>

        {/* Left Side: Visual Plan Tree */}
        <div className={cn(
          "flex flex-col gap-3 pr-2",
          containerWidth < 520 ? "overflow-y-visible" : "overflow-y-auto"
        )}>
          <h4 className="text-sm text-foreground flex items-center gap-1.5">
            📋 计划树结构 (To-Do List)
          </h4>

          {tasks.length === 0 ? (
            <div className="flex flex-col items-center justify-center h-[200px] rounded-lg border border-dashed border-border text-muted-foreground text-center p-4">
              <span className="text-2xl mb-2">🌳</span>
              <p className="text-xs m-0">暂无协作任务。</p>
              <p className="text-xs text-muted-foreground mt-1">
                运行 Agent 时，OMNIX 将自动捕获 ACP 协议数据生成计划树，或者使用右侧仿真引擎进行点火调试。
              </p>
            </div>
          ) : (
            <div className="flex flex-col relative pl-5 border-l-2 border-border">
              {tasks.map((task) => {
                const isActive = task.status === "in_progress";
                const isDone = task.status === "done";

                return (
                  <div
                    key={task.id}
                    className={cn(
                      "relative mb-4 rounded-lg transition-all duration-300",
                      isActive
                        ? "bg-[rgba(79,172,254,0.04)] border border-[rgba(79,172,254,0.2)] p-2.5"
                        : "border border-transparent py-1"
                    )}
                  >
                    {/* Stepper connector bullet */}
                    <div
                      className={cn(
                        "absolute rounded-full transition-all duration-300 z-[2]",
                        isActive
                          ? "-left-[27px] top-[14px] w-3 h-3"
                          : "-left-[26px] top-2 w-2.5 h-2.5",
                        isDone
                          ? "bg-emerald-500"
                          : isActive
                            ? "bg-amber-500"
                            : "bg-muted-foreground",
                        isActive
                          ? "shadow-[0_0_10px_var(--color-warning)]"
                          : isDone
                            ? "shadow-[0_0_8px_var(--color-success)]"
                            : "shadow-none"
                      )}
                    />

                    <div className="flex justify-between items-start gap-2.5">
                      <span
                        className={cn(
                          "text-sm leading-[1.4]",
                          isDone ? "text-muted-foreground line-through" : "text-foreground",
                          isActive ? "font-semibold" : "font-normal"
                        )}
                      >
                        {task.title}
                      </span>
                      <span
                        className={cn(
                          "text-xs px-1.5 py-0.5 rounded font-medium",
                          isDone
                            ? "bg-emerald-500/10 text-emerald-500 border border-emerald-500/20"
                            : isActive
                              ? "bg-amber-500/10 text-amber-500 border border-amber-500/20"
                              : "bg-muted/10 text-muted-foreground border border-transparent"
                        )}
                      >
                        {isDone ? "已完成" : isActive ? "运行中" : "未开始"}
                      </span>
                    </div>
                  </div>
                );
              })}
            </div>
          )}
        </div>

        {/* Right Side: Simulation Panel & Mailbox Message log */}
        <div className={cn(
          "flex flex-col gap-4 pr-2",
          containerWidth < 520
            ? "overflow-y-visible pl-0 pt-4 border-t border-border"
            : "overflow-y-auto pl-4 pt-0 border-l border-border"
        )}>

          {/* Simulator Form */}
          <div className="bg-muted/5 border border-border rounded-[10px] p-3.5">
            <h4 className="text-xs mb-2.5 text-[var(--color-secondary)]">🧪 Team Mode 协议调试仿真器</h4>

            <div className="flex flex-col gap-2.5">
              <div className="grid grid-cols-2 gap-2">
                <div>
                  <label className="text-xs text-secondary-foreground block mb-1">Leader Agent</label>
                  <select
                    value={leader}
                    onChange={(e) => setLeader(e.target.value)}
                    className="w-full bg-background border border-border rounded text-foreground text-xs p-1"
                  >
                    <option value="Claude Code">Claude Code</option>
                    <option value="Google Antigravity">Google Antigravity</option>
                    <option value="OpenCode">OpenCode</option>
                  </select>
                </div>
                <div>
                  <label className="text-xs text-secondary-foreground block mb-1">Teammate Agent</label>
                  <select
                    value={teammate}
                    onChange={(e) => setTeammate(e.target.value)}
                    className="w-full bg-background border border-border rounded text-foreground text-xs p-1"
                  >
                    <option value="Google Antigravity">Google Antigravity</option>
                    <option value="OpenCode">OpenCode</option>
                    <option value="Claude Code">Claude Code</option>
                  </select>
                </div>
              </div>

              <button
                className={cn("btn btn-primary", "w-full py-1.5 text-xs mt-1")}
                onClick={handleIgniteSimulation}
                disabled={loading}
              >
                {loading ? "仿真点火中..." : "🔥 点火团队协同仿真"}
              </button>
            </div>
          </div>

          {/* Mailbox Envelope Logs */}
          <div className="flex flex-col gap-2">
            <h4 className="text-xs text-foreground flex items-center gap-1.5">
              📪 协作信箱数据 (Mailbox Log)
            </h4>

            {mailboxMsgs.length === 0 ? (
              <p className="text-xs text-muted-foreground my-2 text-center">
                信箱空空如也。仿真运行或 Agent 协作时产生的子任务指令信包将落盘在此处。
              </p>
            ) : (
              <div className="flex flex-col gap-2">
                {mailboxMsgs.map((msg) => {
                  const isExpanded = expandedMsg === msg.filename;

                  return (
                    <div
                      key={msg.filename}
                      className="bg-muted/5 border border-border rounded-lg px-3 py-2 cursor-pointer"
                      onClick={() => setExpandedMsg(isExpanded ? null : msg.filename)}
                      role="button"
                      aria-expanded={isExpanded}
                      aria-label="展开或折叠信箱消息详情"
                    >
                      <div className="flex justify-between items-center">
                        <span className="text-xs font-mono text-[var(--color-primary)]">
                          ✉️ {msg.filename.slice(0, 18)}...
                        </span>
                        <span className="text-xs text-muted-foreground">
                          {msg.timestamp.split("T")[1]?.slice(0, 5) || "Active"}
                        </span>
                      </div>

                      <div className="text-xs mt-1.5 mb-0.5 text-secondary-foreground">
                        <strong>{msg.sender}</strong> ➔ <strong>{msg.receiver}</strong>
                      </div>

                      <div className="text-xs text-foreground">
                        指令: <code className="text-[var(--color-warning)] font-mono">{msg.command}</code>
                      </div>

                      {isExpanded && (
                        <div className="mt-2 bg-background p-2 rounded border border-border overflow-x-auto">
                          <pre className="m-0 text-xs font-mono text-secondary-foreground">
                            {JSON.stringify(msg.params, null, 2)}
                          </pre>
                        </div>
                      )}
                    </div>
                  );
                })}
              </div>
            )}
          </div>

        </div>

      </div>
    </div>
  );
};

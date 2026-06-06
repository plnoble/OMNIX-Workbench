import React, { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

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
  params: any;
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
    const unlisten = listen<{ session_id: string; payload: any }>(
      "agent-task-update",
      (event) => {
        const { session_id, payload } = event.payload;
        if (session_id === conversationId && payload && payload.method === "task/plan") {
          const rawTasks = payload.params.tasks || [];
          const formattedTasks: TaskNode[] = rawTasks.map((t: any, idx: number) => ({
            id: t.id,
            conversation_id: session_id,
            title: t.title,
            status: t.status,
            order_num: idx,
            dependencies: [],
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
      alert(`Simulation failed: ${err}`);
      setLoading(false);
    }
  };

  // Helper to calculate progress percentage
  const completedCount = tasks.filter((t) => t.status === "done").length;
  const progressPercent = tasks.length > 0 ? Math.round((completedCount / tasks.length) * 100) : 0;

  return (
    <div className="card animate-fade-in" style={{ height: "100%", display: "flex", flexDirection: "column", gap: "20px", overflow: "hidden" }}>
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", borderBottom: "1px solid var(--border-color)", paddingBottom: "12px" }}>
        <div>
          <h3 style={{ margin: 0, fontSize: "16px", display: "flex", alignItems: "center", gap: "8px" }}>
            👥 Team Mode 协同任务树与计划看板
          </h3>
          <p style={{ margin: "4px 0 0 0", color: "var(--text-secondary)", fontSize: "12px" }}>
            可视化的 Agent 协作计划树，追踪 Leader 分发至辅 Agent 的子任务。
          </p>
        </div>
        <button 
          className="btn btn-secondary" 
          onClick={fetchData} 
          disabled={loading}
          style={{ padding: "4px 8px", fontSize: "11px" }}
        >
          {loading ? "更新中..." : "🔄 刷新"}
        </button>
      </div>

      {/* Progress Section */}
      {tasks.length > 0 && (
        <div style={{ background: "rgba(255,255,255,0.02)", padding: "12px", borderRadius: "8px", border: "1px solid var(--border-color)" }}>
          <div style={{ display: "flex", justifyContent: "space-between", fontSize: "12px", marginBottom: "6px" }}>
            <span style={{ color: "var(--text-secondary)" }}>任务完成度: {completedCount}/{tasks.length}</span>
            <span style={{ color: "var(--color-primary)", fontWeight: 600 }}>{progressPercent}%</span>
          </div>
          <div style={{ width: "100%", height: "6px", background: "rgba(255,255,255,0.05)", borderRadius: "3px", overflow: "hidden" }}>
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
      <div style={{ 
        display: containerWidth < 520 ? "flex" : "grid", 
        flexDirection: containerWidth < 520 ? "column" : undefined,
        gridTemplateColumns: containerWidth < 520 ? undefined : "1.2fr 1fr", 
        gap: "20px", 
        flexGrow: 1, 
        overflowY: containerWidth < 520 ? "auto" : "hidden",
        overflowX: "hidden"
      }}>
        
        {/* Left Side: Visual Plan Tree */}
        <div style={{ 
          display: "flex", 
          flexDirection: "column", 
          gap: "12px", 
          overflowY: containerWidth < 520 ? "visible" : "auto", 
          paddingRight: "8px" 
        }}>
          <h4 style={{ fontSize: "13px", color: "var(--text-primary)", display: "flex", alignItems: "center", gap: "6px" }}>
            📋 计划树结构 (To-Do List)
          </h4>

          {tasks.length === 0 ? (
            <div style={{ 
              display: "flex", 
              flexDirection: "column", 
              alignItems: "center", 
              justifyContent: "center", 
              height: "200px", 
              borderRadius: "8px", 
              border: "1px dashed var(--border-color)",
              color: "var(--text-muted)",
              textAlign: "center",
              padding: "16px"
            }}>
              <span style={{ fontSize: "24px", marginBottom: "8px" }}>🌳</span>
              <p style={{ fontSize: "12px", margin: 0 }}>暂无协作任务。</p>
              <p style={{ fontSize: "11px", color: "var(--text-muted)", marginTop: "4px" }}>
                运行 Agent 时，OMNIX 将自动捕获 ACP 协议数据生成计划树，或者使用右侧仿真引擎进行点火调试。
              </p>
            </div>
          ) : (
            <div style={{ display: "flex", flexDirection: "column", position: "relative", paddingLeft: "20px", borderLeft: "2px solid rgba(255,255,255,0.05)" }}>
              {tasks.map((task) => {
                const isActive = task.status === "in_progress";
                const isDone = task.status === "done";
                
                return (
                  <div 
                    key={task.id} 
                    style={{ 
                      position: "relative", 
                      marginBottom: "16px",
                      background: isActive ? "rgba(79, 172, 254, 0.04)" : "transparent",
                      border: isActive ? "1px solid rgba(79, 172, 254, 0.2)" : "1px solid transparent",
                      borderRadius: "8px",
                      padding: isActive ? "10px" : "4px 0",
                      transition: "all 0.3s"
                    }}
                  >
                    {/* Stepper connector bullet */}
                    <div 
                      style={{ 
                        position: "absolute", 
                        left: isActive ? "-27px" : "-26px", 
                        top: isActive ? "14px" : "8px", 
                        width: isActive ? "12px" : "10px", 
                        height: isActive ? "12px" : "10px", 
                        borderRadius: "50%", 
                        background: isDone ? "var(--color-success)" : isActive ? "var(--color-warning)" : "var(--text-muted)",
                        boxShadow: isActive ? "0 0 10px var(--color-warning)" : isDone ? "0 0 8px var(--color-success)" : "none",
                        transition: "all 0.3s",
                        zIndex: 2
                      }}
                    />

                    <div style={{ display: "flex", justifyContent: "space-between", alignItems: "flex-start", gap: "10px" }}>
                      <span 
                        style={{ 
                          fontSize: "13px", 
                          color: isDone ? "var(--text-muted)" : "var(--text-primary)",
                          textDecoration: isDone ? "line-through" : "none",
                          fontWeight: isActive ? 600 : 400,
                          lineHeight: "1.4"
                        }}
                      >
                        {task.title}
                      </span>
                      <span 
                        style={{ 
                          fontSize: "10px", 
                          padding: "2px 6px", 
                          borderRadius: "4px",
                          fontWeight: 500,
                          background: isDone ? "rgba(16, 185, 129, 0.1)" : isActive ? "rgba(245, 158, 11, 0.1)" : "rgba(255,255,255,0.03)",
                          color: isDone ? "var(--color-success)" : isActive ? "var(--color-warning)" : "var(--text-muted)",
                          border: isDone ? "1px solid rgba(16, 185, 129, 0.2)" : isActive ? "1px solid rgba(245, 158, 11, 0.2)" : "1px solid transparent"
                        }}
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
        <div style={{ 
          display: "flex", 
          flexDirection: "column", 
          gap: "16px", 
          overflowY: containerWidth < 520 ? "visible" : "auto", 
          paddingRight: "8px", 
          borderLeft: containerWidth < 520 ? "none" : "1px solid var(--border-color)", 
          paddingLeft: containerWidth < 520 ? "0" : "16px",
          borderTop: containerWidth < 520 ? "1px solid var(--border-color)" : "none",
          paddingTop: containerWidth < 520 ? "16px" : "0"
        }}>
          
          {/* Simulator Form */}
          <div style={{ background: "rgba(255,255,255,0.01)", border: "1px solid var(--border-color)", borderRadius: "10px", padding: "14px" }}>
            <h4 style={{ fontSize: "12px", margin: "0 0 10px 0", color: "var(--color-secondary)" }}>🧪 Team Mode 协议调试仿真器</h4>
            
            <div style={{ display: "flex", flexDirection: "column", gap: "10px" }}>
              <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: "8px" }}>
                <div>
                  <label style={{ fontSize: "10px", color: "var(--text-secondary)", display: "block", marginBottom: "4px" }}>Leader Agent</label>
                  <select 
                    value={leader} 
                    onChange={(e) => setLeader(e.target.value)}
                    style={{ width: "100%", background: "var(--bg-primary)", border: "1px solid var(--border-color)", borderRadius: "4px", color: "#fff", fontSize: "11px", padding: "4px" }}
                  >
                    <option value="Claude Code">Claude Code</option>
                    <option value="Google Antigravity">Google Antigravity</option>
                    <option value="OpenCode">OpenCode</option>
                  </select>
                </div>
                <div>
                  <label style={{ fontSize: "10px", color: "var(--text-secondary)", display: "block", marginBottom: "4px" }}>Teammate Agent</label>
                  <select 
                    value={teammate} 
                    onChange={(e) => setTeammate(e.target.value)}
                    style={{ width: "100%", background: "var(--bg-primary)", border: "1px solid var(--border-color)", borderRadius: "4px", color: "#fff", fontSize: "11px", padding: "4px" }}
                  >
                    <option value="Google Antigravity">Google Antigravity</option>
                    <option value="OpenCode">OpenCode</option>
                    <option value="Claude Code">Claude Code</option>
                  </select>
                </div>
              </div>

              <button 
                className="btn btn-primary" 
                onClick={handleIgniteSimulation}
                disabled={loading}
                style={{ width: "100%", padding: "6px", fontSize: "12px", marginTop: "4px" }}
              >
                {loading ? "仿真点火中..." : "🔥 点火团队协同仿真"}
              </button>
            </div>
          </div>

          {/* Mailbox Envelope Logs */}
          <div style={{ display: "flex", flexDirection: "column", gap: "8px" }}>
            <h4 style={{ fontSize: "12px", color: "var(--text-primary)", display: "flex", alignItems: "center", gap: "6px" }}>
              📪 协作信箱数据 (Mailbox Log)
            </h4>

            {mailboxMsgs.length === 0 ? (
              <p style={{ fontSize: "11px", color: "var(--text-muted)", margin: "8px 0", textAlign: "center" }}>
                信箱空空如也。仿真运行或 Agent 协作时产生的子任务指令信包将落盘在此处。
              </p>
            ) : (
              <div style={{ display: "flex", flexDirection: "column", gap: "8px" }}>
                {mailboxMsgs.map((msg) => {
                  const isExpanded = expandedMsg === msg.filename;
                  
                  return (
                    <div 
                      key={msg.filename}
                      style={{ 
                        background: "rgba(255,255,255,0.02)", 
                        border: "1px solid var(--border-color)", 
                        borderRadius: "8px", 
                        padding: "8px 12px",
                        cursor: "pointer"
                      }}
                      onClick={() => setExpandedMsg(isExpanded ? null : msg.filename)}
                    >
                      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
                        <span style={{ fontSize: "11px", fontFamily: "var(--font-mono)", color: "var(--color-primary)" }}>
                          ✉️ {msg.filename.slice(0, 18)}...
                        </span>
                        <span style={{ fontSize: "10px", color: "var(--text-muted)" }}>
                          {msg.timestamp.split("T")[1]?.slice(0, 5) || "Active"}
                        </span>
                      </div>
                      
                      <div style={{ fontSize: "11px", margin: "6px 0 2px 0", color: "var(--text-secondary)" }}>
                        <strong>{msg.sender}</strong> ➔ <strong>{msg.receiver}</strong>
                      </div>
                      
                      <div style={{ fontSize: "11px", color: "var(--text-primary)" }}>
                        指令: <code style={{ color: "var(--color-warning)", fontFamily: "var(--font-mono)" }}>{msg.command}</code>
                      </div>

                      {isExpanded && (
                        <div style={{ 
                          marginTop: "8px", 
                          background: "var(--bg-primary)", 
                          padding: "8px", 
                          borderRadius: "4px", 
                          border: "1px solid rgba(255,255,255,0.03)",
                          overflowX: "auto"
                        }}>
                          <pre style={{ margin: 0, fontSize: "10px", fontFamily: "var(--font-mono)", color: "var(--text-secondary)" }}>
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

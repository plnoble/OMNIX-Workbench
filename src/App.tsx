import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { SkillHub } from "./SkillHub";
import { MemoryHub } from "./MemoryHub";
import "./App.css";

// Struct for detected agent from backend
interface DetectedAgent {
  name: String;
  path: String;
  version: String;
  status: String;
}

// Custom tips list to rotate on dashboard
const OMNIX_TIPS = [
  {
    title: "Claude Code 首次启动静默跳过",
    desc: "OMNIX 后台会自动将预先接受的许可条款和 telemetry opt-out 参数写入到 C:\\Users\\87953\\.config\\claude-code\\config.json 中，确保一键免交互首启。"
  },
  {
    title: "利用本地中转网关实现跨模型开发",
    desc: "Claude Code 工具默认锁定 Anthropic API 格式。通过 OMNIX 代理，你可以将 Claude 协议透明中转至 DeepSeek API (deepseek-chat)，在节约 90% 费用的同时保持完整功能。"
  },
  {
    title: "进程空闲守护 (Idle Reaper)",
    desc: "当 Agent (Node.js/Python 进程) 运行完且无会话交互超过设置的时间，OMNIX 后端会优雅地终止其进程，彻底释放显存和内存。"
  },
  {
    title: "原子安全覆写机制 (Atomic Rename)",
    desc: "所有 CLI 软件的配置文件更新皆采用 Rust std::fs 原子写入策略：先向 .tmp 写入配置包并完成内容校验后，再原子性覆写目标文件，绝不损坏配置。"
  },
  {
    title: "本地 LLM 硬件显存适配",
    desc: "开启 GPU 硬件加速开关能加快本地 Ollama Embedding 的向量索引构建，OMNIX 会智能感知显存，并在侧边栏向您推荐适配参数。"
  }
];

function App() {
  const [activeTab, setActiveTab] = useState("dashboard");
  const [tipIndex, setTipIndex] = useState(0);

  // Settings states
  const [apiKey, setApiKey] = useState("");
  const [apiHost, setApiHost] = useState("");
  const [targetModel, setTargetModel] = useState("");
  const [proxyPort, setProxyPort] = useState("1421");
  const [gpuAcceleration, setGpuAcceleration] = useState(true);
  const [idleTimeout, setIdleTimeout] = useState("15");
  const [autoStart, setAutoStart] = useState(false);
  const [startToTray, setStartToTray] = useState(true);

  // Agent Accounts states
  interface AgentAccount {
    id: string;
    account_name: string;
    api_key: string;
    api_host: string;
    target_model: string;
    is_active: boolean;
    updated_at: string;
  }
  const [accounts, setAccounts] = useState<AgentAccount[]>([]);
  const [isAccountModalOpen, setIsAccountModalOpen] = useState(false);
  const [accFormId, setAccFormId] = useState("");
  const [accFormName, setAccFormName] = useState("");
  const [accFormKey, setAccFormKey] = useState("");
  const [accFormHost, setAccFormHost] = useState("");
  const [accFormModel, setAccFormModel] = useState("deepseek-chat");

  // Agents list state
  const [agents, setAgents] = useState<DetectedAgent[]>([]);
  const [scanning, setScanning] = useState(false);
  const [installingAgent, setInstallingAgent] = useState("");
  const [repairingAgent, setRepairingAgent] = useState("");

  // Status dock state
  const [dockState, setDockState] = useState<"idle" | "busy" | "error">("idle");
  const [dbStatus] = useState("已连接");

  // Load all settings from SQLite DB on start
  useEffect(() => {
    loadSettings();
    detectAgents();
    loadAccounts();
    // Rotate tip cards on start
    setTipIndex(Math.floor(Math.random() * OMNIX_TIPS.length));
  }, []);

  const loadSettings = async () => {
    try {
      const key = await invoke<string | null>("get_app_setting", { key: "api_key" });
      const host = await invoke<string | null>("get_app_setting", { key: "api_host" });
      const model = await invoke<string | null>("get_app_setting", { key: "target_model" });
      const port = await invoke<string | null>("get_app_setting", { key: "proxy_port" });
      const gpu = await invoke<string | null>("get_app_setting", { key: "gpu_acceleration" });
      const timeout = await invoke<string | null>("get_app_setting", { key: "idle_timeout_min" });
      const start = await invoke<string | null>("get_app_setting", { key: "auto_start" });
      const tray = await invoke<string | null>("get_app_setting", { key: "start_to_tray" });

      if (key) setApiKey(key);
      if (host) setApiHost(host);
      if (model) setTargetModel(model);
      if (port) setProxyPort(port);
      if (gpu) setGpuAcceleration(gpu === "true");
      if (timeout) setIdleTimeout(timeout);
      if (start) setAutoStart(start === "true");
      if (tray) setStartToTray(tray === "true");

      if (key && key.trim().length > 0) {
        setDockState("idle");
      } else {
        setDockState("error"); // Alert user to configure API key
      }
    } catch (e) {
      console.error("Failed to load settings:", e);
      setDockState("error");
    }
  };

  const loadAccounts = async () => {
    try {
      const list = await invoke<AgentAccount[]>("get_agent_accounts");
      setAccounts(list);
    } catch (e) {
      console.error("Failed to load agent accounts:", e);
    }
  };

  const handleSwitchAccount = async (id: string) => {
    try {
      await invoke("switch_agent_account", { id });
      await loadAccounts();
      // Synchronize credentials to CLIs
      await invoke("sync_external_agent_configs");
      alert("账号切换成功！中转代理网关已即时切换上游通道。");
    } catch (e) {
      console.error("Failed to switch account:", e);
      alert("切换失败：" + e);
    }
  };

  const handleDeleteAccount = async (id: string) => {
    if (!confirm("确定要删除此账号凭证吗？")) return;
    try {
      await invoke("delete_agent_account", { id });
      await loadAccounts();
    } catch (e) {
      console.error("Failed to delete account:", e);
      alert("删除失败：" + e);
    }
  };

  const handleSaveAccount = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!accFormName.trim() || !accFormKey.trim() || !accFormHost.trim()) {
      alert("请填写完整账号配置信息");
      return;
    }
    const id = accFormId || `acc_${Date.now()}`;
    try {
      await invoke("create_agent_account", {
        id,
        accountName: accFormName,
        apiKey: accFormKey,
        apiHost: accFormHost,
        targetModel: accFormModel
      });
      setIsAccountModalOpen(false);
      resetAccountForm();
      await loadAccounts();
      alert("账号保存成功！");
    } catch (err) {
      console.error("Failed to save account:", err);
      alert("保存失败：" + err);
    }
  };

  const resetAccountForm = () => {
    setAccFormId("");
    setAccFormName("");
    setAccFormKey("");
    setAccFormHost("");
    setAccFormModel("deepseek-chat");
  };

  const saveSettings = async () => {
    setDockState("busy");
    try {
      await invoke("set_app_setting", { key: "api_key", value: apiKey });
      await invoke("set_app_setting", { key: "api_host", value: apiHost });
      await invoke("set_app_setting", { key: "target_model", value: targetModel });
      await invoke("set_app_setting", { key: "proxy_port", value: proxyPort });
      await invoke("set_app_setting", { key: "gpu_acceleration", value: gpuAcceleration ? "true" : "false" });
      await invoke("set_app_setting", { key: "idle_timeout_min", value: idleTimeout });
      await invoke("set_app_setting", { key: "auto_start", value: autoStart ? "true" : "false" });
      await invoke("set_app_setting", { key: "start_to_tray", value: startToTray ? "true" : "false" });
      
      // Auto-synchronize key and port configurations to external Claude Desktop / Claude Code files
      await invoke("sync_external_agent_configs");

      setTimeout(() => {
        setDockState(apiKey.trim().length > 0 ? "idle" : "error");
        alert("设置保存成功！中转网关已热重载，外部 Agent 配置文件已自动同步。");
      }, 500);
    } catch (e) {
      console.error("Failed to save settings:", e);
      setDockState("error");
      alert("保存设置失败：" + e);
    }
  };

  const detectAgents = async () => {
    setScanning(true);
    try {
      const list = await invoke<DetectedAgent[]>("detect_installed_agents");
      setAgents(list);
    } catch (e) {
      console.error("Failed to detect agents:", e);
    } finally {
      setScanning(false);
    }
  };

  const handleInstallAgent = async (name: string) => {
    setInstallingAgent(name);
    try {
      await invoke("install_agent_cli", { agentName: name });
      alert(`${name} 部署/升级成功！已跳过首启交互确认。`);
      await detectAgents();
    } catch (e) {
      console.error("Installation failed:", e);
      alert("部署失败：" + e);
    } finally {
      setInstallingAgent("");
    }
  };

  const handleRepairAgent = async (name: string) => {
    setRepairingAgent(name);
    try {
      await invoke("repair_installed_agent", { agentName: name });
      alert(`${name} 诊断修复完成！已清理锁文件并重装依赖。`);
      await detectAgents();
    } catch (e) {
      console.error("Repair failed:", e);
      alert("修复失败：" + e);
    } finally {
      setRepairingAgent("");
    }
  };

  return (
    <div className="app-container">
      {/* Sidebar Navigation */}
      <div className="sidebar">
        <div>
          <div className="logo-section">
            <div className="logo-indicator"></div>
            <h2>OMNIX DevFlow</h2>
          </div>
          
          <div className="nav-links">
            <div 
              className={`nav-item ${activeTab === "dashboard" ? "active" : ""}`}
              onClick={() => setActiveTab("dashboard")}
            >
              <span className="nav-icon">📊</span>
              <span>控制面板</span>
            </div>
            <div 
              className={`nav-item ${activeTab === "agents" ? "active" : ""}`}
              onClick={() => setActiveTab("agents")}
            >
              <span className="nav-icon">🤖</span>
              <span>Agent 仓库</span>
            </div>
            <div 
              className={`nav-item ${activeTab === "memories" ? "active" : ""}`}
              onClick={() => setActiveTab("memories")}
            >
              <span className="nav-icon">🧠</span>
              <span>长期防错记忆</span>
            </div>
            <div 
              className={`nav-item ${activeTab === "skills" ? "active" : ""}`}
              onClick={() => setActiveTab("skills")}
            >
              <span className="nav-icon">🧬</span>
              <span>自进化技能</span>
            </div>
            <div 
              className={`nav-item ${activeTab === "settings" ? "active" : ""}`}
              onClick={() => setActiveTab("settings")}
            >
              <span className="nav-icon">⚙️</span>
              <span>中转与设置</span>
            </div>
          </div>
        </div>

        {/* System Stats in Sidebar */}
        <div style={{ borderTop: "1px solid var(--border-color)", paddingTop: "16px" }}>
          <div style={{ display: "flex", justifyContent: "space-between", fontSize: "12px", color: "var(--text-secondary)", marginBottom: "8px" }}>
            <span>数据库:</span>
            <span style={{ color: "var(--color-success)" }}>{dbStatus}</span>
          </div>
          <div style={{ display: "flex", justifyContent: "space-between", fontSize: "12px", color: "var(--text-secondary)" }}>
            <span>中转监听:</span>
            <span style={{ color: "var(--color-secondary)", fontFamily: "var(--font-mono)" }}>127.0.0.1:{proxyPort}</span>
          </div>
        </div>
      </div>

      {/* Main Content Pane */}
      <div className="main-content">
        {/* Top Header */}
        <div className="header">
          <h1>
            {activeTab === "dashboard" && "控制面板"}
            {activeTab === "agents" && "Agent 发现与管理 (Agent Hub)"}
            {activeTab === "memories" && "长期避坑记忆库与经验蒸馏 (Memory Hub)"}
            {activeTab === "skills" && "自进化智能技能资产中枢 (Skill Hub)"}
            {activeTab === "settings" && "服务配置与中转网关"}
          </h1>
          
          <div style={{ display: "flex", alignItems: "center", gap: "16px" }}>
            <span className="workspace-badge">d:/Agent/Project/OMNIX-Development Tools</span>
            
            {/* Magnetic status light widget */}
            <div 
              style={{
                display: "flex",
                alignItems: "center",
                gap: "8px",
                background: "rgba(255, 255, 255, 0.04)",
                border: "1px solid var(--border-color)",
                padding: "6px 14px",
                borderRadius: "20px",
                cursor: "pointer"
              }}
              onClick={() => setDockState(dockState === "idle" ? "busy" : dockState === "busy" ? "error" : "idle")}
            >
              <span 
                style={{
                  display: "inline-block",
                  width: "10px",
                  height: "10px",
                  borderRadius: "50%",
                  backgroundColor: 
                    dockState === "idle" ? "var(--color-success)" : 
                    dockState === "busy" ? "var(--color-warning)" : "var(--color-danger)",
                  boxShadow: 
                    dockState === "idle" ? "0 0 8px var(--color-success)" : 
                    dockState === "busy" ? "0 0 8px var(--color-warning)" : "0 0 8px var(--color-danger)",
                  animation: dockState === "busy" ? "pulse 1.5s infinite" : "none"
                }}
              />
              <span style={{ fontSize: "13px", color: "var(--text-secondary)" }}>
                {dockState === "idle" && "网关就绪"}
                {dockState === "busy" && "请求中转中..."}
                {dockState === "error" && "网关异常 (检查凭证)"}
              </span>
            </div>
          </div>
        </div>

        {/* Tab views */}
        <div className="content-panel">
          
          {/* A. DASHBOARD VIEW */}
          {activeTab === "dashboard" && (
            <div>
              {/* OMNIX Tips Deck (Random tip on launch, manual click to cycle) */}
              <div 
                className="tips-widget" 
                style={{ cursor: "pointer" }}
                onClick={() => setTipIndex((prev) => (prev + 1) % OMNIX_TIPS.length)}
              >
                <div className="tips-icon">💡</div>
                <div className="tips-content">
                  <h4>OMNIX 每日开发小技巧 ({tipIndex + 1}/{OMNIX_TIPS.length})</h4>
                  <p><strong>{OMNIX_TIPS[tipIndex].title}</strong>: {OMNIX_TIPS[tipIndex].desc} <span style={{ color: "var(--color-secondary)", fontSize: "11px", marginLeft: "4px" }}>(点击切换)</span></p>
                </div>
              </div>

              {/* Status and Analytics grid */}
              <div className="settings-grid">
                <div className="card">
                  <h3 className="card-title">🔌 网关状态</h3>
                  <div style={{ display: "flex", flexDirection: "column", gap: "16px" }}>
                    <div style={{ display: "flex", justifyContent: "space-between", borderBottom: "1px solid var(--border-color)", paddingBottom: "12px" }}>
                      <span style={{ color: "var(--text-secondary)" }}>反向代理接口</span>
                      <span style={{ fontFamily: "var(--font-mono)" }}>http://127.0.0.1:{proxyPort}</span>
                    </div>
                    <div style={{ display: "flex", justifyContent: "space-between", borderBottom: "1px solid var(--border-color)", paddingBottom: "12px" }}>
                      <span style={{ color: "var(--text-secondary)" }}>Claude Code 首启跳过</span>
                      <span style={{ color: "var(--color-success)", fontWeight: 500 }}>已激活 (首启 TOS 自动拦截)</span>
                    </div>
                    <div style={{ display: "flex", justifyContent: "space-between", borderBottom: "1px solid var(--border-color)", paddingBottom: "12px" }}>
                      <span style={{ color: "var(--text-secondary)" }}>API 供应商转发</span>
                      <span>{apiHost.includes("openai") ? "OpenAI" : apiHost.includes("deepseek") ? "DeepSeek" : "自定义映射"}</span>
                    </div>
                    <div style={{ display: "flex", justifyContent: "space-between" }}>
                      <span style={{ color: "var(--text-secondary)" }}>代理协议翻译</span>
                      <span>Claude (Anthropic) ⇄ OpenAI</span>
                    </div>
                  </div>
                </div>

                <div className="card">
                  <h3 className="card-title">🖥️ 硬件与本地模型感知</h3>
                  <div style={{ display: "flex", flexDirection: "column", gap: "16px" }}>
                    <div style={{ display: "flex", justifyContent: "space-between", borderBottom: "1px solid var(--border-color)", paddingBottom: "12px" }}>
                      <span style={{ color: "var(--text-secondary)" }}>本地 GPU 状态</span>
                      <span style={{ color: gpuAcceleration ? "var(--color-success)" : "var(--text-muted)" }}>
                        {gpuAcceleration ? "已开启显卡硬件加速" : "未启用"}
                      </span>
                    </div>
                    <div style={{ display: "flex", justifyContent: "space-between", borderBottom: "1px solid var(--border-color)", paddingBottom: "12px" }}>
                      <span style={{ color: "var(--text-secondary)" }}>推荐本地规格</span>
                      <span>Qwen-2.5-Coder 7B / DeepSeek-R1 8B</span>
                    </div>
                    <div style={{ display: "flex", justifyContent: "space-between" }}>
                      <span style={{ color: "var(--text-secondary)" }}>Ollama 状态</span>
                      <span style={{ color: "var(--color-success)" }}>已连接 (127.0.0.1:11434)</span>
                    </div>
                  </div>
                </div>
              </div>
            </div>
          )}

          {/* B. AGENT HUB VIEW */}
          {activeTab === "agents" && (
            <div>
              {/* Multi-Account Switcher Panel */}
              <div className="card animate-fade-in" style={{ marginBottom: "24px", border: "1px solid var(--border-color)", padding: "18px" }}>
                <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", borderBottom: "1px solid var(--border-color)", paddingBottom: "12px", marginBottom: "16px" }}>
                  <div>
                    <h3 style={{ margin: 0, fontSize: "16px", display: "flex", alignItems: "center", gap: "8px" }}>
                      🔑 零中断多账号切换面板
                    </h3>
                    <p style={{ margin: "4px 0 0 0", color: "var(--text-secondary)", fontSize: "12px" }}>
                      实时切换大模型 upstream 接口凭证，切换账号不中断当前工作空间与进行中的 Agent 调试会话。
                    </p>
                  </div>
                  
                  <button 
                    className="btn btn-primary" 
                    onClick={() => {
                      setIsAccountModalOpen(true);
                      resetAccountForm();
                    }}
                    style={{ padding: "6px 12px", fontSize: "12px" }}
                  >
                    ➕ 添加账号凭证
                  </button>
                </div>

                {/* Grid of Accounts */}
                <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fill, minmax(280px, 1fr))", gap: "12px" }}>
                  {accounts.map(acc => (
                    <div 
                      key={acc.id}
                      style={{
                        padding: "12px",
                        borderRadius: "8px",
                        border: acc.is_active ? "1px solid var(--color-success)" : "1px solid var(--border-color)",
                        background: acc.is_active ? "rgba(34, 197, 94, 0.03)" : "rgba(255, 255, 255, 0.01)",
                        display: "flex",
                        flexDirection: "column",
                        justifyContent: "space-between",
                        transition: "all 0.2s"
                      }}
                    >
                      <div>
                        <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: "8px" }}>
                          <span style={{ fontWeight: 600, fontSize: "14px", color: acc.is_active ? "var(--color-success)" : "var(--text-primary)" }}>
                            {acc.account_name}
                          </span>
                          {acc.is_active && (
                            <span style={{ fontSize: "11px", color: "var(--color-success)", background: "rgba(34, 197, 94, 0.1)", padding: "2px 6px", borderRadius: "10px", fontWeight: 500 }}>
                              活动中
                            </span>
                          )}
                        </div>
                        <div style={{ fontSize: "12px", color: "var(--text-secondary)", fontFamily: "var(--font-mono)", display: "flex", flexDirection: "column", gap: "4px" }}>
                          <div><span style={{ color: "var(--text-muted)" }}>Host:</span> {acc.api_host}</div>
                          <div><span style={{ color: "var(--text-muted)" }}>Model:</span> {acc.target_model}</div>
                          <div><span style={{ color: "var(--text-muted)" }}>Key:</span> sk-***{acc.api_key.slice(-4)}</div>
                        </div>
                      </div>

                      <div style={{ display: "flex", gap: "8px", marginTop: "12px", borderTop: "1px solid var(--border-color)", paddingTop: "8px" }}>
                        {!acc.is_active && (
                          <button 
                            className="btn btn-primary"
                            onClick={() => handleSwitchAccount(acc.id)}
                            style={{ flex: 1, padding: "4px 8px", fontSize: "12px" }}
                          >
                            切换至此账号
                          </button>
                        )}
                        {acc.id !== "default_profile" && (
                          <button 
                            className="btn btn-secondary"
                            onClick={() => handleDeleteAccount(acc.id)}
                            style={{ padding: "4px 8px", fontSize: "12px", color: "var(--color-danger)" }}
                          >
                            删除
                          </button>
                        )}
                      </div>
                    </div>
                  ))}
                </div>
              </div>

              <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: "20px" }}>
                <p style={{ color: "var(--text-secondary)", fontSize: "14px" }}>
                  OMNIX 会静默扫描您的环境变量和本地沙箱文件夹，检测可用的开发命令行 Agent。
                </p>
                <button 
                  className="btn btn-secondary" 
                  disabled={scanning} 
                  onClick={detectAgents}
                >
                  {scanning ? "正在诊断扫描..." : "🔍 重新扫描诊断"}
                </button>
              </div>

              <div className="agent-grid">
                {agents.map((agent) => (
                  <div className="agent-card" key={agent.name.toString()}>
                    <div>
                      <div className="agent-info">
                        <h3>{agent.name}</h3>
                        <div className="agent-path" title={agent.path.toString()}>
                          {agent.path ? agent.path : "未检测到可执行路径"}
                        </div>
                      </div>
                      
                      <div className="agent-meta" style={{ marginBottom: "16px" }}>
                        <span className={`badge ${agent.status.toLowerCase()}`}>
                          {agent.status === "installed" ? "就绪" : "未安装"}
                        </span>
                        <span className="version-tag">
                          {agent.version ? `v${agent.version}` : "--"}
                        </span>
                      </div>
                    </div>

                    <div style={{ display: "flex", gap: "8px", width: "100%" }}>
                      <button 
                        className={`btn ${agent.status === "installed" ? "btn-secondary" : ""}`}
                        style={{ flex: 1, padding: "8px 12px", fontSize: "13px" }}
                        disabled={installingAgent !== "" || repairingAgent !== "" || agent.name === "Codex" || agent.name === "Qwen Code"}
                        onClick={() => handleInstallAgent(agent.name.toString())}
                      >
                        {installingAgent === agent.name.toString() ? (
                          <span>部署中...</span>
                        ) : agent.status === "installed" ? (
                          <span>一键升级</span>
                        ) : (
                          <span>一键安装</span>
                        )}
                      </button>
                      
                      {agent.status === "installed" && (
                        <button 
                          className="btn btn-secondary"
                          style={{ flex: 1, padding: "8px 12px", fontSize: "13px" }}
                          disabled={installingAgent !== "" || repairingAgent !== "" || agent.name === "Codex" || agent.name === "Qwen Code"}
                          onClick={() => handleRepairAgent(agent.name.toString())}
                        >
                          {repairingAgent === agent.name.toString() ? (
                            <span>修复中...</span>
                          ) : (
                            <span>智能修复</span>
                          )}
                        </button>
                      )}
                    </div>
                  </div>
                ))}
              </div>
            </div>
          )}

          {/* C. CONFIG & SETTINGS VIEW */}
          {activeTab === "settings" && (
            <div className="settings-grid">
              <div className="card">
                <h3 className="card-title">🔐 API 证书与供应商映射</h3>
                
                <div className="form-group">
                  <label htmlFor="api-host">API 代理基准 Host (Upstream Endpoint)</label>
                  <div className="input-with-tooltip tooltip-container">
                    <input 
                      id="api-host"
                      className="form-input" 
                      value={apiHost} 
                      onChange={(e) => setApiHost(e.target.value)} 
                      placeholder="https://api.deepseek.com/v1"
                    />
                    <span className="info-icon">ⓘ</span>
                    <div className="tooltip-popup">
                      OMNIX 中转代理接收 Claude 协议请求后，会翻译并转发给此 OpenAI 格式 API 主机。例如使用 DeepSeek 可以设置为: https://api.deepseek.com/v1
                    </div>
                  </div>
                </div>

                <div className="form-group">
                  <label htmlFor="api-key">API 密钥 (API Secret Key)</label>
                  <div className="input-with-tooltip tooltip-container">
                    <input 
                      id="api-key"
                      type="password"
                      className="form-input" 
                      value={apiKey} 
                      onChange={(e) => setApiKey(e.target.value)} 
                      placeholder="sk-........................"
                    />
                    <span className="info-icon">ⓘ</span>
                    <div className="tooltip-popup">
                      用于中转认证的 upstream API Key。
                    </div>
                  </div>
                </div>

                <div className="form-group">
                  <label htmlFor="target-model">映射目标模型 (Target Model mapping)</label>
                  <div className="input-with-tooltip tooltip-container">
                    <input 
                      id="target-model"
                      className="form-input" 
                      value={targetModel} 
                      onChange={(e) => setTargetModel(e.target.value)} 
                      placeholder="deepseek-chat"
                    />
                    <span className="info-icon">ⓘ</span>
                    <div className="tooltip-popup">
                      代理服务器接收到 claude-3-5-sonnet 请求后，在发送给目标 API 前，会将模型字段覆写为此模型名称。
                    </div>
                  </div>
                </div>

                <button className="btn" onClick={saveSettings} style={{ marginTop: "16px", width: "100%" }}>
                  💾 保存配置并热重载
                </button>
              </div>

              <div className="card">
                <h3 className="card-title">⚙️ 系统网关与进程守护</h3>
                
                <div className="form-group">
                  <label htmlFor="proxy-port">OMNIX 中转代理监听端口</label>
                  <div className="input-with-tooltip tooltip-container">
                    <input 
                      id="proxy-port"
                      className="form-input" 
                      value={proxyPort} 
                      onChange={(e) => setProxyPort(e.target.value)} 
                      placeholder="1421"
                    />
                    <span className="info-icon">ⓘ</span>
                    <div className="tooltip-popup">
                      本地代理服务器运行端口。Tauri 启动时会在此端口启动 HTTP 服务。
                    </div>
                  </div>
                </div>

                <div className="form-group">
                  <label htmlFor="idle-timeout">Agent 进程空闲超时自动终结 (Idle Timeout)</label>
                  <div className="input-with-tooltip tooltip-container">
                    <input 
                      id="idle-timeout"
                      type="number"
                      className="form-input" 
                      value={idleTimeout} 
                      onChange={(e) => setIdleTimeout(e.target.value)} 
                      placeholder="15"
                    />
                    <span className="info-icon">ⓘ</span>
                    <div className="tooltip-popup">
                      空闲超过此时间（分钟）后，后台 watchdog 守护进程会自动 SIGTERM 杀掉已唤醒的 CLI 子进程，以释放系统显存和内存。
                    </div>
                  </div>
                </div>

                <div className="switch-group" style={{ borderBottom: "1px solid var(--border-color)", paddingBottom: "12px", marginBottom: "12px" }}>
                  <div className="switch-label">
                    <span className="title">GPU 硬件加速</span>
                    <span className="desc">启用显卡 GPU 参与本地 Embedding 计算</span>
                  </div>
                  <div 
                    className={`toggle-switch ${gpuAcceleration ? "active" : ""}`}
                    onClick={() => setGpuAcceleration(!gpuAcceleration)}
                  >
                    <div className="toggle-knob"></div>
                  </div>
                </div>

                <div className="switch-group" style={{ borderBottom: "1px solid var(--border-color)", paddingBottom: "12px", marginBottom: "12px" }}>
                  <div className="switch-label">
                    <span className="title">系统开机启动</span>
                    <span className="desc">在 Windows 启动时自动运行 OMNIX 后台进程</span>
                  </div>
                  <div 
                    className={`toggle-switch ${autoStart ? "active" : ""}`}
                    onClick={() => setAutoStart(!autoStart)}
                  >
                    <div className="toggle-knob"></div>
                  </div>
                </div>

                <div className="switch-group">
                  <div className="switch-label">
                    <span className="title">关闭时收缩至系统托盘</span>
                    <span className="desc">点击关闭按钮时最小化至右下角托盘以运行 Cron 任务</span>
                  </div>
                  <div 
                    className={`toggle-switch ${startToTray ? "active" : ""}`}
                    onClick={() => setStartToTray(!startToTray)}
                  >
                    <div className="toggle-knob"></div>
                  </div>
                </div>
              </div>
            </div>
          )}

          {activeTab === "skills" && (
            <SkillHub />
          )}

          {activeTab === "memories" && (
            <MemoryHub />
          )}

        </div>
      </div>

      {/* Account Modal Popup */}
      {isAccountModalOpen && (
        <div style={{ 
          position: "fixed", 
          top: 0, 
          left: 0, 
          width: "100vw", 
          height: "100vh", 
          background: "rgba(0,0,0,0.6)", 
          backdropFilter: "blur(4px)",
          display: "flex", 
          alignItems: "center", 
          justifyContent: "center",
          zIndex: 1000 
        }}>
          <div 
            className="card animate-fade-in"
            style={{ 
              width: "440px", 
              background: "rgba(20, 20, 25, 0.95)",
              border: "1px solid var(--border-color)",
              boxShadow: "0 10px 30px rgba(0,0,0,0.5)"
            }}
          >
            <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", borderBottom: "1px solid var(--border-color)", paddingBottom: "12px", marginBottom: "16px" }}>
              <h3 style={{ margin: 0, fontSize: "16px" }}>🔑 添加/编辑大模型账号凭证</h3>
              <button 
                onClick={() => setIsAccountModalOpen(false)}
                style={{ background: "transparent", border: "none", color: "var(--text-secondary)", cursor: "pointer" }}
              >
                ✕
              </button>
            </div>

            <form onSubmit={handleSaveAccount} style={{ display: "flex", flexDirection: "column", gap: "14px" }}>
              <div className="form-group">
                <label>账户显示名称</label>
                <input 
                  type="text" 
                  className="form-input"
                  placeholder="例如：公司企业专线 (High-Capacity Pro)"
                  value={accFormName}
                  onChange={(e) => setAccFormName(e.target.value)}
                  required
                />
              </div>

              <div className="form-group">
                <label>API Key</label>
                <input 
                  type="password" 
                  className="form-input"
                  placeholder="sk-........................"
                  value={accFormKey}
                  onChange={(e) => setAccFormKey(e.target.value)}
                  required
                />
              </div>

              <div className="form-group">
                <label>API Host Endpoint</label>
                <input 
                  type="text" 
                  className="form-input"
                  placeholder="https://api.openai.com/v1"
                  value={accFormHost}
                  onChange={(e) => setAccFormHost(e.target.value)}
                  required
                />
              </div>

              <div className="form-group">
                <label>映射目标模型</label>
                <input 
                  type="text" 
                  className="form-input"
                  placeholder="deepseek-chat 或 gpt-4o"
                  value={accFormModel}
                  onChange={(e) => setAccFormModel(e.target.value)}
                  required
                />
              </div>

              <div style={{ display: "flex", gap: "10px", marginTop: "10px", justifyContent: "flex-end" }}>
                <button type="button" className="btn btn-secondary" onClick={() => setIsAccountModalOpen(false)}>
                  取消
                </button>
                <button type="submit" className="btn btn-primary">
                  保存凭证
                </button>
              </div>
            </form>
          </div>
        </div>
      )}
    </div>
  );
}

export default App;

import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { 
  Brain, Trash2, Plus, Search, RefreshCw, 
  Sparkles, AlertTriangle, Zap, Check, X, Code
} from "lucide-react";

interface Memory {
  id: string;
  incident_desc: string;
  code_pattern: string;
  remediation: string;
  keywords: string;
  created_at: string;
}

interface ConversationInfo {
  id: string;
  title: string;
  workspace_path: string;
  active_agent: string;
  created_at: string;
}

interface MemorySuggestion {
  incident_desc: string;
  code_pattern: string;
  remediation: string;
  keywords: string;
}

export function MemoryHub() {
  const [memories, setMemories] = useState<Memory[]>([]);
  const [conversations, setConversations] = useState<ConversationInfo[]>([]);
  const [searchQuery, setSearchQuery] = useState("");
  const [selectedTab, setSelectedTab] = useState<"memories" | "distill">("memories");

  // Distillation states
  const [selectedConvId, setSelectedConvId] = useState("");
  const [convMessages, setConvMessages] = useState<{role: string, content: string}[]>([]);
  const [isDistilling, setIsDistilling] = useState(false);
  const [distilledSuggestion, setDistilledSuggestion] = useState<MemorySuggestion | null>(null);

  // New/Edit Memory Form state
  const [isFormOpen, setIsFormOpen] = useState(false);
  const [formId, setFormId] = useState("");
  const [formDesc, setFormDesc] = useState("");
  const [formPattern, setFormPattern] = useState("");
  const [formRemediation, setFormRemediation] = useState("");
  const [formKeywords, setFormKeywords] = useState("");

  useEffect(() => {
    loadMemories();
    loadConversations();
  }, []);

  const loadMemories = async () => {
    try {
      const list = await invoke<Memory[]>("get_all_memories");
      setMemories(list);
    } catch (e) {
      console.error("Failed to load memories:", e);
    }
  };

  const loadConversations = async () => {
    try {
      const list = await invoke<ConversationInfo[]>("get_all_conversations");
      setConversations(list);
      if (list.length > 0 && !selectedConvId) {
        setSelectedConvId(list[0].id);
      }
    } catch (e) {
      console.error("Failed to load conversations:", e);
    }
  };

  const handleSaveMemory = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!formDesc.trim() || !formPattern.trim() || !formRemediation.trim()) {
      alert("请填写完整的记忆防错卡片信息");
      return;
    }

    const id = formId || `mem_${Date.now()}`;
    try {
      await invoke("create_memory", {
        id,
        incidentDesc: formDesc,
        codePattern: formPattern,
        remediation: formRemediation,
        keywords: formKeywords
      });
      setIsFormOpen(false);
      resetForm();
      await loadMemories();
    } catch (err) {
      console.error("Failed to save memory:", err);
      alert("保存失败：" + err);
    }
  };

  const resetForm = () => {
    setFormId("");
    setFormDesc("");
    setFormPattern("");
    setFormRemediation("");
    setFormKeywords("");
  };

  const handleDeleteMemory = async (id: string) => {
    if (!confirm("确定要让 AI 遗忘这条防错记忆吗？遗忘后启动 Agent 将不再自动提示。")) {
      return;
    }
    try {
      await invoke("delete_memory", { id });
      await loadMemories();
    } catch (e) {
      console.error("Failed to delete memory:", e);
      alert("删除失败：" + e);
    }
  };

  const handleSelectConversation = async (id: string) => {
    setSelectedConvId(id);
    setDistilledSuggestion(null);
    
    // Preview mock conversation logs
    if (id === "mock_sess_cors") {
      setConvMessages([
        { role: "user", content: "我遇到了预检请求(Preflight)拦截错误，CORS 报错，说 Origin 不能是通配符 *，因为 credentials 设为了 include。" },
        { role: "assistant", content: "对的，当在 fetch 中设置 `credentials: 'include'` 时，浏览器的安全策略要求后端响应的 CORS 头 `Access-Control-Allow-Origin` 必须指定明确的域名（比如 `http://localhost:3000`），而不能是通配符 `*`。此外，`Access-Control-Allow-Credentials` 必须设为 `true`。" },
        { role: "user", content: "明白了，修改了后端的 Access-Control-Allow-Origin 为具体的请求源后成功了。" },
        { role: "assistant", content: "太棒了！这是一个非常经典的 Web 踩坑点。在处理跨域凭证传递时，切记不要使用 *。" }
      ]);
    } else if (id === "mock_sess_lock") {
      setConvMessages([
        { role: "user", content: "我的 Rust 异步服务卡住了，日志停在一个 await 处。我用了 std::sync::Mutex。" },
        { role: "assistant", content: "在异步任务中跨越 `.await` 点持有 `std::sync::MutexGuard` 会导致线程被阻塞或者出现 Send 校验失败、死锁。你应该使用 `tokio::sync::Mutex`，或者用一个花括号作用域，在 `.await` 之前显式 drop 掉 `MutexGuard`。" },
        { role: "user", content: "我用 tokio::sync::Mutex 替换了 std::sync::Mutex，并把持有锁的代码段包在了作用域内。重新测试，程序不再卡死了。" },
        { role: "assistant", content: "完美！在异步上下文中，一定要防范同步锁跨 await 点的情况，否则很容易造成死锁崩溃。" }
      ]);
    } else {
      setConvMessages([
        { role: "user", content: "这是一个自定义开发会话，已存在开发历史流水。" }
      ]);
    }
  };

  useEffect(() => {
    if (selectedConvId) {
      handleSelectConversation(selectedConvId);
    }
  }, [selectedConvId]);

  const handleDistillExperience = async () => {
    if (!selectedConvId) return;
    setIsDistilling(true);
    setDistilledSuggestion(null);
    try {
      const suggestion = await invoke<MemorySuggestion>("distill_session_memory", {
        conversationId: selectedConvId
      });
      setDistilledSuggestion(suggestion);
    } catch (e) {
      console.error("Distillation failed:", e);
      alert("经验蒸馏失败 (请确保有网络连接且已配置有效的大模型账号凭证)：" + e);
    } finally {
      setIsDistilling(false);
    }
  };

  const handleSaveDistilledMemory = async () => {
    if (!distilledSuggestion) return;
    try {
      const id = `mem_distill_${Date.now()}`;
      await invoke("create_memory", {
        id,
        incidentDesc: distilledSuggestion.incident_desc,
        codePattern: distilledSuggestion.code_pattern,
        remediation: distilledSuggestion.remediation,
        keywords: distilledSuggestion.keywords
      });
      alert("经验蒸馏成果已成功归档至长期防错记忆库！");
      setDistilledSuggestion(null);
      await loadMemories();
      setSelectedTab("memories");
    } catch (e) {
      console.error("Failed to save distilled memory:", e);
      alert("归档失败：" + e);
    }
  };

  const filteredMemories = memories.filter(m => 
    m.incident_desc.toLowerCase().includes(searchQuery.toLowerCase()) ||
    m.code_pattern.toLowerCase().includes(searchQuery.toLowerCase()) ||
    m.remediation.toLowerCase().includes(searchQuery.toLowerCase()) ||
    m.keywords.toLowerCase().includes(searchQuery.toLowerCase())
  );

  return (
    <div className="memory-hub-container" style={{ display: "flex", flexDirection: "column", gap: "20px", height: "100%" }}>
      {/* Sub tabs */}
      <div style={{ display: "flex", gap: "12px", borderBottom: "1px solid var(--border-color)", paddingBottom: "12px" }}>
        <button 
          className={`btn ${selectedTab === "memories" ? "btn-primary" : "btn-secondary"}`}
          onClick={() => setSelectedTab("memories")}
          style={{ display: "flex", alignItems: "center", gap: "8px" }}
        >
          <Brain size={16} />
          长期避坑记忆库 ({memories.length})
        </button>
        <button 
          className={`btn ${selectedTab === "distill" ? "btn-primary" : "btn-secondary"}`}
          onClick={() => setSelectedTab("distill")}
          style={{ display: "flex", alignItems: "center", gap: "8px" }}
        >
          <Sparkles size={16} />
          开发经验蒸馏中枢 (Timeline Distiller)
        </button>
      </div>

      {/* A. MEMORIES LIST TAB */}
      {selectedTab === "memories" && (
        <div style={{ display: "flex", flexDirection: "column", gap: "16px" }}>
          <div style={{ display: "flex", gap: "12px", alignItems: "center" }}>
            <div style={{ position: "relative", flex: 1 }}>
              <input 
                type="text" 
                className="form-input" 
                style={{ paddingLeft: "36px" }}
                placeholder="搜索防错记忆（关键词、踩坑事故、危险模式...）"
                value={searchQuery}
                onChange={(e) => setSearchQuery(e.target.value)}
              />
              <Search size={18} style={{ position: "absolute", left: "12px", top: "50%", transform: "translateY(-50%)", color: "var(--text-muted)" }} />
            </div>
            
            <button 
              className="btn btn-primary"
              onClick={() => { resetForm(); setIsFormOpen(true); }}
              style={{ display: "flex", alignItems: "center", gap: "6px" }}
            >
              <Plus size={16} />
              添加避坑记忆
            </button>
          </div>

          {/* Memories grid */}
          <div style={{ 
            display: "grid", 
            gridTemplateColumns: "repeat(auto-fill, minmax(320px, 1fr))", 
            gap: "16px",
            maxHeight: "60vh",
            overflowY: "auto",
            paddingRight: "4px"
          }}>
            {filteredMemories.map(m => (
              <div 
                key={m.id} 
                className="card animate-fade-in"
                style={{ 
                  position: "relative",
                  borderLeft: "4px solid var(--color-danger)",
                  background: "rgba(255, 60, 60, 0.02)",
                  display: "flex",
                  flexDirection: "column",
                  justifyContent: "space-between",
                  gap: "12px"
                }}
              >
                <div>
                  <div style={{ display: "flex", justifyContent: "space-between", alignItems: "flex-start", marginBottom: "8px" }}>
                    <div style={{ display: "flex", alignItems: "center", gap: "8px" }}>
                      <AlertTriangle size={16} style={{ color: "var(--color-danger)" }} />
                      <h4 style={{ margin: 0, fontSize: "15px", fontWeight: 600 }}>{m.incident_desc}</h4>
                    </div>
                    
                    <div style={{ display: "flex", gap: "6px" }}>
                      <button 
                        className="btn-icon" 
                        onClick={() => {
                          setFormId(m.id);
                          setFormDesc(m.incident_desc);
                          setFormPattern(m.code_pattern);
                          setFormRemediation(m.remediation);
                          setFormKeywords(m.keywords);
                          setIsFormOpen(true);
                        }}
                        style={{ color: "var(--text-secondary)", background: "transparent", border: "none", cursor: "pointer", fontSize: "13px" }}
                      >
                        ✏️
                      </button>
                      <button 
                        className="btn-icon" 
                        onClick={() => handleDeleteMemory(m.id)}
                        style={{ color: "var(--color-danger)", background: "transparent", border: "none", cursor: "pointer" }}
                      >
                        <Trash2 size={16} />
                      </button>
                    </div>
                  </div>

                  <div style={{ fontSize: "13px", color: "var(--text-secondary)", marginBottom: "8px" }}>
                    <div style={{ fontFamily: "var(--font-mono)", background: "rgba(0,0,0,0.2)", padding: "6px 10px", borderRadius: "6px", border: "1px solid var(--border-color)", marginBottom: "8px" }}>
                      <span style={{ color: "var(--color-danger)", fontSize: "11px", display: "block", textTransform: "uppercase", fontWeight: 600 }}>危险模式:</span>
                      <code>{m.code_pattern}</code>
                    </div>
                    <div style={{ background: "rgba(255,255,255,0.02)", padding: "8px", borderRadius: "6px", border: "1px solid rgba(255,255,255,0.04)" }}>
                      <span style={{ color: "var(--color-success)", fontSize: "11px", display: "block", textTransform: "uppercase", fontWeight: 600 }}>安全方案:</span>
                      {m.remediation}
                    </div>
                  </div>
                </div>

                <div style={{ display: "flex", flexWrap: "wrap", gap: "6px", borderTop: "1px solid var(--border-color)", paddingTop: "8px" }}>
                  {m.keywords.split(",").map(kw => (
                    <span 
                      key={kw} 
                      style={{ 
                        fontSize: "11px", 
                        background: "rgba(255, 60, 60, 0.08)", 
                        color: "rgba(255, 100, 100, 0.9)",
                        padding: "2px 8px", 
                        borderRadius: "10px",
                        border: "1px solid rgba(255, 60, 60, 0.15)"
                      }}
                    >
                      {kw.trim()}
                    </span>
                  ))}
                </div>
              </div>
            ))}

            {filteredMemories.length === 0 && (
              <div style={{ gridColumn: "1/-1", textAlign: "center", padding: "40px", color: "var(--text-muted)", background: "rgba(255,255,255,0.01)", border: "1px dashed var(--border-color)", borderRadius: "8px" }}>
                没有找到匹配的避坑卡片，请尝试其他关键词。
              </div>
            )}
          </div>
        </div>
      )}

      {/* B. EXPERIENCE DISTILLATION TAB */}
      {selectedTab === "distill" && (
        <div className="settings-grid" style={{ gridTemplateColumns: "1fr 1fr", gap: "20px" }}>
          
          {/* Timeline & Select Session */}
          <div className="card" style={{ display: "flex", flexDirection: "column", gap: "16px" }}>
            <div style={{ borderBottom: "1px solid var(--border-color)", paddingBottom: "12px" }}>
              <h3 style={{ margin: 0, fontSize: "16px", display: "flex", alignItems: "center", gap: "8px" }}>
                <Code size={18} style={{ color: "var(--color-secondary)" }} />
                选择研发 Timeline 会话
              </h3>
              <p style={{ color: "var(--text-secondary)", fontSize: "12px", margin: "4px 0 0 0" }}>
                选择一个最近完成开发任务的会话，扫描其上下文进行蒸馏分析。
              </p>
            </div>

            <div className="form-group">
              <label>最近研发会话历史</label>
              <select 
                className="form-input" 
                value={selectedConvId} 
                onChange={(e) => setSelectedConvId(e.target.value)}
              >
                {conversations.map(c => (
                  <option key={c.id} value={c.id}>
                    {c.title} ({c.active_agent})
                  </option>
                ))}
              </select>
            </div>

            {/* Conversation Log Preview */}
            <div style={{ flex: 1, display: "flex", flexDirection: "column", gap: "8px" }}>
              <label style={{ fontSize: "13px", fontWeight: 500 }}>会话对话摘要</label>
              <div style={{ 
                flex: 1, 
                maxHeight: "260px", 
                overflowY: "auto", 
                background: "rgba(0,0,0,0.3)", 
                border: "1px solid var(--border-color)", 
                borderRadius: "8px", 
                padding: "12px",
                display: "flex",
                flexDirection: "column",
                gap: "10px"
              }}>
                {convMessages.map((msg, i) => (
                  <div key={i} style={{ display: "flex", flexDirection: "column", gap: "4px" }}>
                    <span style={{ 
                      fontSize: "11px", 
                      color: msg.role === "user" ? "var(--color-secondary)" : "var(--color-success)",
                      fontWeight: 600,
                      textTransform: "uppercase"
                    }}>
                      {msg.role === "user" ? "👤 Developer" : "🤖 Agent"}
                    </span>
                    <p style={{ margin: 0, fontSize: "13px", color: "var(--text-secondary)", lineHeight: "1.4" }}>
                      {msg.content}
                    </p>
                  </div>
                ))}
              </div>
            </div>

            <button 
              className="btn btn-primary"
              disabled={isDistilling || !selectedConvId}
              onClick={handleDistillExperience}
              style={{ width: "100%", display: "flex", justifyContent: "center", alignItems: "center", gap: "8px" }}
            >
              {isDistilling ? (
                <>
                  <RefreshCw className="animate-spin" size={16} />
                  正在深度扫描 Timeline 并蒸馏避坑教训...
                </>
              ) : (
                <>
                  <Sparkles size={16} />
                  一键扫描并提炼新踩坑教训
                </>
              )}
            </button>
          </div>

          {/* Distilled Card Suggestion Preview */}
          <div className="card" style={{ display: "flex", flexDirection: "column", justifyContent: "space-between" }}>
            <div style={{ borderBottom: "1px solid var(--border-color)", paddingBottom: "12px", marginBottom: "16px" }}>
              <h3 style={{ margin: 0, fontSize: "16px", display: "flex", alignItems: "center", gap: "8px" }}>
                <Zap size={18} style={{ color: "var(--color-warning)" }} />
                蒸馏卡片预览 (Distilled Card Preview)
              </h3>
              <p style={{ color: "var(--text-secondary)", fontSize: "12px", margin: "4px 0 0 0" }}>
                大模型扫描提取的历史开发事故。您可以修改并确认存入防错库。
              </p>
            </div>

            {distilledSuggestion ? (
              <div style={{ display: "flex", flexDirection: "column", gap: "16px", flex: 1 }}>
                
                <div className="form-group">
                  <label>踩坑事故描述 (Incident)</label>
                  <input 
                    type="text" 
                    className="form-input" 
                    value={distilledSuggestion.incident_desc}
                    onChange={(e) => setDistilledSuggestion({...distilledSuggestion, incident_desc: e.target.value})}
                  />
                </div>

                <div className="form-group">
                  <label>危险模式/触发命令 (Risky Pattern)</label>
                  <input 
                    type="text" 
                    className="form-input" 
                    value={distilledSuggestion.code_pattern}
                    onChange={(e) => setDistilledSuggestion({...distilledSuggestion, code_pattern: e.target.value})}
                    style={{ fontFamily: "var(--font-mono)" }}
                  />
                </div>

                <div className="form-group">
                  <label>避坑安全方案 (Remediation)</label>
                  <textarea 
                    className="form-input" 
                    value={distilledSuggestion.remediation}
                    onChange={(e) => setDistilledSuggestion({...distilledSuggestion, remediation: e.target.value})}
                    style={{ height: "80px", resize: "none" }}
                  />
                </div>

                <div className="form-group">
                  <label>标签/工程规范分类 (Keywords)</label>
                  <input 
                    type="text" 
                    className="form-input" 
                    value={distilledSuggestion.keywords}
                    onChange={(e) => setDistilledSuggestion({...distilledSuggestion, keywords: e.target.value})}
                    placeholder="cors,fetch,lock,deadlock"
                  />
                </div>

                <button 
                  className="btn"
                  onClick={handleSaveDistilledMemory}
                  style={{ width: "100%", marginTop: "10px", background: "linear-gradient(135deg, var(--color-success) 0%, #15803d 100%)", borderColor: "var(--color-success)" }}
                >
                  <Check size={16} style={{ marginRight: "6px" }} />
                  确认存入长期记忆库，并在后续启动中生效
                </button>
              </div>
            ) : (
              <div style={{ flex: 1, display: "flex", flexDirection: "column", alignItems: "center", justifyContent: "center", color: "var(--text-muted)", border: "2px dashed var(--border-color)", borderRadius: "8px", padding: "40px" }}>
                <Brain size={48} style={{ color: "var(--border-color)", marginBottom: "16px" }} />
                <span>等待提取会话，点击左侧「一键扫描并提炼」按钮开始</span>
              </div>
            )}

          </div>

        </div>
      )}

      {/* C. POPUP FORM FOR ADDING/EDITING MEMORY */}
      {isFormOpen && (
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
              width: "480px", 
              background: "rgba(20, 20, 25, 0.95)",
              border: "1px solid var(--border-color)",
              boxShadow: "0 10px 30px rgba(0,0,0,0.5)"
            }}
          >
            <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", borderBottom: "1px solid var(--border-color)", paddingBottom: "12px", marginBottom: "16px" }}>
              <h3 style={{ margin: 0, fontSize: "16px", display: "flex", alignItems: "center", gap: "8px" }}>
                <Brain size={18} style={{ color: "var(--color-danger)" }} />
                {formId ? "编辑避坑记忆卡片" : "新建避坑记忆卡片"}
              </h3>
              <button 
                onClick={() => setIsFormOpen(false)}
                style={{ background: "transparent", border: "none", color: "var(--text-secondary)", cursor: "pointer" }}
              >
                <X size={18} />
              </button>
            </div>

            <form onSubmit={handleSaveMemory} style={{ display: "flex", flexDirection: "column", gap: "14px" }}>
              <div className="form-group">
                <label>踩坑事故描述 (Incident Title)</label>
                <input 
                  type="text" 
                  className="form-input"
                  placeholder="例如：跨域请求中 credentials 导致预检拦截"
                  value={formDesc}
                  onChange={(e) => setFormDesc(e.target.value)}
                  required
                />
              </div>

              <div className="form-group">
                <label>危险模式/触发命令 (Risky snippet / command)</label>
                <input 
                  type="text" 
                  className="form-input" 
                  style={{ fontFamily: "var(--font-mono)" }}
                  placeholder="例如：fetch(url, { credentials: 'include' })"
                  value={formPattern}
                  onChange={(e) => setFormPattern(e.target.value)}
                  required
                />
              </div>

              <div className="form-group">
                <label>安全修复规约 (Remediation Rule)</label>
                <textarea 
                  className="form-input" 
                  style={{ height: "100px", resize: "none" }}
                  placeholder="例如：当 credentials 设为 include 时，Access-Control-Allow-Origin 不能使用通配符 *，必须指定具体 Origin..."
                  value={formRemediation}
                  onChange={(e) => setFormRemediation(e.target.value)}
                  required
                />
              </div>

              <div className="form-group">
                <label>分类标签 (Keywords, 逗号分隔)</label>
                <input 
                  type="text" 
                  className="form-input" 
                  placeholder="例如：cors,fetch,credentials,web"
                  value={formKeywords}
                  onChange={(e) => setFormKeywords(e.target.value)}
                />
              </div>

              <div style={{ display: "flex", gap: "10px", marginTop: "10px", justifyContent: "flex-end" }}>
                <button type="button" className="btn btn-secondary" onClick={() => setIsFormOpen(false)}>
                  取消
                </button>
                <button type="submit" className="btn btn-primary">
                  保存并写入防错库
                </button>
              </div>
            </form>
          </div>
        </div>
      )}
    </div>
  );
}

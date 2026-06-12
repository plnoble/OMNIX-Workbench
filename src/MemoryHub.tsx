import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  Brain, Trash2, Plus, Search, RefreshCw,
  Sparkles, AlertTriangle, Zap, Check, X, Code
} from "lucide-react";
import { cn } from "@/lib/utils";
import { toast } from "@/components/ui/sonner";

interface Memory {
  id: string;
  incident_desc: string;
  code_pattern: string;
  remediation: string;
  keywords: string;
  type: string; // 'preference' | 'experience'
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
  }, []); // eslint-disable-line react-hooks/exhaustive-deps -- mount-only data fetch

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
        handleSelectConversation(list[0].id);
      }
    } catch (e) {
      console.error("Failed to load conversations:", e);
    }
  };

  const handleSaveMemory = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!formDesc.trim() || !formPattern.trim() || !formRemediation.trim()) {
      toast.warning("请填写完整的记忆防错卡片信息");
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
      toast.error("保存失败：" + err);
    }
  };

  const resetForm = () => {
    setFormId("");
    setFormDesc("");
    setFormPattern("");
    setFormRemediation("");
    setFormKeywords("");
  };

  const [pendingDeleteId, setPendingDeleteId] = useState<string | null>(null);

  const handleDeleteMemory = async (id: string) => {
    setPendingDeleteId(id);
  };

  const confirmDeleteMemory = async () => {
    if (!pendingDeleteId) return;
    try {
      await invoke("delete_memory", { id: pendingDeleteId });
      await loadMemories();
    } catch (e) {
      console.error("Failed to delete memory:", e);
      toast.error("删除失败：" + e);
    } finally {
      setPendingDeleteId(null);
    }
  };

  const handleSelectConversation = async (id: string) => {
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

  // Note: Removed redundant useEffect([selectedConvId]) that called handleSelectConversation
  // which itself calls setSelectedConvId — creating a cycle risk. Selection is handled by onClick.

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
      toast.error("经验蒸馏失败 (请确保有网络连接且已配置有效的大模型账号凭证)：" + e);
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
      toast.success("经验蒸馏成果已成功归档至长期防错记忆库！");
      setDistilledSuggestion(null);
      await loadMemories();
      setSelectedTab("memories");
    } catch (e) {
      console.error("Failed to save distilled memory:", e);
      toast.error("归档失败：" + e);
    }
  };

  const filteredMemories = memories.filter(m =>
    m.incident_desc.toLowerCase().includes(searchQuery.toLowerCase()) ||
    m.code_pattern.toLowerCase().includes(searchQuery.toLowerCase()) ||
    m.remediation.toLowerCase().includes(searchQuery.toLowerCase()) ||
    m.keywords.toLowerCase().includes(searchQuery.toLowerCase())
  );

  return (
    <div className="memory-hub-container flex flex-col gap-5 h-full px-1">
      {/* Page header */}
      <div className="border-b border-border pb-3">
        <h2 className="m-0 text-lg font-semibold text-foreground flex items-center gap-2">
          <Brain className="text-red-500" size={20} />
          长期避坑记忆库
        </h2>
        <p className="text-sm text-muted-foreground mt-1 m-0">
          沉淀踩坑教训，让 AI 在后续开发中自动规避重复错误。支持手动归档与从历史会话蒸馏。
        </p>
      </div>

      {/* Sub tabs */}
      <div className="flex gap-2">
        <button
          className={cn(
            "flex items-center gap-2 px-4 py-2 text-sm font-medium rounded-lg cursor-pointer transition-all border",
            selectedTab === "memories"
              ? "bg-accent text-accent-foreground border-accent shadow-sm"
              : "bg-muted/10 text-foreground border-border hover:bg-muted/20"
          )}
          onClick={() => setSelectedTab("memories")}
        >
          <Brain size={16} />
          避坑记忆库 ({memories.length})
        </button>
        <button
          className={cn(
            "flex items-center gap-2 px-4 py-2 text-sm font-medium rounded-lg cursor-pointer transition-all border",
            selectedTab === "distill"
              ? "bg-accent text-accent-foreground border-accent shadow-sm"
              : "bg-muted/10 text-foreground border-border hover:bg-muted/20"
          )}
          onClick={() => setSelectedTab("distill")}
        >
          <Sparkles size={16} />
          经验蒸馏中枢
        </button>
      </div>

      {/* A. MEMORIES LIST TAB */}
      {selectedTab === "memories" && (
        <div className="flex flex-col gap-4">
          <div className="flex gap-3 items-center">
            <div className="relative flex-1">
              <input
                type="text"
                className="w-full pl-10 pr-3 py-2.5 text-sm bg-muted/10 border border-border rounded-lg text-foreground placeholder:text-muted-foreground focus:outline-none focus:border-accent"
                placeholder="搜索防错记忆（关键词、踩坑事故、危险模式...）"
                value={searchQuery}
                onChange={(e) => setSearchQuery(e.target.value)}
              />
              <Search size={16} className="absolute left-3 top-1/2 -translate-y-1/2 text-muted-foreground" />
            </div>

            <button
              className="btn btn-primary flex items-center gap-1.5 px-4 py-2.5 text-sm"
              onClick={() => { resetForm(); setIsFormOpen(true); }}
            >
              <Plus size={16} />
              添加避坑记忆
            </button>
          </div>

          {/* Memories grid */}
          <div className="grid grid-cols-[repeat(auto-fill,minmax(340px,1fr))] gap-4 max-h-[60vh] overflow-y-auto pr-1">
            {filteredMemories.map(m => (
              <div
                key={m.id}
                className="card animate-fade-in relative border-l-4 border-l-red-500 bg-card flex flex-col justify-between gap-3 p-4 shadow-sm hover:shadow-md transition-shadow"
              >
                <div>
                  <div className="flex justify-between items-start mb-3 gap-2">
                    <div className="flex items-center gap-2 flex-1 min-w-0">
                      {m.type === "preference" ? (
                        <span className="text-xs px-2 py-0.5 rounded-md bg-blue-500/10 text-blue-500 dark:text-blue-400 font-medium shrink-0">偏好</span>
                      ) : (
                        <span className="text-xs px-2 py-0.5 rounded-md bg-red-500/10 text-red-500 dark:text-red-400 font-medium shrink-0">经验</span>
                      )}
                      <h4 className="m-0 text-base font-semibold text-foreground truncate">{m.incident_desc}</h4>
                    </div>

                    <div className="flex gap-1 shrink-0">
                      <button
                        className="p-1.5 rounded-md text-muted-foreground bg-transparent border-none cursor-pointer hover:text-foreground hover:bg-muted/20 transition-colors"
                        title="编辑"
                        onClick={() => {
                          setFormId(m.id);
                          setFormDesc(m.incident_desc);
                          setFormPattern(m.code_pattern);
                          setFormRemediation(m.remediation);
                          setFormKeywords(m.keywords);
                          setIsFormOpen(true);
                        }}
                      >
                        ✏️
                      </button>
                      <button
                        className="p-1.5 rounded-md text-muted-foreground bg-transparent border-none cursor-pointer hover:text-destructive hover:bg-destructive/10 transition-colors"
                        title="删除"
                        onClick={() => handleDeleteMemory(m.id)}
                      >
                        <Trash2 size={14} />
                      </button>
                    </div>
                  </div>

                  <div className="text-sm space-y-2 mb-2">
                    <div className="font-mono bg-muted/15 px-3 py-2 rounded-md border border-border">
                      <span className="text-red-500 dark:text-red-400 text-xs block uppercase font-semibold mb-1 tracking-wide">⚠ 危险模式</span>
                      <code className="text-sm text-foreground break-all">{m.code_pattern}</code>
                    </div>
                    <div className="bg-muted/8 p-3 rounded-md border border-border">
                      <span className="text-emerald-500 dark:text-emerald-400 text-xs block uppercase font-semibold mb-1 tracking-wide">✓ 安全方案</span>
                      <p className="text-sm text-foreground m-0 leading-relaxed">{m.remediation}</p>
                    </div>
                  </div>
                </div>

                <div className="flex flex-wrap gap-1.5 border-t border-border pt-3">
                  {m.keywords.split(",").map(kw => (
                    <span
                      key={kw}
                      className="text-xs bg-red-500/8 text-red-500 dark:text-red-400 px-2 py-0.5 rounded-md border border-red-500/15"
                    >
                      {kw.trim()}
                    </span>
                  ))}
                </div>
              </div>
            ))}

            {filteredMemories.length === 0 && (
              <div className="col-span-full text-center p-10 text-muted-foreground bg-muted/10 border border-dashed border-border rounded-lg text-sm">
                没有找到匹配的避坑卡片，请尝试其他关键词。
              </div>
            )}
          </div>
        </div>
      )}

      {/* B. EXPERIENCE DISTILLATION TAB */}
      {selectedTab === "distill" && (
        <div className="settings-grid grid-cols-2 gap-5">

          {/* Timeline & Select Session */}
          <div className="card flex flex-col gap-4">
            <div className="border-b border-border pb-3">
              <h3 className="m-0 text-base flex items-center gap-2">
                <Code size={18} className="text-blue-500" />
                选择研发 Timeline 会话
              </h3>
              <p className="text-secondary-foreground text-xs mt-1">
                选择一个最近完成开发任务的会话，扫描其上下文进行蒸馏分析。
              </p>
            </div>

            <div className="form-group">
              <label>最近研发会话历史</label>
              <select
                className="form-input"
                value={selectedConvId}
                onChange={(e) => {
                  const val = e.target.value;
                  setSelectedConvId(val);
                  handleSelectConversation(val);
                }}
              >
                {conversations.map(c => (
                  <option key={c.id} value={c.id}>
                    {c.title} ({c.active_agent})
                  </option>
                ))}
              </select>
            </div>

            {/* Conversation Log Preview */}
            <div className="flex-1 flex flex-col gap-2">
              <label className="text-sm font-medium">会话对话摘要</label>
              <div className="flex-1 max-h-[260px] overflow-y-auto bg-muted/10 border border-border rounded-lg p-3 flex flex-col gap-2.5">
                {convMessages.map((msg, i) => (
                  <div key={i} className="flex flex-col gap-1">
                    <span className={cn(
                      "text-xs font-semibold uppercase",
                      msg.role === "user" ? "text-blue-500" : "text-emerald-500"
                    )}>
                      {msg.role === "user" ? "👤 Developer" : "🤖 Agent"}
                    </span>
                    <p className="m-0 text-sm text-secondary-foreground leading-snug">
                      {msg.content}
                    </p>
                  </div>
                ))}
              </div>
            </div>

            <button
              className="btn btn-primary w-full flex justify-center items-center gap-2"
              disabled={isDistilling || !selectedConvId}
              onClick={handleDistillExperience}
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
          <div className="card flex flex-col justify-between">
            <div className="border-b border-border pb-3 mb-4">
              <h3 className="m-0 text-base flex items-center gap-2">
                <Zap size={18} className="text-amber-500" />
                蒸馏卡片预览 (Distilled Card Preview)
              </h3>
              <p className="text-secondary-foreground text-xs mt-1">
                大模型扫描提取的历史开发事故。您可以修改并确认存入防错库。
              </p>
            </div>

            {distilledSuggestion ? (
              <div className="flex flex-col gap-4 flex-1">

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
                    className="form-input font-mono"
                    value={distilledSuggestion.code_pattern}
                    onChange={(e) => setDistilledSuggestion({...distilledSuggestion, code_pattern: e.target.value})}
                  />
                </div>

                <div className="form-group">
                  <label>避坑安全方案 (Remediation)</label>
                  <textarea
                    className="form-input h-20 resize-none"
                    value={distilledSuggestion.remediation}
                    onChange={(e) => setDistilledSuggestion({...distilledSuggestion, remediation: e.target.value})}
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
                  className="btn w-full mt-2.5 bg-gradient-to-br from-emerald-500 to-[#15803d] border-emerald-500"
                  onClick={handleSaveDistilledMemory}
                >
                  <Check size={16} className="mr-1.5" />
                  确认存入长期记忆库，并在后续启动中生效
                </button>
              </div>
            ) : (
              <div className="flex-1 flex flex-col items-center justify-center text-muted-foreground border-2 border-dashed border-border rounded-lg p-10">
                <Brain size={48} className="text-[var(--border-color)] mb-4" />
                <span>等待提取会话，点击左侧「一键扫描并提炼」按钮开始</span>
              </div>
            )}

          </div>

        </div>
      )}

      {/* C. POPUP FORM FOR ADDING/EDITING MEMORY */}
      {isFormOpen && (
        <div className="fixed inset-0 bg-black/60 backdrop-blur-sm flex items-center justify-center z-[1000]">
          <div
            className="card animate-fade-in w-[480px] bg-card border border-border shadow-[0_10px_30px_rgba(0,0,0,0.3)] p-5"
          >
            <div className="flex justify-between items-center border-b border-border pb-3 mb-4">
              <h3 className="m-0 text-base flex items-center gap-2">
                <Brain size={18} className="text-red-500" />
                {formId ? "编辑避坑记忆卡片" : "新建避坑记忆卡片"}
              </h3>
              <button
                className="bg-transparent border-none text-muted-foreground cursor-pointer hover:text-foreground"
                onClick={() => setIsFormOpen(false)}
              >
                <X size={18} />
              </button>
            </div>

            <form onSubmit={handleSaveMemory} className="flex flex-col gap-3.5">
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
                  className="form-input font-mono"
                  placeholder="例如：fetch(url, { credentials: 'include' })"
                  value={formPattern}
                  onChange={(e) => setFormPattern(e.target.value)}
                  required
                />
              </div>

              <div className="form-group">
                <label>安全修复规约 (Remediation Rule)</label>
                <textarea
                  className="form-input h-[100px] resize-none"
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

              <div className="flex gap-2.5 mt-2.5 justify-end">
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

      {/* D. CONFIRM DELETE DIALOG */}
      {pendingDeleteId && (
        <div className="fixed inset-0 bg-black/60 backdrop-blur-sm flex items-center justify-center z-[1000]">
          <div className="card animate-fade-in w-[400px] bg-card border border-border shadow-[0_10px_30px_rgba(0,0,0,0.3)] p-5">
            <div className="flex items-center gap-2 mb-4">
              <AlertTriangle size={18} className="text-amber-500" />
              <h3 className="m-0 text-base">确认删除防错记忆</h3>
            </div>
            <p className="text-sm text-secondary-foreground mb-5">
              确定要让 AI 遗忘这条防错记忆吗？遗忘后启动 Agent 将不再自动提示。
            </p>
            <div className="flex gap-2.5 justify-end">
              <button className="btn btn-secondary" onClick={() => setPendingDeleteId(null)}>
                取消
              </button>
              <button className="btn bg-red-600 border-red-600 hover:bg-red-700" onClick={confirmDeleteMemory}>
                确认删除
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

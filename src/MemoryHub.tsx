import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  AlertTriangle,
  Brain,
  Check,
  Code2,
  FileDiff,
  Plus,
  Search,
  Sparkles,
  Trash2,
  X,
} from "lucide-react";

import { Button } from "@/components/ui/button";
import { toast } from "@/components/ui/sonner";
import { cn } from "@/lib/utils";
import {
  conversationApi,
  distillationApi,
  modelApi,
  type DistillationCandidate,
} from "@/lib/tauri-api";
import type { ConversationInfo, ConversationMessage, PlatformModel } from "@/types";

interface MemoryRecord {
  id: string;
  incident_desc: string;
  code_pattern: string;
  remediation: string;
  keywords: string;
  created_at: string;
}

type InboxFilter = "pending" | "approved" | "rejected" | "all";

const candidateLabels: Record<DistillationCandidate["candidate_type"], string> = {
  memory: "防错记忆",
  skill: "技能草案",
  protocol: "协议建议",
};

function safeJson(value: string): Record<string, unknown> {
  try {
    return JSON.parse(value) as Record<string, unknown>;
  } catch {
    return {};
  }
}

export function MemoryHub() {
  const [activeView, setActiveView] = useState<"memory" | "inbox">("memory");
  const [memories, setMemories] = useState<MemoryRecord[]>([]);
  const [query, setQuery] = useState("");
  const [showCreate, setShowCreate] = useState(false);
  const [form, setForm] = useState({ incident: "", pattern: "", remediation: "", keywords: "" });

  const [conversations, setConversations] = useState<ConversationInfo[]>([]);
  const [selectedConversation, setSelectedConversation] = useState("");
  const [messages, setMessages] = useState<ConversationMessage[]>([]);
  const [models, setModels] = useState<PlatformModel[]>([]);
  const [selectedModel, setSelectedModel] = useState("");
  const [candidates, setCandidates] = useState<DistillationCandidate[]>([]);
  const [filter, setFilter] = useState<InboxFilter>("pending");
  const [isGenerating, setIsGenerating] = useState(false);
  const [reviewingId, setReviewingId] = useState("");

  const loadMemories = async () => {
    setMemories(await invoke<MemoryRecord[]>("get_all_memories"));
  };

  const loadInbox = async (nextFilter = filter) => {
    setCandidates(await distillationApi.list(nextFilter));
  };

  useEffect(() => {
    Promise.all([loadMemories(), conversationApi.list(), modelApi.getActive(), distillationApi.list("pending")])
      .then(([, conversationList, modelList, inbox]) => {
        setConversations(conversationList);
        setModels(modelList);
        setCandidates(inbox);
        if (conversationList[0]) setSelectedConversation(conversationList[0].id);
        if (modelList[0]) setSelectedModel(`${modelList[0].platform_id}:${modelList[0].model_name}`);
      })
      .catch((error) => toast.error(`读取记忆数据失败：${error}`));
  }, []);

  useEffect(() => {
    if (!selectedConversation) {
      setMessages([]);
      return;
    }
    conversationApi.getMessages(selectedConversation)
      .then(setMessages)
      .catch((error) => toast.error(`读取会话失败：${error}`));
  }, [selectedConversation]);

  const filteredMemories = useMemo(() => {
    const needle = query.trim().toLowerCase();
    if (!needle) return memories;
    return memories.filter((memory) =>
      [memory.incident_desc, memory.code_pattern, memory.remediation, memory.keywords]
        .some((value) => value.toLowerCase().includes(needle)),
    );
  }, [memories, query]);

  const createMemory = async () => {
    if (!form.incident.trim() || !form.remediation.trim()) {
      toast.warning("请填写事件说明和修复方案");
      return;
    }
    await invoke("create_memory", {
      id: `memory_${Date.now()}`,
      incidentDesc: form.incident,
      codePattern: form.pattern,
      remediation: form.remediation,
      keywords: form.keywords,
      memType: "experience",
    });
    setForm({ incident: "", pattern: "", remediation: "", keywords: "" });
    setShowCreate(false);
    await loadMemories();
    toast.success("记忆已保存");
  };

  const deleteMemory = async (id: string) => {
    await invoke("delete_memory", { id });
    await loadMemories();
  };

  const generateCandidates = async () => {
    if (!selectedConversation || !selectedModel) {
      toast.warning("请选择真实会话和蒸馏模型");
      return;
    }
    setIsGenerating(true);
    try {
      const created = await distillationApi.generate(selectedConversation, selectedModel);
      setFilter("pending");
      await loadInbox("pending");
      toast.success(`已生成 ${created.length} 个待审候选`);
    } catch (error) {
      toast.error(`蒸馏失败：${error}`);
    } finally {
      setIsGenerating(false);
    }
  };

  const reviewCandidate = async (candidateId: string, approved: boolean) => {
    setReviewingId(candidateId);
    try {
      await distillationApi.review(candidateId, approved);
      await Promise.all([loadInbox(), loadMemories()]);
      toast.success(approved ? "候选已批准" : "候选已拒绝");
    } catch (error) {
      toast.error(`处理候选失败：${error}`);
    } finally {
      setReviewingId("");
    }
  };

  return (
    <div className="h-full overflow-y-auto bg-background">
      <div className="mx-auto max-w-6xl px-6 py-7">
        <header className="border-b border-border pb-5">
          <div className="flex items-center gap-2">
            <Brain className="h-5 w-5 text-primary" />
            <h2 className="text-xl font-semibold">经验与记忆</h2>
          </div>
          <p className="mt-2 text-sm text-muted-foreground">
            保留有证据的开发经验。模型只生成候选，是否进入长期记忆、技能库或项目协议由你决定。
          </p>
        </header>

        <div className="mt-5 flex gap-1 border-b border-border">
          {([ ["memory", "长期记忆"], ["inbox", "蒸馏收件箱"] ] as const).map(([value, label]) => (
            <button
              key={value}
              type="button"
              onClick={() => setActiveView(value)}
              className={cn(
                "border-b-2 px-4 py-2 text-sm font-medium",
                activeView === value ? "border-primary text-foreground" : "border-transparent text-muted-foreground",
              )}
            >
              {label}
            </button>
          ))}
        </div>

        {activeView === "memory" ? (
          <section className="py-5">
            <div className="flex flex-wrap items-center justify-between gap-3">
              <div className="relative min-w-64 flex-1 max-w-xl">
                <Search className="absolute left-3 top-2.5 h-4 w-4 text-muted-foreground" />
                <input
                  value={query}
                  onChange={(event) => setQuery(event.target.value)}
                  placeholder="搜索事件、危险模式或标签"
                  className="h-9 w-full rounded-md border border-border bg-background pl-9 pr-3 text-sm"
                />
              </div>
              <Button onClick={() => setShowCreate((value) => !value)}>
                <Plus className="h-4 w-4" /> 新建记忆
              </Button>
            </div>

            {showCreate && (
              <div className="mt-5 border-y border-border bg-muted/20 py-5">
                <div className="grid gap-4 md:grid-cols-2">
                  <label className="text-sm">事件说明
                    <textarea className="mt-1 min-h-24 w-full rounded-md border border-border bg-background p-3" value={form.incident} onChange={(event) => setForm({ ...form, incident: event.target.value })} />
                  </label>
                  <label className="text-sm">危险模式
                    <textarea className="mt-1 min-h-24 w-full rounded-md border border-border bg-background p-3 font-mono text-xs" value={form.pattern} onChange={(event) => setForm({ ...form, pattern: event.target.value })} />
                  </label>
                  <label className="text-sm">修复方案
                    <textarea className="mt-1 min-h-24 w-full rounded-md border border-border bg-background p-3" value={form.remediation} onChange={(event) => setForm({ ...form, remediation: event.target.value })} />
                  </label>
                  <label className="text-sm">标签
                    <input className="mt-1 h-9 w-full rounded-md border border-border bg-background px-3" value={form.keywords} onChange={(event) => setForm({ ...form, keywords: event.target.value })} placeholder="rust, async, deadlock" />
                  </label>
                </div>
                <div className="mt-4 flex justify-end gap-2">
                  <Button variant="outline" onClick={() => setShowCreate(false)}>取消</Button>
                  <Button onClick={createMemory}>保存</Button>
                </div>
              </div>
            )}

            <div className="mt-5 grid gap-3 lg:grid-cols-2">
              {filteredMemories.map((memory) => (
                <article key={memory.id} className="rounded-lg border border-border p-4">
                  <div className="flex items-start justify-between gap-3">
                    <h3 className="text-sm font-semibold leading-6">{memory.incident_desc}</h3>
                    <button type="button" className="p-1 text-muted-foreground hover:text-destructive" title="删除记忆" onClick={() => deleteMemory(memory.id)}>
                      <Trash2 className="h-4 w-4" />
                    </button>
                  </div>
                  {memory.code_pattern && <pre className="mt-3 overflow-x-auto rounded-md bg-muted p-3 text-xs">{memory.code_pattern}</pre>}
                  <p className="mt-3 text-sm leading-6 text-muted-foreground">{memory.remediation}</p>
                  <div className="mt-3 text-xs text-muted-foreground">{memory.keywords || "无标签"}</div>
                </article>
              ))}
            </div>
            {filteredMemories.length === 0 && <div className="py-16 text-center text-sm text-muted-foreground">暂无长期记忆</div>}
          </section>
        ) : (
          <section className="py-5">
            <div className="grid gap-4 lg:grid-cols-[minmax(0,1fr)_minmax(280px,0.7fr)]">
              <div>
                <label className="text-sm font-medium">来源会话</label>
                <select className="mt-2 h-10 w-full rounded-md border border-border bg-background px-3 text-sm" value={selectedConversation} onChange={(event) => setSelectedConversation(event.target.value)}>
                  <option value="">选择有真实历史的会话</option>
                  {conversations.map((conversation) => <option key={conversation.id} value={conversation.id}>{conversation.title} · {conversation.active_agent}</option>)}
                </select>
                <label className="mt-4 block text-sm font-medium">蒸馏模型</label>
                <select className="mt-2 h-10 w-full rounded-md border border-border bg-background px-3 text-sm" value={selectedModel} onChange={(event) => setSelectedModel(event.target.value)}>
                  <option value="">选择 Models 中已启用的模型</option>
                  {models.map((model) => <option key={model.id} value={`${model.platform_id}:${model.model_name}`}>{model.model_name} · {model.platform_id}</option>)}
                </select>
                <Button className="mt-4" onClick={generateCandidates} disabled={isGenerating || !selectedConversation || !selectedModel}>
                  <Sparkles className="h-4 w-4" /> {isGenerating ? "正在蒸馏" : "生成候选"}
                </Button>
              </div>
              <div className="border-l border-border pl-4">
                <div className="text-sm font-medium">会话证据预览</div>
                <div className="mt-2 max-h-48 space-y-2 overflow-y-auto text-xs text-muted-foreground">
                  {messages.slice(-8).map((message) => <p key={message.id}><strong>{message.role}</strong>：{message.content.slice(0, 180)}</p>)}
                  {messages.length === 0 && <p>尚无可用消息。</p>}
                </div>
              </div>
            </div>

            <div className="mt-7 flex flex-wrap items-center justify-between gap-3 border-t border-border pt-5">
              <h3 className="font-semibold">候选队列</h3>
              <div className="flex gap-1">
                {([ ["pending", "待审"], ["approved", "已批准"], ["rejected", "已拒绝"], ["all", "全部"] ] as const).map(([value, label]) => (
                  <button key={value} type="button" onClick={() => { setFilter(value); loadInbox(value); }} className={cn("rounded-md px-3 py-1.5 text-xs", filter === value ? "bg-primary text-primary-foreground" : "bg-muted text-muted-foreground")}>{label}</button>
                ))}
              </div>
            </div>

            <div className="mt-4 space-y-3">
              {candidates.map((candidate) => {
                const payload = safeJson(candidate.payload_json);
                const Icon = candidate.candidate_type === "memory" ? AlertTriangle : candidate.candidate_type === "skill" ? Code2 : FileDiff;
                return (
                  <article key={candidate.id} className="rounded-lg border border-border p-4">
                    <div className="flex flex-wrap items-start justify-between gap-3">
                      <div>
                        <div className="flex items-center gap-2 text-xs text-muted-foreground"><Icon className="h-4 w-4" /> {candidateLabels[candidate.candidate_type]}</div>
                        <h4 className="mt-1 font-semibold">{candidate.title}</h4>
                        <p className="mt-2 text-sm leading-6 text-muted-foreground">{candidate.summary}</p>
                      </div>
                      <span className="rounded-full border border-border px-2 py-1 text-xs">{candidate.status}</span>
                    </div>
                    <pre className="mt-3 max-h-56 overflow-auto rounded-md bg-muted p-3 text-xs whitespace-pre-wrap">{JSON.stringify(payload, null, 2)}</pre>
                    <div className="mt-3 text-xs text-muted-foreground">证据消息：{candidate.evidence_json}</div>
                    {candidate.status === "pending" && (
                      <div className="mt-4 flex justify-end gap-2">
                        <Button variant="outline" disabled={reviewingId === candidate.id} onClick={() => reviewCandidate(candidate.id, false)}><X className="h-4 w-4" /> 拒绝</Button>
                        <Button disabled={reviewingId === candidate.id} onClick={() => reviewCandidate(candidate.id, true)}><Check className="h-4 w-4" /> 批准</Button>
                      </div>
                    )}
                  </article>
                );
              })}
              {candidates.length === 0 && <div className="py-16 text-center text-sm text-muted-foreground">当前筛选条件下没有候选</div>}
            </div>
          </section>
        )}
      </div>
    </div>
  );
}

import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  AlertTriangle,
  Brain,
  Check,
  Code2,
  Combine,
  FileDiff,
  FolderOpen,
  GraduationCap,
  Plus,
  RefreshCw,
  Search,
  Sparkles,
  Trash2,
  X,
} from "lucide-react";

import { Button } from "@/components/ui/button";
import { toast } from "@/components/ui/sonner";
import { cn } from "@/lib/utils";
import { EvolutionPanel } from "@/components/EvolutionPanel";
import {
  conversationApi,
  distillationApi,
  evolutionApi,
  modelApi,
  projectProtocolApi,
  shellApi,
  type DistillationCandidate,
  type LessonsInfo,
} from "@/lib/tauri-api";
import type { ConversationInfo, ConversationMessage, PlatformModel } from "@/types";

interface MemoryRecord {
  id: string;
  incident_desc: string;
  code_pattern: string;
  remediation: string;
  keywords: string;
  created_at: string;
  confidence?: number;
  seen_count?: number;
  repeated_count?: number;
  status?: string;
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
  const [activeView, setActiveView] = useState<"memory" | "inbox" | "lessons" | "evolution">("memory");
  const [memories, setMemories] = useState<MemoryRecord[]>([]);
  const [lessons, setLessons] = useState<LessonsInfo | null>(null);
  const [maintaining, setMaintaining] = useState(false);
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

  const loadLessons = async () => {
    try {
      setLessons(await evolutionApi.preview());
    } catch {
      /* lessons preview is best-effort */
    }
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
    loadLessons();
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

  const reindexEmbeddings = async () => {
    setMaintaining(true);
    try {
      const n = await evolutionApi.reindex();
      toast.success(n > 0 ? `已为 ${n} 条经验建立语义索引` : "经验索引已是最新");
      await loadLessons();
    } catch (error) {
      toast.error(`重建索引失败：${error}`);
    } finally {
      setMaintaining(false);
    }
  };

  const consolidate = async () => {
    setMaintaining(true);
    try {
      const n = await evolutionApi.consolidate();
      toast.success(n > 0 ? `已合并 ${n} 条近似重复经验` : "没有发现可合并的重复经验");
      await Promise.all([loadMemories(), loadLessons()]);
    } catch (error) {
      toast.error(`整理去重失败：${error}`);
    } finally {
      setMaintaining(false);
    }
  };

  const distillExternalWorkspace = async () => {
    if (!selectedModel) {
      toast.warning("请先选择蒸馏模型");
      return;
    }
    let folder: string | null = null;
    try {
      folder = await shellApi.pickDirectory();
    } catch (error) {
      toast.error(`打开文件夹选择器失败：${error}`);
      return;
    }
    if (!folder) return;
    setIsGenerating(true);
    try {
      const created = await distillationApi.generateFromWorkspace(folder, selectedModel);
      setFilter("pending");
      await loadInbox("pending");
      toast.success(`已从外部工作区蒸馏出 ${created.length} 个待审候选`);
    } catch (error) {
      toast.error(`蒸馏外部工作区失败：${error}`);
    } finally {
      setIsGenerating(false);
    }
  };

  const archiveAndDistill = async () => {
    const conv = conversations.find((item) => item.id === selectedConversation);
    if (!conv) {
      toast.warning("请先选择一个会话");
      return;
    }
    if (!conv.workspace_path || conv.workspace_path === "direct") {
      toast.warning("该会话没有关联工作区；可直接用「生成候选」蒸馏对话。");
      return;
    }
    setIsGenerating(true);
    try {
      const run = await projectProtocolApi.archiveAndDistill(conv.workspace_path);
      let llmCount = 0;
      if (selectedModel) {
        const created = await distillationApi.generate(selectedConversation, selectedModel);
        llmCount = created.length;
      }
      setFilter("pending");
      await loadInbox("pending");
      toast.success(
        `已归档项目并蒸馏：协议记忆 ${run.memory_count} 条、协议提案 ${run.proposal_count} 条` +
          (selectedModel ? `、模型候选 ${llmCount} 条` : "") + " 进入待审",
      );
    } catch (error) {
      toast.error(`归档蒸馏失败：${error}`);
    } finally {
      setIsGenerating(false);
    }
  };

  const reviewCandidate = async (candidateId: string, approved: boolean) => {
    setReviewingId(candidateId);
    try {
      await distillationApi.review(candidateId, approved);
      await Promise.all([loadInbox(), loadMemories(), loadLessons()]);
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
          {([
            ["memory", "长期记忆"],
            ["inbox", "蒸馏收件箱"],
            ["lessons", `经验回注${lessons && lessons.count ? ` (${lessons.count})` : ""}`],
            ["evolution", "进化中枢"],
          ] as Array<["memory" | "inbox" | "lessons" | "evolution", string]>).map(([value, label]) => (
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
                  <div className="mt-3 flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
                    <span>{memory.keywords || "无标签"}</span>
                    {(memory.repeated_count ?? 0) > 0 && (
                      <span className="rounded-full bg-destructive/15 px-2 py-0.5 font-medium text-destructive" title="该经验注入后又发生了同类错误，可能需要改写或加强">
                        失效 ×{memory.repeated_count}
                      </span>
                    )}
                  </div>
                </article>
              ))}
            </div>
            {filteredMemories.length === 0 && <div className="py-16 text-center text-sm text-muted-foreground">暂无长期记忆</div>}
          </section>
        ) : activeView === "inbox" ? (
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
                <div className="mt-4 flex flex-wrap gap-2">
                  <Button onClick={generateCandidates} disabled={isGenerating || !selectedConversation || !selectedModel}>
                    <Sparkles className="h-4 w-4" /> {isGenerating ? "正在蒸馏" : "生成候选"}
                  </Button>
                  <Button variant="outline" onClick={archiveAndDistill} disabled={isGenerating || !selectedConversation}>
                    <Brain className="h-4 w-4" /> 结束项目并蒸馏
                  </Button>
                  <Button variant="outline" onClick={distillExternalWorkspace} disabled={isGenerating || !selectedModel}>
                    <FolderOpen className="h-4 w-4" /> 蒸馏外部工作区
                  </Button>
                </div>
                <p className="mt-2 text-xs text-muted-foreground">
                  「结束项目并蒸馏」会把该工作区的协议记录归档为防错记忆与协议提案；若已选模型，还会同时对对话做模型蒸馏。
                  「蒸馏外部工作区」用于本软件之前就已用协议开发的项目——选择该文件夹，直接从它的 .omx/development 记录蒸馏，无需会话。
                </p>
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
        ) : activeView === "lessons" ? (
          <section className="py-5">
            <div className="rounded-lg border border-border bg-muted/20 p-5">
              <div className="flex items-start gap-3">
                <GraduationCap className="mt-0.5 h-5 w-5 text-primary" />
                <div className="flex-1">
                  <h3 className="font-semibold">经验回注 — 让下次开发少犯同样的错</h3>
                  <p className="mt-1 text-sm leading-6 text-muted-foreground">
                    已批准的长期经验（{lessons?.count ?? 0} 条）会在<strong className="text-foreground">每次 Agent 启动时自动注入</strong>到该工作区的上下文文件
                    （Claude Code → <code className="rounded bg-muted px-1">CLAUDE.md</code>、Codex → <code className="rounded bg-muted px-1">AGENTS.md</code>）顶部的
                    <code className="rounded bg-muted px-1">OMNIX MEMORY</code> 受管块里。Agent 会自动加载这段内容——无需手动同步、也不依赖 Agent 主动去读某个文件。
                    这是「开发 → 记录 → 蒸馏 → 下次少犯错」闭环的最后一环。
                  </p>
                  <p className="mt-2 text-xs leading-5 text-muted-foreground">
                    注入时按<strong className="text-foreground">与当前工作区的相关性</strong>排序（栈/语言/标签），只放最相关的前 20 条。
                    「重建索引」为经验建立语义向量；「整理去重」合并近似重复，避免越积越乱。
                  </p>
                </div>
              </div>
              <div className="mt-4 flex flex-wrap items-center gap-2">
                <Button variant="outline" size="sm" onClick={reindexEmbeddings} disabled={maintaining}>
                  <RefreshCw className={cn("h-4 w-4", maintaining && "animate-spin")} /> 重建索引
                </Button>
                <Button variant="outline" size="sm" onClick={consolidate} disabled={maintaining}>
                  <Combine className="h-4 w-4" /> 整理去重
                </Button>
              </div>
            </div>

            <div className="mt-5">
              <div className="text-sm font-medium">将注入到 Agent 的记忆块预览</div>
              <pre className="mt-2 max-h-[420px] overflow-auto rounded-md border border-border bg-muted p-4 text-xs whitespace-pre-wrap">
                {lessons?.content || "暂无内容"}
              </pre>
            </div>
          </section>
        ) : (
          <EvolutionPanel />
        )}
      </div>
    </div>
  );
}

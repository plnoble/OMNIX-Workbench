import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Copy, Search, Star, Wand2, Plus, Download, Upload, Trash2 } from "lucide-react";

import { Button } from "@/components/ui/button";
import { agentTemplateApi, customAssistantApi, type CustomAssistant } from "@/lib/tauri-api";
import type { AgentTemplate } from "@/lib/tauri-api";
import { cn } from "@/lib/utils";
import { toast } from "@/components/ui/sonner";

/** Map a stored custom assistant onto the AgentTemplate shape for uniform rendering. */
function customToTemplate(c: CustomAssistant): AgentTemplate {
  return {
    slug: c.slug, name: c.name, description: c.description,
    category: c.category || "自定义", icon: "✨", accent: "",
    instructions: c.instructions, skills: [],
  };
}

interface AssistantsTabProps {
  onUseTemplate?: (template: AgentTemplate) => void;
}

const FAVORITES_KEY = "omnix.assistantTemplateFavorites";
const FEATURED_SLUGS = [
  "bug-fixer",
  "code-reviewer",
  "frontend-builder",
  "architecture-advisor",
  "git-expert",
  "rca-writer",
  "prd-drafter",
  "summarizer",
  "translator-zh-en",
  "sql-expert",
  "security-auditor",
];

function readFavorites() {
  try {
    return new Set<string>(JSON.parse(localStorage.getItem(FAVORITES_KEY) || "[]"));
  } catch {
    return new Set<string>();
  }
}

function saveFavorites(favorites: Set<string>) {
  localStorage.setItem(FAVORITES_KEY, JSON.stringify(Array.from(favorites)));
}

export function AssistantsTab({ onUseTemplate }: AssistantsTabProps) {
  const [templates, setTemplates] = useState<AgentTemplate[]>([]);
  const [customSlugs, setCustomSlugs] = useState<Set<string>>(new Set());
  const [query, setQuery] = useState("");
  const [category, setCategory] = useState("all");
  const [favorites, setFavorites] = useState<Set<string>>(() => readFavorites());
  const [selectedSlug, setSelectedSlug] = useState<string | null>(null);
  const [showCreate, setShowCreate] = useState(false);
  const [form, setForm] = useState({ name: "", category: "自定义", description: "", instructions: "" });
  const fileInputRef = useRef<HTMLInputElement>(null);

  const loadAll = useCallback(async () => {
    try {
      const [builtins, customs] = await Promise.all([
        agentTemplateApi.getAll(),
        customAssistantApi.list().catch(() => [] as CustomAssistant[]),
      ]);
      setCustomSlugs(new Set(customs.map((c) => c.slug)));
      const ordered = [...builtins].sort((a, b) => {
        const aIndex = FEATURED_SLUGS.indexOf(a.slug);
        const bIndex = FEATURED_SLUGS.indexOf(b.slug);
        if (aIndex >= 0 || bIndex >= 0) return (aIndex < 0 ? 999 : aIndex) - (bIndex < 0 ? 999 : bIndex);
        return a.name.localeCompare(b.name);
      });
      // Custom assistants first, then built-ins.
      const all = [...customs.map(customToTemplate), ...ordered];
      setTemplates(all);
      setSelectedSlug((prev) => prev || all[0]?.slug || null);
    } catch (error) {
      toast.error("加载助手失败：" + error);
    }
  }, []);

  useEffect(() => {
    void loadAll();
  }, [loadAll]);

  const categories = useMemo(
    () => ["all", ...Array.from(new Set(templates.map((template) => template.category))).sort()],
    [templates],
  );

  const filtered = useMemo(() => {
    const keyword = query.trim().toLowerCase();
    return templates.filter((template) => {
      const matchesCategory = category === "all" || template.category === category;
      const matchesQuery =
        !keyword ||
        template.name.toLowerCase().includes(keyword) ||
        template.description.toLowerCase().includes(keyword) ||
        template.instructions.toLowerCase().includes(keyword);
      return matchesCategory && matchesQuery;
    });
  }, [category, query, templates]);

  const selected = templates.find((template) => template.slug === selectedSlug) ?? filtered[0] ?? templates[0];

  const toggleFavorite = (slug: string) => {
    const next = new Set(favorites);
    if (next.has(slug)) next.delete(slug);
    else next.add(slug);
    setFavorites(next);
    saveFavorites(next);
  };

  const copyTemplate = async (template: AgentTemplate) => {
    try {
      await navigator.clipboard.writeText(template.instructions);
      toast.success("提示词已复制");
    } catch (error) {
      toast.error("复制失败", { description: String(error) });
    }
  };

  const createAssistant = async () => {
    if (!form.name.trim() || !form.instructions.trim()) {
      toast.error("请填写名称和提示词");
      return;
    }
    try {
      const saved = await customAssistantApi.save(form);
      setShowCreate(false);
      setForm({ name: "", category: "自定义", description: "", instructions: "" });
      await loadAll();
      setSelectedSlug(saved.slug);
      toast.success("已创建自定义助手");
    } catch (error) {
      toast.error("创建失败", { description: String(error) });
    }
  };

  // Share: export an assistant to a JSON file the user can send to others.
  const exportAssistant = (template: AgentTemplate) => {
    const payload = {
      omnix_assistant: 1,
      name: template.name,
      description: template.description,
      category: template.category,
      instructions: template.instructions,
    };
    const blob = new Blob([JSON.stringify(payload, null, 2)], { type: "application/json" });
    const url = URL.createObjectURL(blob);
    const link = document.createElement("a");
    link.href = url;
    link.download = `${template.slug || "assistant"}.json`;
    link.click();
    URL.revokeObjectURL(url);
    toast.success("已导出助手 JSON");
  };

  const importAssistant = async (file: File) => {
    try {
      const data = JSON.parse(await file.text());
      if (!data.name || !data.instructions) throw new Error("不是有效的 OMNIX 助手文件");
      const saved = await customAssistantApi.save({
        name: String(data.name),
        description: String(data.description ?? ""),
        category: String(data.category ?? "自定义"),
        instructions: String(data.instructions),
      });
      await loadAll();
      setSelectedSlug(saved.slug);
      toast.success(`已导入助手「${saved.name}」`);
    } catch (error) {
      toast.error("导入失败", { description: String(error) });
    }
  };

  const deleteAssistant = async (slug: string) => {
    if (!window.confirm("删除该自定义助手？")) return;
    try {
      await customAssistantApi.remove(slug);
      setSelectedSlug(null);
      await loadAll();
    } catch (error) {
      toast.error("删除失败", { description: String(error) });
    }
  };

  return (
    <div className="flex h-full min-h-0 flex-1 overflow-hidden bg-background">
      <aside className="hidden w-80 shrink-0 border-r border-border glass-surface p-4 lg:flex lg:flex-col">
        <div className="flex items-center justify-between gap-2">
          <div className="flex items-center gap-2 text-base font-semibold">
            <Wand2 className="h-4 w-4 text-primary" />
            助手模板库
          </div>
          <div className="flex items-center gap-1">
            <button onClick={() => { setShowCreate(true); setSelectedSlug(null); }} title="新建自定义助手" className="rounded p-1.5 text-muted-foreground hover:bg-muted/30 hover:text-foreground">
              <Plus className="h-4 w-4" />
            </button>
            <button onClick={() => fileInputRef.current?.click()} title="导入助手 JSON（分享）" className="rounded p-1.5 text-muted-foreground hover:bg-muted/30 hover:text-foreground">
              <Upload className="h-4 w-4" />
            </button>
          </div>
        </div>
        <input
          ref={fileInputRef}
          type="file"
          accept="application/json,.json"
          className="hidden"
          onChange={(e) => {
            const file = e.target.files?.[0];
            if (file) void importAssistant(file);
            e.target.value = "";
          }}
        />
        <div className="mt-4 flex items-center gap-2 rounded-md border border-border bg-background px-2">
          <Search className="h-4 w-4 text-muted-foreground" />
          <input
            className="h-9 min-w-0 flex-1 bg-transparent text-sm outline-none"
            placeholder="搜索助手..."
            value={query}
            onChange={(event) => setQuery(event.target.value)}
          />
        </div>
        <select
          className="mt-3 h-9 rounded-md border border-border bg-background px-2 text-sm"
          value={category}
          onChange={(event) => setCategory(event.target.value)}
        >
          {categories.map((item) => (
            <option key={item} value={item}>{item === "all" ? "全部分类" : item}</option>
          ))}
        </select>
        <div className="mt-4 min-h-0 flex-1 space-y-2 overflow-y-auto pr-1">
          {filtered.map((template) => (
            <button
              key={template.slug}
              type="button"
              className={cn(
                "w-full rounded-md border p-3 text-left transition-colors",
                selected?.slug === template.slug ? "border-primary/40 bg-primary/10" : "border-border bg-background/50 hover:bg-muted/20",
              )}
              onClick={() => setSelectedSlug(template.slug)}
            >
              <div className="flex items-start justify-between gap-2">
                <div className="min-w-0">
                  <div className="flex items-center gap-1.5">
                    <span className="truncate text-sm font-semibold">{template.name}</span>
                    {customSlugs.has(template.slug) && <span className="shrink-0 rounded bg-primary/15 px-1 text-[10px] text-primary">自定义</span>}
                  </div>
                  <div className="mt-1 text-xs text-muted-foreground">{template.category}</div>
                </div>
                <Star className={cn("h-4 w-4 shrink-0", favorites.has(template.slug) ? "fill-current text-amber-400" : "text-muted-foreground/50")} />
              </div>
              <p className="mt-2 line-clamp-2 text-xs leading-5 text-muted-foreground">{template.description}</p>
            </button>
          ))}
        </div>
      </aside>

      <main className="min-w-0 flex-1 overflow-y-auto p-6">
        {showCreate ? (
          <div className="mx-auto max-w-3xl">
            <div className="mb-4 flex items-center gap-2 text-lg font-semibold">
              <Plus className="h-5 w-5 text-primary" /> 新建自定义助手
            </div>
            <div className="flex flex-col gap-3 rounded-md border border-border glass-surface p-4">
              <div className="grid grid-cols-2 gap-3">
                <label className="flex flex-col gap-1 text-xs text-muted-foreground">
                  名称
                  <input className="rounded border border-border bg-background px-2 py-1.5 text-sm" value={form.name} onChange={(e) => setForm({ ...form, name: e.target.value })} placeholder="如：法律合同审阅助手" />
                </label>
                <label className="flex flex-col gap-1 text-xs text-muted-foreground">
                  分类
                  <input className="rounded border border-border bg-background px-2 py-1.5 text-sm" value={form.category} onChange={(e) => setForm({ ...form, category: e.target.value })} placeholder="自定义" />
                </label>
              </div>
              <label className="flex flex-col gap-1 text-xs text-muted-foreground">
                简介
                <input className="rounded border border-border bg-background px-2 py-1.5 text-sm" value={form.description} onChange={(e) => setForm({ ...form, description: e.target.value })} placeholder="一句话说明这个助手做什么" />
              </label>
              <label className="flex flex-col gap-1 text-xs text-muted-foreground">
                提示词 / 系统指令
                <textarea className="min-h-[200px] rounded border border-border bg-background px-2 py-1.5 font-mono text-sm leading-6" value={form.instructions} onChange={(e) => setForm({ ...form, instructions: e.target.value })} placeholder="你是一个……请按以下要求工作……" />
              </label>
              <div className="flex justify-end gap-2">
                <Button variant="outline" onClick={() => { setShowCreate(false); void loadAll(); }}>取消</Button>
                <Button onClick={() => void createAssistant()}>创建</Button>
              </div>
            </div>
          </div>
        ) : selected ? (
          <div className="mx-auto max-w-5xl">
            <div className="mb-5 flex flex-wrap items-start justify-between gap-3">
              <div className="min-w-0">
                <div className="flex items-center gap-2 text-lg font-semibold">
                  <Wand2 className="h-5 w-5 text-primary" />
                  {selected.name}
                </div>
                <p className="mt-1 max-w-3xl text-sm leading-6 text-muted-foreground">{selected.description}</p>
              </div>
              <div className="flex gap-2">
                <Button variant="outline" onClick={() => toggleFavorite(selected.slug)}>
                  <Star className={cn("h-4 w-4", favorites.has(selected.slug) && "fill-current text-amber-400")} />
                  收藏
                </Button>
                <Button variant="outline" onClick={() => copyTemplate(selected)}>
                  <Copy className="h-4 w-4" />
                  复制
                </Button>
                <Button variant="outline" onClick={() => exportAssistant(selected)} title="导出为 JSON 分享">
                  <Download className="h-4 w-4" />
                  分享
                </Button>
                {customSlugs.has(selected.slug) && (
                  <Button variant="outline" onClick={() => void deleteAssistant(selected.slug)} className="text-destructive hover:text-destructive">
                    <Trash2 className="h-4 w-4" />
                    删除
                  </Button>
                )}
                <Button onClick={() => onUseTemplate?.(selected)}>
                  <Wand2 className="h-4 w-4" />
                  带入工作
                </Button>
              </div>
            </div>

            <div className="grid gap-4 xl:grid-cols-[minmax(0,1fr)_280px]">
              <section className="rounded-md border border-border glass-surface p-4">
                <div className="mb-3 text-sm font-semibold">提示词</div>
                <pre className="max-h-[62vh] overflow-y-auto whitespace-pre-wrap break-words rounded-md border border-border bg-background p-4 text-sm leading-6">
                  {selected.instructions}
                </pre>
              </section>

              <aside className="rounded-md border border-border glass-surface p-4">
                <div className="text-sm font-semibold">关联技能</div>
                <div className="mt-3 space-y-2">
                  {selected.skills.length === 0 ? (
                    <div className="text-sm text-muted-foreground">未绑定技能</div>
                  ) : selected.skills.map((skill) => (
                    <div key={skill.name} className="rounded-md border border-border bg-background/60 p-3">
                      <div className="text-sm font-medium">{skill.name}</div>
                      <p className="mt-1 text-xs leading-5 text-muted-foreground">{skill.description}</p>
                    </div>
                  ))}
                </div>
              </aside>
            </div>
          </div>
        ) : (
          <div className="flex h-full items-center justify-center text-sm text-muted-foreground">
            暂无助手模板
          </div>
        )}
      </main>
    </div>
  );
}

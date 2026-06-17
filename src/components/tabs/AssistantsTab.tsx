import { useEffect, useMemo, useState } from "react";
import { Copy, Search, Star, Wand2 } from "lucide-react";

import { Button } from "@/components/ui/button";
import { agentTemplateApi } from "@/lib/tauri-api";
import type { AgentTemplate } from "@/lib/tauri-api";
import { cn } from "@/lib/utils";
import { toast } from "@/components/ui/sonner";

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
  const [query, setQuery] = useState("");
  const [category, setCategory] = useState("all");
  const [favorites, setFavorites] = useState<Set<string>>(() => readFavorites());
  const [selectedSlug, setSelectedSlug] = useState<string | null>(null);

  useEffect(() => {
    agentTemplateApi
      .getAll()
      .then((items) => {
        const ordered = [...items].sort((a, b) => {
          const aIndex = FEATURED_SLUGS.indexOf(a.slug);
          const bIndex = FEATURED_SLUGS.indexOf(b.slug);
          if (aIndex >= 0 || bIndex >= 0) return (aIndex < 0 ? 999 : aIndex) - (bIndex < 0 ? 999 : bIndex);
          return a.name.localeCompare(b.name);
        });
        setTemplates(ordered);
        setSelectedSlug((prev) => prev || ordered[0]?.slug || null);
      })
      .catch((error) => toast.error("加载助手模板失败：" + error));
  }, []);

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

  return (
    <div className="flex h-full min-h-0 flex-1 overflow-hidden bg-background">
      <aside className="hidden w-80 shrink-0 border-r border-border bg-card/30 p-4 lg:flex lg:flex-col">
        <div className="flex items-center gap-2 text-base font-semibold">
          <Wand2 className="h-4 w-4 text-primary" />
          助手模板库
        </div>
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
                  <div className="truncate text-sm font-semibold">{template.name}</div>
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
        {selected ? (
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
                <Button onClick={() => onUseTemplate?.(selected)}>
                  <Wand2 className="h-4 w-4" />
                  带入工作
                </Button>
              </div>
            </div>

            <div className="grid gap-4 xl:grid-cols-[minmax(0,1fr)_280px]">
              <section className="rounded-md border border-border bg-card/50 p-4">
                <div className="mb-3 text-sm font-semibold">提示词</div>
                <pre className="max-h-[62vh] overflow-y-auto whitespace-pre-wrap break-words rounded-md border border-border bg-background p-4 text-sm leading-6">
                  {selected.instructions}
                </pre>
              </section>

              <aside className="rounded-md border border-border bg-card/50 p-4">
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

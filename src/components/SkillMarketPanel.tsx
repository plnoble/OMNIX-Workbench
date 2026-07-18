/**
 * SkillMarketPanel — 技能市场（从 GitHub/Anthropic 等来源搜索并导入技能）。
 * 从旧版 SkillHub 拆出的精简版；导入的技能进入待定池，走审核门。
 */
import { useState } from "react";
import { Download, Eye, Loader2, Search, X } from "lucide-react";
import { toast } from "sonner";

import { skillLibraryApi, type MarketSkill } from "@/lib/tauri-api";

export function SkillMarketPanel({ onClose, onImported }: { onClose: () => void; onImported: () => void }) {
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<MarketSkill[]>([]);
  const [searching, setSearching] = useState(false);
  const [busy, setBusy] = useState<string | null>(null);
  const [preview, setPreview] = useState<{ name: string; content: string } | null>(null);

  const search = async () => {
    if (!query.trim()) return;
    setSearching(true);
    try {
      setResults(await skillLibraryApi.searchMarket(query.trim()));
    } catch (e) {
      toast.error(`搜索失败：${e}`);
    } finally {
      setSearching(false);
    }
  };

  const handlePreview = async (skill: MarketSkill) => {
    setBusy(skill.name);
    try {
      const p = await skillLibraryApi.previewMarket(skill);
      setPreview({ name: skill.name, content: p.content });
    } catch (e) {
      toast.error(`预览失败：${e}`);
    } finally {
      setBusy(null);
    }
  };

  const handleImport = async (skill: MarketSkill) => {
    setBusy(skill.name);
    try {
      const name = await skillLibraryApi.importMarket(skill, false);
      toast.success(`「${name}」已导入待定池——审核通过后才可晋升正式`);
      onImported();
    } catch (e) {
      const msg = String(e);
      if (msg.includes("已存在") && window.confirm(`${msg}\n\n覆盖导入？`)) {
        try {
          await skillLibraryApi.importMarket(skill, true);
          toast.success(`「${skill.name}」已覆盖导入待定池`);
          onImported();
        } catch (e2) {
          toast.error(String(e2));
        }
      } else {
        toast.error(msg);
      }
    } finally {
      setBusy(null);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-8" onClick={onClose}>
      <div
        className="flex max-h-full w-full max-w-2xl flex-col gap-3 rounded-xl border border-border bg-background p-4 shadow-2xl"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center justify-between">
          <span className="text-sm font-semibold">技能市场</span>
          <button onClick={onClose}>
            <X className="h-4 w-4 text-muted-foreground hover:text-foreground" />
          </button>
        </div>
        <div className="flex gap-2">
          <input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && void search()}
            placeholder="搜索外部技能（如 code review / commit / rust）…"
            className="h-9 flex-1 rounded-lg border border-border bg-background px-3 text-sm outline-none focus:border-primary"
          />
          <button
            onClick={() => void search()}
            disabled={searching}
            className="inline-flex h-9 items-center gap-1.5 rounded-lg bg-primary px-4 text-sm font-medium text-primary-foreground hover:opacity-90 disabled:opacity-50"
          >
            {searching ? <Loader2 className="h-4 w-4 animate-spin" /> : <Search className="h-4 w-4" />}
            搜索
          </button>
        </div>
        <div className="flex min-h-0 flex-1 flex-col gap-1.5 overflow-y-auto">
          {results.length === 0 && (
            <div className="rounded-lg border border-dashed border-border p-8 text-center text-xs text-muted-foreground">
              {searching ? "搜索中…" : "输入关键词搜索 GitHub 等来源的技能"}
            </div>
          )}
          {results.map((skill) => (
            <div key={`${skill.repo_url}-${skill.name}`} className="rounded-lg border border-border glass-surface p-2.5">
              <div className="flex items-center gap-2">
                <span className="truncate text-sm font-medium">{skill.name}</span>
                <span className="ml-auto flex shrink-0 items-center gap-1">
                  <button
                    onClick={() => void handlePreview(skill)}
                    disabled={busy !== null}
                    title="预览内容"
                    className="rounded p-1 text-muted-foreground hover:bg-muted/40 hover:text-foreground disabled:opacity-50"
                  >
                    {busy === skill.name ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <Eye className="h-3.5 w-3.5" />}
                  </button>
                  <button
                    onClick={() => void handleImport(skill)}
                    disabled={busy !== null}
                    title="导入待定池"
                    className="rounded p-1 text-muted-foreground hover:bg-muted/40 hover:text-primary disabled:opacity-50"
                  >
                    <Download className="h-3.5 w-3.5" />
                  </button>
                </span>
              </div>
              <p className="mt-0.5 line-clamp-2 text-xs text-muted-foreground">{skill.description}</p>
              <p className="mt-0.5 truncate text-[10px] text-muted-foreground/70">{skill.repo_url}</p>
            </div>
          ))}
        </div>

        {preview && (
          <div className="fixed inset-0 z-[60] flex items-center justify-center bg-black/50 p-10" onClick={() => setPreview(null)}>
            <div
              className="flex max-h-full w-full max-w-3xl flex-col rounded-xl border border-border bg-background shadow-2xl"
              onClick={(e) => e.stopPropagation()}
            >
              <div className="flex items-center justify-between border-b border-border px-4 py-2.5">
                <span className="text-sm font-semibold">{preview.name}</span>
                <button onClick={() => setPreview(null)} className="rounded px-2 py-1 text-sm text-muted-foreground hover:bg-muted/40">
                  关闭
                </button>
              </div>
              <pre className="flex-1 overflow-auto whitespace-pre-wrap p-4 text-xs leading-5">{preview.content}</pre>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

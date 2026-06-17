import { useState } from "react";
import { Edit, Globe, Plus, Search, Trash2 } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { cn } from "@/lib/utils";
import type { SearchProvider, WebSearchResult } from "@/types";

interface SearchResourceTabProps {
  providers: SearchProvider[];
  selectedProviderId: string;
  results: WebSearchResult[];
  query: string;
  isSearching: boolean;
  onSetQuery: (query: string) => void;
  onSetSelectedProviderId: (id: string) => void;
  onSearch: (query: string) => Promise<WebSearchResult[]>;
  onAddProvider: () => void;
  onEditProvider: (provider: SearchProvider) => void;
  onDeleteProvider: (id: string) => Promise<void>;
}

export function SearchResourceTab({
  providers,
  selectedProviderId,
  results,
  query,
  isSearching,
  onSetQuery,
  onSetSelectedProviderId,
  onSearch,
  onAddProvider,
  onEditProvider,
  onDeleteProvider,
}: SearchResourceTabProps) {
  const [error, setError] = useState<string | null>(null);

  const runSearch = async () => {
    if (!query.trim()) return;
    setError(null);
    try {
      await onSearch(query.trim());
    } catch (err) {
      setError(String(err));
    }
  };

  return (
    <div className="flex h-full flex-1 overflow-hidden bg-background">
      <aside className="w-80 shrink-0 border-r border-border p-5">
        <div className="mb-4 flex items-center justify-between gap-3">
          <div>
            <div className="flex items-center gap-2 text-base font-semibold">
              <Globe className="h-4 w-4 text-primary" />
              搜索供应商
            </div>
            <p className="mt-1 text-xs text-muted-foreground">普通对话可手动启用搜索。</p>
          </div>
          <Button size="sm" variant="outline" onClick={onAddProvider}>
            <Plus className="h-3.5 w-3.5" />
            添加
          </Button>
        </div>

        <div className="space-y-2">
          {providers.length === 0 ? (
            <div className="rounded-md border border-dashed border-border p-4 text-sm text-muted-foreground">
              还没有搜索供应商，可以添加 SearXNG、Bing、Google 或免 Key 供应商。
            </div>
          ) : providers.map((provider) => (
            <div
              key={provider.id}
              className={cn(
                "rounded-md border p-3",
                selectedProviderId === provider.id ? "border-primary/40 bg-primary/10" : "border-border bg-card/40"
              )}
            >
              <button className="w-full text-left" onClick={() => onSetSelectedProviderId(provider.id)}>
                <div className="flex items-center justify-between gap-3">
                  <span className="truncate text-sm font-semibold">{provider.name}</span>
                  <span className={cn("rounded px-1.5 py-0.5 text-[10px]", provider.is_enabled ? "bg-success/15 text-success" : "bg-muted text-muted-foreground")}>
                    {provider.is_enabled ? "ON" : "OFF"}
                  </span>
                </div>
                <div className="mt-1 truncate text-xs text-muted-foreground">{provider.api_type}</div>
              </button>
              <div className="mt-3 flex gap-2">
                <Button size="sm" variant="ghost" className="h-7 px-2 text-xs" onClick={() => onEditProvider(provider)}>
                  <Edit className="h-3 w-3" />
                  编辑
                </Button>
                <Button size="sm" variant="ghost" className="h-7 px-2 text-xs text-destructive" onClick={() => onDeleteProvider(provider.id)}>
                  <Trash2 className="h-3 w-3" />
                  删除
                </Button>
              </div>
            </div>
          ))}
        </div>
      </aside>

      <section className="flex min-w-0 flex-1 flex-col p-6">
        <div className="mb-5">
          <div className="text-lg font-semibold">搜索调试</div>
          <p className="mt-1 text-sm text-muted-foreground">这里用于验证搜索供应商是否可用，开发工作区和 Team 默认不自动接入搜索。</p>
        </div>

        <div className="flex gap-2">
          <Input
            value={query}
            onChange={(event) => onSetQuery(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter") runSearch();
            }}
            placeholder="输入搜索问题..."
          />
          <Button onClick={runSearch} disabled={isSearching || !query.trim()}>
            <Search className="h-4 w-4" />
            {isSearching ? "搜索中" : "搜索"}
          </Button>
        </div>

        {error && <div className="mt-3 rounded-md border border-destructive/30 bg-destructive/10 p-3 text-sm text-destructive">{error}</div>}

        <div className="mt-5 min-h-0 flex-1 overflow-y-auto">
          {results.length === 0 ? (
            <div className="rounded-md border border-dashed border-border p-8 text-center text-sm text-muted-foreground">
              搜索结果会显示在这里。
            </div>
          ) : (
            <div className="space-y-3">
              {results.map((result, index) => (
                <a
                  key={`${result.url}-${index}`}
                  href={result.url}
                  target="_blank"
                  rel="noreferrer"
                  className="block rounded-md border border-border bg-card/40 p-4 hover:bg-muted/20"
                >
                  <div className="text-sm font-semibold">{result.title}</div>
                  <p className="mt-2 text-sm leading-6 text-muted-foreground">{result.snippet}</p>
                  <div className="mt-2 truncate text-xs text-primary">{result.url}</div>
                </a>
              ))}
            </div>
          )}
        </div>
      </section>
    </div>
  );
}

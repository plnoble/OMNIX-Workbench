/**
 * SearchResourceTab — 搜索：配供应商、试搜、回看历史，一页做完。
 *
 * 以前这页只能「试搜」，真正的增删改在 设置 → 系统 → 搜索服务：配一个 Key 要跨
 * 两个页面，而两边读的是同一张 `search_providers` 表。现在配置搬过来了，设置那
 * 一组已删除。
 *
 * 搜索历史（`search_history` 表）也接在这里。它一直在写——每次搜索都存一条，
 * 命令、hook、类型全都齐了——但**从来没有任何界面读过它**。
 */
import { useEffect, useState } from "react";
import { useSearchStore } from "@/store/AppStore";
import { Edit, ExternalLink, Globe, History, Plus, Save, Search, Trash2 } from "lucide-react";
import { openUrl } from "@tauri-apps/plugin-opener";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { toast } from "@/components/ui/sonner";
import { cn } from "@/lib/utils";
import { searchApi } from "@/lib/tauri-api";
import type { SearchProvider } from "@/types";

/** 后端 `commands/search.rs` 认得的类型，顺序即弹窗里的排列顺序。 */
const API_TYPES = ["tavily", "exa", "jina", "brave", "zhipu", "google", "duckduckgo"] as const;

/**
 * 预设。两条规则：**必须有免费额度或试用额度**，且链接**直达申请 Key 的那一页**
 * ——不是官网首页，点进去一脸茫然是之前最大的问题。额度按 2026 年 8 月核实的
 * 公开信息标注，各家随时会改。
 */
const PRESETS: { name: string; type: string; url: string; color: string; desc: string; best?: boolean }[] = [
  { name: "Tavily", type: "tavily", url: "https://app.tavily.com/home", color: "text-blue-400", desc: "每月 1000 次免费 · 不用绑卡", best: true },
  { name: "Exa", type: "exa", url: "https://dashboard.exa.ai/api-keys", color: "text-emerald-400", desc: "注册送 $10（约 1000 次）" },
  { name: "Jina", type: "jina", url: "https://jina.ai/api-dashboard/", color: "text-cyan-400", desc: "有免费额度 · 页面直接给试用 Key" },
  { name: "Brave Search", type: "brave", url: "https://api-dashboard.search.brave.com/register", color: "text-orange-400", desc: "需绑卡 · 每月 $5 额度（约 1000 次）" },
  { name: "智谱搜索", type: "zhipu", url: "https://open.bigmodel.cn/usercenter/apikeys", color: "text-violet-400", desc: "国内直连 · 按次计费，有新用户额度" },
  { name: "Google", type: "google", url: "https://programmablesearchengine.google.com/controlpanel/create", color: "text-sky-400", desc: "每天 100 次免费 · 要先建搜索引擎拿 CX" },
];

export function SearchResourceTab() {
  // 以前是 21 个 prop 从 App.tsx 逐个透传；下面只是把 store 的命名对到组件内原有命名。
  const search = useSearchStore();
  const {
    providers, selectedProviderId, results, history, isSearching,
    setSelectedProviderId: onSetSelectedProviderId,
    setSearchQuery: onSetQuery,
    searchQuery: query,
    search: onSearch,
    loadHistory: onLoadHistory,
    deleteHistoryItem: onDeleteHistoryItem,
    clearHistory: onClearHistory,
    deleteProvider: onDeleteProvider,
    showSearchProviderModal: showProviderModal,
    editingSearchProvider: editingProvider,
    searchProviderForm: providerForm,
    closeSearchProviderModal: onCloseProviderModal,
    updateSearchProviderForm: onUpdateProviderForm,
  } = search;
  const onAddProvider = () => search.openSearchProviderModal();
  const onEditProvider = (provider: SearchProvider) => search.openSearchProviderModal(provider);
  const onSaveProvider = () => search.saveProvider(search.searchProviderForm);
  const [error, setError] = useState<string | null>(null);
  const [showHistory, setShowHistory] = useState(false);
  const [testing, setTesting] = useState(false);

  useEffect(() => { void onLoadHistory(); }, [onLoadHistory]);

  const runSearch = async (q?: string) => {
    const text = (q ?? query).trim();
    if (!text) return;
    if (q) onSetQuery(q);
    setError(null);
    try {
      await onSearch(text);
      void onLoadHistory();
    } catch (err) {
      setError(String(err));
    }
  };

  const applyPreset = (preset: typeof PRESETS[number]) => {
    onAddProvider();
    onUpdateProviderForm("api_type", preset.type);
    onUpdateProviderForm("name", preset.name);
    onUpdateProviderForm("api_address", "");
    openUrl(preset.url).catch(() => window.open(preset.url, "_blank"));
  };

  return (
    <div className="flex h-full flex-1 overflow-hidden bg-background">
      <aside className="flex w-80 shrink-0 flex-col overflow-y-auto border-r border-border p-5">
        <div className="mb-4 flex items-center justify-between gap-3">
          <div className="min-w-0">
            <div className="flex items-center gap-2 text-base font-semibold">
              <Globe className="h-4 w-4 text-primary" /> 搜索供应商
            </div>
            <p className="mt-1 text-xs leading-5 text-muted-foreground">
              对话里的「联网搜索」和 Agent 的 <code>web_search</code> 工具都用这里配的 Key。
            </p>
          </div>
          <Button size="sm" variant="outline" onClick={onAddProvider}>
            <Plus className="h-3.5 w-3.5" /> 添加
          </Button>
        </div>

        {/* 没配过的时候，预设摆在最前面——这是唯一有用的下一步。 */}
        <div className="mb-4">
          <div className="mb-1.5 text-xs font-medium text-muted-foreground">还没有 Key？点一个去申请</div>
          <div className="flex flex-col gap-1">
            {PRESETS.map((preset) => (
              <button
                key={preset.name}
                onClick={() => applyPreset(preset)}
                title={`打开 ${preset.name} 的 Key 申请页，并把新增表单预填成这个供应商`}
                className={cn(
                  "flex items-start gap-1.5 rounded-md border bg-background/20 p-2 text-left transition hover:bg-background/50",
                  preset.best ? "border-success/40" : "border-border/40"
                )}
              >
                <ExternalLink className={cn("mt-0.5 h-3 w-3 shrink-0", preset.color)} />
                <div className="flex min-w-0 flex-col">
                  <span className="flex items-center gap-1 text-xs font-medium">
                    {preset.name}
                    {preset.best && <span className="rounded bg-success/15 px-1 text-[10px] text-success">推荐</span>}
                  </span>
                  <span className="text-[11px] leading-tight text-muted-foreground">{preset.desc}</span>
                </div>
              </button>
            ))}
          </div>
          <p className="mt-2 text-[11px] leading-4 text-muted-foreground">
            已移除：<strong>Bing</strong>（微软 2025-08-11 下线搜索 API，旧 Key 返回 410）、
            <strong>博查</strong>（只有按次计费）、<strong>SearXNG</strong>（要自己跑 Docker）。
            <strong>DuckDuckGo</strong> 免 Key 但只有「即时答案」、不是网页搜索，仅作兜底。
          </p>
        </div>

        <div className="space-y-2">
          {providers.length === 0 ? (
            <div className="rounded-md border border-dashed border-border p-4 text-xs text-muted-foreground">
              还没有配置任何供应商。上面挑一个申请 Key，回来粘贴即可。
            </div>
          ) : providers.map((provider) => (
            <div
              key={provider.id}
              className={cn(
                "rounded-md border p-3",
                selectedProviderId === provider.id ? "border-primary/40 bg-primary/10" : "border-border glass-surface"
              )}
            >
              <button className="w-full text-left" onClick={() => onSetSelectedProviderId(provider.id)}>
                <div className="flex items-center justify-between gap-3">
                  <span className="truncate text-sm font-semibold">{provider.name || "（未命名）"}</span>
                  <span className={cn("rounded px-1.5 py-0.5 text-[10px]", provider.is_enabled ? "bg-success/15 text-success" : "bg-muted text-muted-foreground")}>
                    {provider.is_enabled ? "ON" : "OFF"}
                  </span>
                </div>
                <div className="mt-1 flex items-center gap-1.5 text-xs text-muted-foreground">
                  <span className="truncate">{provider.api_type}</span>
                  {/* 「配没配 Key」是排查搜不出东西时第一个要看的事，别藏在弹窗里。 */}
                  <span className={provider.api_key ? "text-success" : "text-warning"}>
                    {provider.api_key ? "· 有 Key" : "· 无 Key"}
                  </span>
                </div>
              </button>
              <div className="mt-3 flex gap-2">
                <Button size="sm" variant="ghost" className="h-7 px-2 text-xs" onClick={() => onEditProvider(provider)}>
                  <Edit className="h-3 w-3" /> 编辑
                </Button>
                <Button size="sm" variant="ghost" className="h-7 px-2 text-xs text-destructive" onClick={() => onDeleteProvider(provider.id)}>
                  <Trash2 className="h-3 w-3" /> 删除
                </Button>
              </div>
            </div>
          ))}
        </div>
      </aside>

      <section className="flex min-w-0 flex-1 flex-col p-6">
        <div className="mb-5 flex items-start justify-between gap-3">
          <div className="min-w-0">
            <div className="text-lg font-semibold">搜索调试</div>
            <p className="mt-1 text-sm text-muted-foreground">
              在这里搜一次就能确认供应商是否可用。左边选中哪个就用哪个，不选则用第一个启用的。
            </p>
          </div>
          <Button variant="outline" size="sm" onClick={() => setShowHistory((v) => !v)}>
            <History className="h-3.5 w-3.5" /> 历史 {history.length > 0 && `(${history.length})`}
          </Button>
        </div>

        <div className="flex gap-2">
          <Input
            value={query}
            onChange={(event) => onSetQuery(event.target.value)}
            onKeyDown={(event) => { if (event.key === "Enter") void runSearch(); }}
            placeholder="输入搜索问题..."
          />
          <Button onClick={() => void runSearch()} disabled={isSearching || !query.trim()}>
            <Search className="h-4 w-4" /> {isSearching ? "搜索中" : "搜索"}
          </Button>
        </div>

        {error && <div className="mt-3 rounded-md border border-destructive/30 bg-destructive/10 p-3 text-sm text-destructive">{error}</div>}

        {showHistory && (
          <div className="mt-4 rounded-md border border-border glass-surface p-3">
            <div className="mb-2 flex items-center justify-between">
              <span className="text-xs font-medium text-muted-foreground">搜索历史</span>
              {history.length > 0 && (
                <button className="text-xs text-destructive hover:underline" onClick={() => void onClearHistory()}>
                  全部清空
                </button>
              )}
            </div>
            {history.length === 0 ? (
              <div className="py-2 text-center text-xs text-muted-foreground">还没有搜索记录。</div>
            ) : (
              <div className="max-h-52 space-y-1 overflow-y-auto">
                {history.map((entry) => (
                  <div key={entry.id} className="flex items-center gap-2 rounded px-2 py-1 text-xs hover:bg-muted/20">
                    <button className="min-w-0 flex-1 truncate text-left" onClick={() => void runSearch(entry.query)} title="再搜一次">
                      {entry.query}
                    </button>
                    <span className="shrink-0 text-muted-foreground">{entry.result_count} 条 · {entry.created_at.slice(0, 16)}</span>
                    <button className="shrink-0 text-muted-foreground hover:text-destructive" onClick={() => void onDeleteHistoryItem(entry.id)}>
                      <Trash2 className="h-3 w-3" />
                    </button>
                  </div>
                ))}
              </div>
            )}
          </div>
        )}

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
                  className="block rounded-md border border-border glass-surface p-4 hover:bg-muted/20"
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

      {showProviderModal && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
          <div className="glass-card mx-4 max-h-[80vh] w-full max-w-[480px] overflow-y-auto p-6">
            <h3 className="mb-4 text-lg font-semibold">{editingProvider ? "编辑搜索引擎" : "新增搜索引擎"}</h3>
            <div className="flex flex-col gap-3">
              <div className="space-y-1.5">
                <Label>名称</Label>
                <Input value={providerForm.name} onChange={(e) => onUpdateProviderForm("name", e.target.value)} placeholder="例如: Tavily" />
              </div>
              <div className="space-y-1.5">
                <Label>类型</Label>
                <div className="flex flex-wrap gap-1.5">
                  {API_TYPES.map((t) => (
                    <button
                      key={t}
                      onClick={() => onUpdateProviderForm("api_type", t)}
                      className={cn(
                        "rounded-md border px-2.5 py-1.5 text-xs",
                        providerForm.api_type === t ? "border-primary bg-primary/10" : "border-border"
                      )}
                    >
                      {t.toUpperCase()}
                    </button>
                  ))}
                </div>
              </div>
              <div className="space-y-1.5">
                <Label>{providerForm.api_type === "google" ? "搜索引擎 ID（CX）" : "API 地址"}</Label>
                <Input
                  value={providerForm.api_address}
                  onChange={(e) => onUpdateProviderForm("api_address", e.target.value)}
                  placeholder={providerForm.api_type === "google"
                    ? "Google 必填：在可编程搜索引擎控制台创建后得到的 cx 值"
                    : "https://api.example.com（留空用供应商默认）"}
                />
                {providerForm.api_type === "google" && (
                  <span className="text-[11px] text-muted-foreground">
                    Google 要两样东西：这里填搜索引擎 ID（cx），下面填 API Key。缺一个都搜不出结果。
                  </span>
                )}
              </div>
              <div className="space-y-1.5">
                <Label>API Key</Label>
                <Input type="password" value={providerForm.api_key} onChange={(e) => onUpdateProviderForm("api_key", e.target.value)} placeholder="留空则无需认证（只有 DuckDuckGo 是这样）" />
              </div>
              <div className="flex items-center gap-2">
                <Switch checked={providerForm.is_enabled} onCheckedChange={(v) => onUpdateProviderForm("is_enabled", v)} id="sp_enabled" />
                <Label htmlFor="sp_enabled">启用</Label>
              </div>
            </div>
            <div className="mt-4 flex justify-end gap-2">
              <Button variant="ghost" onClick={onCloseProviderModal}>取消</Button>
              <Button
                variant="outline"
                disabled={testing}
                onClick={async () => {
                  setTesting(true);
                  try {
                    // 先存再测：测的是库里那一条，不是表单里的草稿。
                    await onSaveProvider();
                    const hits = await searchApi.search("hello world", providerForm.id, 3);
                    toast.success(`连通成功，返回 ${hits.length} 条结果`);
                  } catch (e) {
                    toast.error("连通失败", { description: String(e) });
                  } finally {
                    setTesting(false);
                  }
                }}
              >
                <Search className="h-4 w-4" /> {testing ? "测试中…" : "测试连接"}
              </Button>
              <Button onClick={async () => {
                try {
                  await onSaveProvider();
                  toast.success("已保存");
                  onCloseProviderModal();
                } catch (e) {
                  toast.error("保存失败", { description: String(e) });
                }
              }}>
                <Save className="h-4 w-4" /> 保存
              </Button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

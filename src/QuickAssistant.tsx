/**
 * QuickAssistant — 划词助手浮动窗口
 *
 * Cherry Studio 风格：
 * - 鼠标选中文字 → 自动捕获 → 显示浮动操作栏（翻译/解释/总结/搜索/复制）
 * - 点击操作 → 执行对应功能，结果流式渲染
 * - 模型从大模型平台已配置模型中选择（下拉）
 * - Ctrl+Shift+Space 手动唤起，ESC 关闭
 */

import { useState, useEffect, useCallback, useRef } from "react";
import { listen } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import { readText, writeText } from "@tauri-apps/plugin-clipboard-manager";
import { qaApi, settingsApi, modelApi } from "@/lib/tauri-api";
import { useTranslation } from "@/hooks/useTranslation";
import { BUILTIN_LANGUAGES, getLanguageByCode } from "@/lib/translate-constants";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Badge } from "@/components/ui/badge";
import { toast } from "@/components/ui/sonner";
import { cn } from "@/lib/utils";
import ReactMarkdown from "react-markdown";
import {
  Search, Copy, X, Loader2,
  Languages, ArrowRightLeft, Lightbulb, FileText,
  Square, Globe, ClipboardCopy, Sparkles,
} from "lucide-react";
import type { SearchResult } from "@/types";

// ── Action Definitions ─────────────────────────────────

type QAction = "translate" | "explain" | "summarize" | "refine" | "search" | "copy";

interface ActionDef {
  id: QAction;
  label: string;
  icon: React.ReactNode;
  color: string;
  promptPrefix?: string;
}

const ACTIONS: ActionDef[] = [
  { id: "translate", label: "翻译",   icon: <Languages className="h-3.5 w-3.5" />,  color: "text-violet-500" },
  { id: "explain",   label: "解释",   icon: <Lightbulb className="h-3.5 w-3.5" />,  color: "text-amber-500",
    promptPrefix: "Please explain the following text in a clear and detailed way. Use simple language and provide examples where helpful:\n\n" },
  { id: "summarize", label: "总结",   icon: <FileText className="h-3.5 w-3.5" />,   color: "text-emerald-500",
    promptPrefix: "Please provide a concise summary of the following text. Capture the key points and main ideas:\n\n" },
  { id: "refine",    label: "精炼",   icon: <Sparkles className="h-3.5 w-3.5" />,   color: "text-pink-500",
    promptPrefix: "Please refine and polish the following text. Improve clarity, grammar, and style while preserving the original meaning:\n\n" },
  { id: "search",    label: "搜索",   icon: <Globe className="h-3.5 w-3.5" />,       color: "text-blue-500" },
  { id: "copy",      label: "复制",   icon: <ClipboardCopy className="h-3.5 w-3.5" />, color: "text-gray-500" },
];

// ── Friendly Error Messages ────────────────────────────

function friendlyError(raw: unknown): string {
  const msg = String(raw);
  if (msg.includes("401") || msg.includes("Unauthorized") || msg.includes("Incorrect API key"))
    return "❌ API Key 无效或未配置，请在设置中检查密钥";
  if (msg.includes("429") || msg.includes("rate_limit") || msg.includes("Too Many Requests"))
    return "❌ 请求过于频繁，请稍后再试";
  if (msg.includes("500") || msg.includes("Internal Server Error"))
    return "❌ 服务器内部错误，请稍后再试";
  if (msg.includes("No active model platforms") || msg.includes("No enabled platform"))
    return "❌ 未配置可用模型，请在设置中启用模型平台并填入 API Key";
  if (msg.includes("API Key is not configured"))
    return "❌ 该模型平台未配置 API Key，请在设置中填写";
  if (msg.includes("connection refused") || msg.includes("ECONNREFUSED"))
    return "❌ 无法连接到代理网关，请确认应用已正常启动";
  return `❌ 错误: ${msg.slice(0, 120)}`;
}

// ── QuickAssistant Component ───────────────────────────

export function QuickAssistant() {
  // Core state
  const [query, setQuery] = useState("");
  const [answer, setAnswer] = useState("");
  const [sources, setSources] = useState<SearchResult[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [isStreaming, setIsStreaming] = useState(false);
  const [chatModel, setChatModel] = useState("");
  const [availableModels, setAvailableModels] = useState<string[]>([]);

  // Action mode
  const [action, setAction] = useState<QAction | null>(null);
  const translation = useTranslation();
  const [targetLang, setTargetLang] = useState(""); // empty = auto (smart bidirectional)

  // Auto-capture state
  const [capturedText, setCapturedText] = useState("");
  const [capturedWindow, setCapturedWindow] = useState("");
  const [showCaptureBar, setShowCaptureBar] = useState(false);

  // Stream cleanup ref
  const unlistenStreamRef = useRef<(() => void) | null>(null);

  const isTranslateMode = action === "translate";
  const currentActionDef = action ? ACTIONS.find(a => a.id === action) : null;
  const resultText = isTranslateMode ? translation.translatedText : answer;
  const isWorking = isTranslateMode ? translation.isTranslating : isLoading;

  // ── Load settings & available models ────────────────────

  useEffect(() => {
    (async () => {
      try {
        const [model, targetModel] = await Promise.all([
          settingsApi.get("quick_assistant_model"),
          settingsApi.get("target_model"),
        ]);
        // Use QA-specific model if set, else global default
        const initialModel = model || targetModel || "";
        setChatModel(initialModel);

        // Load available models from platform
        const names = await modelApi.getAvailableNames();
        setAvailableModels(names);
        // If no model set yet, pick the first available
        if (!initialModel && names.length > 0) {
          setChatModel(names[0]);
        }

        await translation.loadTranslationSettings();
      } catch (e) {
        console.error("[QA] Failed to load settings:", e);
      }
    })();
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  // ── Listen for events ────────────────────────────────

  useEffect(() => {
    // Auto-capture: when text is selected anywhere, show the action bar
    const unlistenAutoCapture = listen<{ text: string; window_title: string }>(
      "selection-auto-captured",
      (event) => {
        setCapturedText(event.payload.text);
        setCapturedWindow(event.payload.window_title);
        setQuery(event.payload.text);
        setShowCaptureBar(true);
        // Reset previous results
        setAnswer("");
        setAction(null);
        setSources([]);
      }
    );

    // Manual QA toggle via shortcut
    const unlistenShown = listen("qa-shown", async () => {
      try {
        const text = await readText();
        if (text && text.trim()) {
          setCapturedText(text.trim());
          setQuery(text.trim());
          setShowCaptureBar(true);
        }
      } catch (e) {
        console.error("[QA] Failed to read clipboard:", e);
      }
    });

    // Preset text (from other parts of the app)
    const unlistenPreset = listen<string>("qa-preset-text", (event) => {
      setCapturedText(event.payload);
      setQuery(event.payload);
      setShowCaptureBar(true);
    });

    return () => {
      unlistenAutoCapture.then(fn => fn());
      unlistenShown.then(fn => fn());
      unlistenPreset.then(fn => fn());
    };
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  // ── ESC to close ─────────────────────────────────────

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        if (isStreaming) {
          stopStreaming();
        } else {
          hideWindow();
        }
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [isStreaming]); // eslint-disable-line react-hooks/exhaustive-deps

  // ── Window control ───────────────────────────────────

  const hideWindow = useCallback(async () => {
    try {
      await qaApi.toggle(false);
    } catch { /* ignore */ }
  }, []);

  // ── Stop streaming ───────────────────────────────────

  const stopStreaming = useCallback(() => {
    if (unlistenStreamRef.current) {
      unlistenStreamRef.current();
      unlistenStreamRef.current = null;
    }
    setIsStreaming(false);
    setIsLoading(false);
  }, []);

  // ── Execute action ────────────────────────────────────

  const executeAction = useCallback(async (pickedAction: QAction, textOverride?: string) => {
    const text = (textOverride || query).trim();
    if (!text) return;

    setAction(pickedAction);
    setShowCaptureBar(false);

    // Special: Copy action — just copy to clipboard
    if (pickedAction === "copy") {
      try {
        await writeText(text);
        toast.success("已复制到剪贴板");
      } catch {
        toast.error("复制失败");
      }
      return;
    }

    // Special: Search action — detect URI/file path, or open search engine
    if (pickedAction === "search") {
      try {
        // Cherry Studio: if text looks like a URL or file path, open directly
        const isUrl = /^(https?:\/\/|ftp:\/\/|file:\/\/)/i.test(text);
        const isFilePath = /^([A-Za-z]:\\|\/)/.test(text);
        if (isUrl || isFilePath) {
          await openUrl(isUrl ? text : `file://${text}`);
        } else {
          await openUrl(`https://www.bing.com/search?q=${encodeURIComponent(text)}`);
        }
      } catch {
        // Fallback: try Google
        try { await openUrl(`https://www.google.com/search?q=${encodeURIComponent(text)}`); } catch { /* */ }
      }
      return;
    }

    // Translate: use translation hook (pass targetLang if user explicitly chose, else let bidirectional logic pick)
    if (pickedAction === "translate") {
      await translation.translate(text, targetLang || undefined);
      return;
    }

    // Explain / Summarize — use streaming QA
    setIsLoading(true);
    setIsStreaming(true);
    setAnswer("");
    setSources([]);

    try {
      const actionDef = ACTIONS.find(a => a.id === pickedAction)!;
      const fullQuery = actionDef.promptPrefix
        ? `${actionDef.promptPrefix}${text}`
        : text;

      const unlistenChunk = listen<string>("qa-stream-chunk", (event) => {
        setAnswer(prev => prev + event.payload);
      });

      const unlistenDone = listen<Record<string, unknown>>("qa-stream-done", (event) => {
        const payload = event.payload;
        if (Array.isArray(payload.sources)) {
          const validated = payload.sources.filter(
            (s): s is SearchResult =>
              typeof s === "object" && s !== null &&
              typeof (s as Record<string, unknown>).chunk_id === "string" &&
              typeof (s as Record<string, unknown>).content === "string"
          );
          setSources(validated);
        }
        setIsStreaming(false);
        setIsLoading(false);
        unlistenChunk.then(fn => fn());
        unlistenDone.then(fn => fn());
        unlistenStreamRef.current = null;
      });

      const unlistenError = listen<string>("qa-stream-error", (event) => {
        setAnswer(prev => prev + friendlyError(event.payload));
        setIsStreaming(false);
        setIsLoading(false);
        unlistenChunk.then(fn => fn());
        unlistenDone.then(fn => fn());
        unlistenStreamRef.current = null;
      });

      unlistenStreamRef.current = () => {
        unlistenChunk.then(fn => fn());
        unlistenDone.then(fn => fn());
        unlistenError.then(fn => fn());
      };

      await qaApi.queryStream({
        query: fullQuery,
        useKb: false,
        chatModel,
        embeddingModel: undefined,
      });
    } catch (e) {
      setAnswer(friendlyError(e));
      setIsStreaming(false);
      setIsLoading(false);
    }
  }, [query, chatModel, targetLang, translation]);

  // ── Copy result ──────────────────────────────────────

  const handleCopy = useCallback(async () => {
    const text = isTranslateMode ? translation.translatedText : answer;
    if (!text) return;
    try {
      await navigator.clipboard.writeText(text);
      toast.success("已复制到剪贴板");
    } catch {
      toast.error("复制失败");
    }
  }, [answer, isTranslateMode, translation.translatedText]);

  // ── Render ───────────────────────────────────────────

  return (
    <div className="flex flex-col h-screen bg-background/95 backdrop-blur-xl border border-border/50 rounded-xl shadow-2xl overflow-hidden">
      {/* Header: captured text preview + action bar */}
      {showCaptureBar && capturedText ? (
        <div className="border-b border-border/50 bg-muted/30">
          {/* Captured text preview */}
          <div className="px-3 pt-2.5 pb-1.5">
            <div className="flex items-center gap-1.5 mb-1">
              <ClipboardCopy className="h-3 w-3 text-muted-foreground" />
              <span className="text-xs text-muted-foreground">
                {capturedWindow ? `来自 ${capturedWindow}` : "已捕获文字"}
              </span>
              <button
                className="ml-auto text-xs text-muted-foreground hover:text-foreground"
                onClick={() => setShowCaptureBar(false)}
              >
                ✕
              </button>
            </div>
            <p className="text-xs text-foreground/80 line-clamp-2">
              {capturedText.slice(0, 200)}{capturedText.length > 200 ? "…" : ""}
            </p>
          </div>
          {/* Action buttons */}
          <div className="flex items-center gap-1 px-2 pb-2">
            {ACTIONS.map(a => (
              <button
                key={a.id}
                className={cn(
                  "flex items-center gap-1 px-3 py-1.5 rounded-md text-xs font-medium transition-all",
                  "bg-background/60 hover:bg-background border border-border/40",
                  a.color,
                )}
                onClick={() => executeAction(a.id, capturedText)}
              >
                {a.icon}
                {a.label}
              </button>
            ))}
          </div>
        </div>
      ) : (
        /* Default header: input + model selector + close */
        <div className="flex items-center gap-2 p-3 border-b border-border/50 bg-card/50">
          <Input
            placeholder="输入问题..."
            value={query}
            onChange={e => setQuery(e.target.value)}
            className="flex-1 h-7 text-sm border-0 bg-transparent focus-visible:ring-0 focus-visible:ring-offset-0"
            onKeyDown={e => e.key === "Enter" && !isStreaming && action && executeAction(action)}
            autoFocus
          />
          {isStreaming ? (
            <Button size="sm" variant="ghost" className="h-7 w-7 p-0 text-destructive" onClick={stopStreaming} title="停止生成">
              <Square className="h-3.5 w-3.5" />
            </Button>
          ) : action && action !== "search" && action !== "copy" ? (
            <Button size="sm" className="h-7 w-7 p-0" onClick={() => executeAction(action)} disabled={isWorking || !query.trim()}>
              {isWorking ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <Search className="h-3.5 w-3.5" />}
            </Button>
          ) : null}
          <Button size="sm" variant="ghost" className="h-7 w-7 p-0" onClick={hideWindow}>
            <X className="h-3.5 w-3.5" />
          </Button>
        </div>
      )}

      {/* Translation language bar */}
      {isTranslateMode && (
        <div className="flex items-center gap-2 px-3 py-1 border-b border-border/20 bg-muted/10 text-xs">
          {translation.detectedLang !== "unknown" && (
            <>
              <Badge variant="outline" className="text-xs h-5 px-1.5">
                {getLanguageByCode(translation.detectedLang)?.emoji} {getLanguageByCode(translation.detectedLang)?.value}
              </Badge>
              <ArrowRightLeft className="h-3 w-3 text-muted-foreground" />
            </>
          )}
          <select
            className="h-5 rounded bg-background border border-border text-xs px-1 flex-1 max-w-40"
            value={targetLang}
            onChange={e => setTargetLang(e.target.value)}
          >
            <option value="">🔄 自动</option>
            {BUILTIN_LANGUAGES.map(l => (
              <option key={l.langCode} value={l.langCode}>{l.emoji} {l.value}</option>
            ))}
          </select>
          <span className="text-muted-foreground whitespace-nowrap">
            智能双向: {getLanguageByCode(translation.preferredLang)?.emoji}↔{getLanguageByCode(translation.alterLang)?.emoji}
          </span>
        </div>
      )}

      {/* Result area */}
      <div className="flex-1 overflow-y-auto p-4">
        {!resultText && !isWorking && !showCaptureBar && (
          <div className="text-center text-muted-foreground text-sm mt-12">
            <Languages className="h-8 w-8 mx-auto mb-3 opacity-20" />
            <p>选中文字自动弹出操作栏</p>
            <p className="text-xs mt-1">Ctrl+Shift+Space 手动唤起</p>
          </div>
        )}
        {isWorking && !resultText && (
          <div className="flex items-center gap-2 text-sm text-muted-foreground mt-8 justify-center">
            <Loader2 className="h-4 w-4 animate-spin text-primary" />
            <span>
              {isTranslateMode ? "翻译中…" :
               action === "explain" ? "解释中…" :
               action === "summarize" ? "总结中…" :
               "生成回答…"}
            </span>
          </div>
        )}
        {resultText && (
          <div className="space-y-3">
            {/* Action badge */}
            {isTranslateMode && translation.detectedLang !== "unknown" && (
              <div className="flex items-center gap-1.5">
                <Languages className="h-3 w-3 text-violet-500" />
                <Badge variant="outline" className="text-xs h-5 bg-violet-500/10 text-violet-500 border-violet-500/30">
                  {getLanguageByCode(translation.detectedLang)?.value} → {getLanguageByCode(targetLang)?.value}
                </Badge>
              </div>
            )}
            {currentActionDef && action !== "translate" && (
              <div className="flex items-center gap-1.5">
                <span className={currentActionDef.color}>{currentActionDef.icon}</span>
                <Badge variant="outline" className={cn("text-xs h-5", currentActionDef.color)}>
                  {currentActionDef.label}
                </Badge>
                {isStreaming && (
                  <span className="text-xs text-muted-foreground animate-pulse">生成中…</span>
                )}
              </div>
            )}
            {/* Markdown rendered result */}
            <div className="text-sm leading-relaxed prose prose-sm max-w-none
              prose-p:my-1 prose-pre:my-2 prose-code:text-xs prose-code:before:content-[''] prose-code:after:content-['']
              prose-headings:my-2 prose-ul:my-1 prose-ol:my-1 prose-li:my-0.5
              prose-pre:bg-muted/50 prose-pre:border prose-pre:border-border/50 prose-pre:rounded-md prose-pre:p-3">
              <ReactMarkdown>{resultText}</ReactMarkdown>
              {isStreaming && (
                <span className="inline-block w-1.5 h-4 bg-primary animate-pulse ml-0.5 align-text-bottom" />
              )}
            </div>
            {sources.length > 0 && (
              <details className="mt-2">
                <summary className="text-xs text-primary cursor-pointer">
                  查看引用来源 ({sources.length})
                </summary>
                <div className="mt-1 space-y-1">
                  {sources.map((s, i) => (
                    <p key={i} className="text-xs text-muted-foreground line-clamp-2">
                      [{i + 1}] {s.content.slice(0, 120)}…
                    </p>
                  ))}
                </div>
              </details>
            )}
          </div>
        )}
        {isTranslateMode && translation.translateError && (
          <div className="text-xs text-destructive bg-destructive/10 rounded px-2 py-1.5 mt-2">
            {friendlyError(translation.translateError)}
          </div>
        )}
      </div>

      {/* Footer: model selector + copy + retry */}
      <div className="flex items-center gap-2 p-2 border-t border-border/30">
        {/* Model dropdown */}
        <select
          className="h-6 rounded bg-background border border-border text-xs px-1.5 flex-shrink-0 max-w-36"
          value={isTranslateMode ? (translation.translateModel || chatModel) : chatModel}
          onChange={e => {
            const v = e.target.value;
            if (isTranslateMode) {
              translation.saveTranslationSettings({ translateModel: v });
            } else {
              setChatModel(v);
            }
          }}
        >
          {availableModels.length === 0 && (
            <option value="">请先配置模型</option>
          )}
          {availableModels.map(m => (
            <option key={m} value={m}>{m}</option>
          ))}
        </select>
        {resultText && (
          <>
            <Button size="sm" variant="ghost" className="h-6 text-xs gap-1 ml-auto" onClick={handleCopy}>
              <Copy className="h-3 w-3" />
              复制
            </Button>
            {!isStreaming && action && action !== "search" && action !== "copy" && (
              <Button size="sm" variant="ghost" className="h-6 text-xs gap-1" onClick={() => executeAction(action)}>
                {isTranslateMode ? <Languages className="h-3 w-3" /> : <Search className="h-3 w-3" />}
                重试
              </Button>
            )}
          </>
        )}
      </div>
    </div>
  );
}

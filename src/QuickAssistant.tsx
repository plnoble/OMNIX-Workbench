/**
 * QuickAssistant — 划词助手浮动窗口
 *
 * 紧凑浮动 UI，圆角毛玻璃风格:
 * - 顶部: 查询输入框 + 动作按钮行
 * - 中部: AI 回答 / 翻译区域（Markdown 实时流式渲染）
 * - 底部: 复制 + 关闭按钮
 *
 * 支持动作: 问答、翻译、解释、总结、精炼
 * 支持流式输出: 逐字增量渲染，实时 Markdown
 * 支持划词动作: Ctrl+Alt+C 捕获后显示动作选择器
 * 全局快捷键 Alt+Space 唤起，ESC 关闭
 */

import { useState, useEffect, useCallback, useRef } from "react";
import { listen } from "@tauri-apps/api/event";
import { register, unregister } from "@tauri-apps/plugin-global-shortcut";
import { readText } from "@tauri-apps/plugin-clipboard-manager";
import { qaApi, settingsApi, knowledgeApi } from "@/lib/tauri-api";
import { useTranslation } from "@/hooks/useTranslation";
import { BUILTIN_LANGUAGES, getLanguageByCode } from "@/lib/translate-constants";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Badge } from "@/components/ui/badge";
import { toast } from "@/components/ui/sonner";
import { cn } from "@/lib/utils";
import ReactMarkdown from "react-markdown";
import {
  Search, Copy, X, Loader2, Sparkles, BookOpen, Brain, Zap,
  Languages, ArrowRightLeft, Lightbulb, FileText, Wand2,
  ClipboardCopy, Square,
} from "lucide-react";
import type { SearchResult, EmbeddingModelInfo } from "@/types";

// ── Action Definitions ─────────────────────────────────

type QAction = "qa" | "translate" | "explain" | "summarize" | "refine";

interface ActionDef {
  id: QAction;
  label: string;
  icon: React.ReactNode;
  color: string;
  promptPrefix?: string;
}

const ACTIONS: ActionDef[] = [
  { id: "qa",        label: "问答",   icon: <Sparkles className="h-3.5 w-3.5" />,   color: "text-cyan-400" },
  { id: "translate", label: "翻译",   icon: <Languages className="h-3.5 w-3.5" />,  color: "text-violet-400" },
  { id: "explain",   label: "解释",   icon: <Lightbulb className="h-3.5 w-3.5" />,  color: "text-amber-400",
    promptPrefix: "Please explain the following text in a clear and detailed way. Use simple language and provide examples where helpful:\n\n" },
  { id: "summarize", label: "总结",   icon: <FileText className="h-3.5 w-3.5" />,   color: "text-emerald-400",
    promptPrefix: "Please provide a concise summary of the following text. Capture the key points and main ideas:\n\n" },
  { id: "refine",    label: "精炼",   icon: <Wand2 className="h-3.5 w-3.5" />,      color: "text-pink-400",
    promptPrefix: "Please refine and improve the following text. Fix grammar, enhance clarity, and make it more polished while preserving the original meaning:\n\n" },
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
  if (msg.includes("No text captured") || msg.includes("clipboard is empty"))
    return "⚠️ 未捕获到文字，请确保有文字被选中";
  if (msg.includes("Failed to open clipboard") || msg.includes("locked by another"))
    return "⚠️ 剪贴板被其他应用锁定，请稍后重试";
  return `❌ 错误: ${msg.slice(0, 120)}`;
}

// ── QuickAssistant Component ───────────────────────────

export function QuickAssistant() {
  const [query, setQuery] = useState("");
  const [answer, setAnswer] = useState("");
  const [sources, setSources] = useState<SearchResult[]>([]);
  const [usedKb, setUsedKb] = useState(false);
  const [isLoading, setIsLoading] = useState(false);
  const [isStreaming, setIsStreaming] = useState(false);
  const [useKb, setUseKb] = useState(true);
  const [chatModel, setChatModel] = useState("deepseek-chat");
  const [embedModel, setEmbedModel] = useState("");
  const [_embeddingModels, setEmbeddingModels] = useState<EmbeddingModelInfo[]>([]);

  // Action mode
  const [action, setAction] = useState<QAction>("qa");
  const translation = useTranslation();
  const [targetLang, setTargetLang] = useState("zh-cn");

  // Clipboard preview & action picker
  const [clipboardPreview, setClipboardPreview] = useState("");
  const [showActionPicker, setShowActionPicker] = useState(false);

  // Stream cleanup ref
  const unlistenStreamRef = useRef<(() => void) | null>(null);

  const isTranslateMode = action === "translate";
  const isQaMode = action === "qa";
  const isActionMode = !isTranslateMode && !isQaMode;
  const currentActionDef = ACTIONS.find(a => a.id === action)!;

  // ── Load settings ────────────────────────────────────

  useEffect(() => {
    (async () => {
      try {
        const [model, useKbStr, embStr] = await Promise.all([
          settingsApi.get("quick_assistant_model"),
          settingsApi.get("quick_assistant_use_kb"),
          settingsApi.get("quick_assistant_embedding_model"),
        ]);
        if (model) setChatModel(model);
        if (useKbStr === "false") setUseKb(false);
        if (embStr) setEmbedModel(embStr);

        const models = await knowledgeApi.getEmbeddingModels();
        setEmbeddingModels(models);
        if (models.length > 0 && !embStr) {
          setEmbedModel(models[0].model_name);
        }

        await translation.loadTranslationSettings();
      } catch (e) {
        console.error("[QA] Failed to load settings:", e);
      }
    })();
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  // ── Register global shortcut ─────────────────────────

  useEffect(() => {
    (async () => {
      try {
        const shortcut = await settingsApi.get("quick_assistant_shortcut") || "Alt+Space";
        await register(shortcut, (event) => {
          if (event.state === "Pressed") {
            toggleWindow();
          }
        });
      } catch (e) {
        console.error("[QA] Failed to register shortcut:", e);
      }
    })();

    return () => {
      (async () => {
        try {
          const shortcut = await settingsApi.get("quick_assistant_shortcut") || "Alt+Space";
          await unregister(shortcut);
        } catch { /* ignore */ }
      })();
    };
  }, []);

  // ── Listen for events ────────────────────────────────

  useEffect(() => {
    const unlistenShown = listen("qa-shown", async () => {
      try {
        const text = await readText();
        if (text && text.trim()) {
          setClipboardPreview(text.trim());
          setShowActionPicker(true);
        }
      } catch (e) {
        console.error("[QA] Failed to read clipboard:", e);
      }
    });

    // When text is captured via Ctrl+Alt+C, show action picker
    const unlistenPreset = listen<string>("qa-preset-text", (event) => {
      setClipboardPreview(event.payload);
      setShowActionPicker(true);
      setQuery(event.payload);
    });

    const unlistenTranslate = listen<string>("qa-translate-text", (event) => {
      setClipboardPreview(event.payload);
      setQuery(event.payload);
      setShowActionPicker(false);
      setAction("translate");
      // Auto-execute translate
      setTimeout(() => executeAction("translate", event.payload), 100);
    });

    return () => {
      unlistenShown.then(fn => fn());
      unlistenPreset.then(fn => fn());
      unlistenTranslate.then(fn => fn());
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

  const toggleWindow = useCallback(async () => {
    try {
      const { getCurrentWindow } = await import("@tauri-apps/api/window");
      const win = getCurrentWindow();
      if (win.label === "quick-assistant") {
        if (await win.isVisible()) {
          await win.hide();
        } else {
          await win.show();
          await win.setFocus();
        }
      }
    } catch (e) {
      console.error("[QA] Toggle failed:", e);
    }
  }, []);

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

  // ── Execute action with streaming ────────────────────

  const executeAction = useCallback(async (overrideAction?: QAction, overrideText?: string) => {
    const activeAction = overrideAction || action;
    const text = (overrideText || query).trim();
    if (!text) return;

    if (activeAction === "translate") {
      setShowActionPicker(false);
      await translation.translate(text, targetLang);
      return;
    }

    // QA, explain, summarize, refine — use streaming
    setShowActionPicker(false);
    setIsLoading(true);
    setIsStreaming(true);
    setAnswer("");
    setSources([]);
    setUsedKb(false);

    try {
      const actionDef = ACTIONS.find(a => a.id === activeAction)!;
      const fullQuery = actionDef.promptPrefix
        ? `${actionDef.promptPrefix}${text}`
        : text;

      // Listen for stream events
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
        if (typeof payload.used_kb === "boolean") {
          setUsedKb(payload.used_kb);
        }
        setIsStreaming(false);
        setIsLoading(false);
        // Cleanup listeners
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

      // Store cleanup ref
      unlistenStreamRef.current = () => {
        unlistenChunk.then(fn => fn());
        unlistenDone.then(fn => fn());
        unlistenError.then(fn => fn());
      };

      // Kick off the streaming command
      await qaApi.queryStream({
        query: fullQuery,
        useKb: activeAction === "qa" ? useKb : false,
        chatModel,
        embeddingModel: useKb && activeAction === "qa" ? embedModel : undefined,
      });
    } catch (e) {
      setAnswer(friendlyError(e));
      setIsStreaming(false);
      setIsLoading(false);
    }
  }, [query, action, useKb, chatModel, embedModel, targetLang, translation]);

  // ── Copy ─────────────────────────────────────────────

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

  // ── Pick action from action picker ───────────────────

  const pickAction = useCallback((pickedAction: QAction) => {
    setAction(pickedAction);
    setShowActionPicker(false);
    // Auto-execute with the clipboard text
    if (clipboardPreview) {
      setQuery(clipboardPreview);
      setTimeout(() => executeAction(pickedAction, clipboardPreview), 50);
    }
  }, [clipboardPreview, executeAction]);

  // ── Render ───────────────────────────────────────────

  const resultText = isTranslateMode ? translation.translatedText : answer;
  const isWorking = isTranslateMode ? translation.isTranslating : isLoading;

  return (
    <div className="flex flex-col h-screen bg-background/95 backdrop-blur-xl border border-border/50 rounded-xl shadow-2xl overflow-hidden">
      {/* Header */}
      <div className="flex items-center gap-2 p-3 border-b border-border/50 bg-card/50">
        <span className={cn("flex-shrink-0", currentActionDef.color)}>
          {currentActionDef.icon}
        </span>
        <Input
          placeholder={
            isTranslateMode ? "输入要翻译的文本..." :
            isActionMode ? `输入要${currentActionDef.label}的文本...` :
            "输入问题或选中文字..."
          }
          value={query}
          onChange={e => setQuery(e.target.value)}
          className="flex-1 h-7 text-sm border-0 bg-transparent focus-visible:ring-0 focus-visible:ring-offset-0"
          onKeyDown={e => e.key === "Enter" && !isStreaming && executeAction()}
          autoFocus
        />
        {isStreaming ? (
          <Button size="sm" variant="ghost" className="h-7 w-7 p-0 text-destructive" onClick={stopStreaming} title="停止生成">
            <Square className="h-3.5 w-3.5" />
          </Button>
        ) : (
          <Button size="sm" className="h-7 w-7 p-0" onClick={() => executeAction()} disabled={isWorking || !query.trim()}>
            {isWorking ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <Search className="h-3.5 w-3.5" />}
          </Button>
        )}
        <Button size="sm" variant="ghost" className="h-7 w-7 p-0" onClick={hideWindow}>
          <X className="h-3.5 w-3.5" />
        </Button>
      </div>

      {/* Action bar — horizontal action buttons */}
      <div className="flex items-center gap-1 px-3 py-1.5 border-b border-border/30 bg-muted/20">
        {ACTIONS.map(a => (
          <button
            key={a.id}
            className={cn(
              "flex items-center gap-1 px-2 py-1 rounded text-[10px] transition-colors",
              action === a.id
                ? cn("bg-white/10", a.color)
                : "text-muted-foreground hover:bg-white/5",
            )}
            onClick={() => { setAction(a.id); if (query.trim()) executeAction(a.id); }}
          >
            {a.icon}
            <span className="hidden sm:inline">{a.label}</span>
          </button>
        ))}

        {/* Model selector (compact) */}
        <span className="text-muted-foreground ml-auto text-[10px]">模型:</span>
        {isTranslateMode ? (
          <Input
            value={translation.translateModel}
            onChange={e => translation.saveTranslationSettings({ translateModel: e.target.value })}
            className="h-5 w-20 text-[10px] border-0 bg-transparent px-1"
            placeholder="默认"
          />
        ) : (
          <Input
            value={chatModel}
            onChange={e => setChatModel(e.target.value)}
            className="h-5 w-20 text-[10px] border-0 bg-transparent px-1"
          />
        )}
        {isQaMode && (
          <button
            className={cn(
              "flex items-center gap-1 px-1.5 py-0.5 rounded transition-colors text-[10px]",
              useKb ? "bg-cyan-500/20 text-cyan-400" : "bg-muted text-muted-foreground",
            )}
            onClick={() => setUseKb(!useKb)}
          >
            <BookOpen className="h-3 w-3" />
            KB
          </button>
        )}
      </div>

      {/* Translation sub-bar */}
      {isTranslateMode && (
        <div className="flex items-center gap-2 px-3 py-1 border-b border-border/20 bg-muted/10 text-[10px]">
          {translation.detectedLang !== "unknown" && (
            <>
              <Badge variant="outline" className="text-[9px] h-5 px-1.5">
                {getLanguageByCode(translation.detectedLang)?.emoji} {getLanguageByCode(translation.detectedLang)?.value}
              </Badge>
              <ArrowRightLeft className="h-3 w-3 text-muted-foreground" />
            </>
          )}
          <select
            className="h-5 rounded bg-background border border-border text-[10px] px-1 flex-1 max-w-40"
            value={targetLang}
            onChange={e => setTargetLang(e.target.value)}
          >
            {BUILTIN_LANGUAGES.map(l => (
              <option key={l.langCode} value={l.langCode}>{l.emoji} {l.value}</option>
            ))}
          </select>
        </div>
      )}

      {/* ── Action Picker (shown after text capture) ── */}
      {showActionPicker && clipboardPreview && !resultText && !isWorking && (
        <div className="mx-3 mt-3 border border-border/50 rounded-lg bg-muted/20 overflow-hidden">
          {/* Preview of captured text */}
          <div className="px-3 py-2 border-b border-border/30">
            <div className="flex items-center gap-1.5 mb-1">
              <ClipboardCopy className="h-3 w-3 text-muted-foreground" />
              <span className="text-[10px] text-muted-foreground">已捕获文字</span>
              <button
                className="ml-auto text-[9px] text-muted-foreground hover:text-foreground"
                onClick={() => setShowActionPicker(false)}
              >
                ✕
              </button>
            </div>
            <p className="text-xs text-foreground/80 line-clamp-3">
              {clipboardPreview.slice(0, 300)}{clipboardPreview.length > 300 ? "…" : ""}
            </p>
          </div>
          {/* Action buttons */}
          <div className="flex items-center gap-1 px-2 py-2">
            <span className="text-[10px] text-muted-foreground mr-1">选择动作:</span>
            {ACTIONS.map(a => (
              <button
                key={a.id}
                className={cn(
                  "flex items-center gap-1 px-2.5 py-1.5 rounded-md text-[11px] font-medium transition-all",
                  "bg-white/5 hover:bg-white/10 border border-transparent hover:border-border/50",
                  a.color,
                )}
                onClick={() => pickAction(a.id)}
              >
                {a.icon}
                {a.label}
              </button>
            ))}
          </div>
        </div>
      )}

      {/* Result area */}
      <div className="flex-1 overflow-y-auto p-4">
        {!resultText && !isWorking && !showActionPicker && (
          <div className="text-center text-muted-foreground text-sm mt-12">
            <Zap className="h-8 w-8 mx-auto mb-3 opacity-20" />
            <p>按 Alt+Space 唤起</p>
            <p className="text-xs mt-1">Ctrl+Alt+C 划词捕获</p>
            <div className="flex items-center justify-center gap-3 mt-4 text-[10px]">
              {ACTIONS.map(a => (
                <span key={a.id} className={cn("cursor-pointer hover:underline", a.color)} onClick={() => setAction(a.id)}>
                  {a.label}
                </span>
              ))}
            </div>
          </div>
        )}
        {isWorking && !resultText && (
          <div className="flex items-center gap-2 text-sm text-muted-foreground mt-8 justify-center">
            <Loader2 className="h-4 w-4 animate-spin text-cyan-400" />
            <span>
              {isTranslateMode ? "翻译中…" :
               action === "explain" ? "解释中…" :
               action === "summarize" ? "总结中…" :
               action === "refine" ? "精炼中…" :
               (useKb ? "检索知识库并生成回答…" : "生成回答…")}
            </span>
          </div>
        )}
        {resultText && (
          <div className="space-y-3">
            {/* Action badge */}
            {isQaMode && usedKb && (
              <div className="flex items-center gap-1.5">
                <Brain className="h-3 w-3 text-cyan-400" />
                <Badge variant="outline" className="text-[10px] h-5 bg-cyan-500/10 text-cyan-400 border-cyan-500/30">
                  知识库增强
                </Badge>
              </div>
            )}
            {isTranslateMode && translation.detectedLang !== "unknown" && (
              <div className="flex items-center gap-1.5">
                <Languages className="h-3 w-3 text-violet-400" />
                <Badge variant="outline" className="text-[10px] h-5 bg-violet-500/10 text-violet-400 border-violet-500/30">
                  {getLanguageByCode(translation.detectedLang)?.value} → {getLanguageByCode(targetLang)?.value}
                </Badge>
              </div>
            )}
            {isActionMode && (
              <div className="flex items-center gap-1.5">
                <span className={currentActionDef.color}>{currentActionDef.icon}</span>
                <Badge variant="outline" className={cn("text-[10px] h-5", currentActionDef.color)}>
                  {currentActionDef.label}
                </Badge>
                {isStreaming && (
                  <span className="text-[9px] text-muted-foreground animate-pulse">生成中…</span>
                )}
              </div>
            )}
            {/* Markdown rendered result with streaming cursor */}
            <div className="text-sm leading-relaxed prose prose-invert prose-sm max-w-none
              prose-p:my-1 prose-pre:my-2 prose-code:text-xs prose-code:before:content-[''] prose-code:after:content-['']
              prose-headings:my-2 prose-ul:my-1 prose-ol:my-1 prose-li:my-0.5
              prose-pre:bg-muted/50 prose-pre:border prose-pre:border-border/50 prose-pre:rounded-md prose-pre:p-3">
              <ReactMarkdown>{resultText}</ReactMarkdown>
              {/* Streaming cursor */}
              {isStreaming && (
                <span className="inline-block w-1.5 h-4 bg-cyan-400 animate-pulse ml-0.5 align-text-bottom" />
              )}
            </div>
            {isQaMode && sources.length > 0 && (
              <details className="mt-2">
                <summary className="text-[10px] text-cyan-400 cursor-pointer">
                  查看引用来源 ({sources.length})
                </summary>
                <div className="mt-1 space-y-1">
                  {sources.map((s, i) => (
                    <p key={i} className="text-[10px] text-muted-foreground line-clamp-2">
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

      {/* Footer */}
      {resultText && (
        <div className="flex items-center gap-2 p-2 border-t border-border/30">
          <Button size="sm" variant="ghost" className="h-6 text-[10px] gap-1" onClick={handleCopy}>
            <Copy className="h-3 w-3" />
            复制
          </Button>
          {!isStreaming && (
            <Button size="sm" variant="ghost" className="h-6 text-[10px] gap-1" onClick={() => executeAction()}>
              {isTranslateMode ? <Languages className="h-3 w-3" /> : <Search className="h-3 w-3" />}
              {isTranslateMode ? "重新翻译" : `重新${currentActionDef.label}`}
            </Button>
          )}
        </div>
      )}
    </div>
  );
}

/**
 * TranslateTab — dedicated multi-language translation page (R5+, Google/有道
 * style) backed by the app's configured AI models. Two panes (source ↔ target),
 * language pickers with swap, model selector, copy, and recent history.
 */
import { useCallback, useEffect, useState } from "react";
import { Languages, ArrowLeftRight, Copy, Loader2, Trash2, Star } from "lucide-react";

import { useTranslation } from "@/hooks/useTranslation";
import { BUILTIN_LANGUAGES, getLanguageByCode } from "@/lib/translate-constants";
import { Button } from "@/components/ui/button";
import { TranslationHistoryPanel } from "@/components/TranslationHistoryPanel";
import { toast } from "@/components/ui/sonner";

const AUTO = "auto";

export function TranslateTab() {
  // 这一页曾经自己重写了一遍翻译逻辑，而且写漏了三处：不调用语言检测（"检测：X"
  // 其实是把你选的值原样回显）、无视「翻译模型」设置（只读 target_model）、
  // 源语言压根没进提示词。`useTranslation` 早就把这些做对了——用它，别再写第二套。
  const translation = useTranslation();

  const [source, setSource] = useState("");
  const [sourceLang, setSourceLang] = useState(AUTO);
  const [targetLang, setTargetLang] = useState("en-us");
  const [historyKey, setHistoryKey] = useState(0);

  useEffect(() => {
    void translation.loadTranslationSettings();
  }, []); // eslint-disable-line react-hooks/exhaustive-deps -- mount-only

  const result = translation.translatedText;
  const busy = translation.isTranslating;
  // 自动检测时才显示检测结果；用户自己选了源语言就没什么可"检测"的。
  const detected = sourceLang === AUTO ? translation.detectedLang : "";

  const translate = useCallback(async () => {
    if (!source.trim()) return;
    const text = await translation.translate(
      source.trim(),
      targetLang,
      sourceLang === AUTO ? undefined : sourceLang,
    );
    if (text === null) {
      toast.error("翻译失败", { description: translation.translateError ?? "未知错误" });
      return;
    }
    setHistoryKey((k) => k + 1);
  }, [source, targetLang, sourceLang, translation]);

  const swap = () => {
    // Swap languages; if source was auto, use the detected language.
    const newSource = sourceLang === AUTO ? (detected && detected !== "unknown" ? detected : targetLang) : sourceLang;
    setSourceLang(targetLang);
    setTargetLang(newSource);
    // Move the result up into the source box for round-tripping.
    if (result) setSource(result);
  };

  const copy = () => {
    if (!result) return;
    navigator.clipboard.writeText(result).then(
      () => toast.success("已复制译文"),
      () => toast.error("复制失败"),
    );
  };

  return (
    <div className="flex h-full w-full flex-col overflow-hidden">
      <div className="flex items-center gap-2 border-b border-border px-4 py-3">
        <Languages className="h-4 w-4" />
        <span className="text-sm font-semibold">翻译</span>
        <span className="ml-auto text-xs text-muted-foreground">使用「设置 → 内置功能默认模型」</span>
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto p-4">
        {/* Language bar */}
        <div className="mb-3 flex items-center gap-2">
          <select
            className="h-9 flex-1 rounded-md border border-border bg-background px-2 text-sm"
            value={sourceLang}
            onChange={(e) => setSourceLang(e.target.value)}
          >
            <option value={AUTO}>🌐 自动检测{detected && detected !== "unknown" && sourceLang === AUTO ? `（${getLanguageByCode(detected)?.value ?? detected}）` : ""}</option>
            {BUILTIN_LANGUAGES.map((l) => <option key={l.langCode} value={l.langCode}>{l.emoji} {l.value}</option>)}
          </select>
          <button onClick={swap} title="交换语言" className="rounded-md border border-border p-2 text-muted-foreground hover:bg-muted/30 hover:text-foreground">
            <ArrowLeftRight className="h-4 w-4" />
          </button>
          <select
            className="h-9 flex-1 rounded-md border border-border bg-background px-2 text-sm"
            value={targetLang}
            onChange={(e) => setTargetLang(e.target.value)}
          >
            {BUILTIN_LANGUAGES.map((l) => <option key={l.langCode} value={l.langCode}>{l.emoji} {l.value}</option>)}
          </select>
        </div>

        {/* Two panes */}
        <div className="grid gap-3 lg:grid-cols-2">
          <div className="flex flex-col rounded-lg border border-border glass-surface">
            <textarea
              value={source}
              onChange={(e) => setSource(e.target.value)}
              onKeyDown={(e) => { if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) { e.preventDefault(); void translate(); } }}
              placeholder="输入要翻译的文字…（Ctrl+Enter 翻译）"
              className="min-h-[240px] flex-1 resize-none rounded-lg bg-transparent p-3 text-sm leading-6 outline-none"
            />
            <div className="flex items-center justify-between border-t border-border px-3 py-1.5">
              <span className="text-[11px] text-muted-foreground">{source.length} 字符</span>
              <Button size="sm" className="h-7 gap-1" onClick={() => void translate()} disabled={busy || !source.trim()}>
                {busy ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <Languages className="h-3.5 w-3.5" />} 翻译
              </Button>
            </div>
          </div>

          <div className="flex flex-col rounded-lg border border-border glass-surface">
            <div className="min-h-[240px] flex-1 whitespace-pre-wrap p-3 text-sm leading-6">
              {result ? result : <span className="text-muted-foreground">译文显示在这里</span>}
            </div>
            <div className="flex items-center justify-between border-t border-border px-3 py-1.5">
              <span className="text-[11px] text-muted-foreground">
                {detected && detected !== "unknown" ? `检测：${getLanguageByCode(detected)?.value ?? detected} → ${getLanguageByCode(targetLang)?.value ?? targetLang}` : ""}
              </span>
              <div className="flex gap-1">
                <button onClick={() => setSource("")} title="清空" className="rounded p-1.5 text-muted-foreground hover:bg-muted/30 hover:text-foreground">
                  <Trash2 className="h-3.5 w-3.5" />
                </button>
                <button onClick={copy} disabled={!result} title="复制译文" className="rounded p-1.5 text-muted-foreground hover:bg-muted/30 hover:text-foreground disabled:opacity-40">
                  <Copy className="h-3.5 w-3.5" />
                </button>
              </div>
            </div>
          </div>
        </div>

        <p className="mt-2 flex items-center gap-1 text-[11px] text-muted-foreground">
          <Star className="h-3 w-3" /> 翻译使用「设置 → 内置功能默认模型」，无需在此单独选模型；译文质量取决于该模型。
        </p>

        <div className="mt-4">
          <TranslationHistoryPanel key={historyKey} />
        </div>
      </div>
    </div>
  );
}

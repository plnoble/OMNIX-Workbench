import { useEffect, useState } from "react";
import { usePlatformsStore, useSelectionStore, useTranslationStore } from "@/store/AppStore";
import { Clipboard, Languages, MousePointerClick, ShieldOff, Trash2 } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import { Textarea } from "@/components/ui/textarea";
import { BUILTIN_LANGUAGES } from "@/lib/translate-constants";
import { QuickActionsEditor } from "@/components/QuickActionsEditor";
import { TranslationHistoryPanel } from "@/components/TranslationHistoryPanel";

export function QuickAssistantTab() {
  // 以前是 28 个 prop 横跨 selection / translation / platforms 三个 hook。
  const selection = useSelectionStore();
  const translation = useTranslationStore();
  const platforms = usePlatformsStore();
  const {
    captureMode, showOnCapture, preserveClipboard, autoCaptureEnabled,
    blacklist, isCapturing, lastCapture, captureError,
    selectionHistory: history,
    captureTextOnly: onTestCapture,
    loadHistory: onLoadHistory,
    clearHistory: onClearHistory,
  } = selection;
  const { preferredLang, alterLang, translateModel, customPrompt, autoDetect } = translation;
  const availableModels = platforms.activeModels.map(
    (model) => `${model.platform_id}:${model.model_name}`,
  );
  const onSetCaptureMode = (value: string) =>
    selection.saveSelectionSettings({ captureMode: value as "hybrid" | "uia_only" | "clipboard_only" });
  const onSetShowOnCapture = (value: boolean) => selection.saveSelectionSettings({ showOnCapture: value });
  const onSetPreserveClipboard = (value: boolean) => selection.saveSelectionSettings({ preserveClipboard: value });
  const onSetAutoCaptureEnabled = (value: boolean) => selection.saveSelectionSettings({ autoCaptureEnabled: value });
  const onSetBlacklist = (value: string[]) => selection.saveSelectionSettings({ blacklist: value });
  const onSetPreferredLang = (value: string) => translation.saveTranslationSettings({ preferredLang: value });
  const onSetAlterLang = (value: string) => translation.saveTranslationSettings({ alterLang: value });
  const onSetTranslateModel = (value: string) => translation.saveTranslationSettings({ translateModel: value });
  const onSetCustomPrompt = (value: string) => translation.saveTranslationSettings({ customPrompt: value });
  const onSetAutoDetect = (value: boolean) => translation.saveTranslationSettings({ autoDetect: value });
  const [testing, setTesting] = useState(false);

  useEffect(() => {
    onLoadHistory().catch(() => undefined);
  }, [onLoadHistory]);

  const testCapture = async () => {
    setTesting(true);
    try {
      await onTestCapture();
    } finally {
      setTesting(false);
    }
  };

  return (
    <div className="flex h-full flex-1 overflow-hidden bg-background">
      <section className="min-w-0 flex-1 overflow-y-auto p-6">
        <div className="mb-6 flex flex-wrap items-start justify-between gap-4">
          <div>
            <div className="flex items-center gap-2 text-lg font-semibold">
              <MousePointerClick className="h-5 w-5 text-primary" />
              快捷助手
            </div>
            <p className="mt-1 max-w-3xl text-sm leading-6 text-muted-foreground">
              划词后执行翻译、解释、总结、润色、搜索或复制。Windows 体验优先，macOS/Linux 先作为 Labs。
            </p>
          </div>
          <div className="rounded-md border border-warning/30 bg-warning/10 px-3 py-2 text-xs text-warning">
            <ShieldOff className="mb-1 h-3.5 w-3.5" />
            默认保守触发，避免误捕获敏感应用
          </div>
        </div>

        <div className="grid grid-cols-1 gap-4 xl:grid-cols-2">
          <div className="rounded-md border border-border glass-surface p-4">
            <div className="mb-4 flex items-center gap-2 text-sm font-semibold">
              <Clipboard className="h-4 w-4" />
              捕获设置
            </div>
            <div className="space-y-4">
              <label className="block">
                <span className="mb-1 block text-xs text-muted-foreground">捕获模式</span>
                <select
                  value={captureMode}
                  onChange={(event) => onSetCaptureMode(event.target.value)}
                  className="h-9 w-full rounded-md border border-border bg-background px-2 text-sm"
                >
                  <option value="hybrid">混合模式</option>
                  <option value="uia_only">Windows UIA</option>
                  <option value="clipboard_only">剪贴板</option>
                </select>
              </label>
              <div className="flex items-center justify-between gap-3">
                <span className="text-sm">捕获后显示助手窗口</span>
                <Switch checked={showOnCapture} onCheckedChange={onSetShowOnCapture} />
              </div>
              <div className="flex items-center justify-between gap-3">
                <span className="text-sm">尽量保留原剪贴板</span>
                <Switch checked={preserveClipboard} onCheckedChange={onSetPreserveClipboard} />
              </div>
              <div className="flex items-center justify-between gap-3">
                <span className="text-sm">自动监听划词</span>
                <Switch checked={autoCaptureEnabled} onCheckedChange={onSetAutoCaptureEnabled} />
              </div>
              <label className="block">
                <span className="mb-1 block text-xs text-muted-foreground">应用黑名单（每行一个进程或窗口关键词）</span>
                <Textarea
                  value={blacklist.join("\n")}
                  onChange={(event) => onSetBlacklist(event.target.value.split(/\r?\n/))}
                  className="min-h-24 text-xs"
                />
              </label>
              <Button onClick={testCapture} disabled={testing || isCapturing}>
                {testing || isCapturing ? "捕获中" : "测试捕获"}
              </Button>
              {captureError && <div className="rounded-md border border-destructive/30 bg-destructive/10 p-3 text-sm text-destructive">{captureError}</div>}
              {lastCapture && (
                <div className="rounded-md border border-border bg-background/50 p-3">
                  <div className="mb-1 text-xs text-muted-foreground">最近捕获</div>
                  <div className="max-h-28 overflow-y-auto whitespace-pre-wrap text-sm">{lastCapture}</div>
                </div>
              )}
            </div>
          </div>

          <div className="rounded-md border border-border glass-surface p-4">
            <div className="mb-4 flex items-center gap-2 text-sm font-semibold">
              <Languages className="h-4 w-4" />
              翻译与动作
            </div>
            <div className="grid grid-cols-1 gap-3 md:grid-cols-2">
              <label className="block">
                <span className="mb-1 block text-xs text-muted-foreground">主要语言</span>
                <select value={preferredLang} onChange={(event) => onSetPreferredLang(event.target.value)} className="h-9 w-full rounded-md border border-border bg-background px-2 text-sm">
                  {BUILTIN_LANGUAGES.map((lang) => <option key={lang.langCode} value={lang.langCode}>{lang.emoji} {lang.value}</option>)}
                </select>
              </label>
              <label className="block">
                <span className="mb-1 block text-xs text-muted-foreground">备用语言</span>
                <select value={alterLang} onChange={(event) => onSetAlterLang(event.target.value)} className="h-9 w-full rounded-md border border-border bg-background px-2 text-sm">
                  {BUILTIN_LANGUAGES.map((lang) => <option key={lang.langCode} value={lang.langCode}>{lang.emoji} {lang.value}</option>)}
                </select>
              </label>
              <label className="block md:col-span-2">
                <span className="mb-1 block text-xs text-muted-foreground">翻译模型</span>
                <select value={translateModel} onChange={(event) => onSetTranslateModel(event.target.value)} className="h-9 w-full rounded-md border border-border bg-background px-2 text-sm">
                  <option value="">跟随默认模型</option>
                  {availableModels.map((model) => <option key={model} value={model}>{model}</option>)}
                </select>
              </label>
              <div className="flex items-center justify-between gap-3 md:col-span-2">
                <span className="text-sm">自动检测源语言</span>
                <Switch checked={autoDetect} onCheckedChange={onSetAutoDetect} />
              </div>
              <label className="block md:col-span-2">
                <span className="mb-1 block text-xs text-muted-foreground">自定义提示词</span>
                <Textarea value={customPrompt} onChange={(event) => onSetCustomPrompt(event.target.value)} className="min-h-24" />
              </label>
            </div>
          </div>

          <QuickActionsEditor />

          <TranslationHistoryPanel />
        </div>
      </section>

      <aside className="hidden w-96 shrink-0 border-l border-border glass-surface p-5 xl:block">
        <div className="mb-4 flex items-center justify-between gap-3">
          <div>
            <div className="text-sm font-semibold">捕获历史</div>
            <p className="mt-1 text-xs text-muted-foreground">用于回看最近的划词内容。</p>
          </div>
          <Button size="sm" variant="outline" onClick={onClearHistory}>
            <Trash2 className="h-3.5 w-3.5" />
            清空
          </Button>
        </div>
        <div className="space-y-2 overflow-y-auto">
          {history.length === 0 ? (
            <div className="rounded-md border border-dashed border-border p-4 text-sm text-muted-foreground">还没有历史记录。</div>
          ) : history.slice(0, 20).map((item) => (
            <div key={item.id} className="rounded-md border border-border bg-background/50 p-3">
              <div className="line-clamp-3 whitespace-pre-wrap text-sm">{item.captured_text}</div>
              <div className="mt-2 text-xs text-muted-foreground">{item.process_name || item.window_title || item.source} · {item.created_at}</div>
            </div>
          ))}
        </div>
      </aside>
    </div>
  );
}

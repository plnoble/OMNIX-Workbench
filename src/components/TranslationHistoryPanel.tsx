import { useEffect } from "react";
import { Languages, Copy, Trash2, Trash } from "lucide-react";

import { useTranslation } from "@/hooks/useTranslation";
import { getLanguageByCode } from "@/lib/translate-constants";
import { Button } from "@/components/ui/button";
import { toast } from "@/components/ui/sonner";

/**
 * TranslationHistoryPanel — surfaces the translation history that the backend
 * already records (`get_translation_history`) but no UI showed (R5 翻译 polish).
 * Each entry can be copied or removed; the list can be cleared.
 */
export function TranslationHistoryPanel() {
  const t = useTranslation();

  useEffect(() => {
    void t.loadHistory();
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  const copy = (text: string) => {
    navigator.clipboard.writeText(text).then(
      () => toast.success("已复制译文"),
      () => toast.error("复制失败"),
    );
  };

  return (
    <div className="rounded-md border border-border bg-card/40 p-4">
      <div className="mb-3 flex items-center justify-between">
        <div className="flex items-center gap-2 text-sm font-semibold">
          <Languages className="h-4 w-4" /> 翻译历史 {t.translationHistory.length > 0 && `(${t.translationHistory.length})`}
        </div>
        {t.translationHistory.length > 0 && (
          <Button size="sm" variant="outline" onClick={() => void t.clearHistory()}>
            <Trash className="h-3.5 w-3.5" /> 清空
          </Button>
        )}
      </div>

      {t.translationHistory.length === 0 ? (
        <div className="rounded border border-dashed border-border px-3 py-4 text-center text-xs text-muted-foreground">
          还没有翻译记录。在划词助手里翻译后会出现在这里。
        </div>
      ) : (
        <div className="flex max-h-80 flex-col gap-2 overflow-y-auto">
          {t.translationHistory.map((entry) => {
            const src = getLanguageByCode(entry.source_lang);
            const tgt = getLanguageByCode(entry.target_lang);
            return (
              <div key={entry.id} className="rounded-lg border border-border px-3 py-2">
                <div className="mb-1 flex items-center gap-2 text-[11px] text-muted-foreground">
                  <span>{src?.emoji} {src?.value ?? entry.source_lang}</span>
                  <span>→</span>
                  <span>{tgt?.emoji} {tgt?.value ?? entry.target_lang}</span>
                  <span className="ml-auto">{new Date(entry.created_at + "Z").toLocaleString()}</span>
                  <button onClick={() => copy(entry.target_text)} title="复制译文" className="rounded p-0.5 hover:bg-muted/30 hover:text-foreground">
                    <Copy className="h-3 w-3" />
                  </button>
                  <button onClick={() => void t.deleteHistoryItem(entry.id)} title="删除" className="rounded p-0.5 hover:bg-destructive/10 hover:text-destructive">
                    <Trash2 className="h-3 w-3" />
                  </button>
                </div>
                <div className="line-clamp-2 text-xs text-muted-foreground">{entry.source_text}</div>
                <div className="mt-0.5 line-clamp-3 text-sm text-foreground">{entry.target_text}</div>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}

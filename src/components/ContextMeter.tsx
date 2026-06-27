import { useCallback, useEffect, useState } from "react";
import { Gauge, Scissors, Loader2 } from "lucide-react";

import { contextBudgetApi, contextCompactApi, type ContextBudget } from "@/lib/tauri-api";
import { contextWindowFor } from "@/lib/constants";
import { toast } from "@/components/ui/sonner";

/**
 * ContextMeter — accurate context-window budget over the OMNIX-stored
 * conversation transcript (R4 (a)). OMNIX owns the `messages` table (what it
 * replays on resume), so this count is precise for that transcript; the live
 * CLI session may compact independently. Shows used/window tokens, a colored
 * fill, and a 压缩 action that summarizes old messages when the transcript grows
 * large. Hidden until the conversation actually has content.
 */
interface Props {
  conversationId: string;
  modelName?: string | null;
  /** Bump to refetch (e.g. after each turn). */
  refreshSignal: number;
  /** Called after a successful compaction so the parent can reload messages. */
  onCompacted?: () => void;
}

function fmt(n: number): string {
  if (n >= 1000) return `${(n / 1000).toFixed(n >= 10000 ? 0 : 1)}K`;
  return `${n}`;
}

const STATUS_FILL: Record<ContextBudget["status"], string> = {
  ok: "bg-gradient-to-r from-cyan-500 to-blue-500",
  warning: "bg-gradient-to-r from-amber-500 to-orange-500",
  critical: "bg-gradient-to-r from-orange-500 to-red-500",
};

export function ContextMeter({ conversationId, modelName, refreshSignal, onCompacted }: Props) {
  const [budget, setBudget] = useState<ContextBudget | null>(null);
  const [compacting, setCompacting] = useState(false);

  const load = useCallback(async () => {
    if (!conversationId) {
      setBudget(null);
      return;
    }
    try {
      const b = await contextBudgetApi.getBudget(conversationId, contextWindowFor(modelName));
      setBudget(b);
    } catch {
      setBudget(null);
    }
  }, [conversationId, modelName]);

  useEffect(() => {
    void load();
  }, [load, refreshSignal]);

  const compact = async () => {
    if (!window.confirm("压缩较早的对话：把旧消息汇总成一条摘要，仅保留最近 20 条。OMNIX 存储的对话会被改写，不可撤销。继续？")) return;
    setCompacting(true);
    try {
      const result = await contextCompactApi.compact(conversationId, 20);
      if (result.compacted > 0) {
        toast.success(`已压缩 ${result.compacted} 条旧消息为摘要`);
        onCompacted?.();
      } else {
        toast.info(result.message || "消息较少，无需压缩");
      }
      await load();
    } catch (e) {
      toast.error("压缩失败", { description: String(e) });
    } finally {
      setCompacting(false);
    }
  };

  // Hide for empty/brand-new conversations — nothing useful to show yet.
  if (!budget || budget.message_count === 0 || budget.estimated_tokens <= 0) return null;

  const pct = Math.min(100, budget.usage_percent);
  const color = budget.status === "critical" ? "text-red-400" : budget.status === "warning" ? "text-amber-400" : "text-muted-foreground";

  return (
    <div className="flex items-center gap-2" title={`OMNIX 存储的对话约 ${budget.estimated_tokens.toLocaleString()} tokens / ${budget.model_limit.toLocaleString()} 上下文窗口（${budget.message_count} 条消息）。这是 OMNIX 转录的精确估算；实际由所选模型/CLI 管理。`}>
      <Gauge className={`h-3.5 w-3.5 shrink-0 ${color}`} />
      <div className="h-1.5 w-24 overflow-hidden rounded-full bg-muted/25">
        <div className={`h-full rounded-full ${STATUS_FILL[budget.status]}`} style={{ width: `${Math.max(2, pct)}%` }} />
      </div>
      <span className={`shrink-0 text-[11px] tabular-nums ${color}`}>
        {fmt(budget.estimated_tokens)}/{fmt(budget.model_limit)}
      </span>
      {budget.status !== "ok" && (
        <button
          onClick={() => void compact()}
          disabled={compacting}
          title="压缩较早的对话以释放上下文"
          className="flex shrink-0 items-center gap-1 rounded px-1.5 py-0.5 text-[11px] text-muted-foreground hover:bg-muted/30 hover:text-foreground disabled:opacity-50"
        >
          {compacting ? <Loader2 className="h-3 w-3 animate-spin" /> : <Scissors className="h-3 w-3" />} 压缩
        </button>
      )}
    </div>
  );
}

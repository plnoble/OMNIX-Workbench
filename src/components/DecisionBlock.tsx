/**
 * DecisionBlock — renders an `omnix-decision` block as selectable option cards
 * (单选/多选)。选择后回注为下一条消息，agent 基于选择继续。
 */
import { useState } from "react";
import { Check, CircleHelp, Send, ThumbsUp } from "lucide-react";

import { cn } from "@/lib/utils";
import type { DecisionSpec } from "@/lib/decisionBlock";

interface Props {
  spec: DecisionSpec;
  /** Absent when sending isn't possible in this context (e.g. history view). */
  onDecide?: (chosen: string[], note: string) => void;
}

export function DecisionBlock({ spec, onDecide }: Props) {
  const [chosen, setChosen] = useState<Set<string>>(new Set());
  const [note, setNote] = useState("");
  const [submitted, setSubmitted] = useState<string[] | null>(null);

  const toggle = (label: string) => {
    if (submitted) return;
    setChosen((prev) => {
      const next = new Set(prev);
      if (spec.multi) {
        if (next.has(label)) next.delete(label);
        else next.add(label);
      } else {
        next.clear();
        next.add(label);
      }
      return next;
    });
  };

  const submit = () => {
    if (chosen.size === 0 || !onDecide) return;
    const picks = spec.options.map((o) => o.label).filter((l) => chosen.has(l));
    setSubmitted(picks);
    onDecide(picks, note);
  };

  return (
    <div className="my-2 rounded-lg border border-primary/30 bg-primary/5 p-3">
      <div className="flex items-center gap-2 text-sm font-semibold">
        <CircleHelp className="h-4 w-4 shrink-0 text-primary" />
        {spec.question}
        <span className="ml-auto shrink-0 text-[10px] font-normal text-muted-foreground">
          {spec.multi ? "可多选" : "单选"}
        </span>
      </div>
      <div className="mt-2 flex flex-col gap-1.5">
        {spec.options.map((opt) => {
          const active = submitted ? submitted.includes(opt.label) : chosen.has(opt.label);
          return (
            <button
              key={opt.label}
              onClick={() => toggle(opt.label)}
              disabled={!!submitted}
              className={cn(
                "flex items-start gap-2.5 rounded-md border p-2.5 text-left transition",
                active
                  ? "border-primary bg-primary/10"
                  : "border-border bg-background/60 hover:border-primary/50",
                submitted && !active && "opacity-50",
              )}
            >
              <span
                className={cn(
                  "mt-0.5 flex h-4 w-4 shrink-0 items-center justify-center border",
                  spec.multi ? "rounded" : "rounded-full",
                  active ? "border-primary bg-primary text-primary-foreground" : "border-muted-foreground/40",
                )}
              >
                {active && <Check className="h-3 w-3" />}
              </span>
              <span className="min-w-0">
                <span className="flex items-center gap-1.5 text-sm font-medium">
                  {opt.label}
                  {opt.recommended && (
                    <span className="inline-flex items-center gap-0.5 rounded bg-success/15 px-1 py-0.5 text-[10px] font-normal text-success">
                      <ThumbsUp className="h-2.5 w-2.5" /> 推荐
                    </span>
                  )}
                </span>
                {opt.description && (
                  <span className="mt-0.5 block text-xs leading-5 text-muted-foreground">
                    {opt.description}
                  </span>
                )}
              </span>
            </button>
          );
        })}
      </div>
      {submitted ? (
        <div className="mt-2 text-xs text-success">✅ 已选择：{submitted.join("、")}</div>
      ) : onDecide ? (
        <div className="mt-2 flex items-center gap-2">
          <input
            value={note}
            onChange={(e) => setNote(e.target.value)}
            placeholder="（可选）补充说明…"
            className="h-8 flex-1 rounded-md border border-border bg-background px-2 text-xs outline-none focus:border-primary"
          />
          <button
            onClick={submit}
            disabled={chosen.size === 0}
            className="inline-flex h-8 items-center gap-1.5 rounded-md bg-primary px-3 text-xs font-medium text-primary-foreground hover:opacity-90 disabled:opacity-40"
          >
            <Send className="h-3 w-3" /> 确认选择
          </button>
        </div>
      ) : (
        <div className="mt-2 text-[10px] text-muted-foreground">（历史记录——当时的选择已随后续消息发送）</div>
      )}
    </div>
  );
}

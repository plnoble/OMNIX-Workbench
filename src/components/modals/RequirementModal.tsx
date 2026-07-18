/**
 * RequirementModal — SDD requirement draft.
 *
 * Collects a structured requirement (background / goals / acceptance criteria),
 * then either asks the agent to CLARIFY it (a conversational refinement turn) or
 * to GENERATE a plan file. The modal only assembles the draft Markdown; the
 * parent orchestrates the actual send + plan-path reservation.
 */
import { useState } from "react";
import { createPortal } from "react-dom";
import { ClipboardList, MessageCircleQuestion, X } from "lucide-react";

import { Button } from "@/components/ui/button";

interface RequirementModalProps {
  open: boolean;
  onClose: () => void;
  onClarify: (draft: string, title: string) => void | Promise<void>;
  onGeneratePlan: (draft: string, title: string) => void | Promise<void>;
}

function assembleDraft(fields: {
  title: string;
  background: string;
  goals: string;
  acceptance: string;
  notes: string;
}): string {
  const sections: string[] = [];
  sections.push(`# ${fields.title.trim() || "未命名需求"}`);
  const section = (heading: string, body: string) => {
    if (body.trim()) sections.push(`## ${heading}\n${body.trim()}`);
  };
  section("背景", fields.background);
  section("目标", fields.goals);
  section("验收标准", fields.acceptance);
  section("备注", fields.notes);
  return sections.join("\n\n");
}

export function RequirementModal({ open, onClose, onClarify, onGeneratePlan }: RequirementModalProps) {
  const [title, setTitle] = useState("");
  const [background, setBackground] = useState("");
  const [goals, setGoals] = useState("");
  const [acceptance, setAcceptance] = useState("");
  const [notes, setNotes] = useState("");
  const [busy, setBusy] = useState(false);

  if (!open) return null;

  const hasContent = [title, background, goals, acceptance, notes].some((v) => v.trim());

  const run = async (action: "clarify" | "plan") => {
    if (!hasContent || busy) return;
    const draft = assembleDraft({ title, background, goals, acceptance, notes });
    setBusy(true);
    try {
      if (action === "clarify") await onClarify(draft, title.trim());
      else await onGeneratePlan(draft, title.trim());
      onClose();
    } finally {
      setBusy(false);
    }
  };

  const field = (
    label: string,
    value: string,
    setValue: (v: string) => void,
    placeholder: string,
    rows = 3,
  ) => (
    <label className="block">
      <span className="text-xs font-medium text-muted-foreground">{label}</span>
      <textarea
        value={value}
        onChange={(e) => setValue(e.target.value)}
        placeholder={placeholder}
        rows={rows}
        className="mt-1 w-full resize-y rounded-md border border-border bg-background px-3 py-2 text-sm leading-6 focus:border-accent focus:outline-none"
      />
    </label>
  );

  return createPortal(
    <div className="fixed inset-0 z-[1000] flex items-center justify-center bg-black/60 p-4 backdrop-blur-sm">
      <div className="flex max-h-[88vh] w-full max-w-2xl flex-col overflow-hidden rounded-lg border border-border glass-surface shadow-xl">
        <div className="flex items-center justify-between border-b border-border px-5 py-3">
          <div className="flex items-center gap-2">
            <ClipboardList className="h-4 w-4 text-accent" />
            <h3 className="m-0 text-base font-semibold text-foreground">新建需求</h3>
          </div>
          <button
            className="rounded p-1 text-muted-foreground hover:bg-muted/30 hover:text-foreground"
            onClick={onClose}
            aria-label="关闭"
          >
            <X className="h-4 w-4" />
          </button>
        </div>

        <div className="min-h-0 flex-1 space-y-3 overflow-y-auto px-5 py-4">
          <label className="block">
            <span className="text-xs font-medium text-muted-foreground">标题</span>
            <input
              value={title}
              onChange={(e) => setTitle(e.target.value)}
              placeholder="一句话概括这个需求，例如「登录接口加限流」"
              className="mt-1 w-full rounded-md border border-border bg-background px-3 py-2 text-sm focus:border-accent focus:outline-none"
            />
          </label>
          {field("背景", background, setBackground, "为什么要做这件事？现状是什么？")}
          {field("目标", goals, setGoals, "希望达到的结果，尽量具体、可衡量。")}
          {field("验收标准", acceptance, setAcceptance, "满足哪些条件才算做完？（会写进计划）")}
          {field("备注（可选）", notes, setNotes, "约束、边界、涉及的模块、参考资料……", 2)}
          <p className="text-xs text-muted-foreground">
            「澄清讨论」让 Agent 先追问和补充；「生成计划」让 Agent 把需求写成
            <code className="mx-1">.omx/plans/</code>下的可跟踪计划文件（含步骤、测试、验收）。
          </p>
        </div>

        <div className="flex items-center justify-end gap-2 border-t border-border px-5 py-3">
          <Button variant="outline" size="sm" disabled={!hasContent || busy} onClick={() => void run("clarify")}>
            <MessageCircleQuestion className="h-4 w-4" /> 澄清讨论
          </Button>
          <Button size="sm" disabled={!hasContent || busy} onClick={() => void run("plan")}>
            <ClipboardList className="h-4 w-4" /> 生成计划
          </Button>
        </div>
      </div>
    </div>,
    document.body,
  );
}

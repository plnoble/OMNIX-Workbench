/**
 * PlanPanel — SDD plan viewer + thread todo.
 *
 * Reads plan files the agent wrote under `.omx/plans/`, renders the selected
 * plan's Markdown, and turns its `- [ ]` checkboxes into a trackable checklist
 * (toggling rewrites the file). This is OMNIX's "plan panel + thread todo".
 */
import { useCallback, useEffect, useState } from "react";
import ReactMarkdown from "react-markdown";
import { ClipboardList, RefreshCw, X } from "lucide-react";

import { cn } from "@/lib/utils";
import { toast } from "@/components/ui/sonner";
import { sddApi, type PlanFile, type PlanTodo } from "@/lib/tauri-api";

interface PlanPanelProps {
  workspacePath: string;
  /// Bump to force a reload of the plan list (e.g. after generating a plan).
  refreshKey?: number;
  onClose: () => void;
}

export function PlanPanel({ workspacePath, refreshKey, onClose }: PlanPanelProps) {
  const [plans, setPlans] = useState<PlanFile[]>([]);
  const [selected, setSelected] = useState<string>("");
  const [content, setContent] = useState<string>("");
  const [todos, setTodos] = useState<PlanTodo[]>([]);
  const [loading, setLoading] = useState(false);

  const loadList = useCallback(async () => {
    if (!workspacePath || workspacePath === "direct") return;
    setLoading(true);
    try {
      const list = await sddApi.listPlans(workspacePath);
      setPlans(list);
      // Auto-select the newest plan if nothing is selected or the selection vanished.
      setSelected((current) =>
        current && list.some((p) => p.relative_path === current)
          ? current
          : list[0]?.relative_path ?? "",
      );
    } catch (error) {
      toast.error(`读取计划失败：${error}`);
    } finally {
      setLoading(false);
    }
  }, [workspacePath]);

  useEffect(() => {
    void loadList();
  }, [loadList, refreshKey]);

  const loadPlan = useCallback(async (relativePath: string) => {
    if (!relativePath) {
      setContent("");
      setTodos([]);
      return;
    }
    try {
      const [md, items] = await sddApi.readPlan(workspacePath, relativePath);
      setContent(md);
      setTodos(items);
    } catch (error) {
      toast.error(`打开计划失败：${error}`);
    }
  }, [workspacePath]);

  useEffect(() => {
    void loadPlan(selected);
  }, [selected, loadPlan]);

  const toggleTodo = async (todo: PlanTodo) => {
    try {
      const updated = await sddApi.toggleTodo(workspacePath, selected, todo.line_index, !todo.done);
      setTodos(updated);
      // Keep the rendered Markdown in sync with the checkbox change.
      await loadPlan(selected);
      void loadList(); // refresh counts in the list
    } catch (error) {
      toast.error(`更新待办失败：${error}`);
    }
  };

  const doneCount = todos.filter((t) => t.done).length;

  return (
    <aside className="flex w-80 shrink-0 flex-col border-l border-border bg-background/60 min-[1500px]:w-96">
      <div className="flex items-center justify-between border-b border-border px-3 py-2">
        <div className="flex items-center gap-1.5 text-sm font-semibold">
          <ClipboardList className="h-4 w-4 text-accent" /> 计划
        </div>
        <div className="flex items-center gap-0.5">
          <button
            className="rounded p-1 text-muted-foreground hover:bg-muted/30 hover:text-foreground"
            title="刷新"
            onClick={() => void loadList()}
          >
            <RefreshCw className={cn("h-3.5 w-3.5", loading && "animate-spin")} />
          </button>
          <button
            className="rounded p-1 text-muted-foreground hover:bg-muted/30 hover:text-foreground"
            title="收起"
            onClick={onClose}
          >
            <X className="h-3.5 w-3.5" />
          </button>
        </div>
      </div>

      {/* Plan selector */}
      {plans.length > 0 && (
        <select
          className="m-2 h-8 rounded-md border border-border bg-background px-2 text-xs"
          value={selected}
          onChange={(e) => setSelected(e.target.value)}
        >
          {plans.map((p) => (
            <option key={p.relative_path} value={p.relative_path}>
              {p.title} · {p.todo_done}/{p.todo_total} · {p.updated_at}
            </option>
          ))}
        </select>
      )}

      <div className="min-h-0 flex-1 overflow-y-auto px-3 py-2">
        {plans.length === 0 ? (
          <div className="rounded-md border border-dashed border-border px-3 py-8 text-center text-xs text-muted-foreground">
            还没有计划。用「需求」按钮写下需求，让 Agent 生成 <code>.omx/plans/</code> 里的计划。
          </div>
        ) : (
          <>
            {/* Thread todo (checkbox checklist parsed from the plan) */}
            {todos.length > 0 && (
              <div className="mb-3 rounded-md border border-border bg-card/40 p-2">
                <div className="mb-1.5 flex items-center justify-between text-xs font-medium text-muted-foreground">
                  <span>任务清单</span>
                  <span>{doneCount}/{todos.length}</span>
                </div>
                <div className="space-y-1">
                  {todos.map((todo) => (
                    <label key={todo.line_index} className="flex cursor-pointer items-start gap-2 text-xs">
                      <input
                        type="checkbox"
                        checked={todo.done}
                        onChange={() => void toggleTodo(todo)}
                        className="mt-0.5 shrink-0"
                      />
                      <span className={cn("leading-5", todo.done && "text-muted-foreground line-through")}>
                        {todo.text}
                      </span>
                    </label>
                  ))}
                </div>
              </div>
            )}

            {/* Full plan markdown */}
            <div className="prose-plan text-xs leading-6 text-foreground/90">
              <ReactMarkdown>{content}</ReactMarkdown>
            </div>
          </>
        )}
      </div>
    </aside>
  );
}

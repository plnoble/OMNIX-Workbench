import { useCallback, useEffect, useState } from "react";
import { Wand2, Plus, Trash2, Pencil } from "lucide-react";

import { quickActionApi, type QuickAction } from "@/lib/tauri-api";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import { toast } from "@/components/ui/sonner";

/**
 * QuickActionsEditor — manage custom Quick Assistant actions (划词助手深挖).
 * Each action is an emoji + label + prompt template where `{{text}}` is replaced
 * by the selected text. Self-contained; rendered inside QuickAssistantTab.
 */
const empty = { id: undefined as string | undefined, label: "", emoji: "✨", prompt_template: "", enabled: true };

export function QuickActionsEditor() {
  const [actions, setActions] = useState<QuickAction[]>([]);
  const [form, setForm] = useState(empty);
  const [editing, setEditing] = useState(false);

  const load = useCallback(async () => {
    try {
      setActions(await quickActionApi.list());
    } catch (e) {
      toast.error("加载自定义动作失败", { description: String(e) });
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  const save = async () => {
    if (!form.label.trim() || !form.prompt_template.trim()) {
      toast.error("请填写名称和提示词");
      return;
    }
    try {
      await quickActionApi.save({
        id: form.id,
        label: form.label.trim(),
        emoji: form.emoji.trim() || "✨",
        promptTemplate: form.prompt_template,
        enabled: form.enabled,
        orderNum: form.id ? actions.find((a) => a.id === form.id)?.order_num ?? 0 : actions.length,
      });
      setForm(empty);
      setEditing(false);
      await load();
      toast.success("已保存自定义动作");
    } catch (e) {
      toast.error("保存失败", { description: String(e) });
    }
  };

  const edit = (a: QuickAction) => {
    setForm({ id: a.id, label: a.label, emoji: a.emoji, prompt_template: a.prompt_template, enabled: a.enabled });
    setEditing(true);
  };

  const remove = async (id: string) => {
    if (!window.confirm("删除该自定义动作？")) return;
    try {
      await quickActionApi.remove(id);
      await load();
    } catch (e) {
      toast.error("删除失败", { description: String(e) });
    }
  };

  return (
    <div className="rounded-md border border-border bg-card/40 p-4">
      <div className="mb-3 flex items-center justify-between">
        <div className="flex items-center gap-2 text-sm font-semibold">
          <Wand2 className="h-4 w-4" /> 自定义动作
        </div>
        {!editing && (
          <Button size="sm" variant="outline" onClick={() => { setForm(empty); setEditing(true); }}>
            <Plus className="h-3.5 w-3.5" /> 新建动作
          </Button>
        )}
      </div>
      <p className="mb-3 text-xs text-muted-foreground">
        在划词操作栏里加入你自己的动作。提示词里用 <code className="rounded bg-muted/40 px-1">{"{{text}}"}</code> 代表选中文字（不写则自动追加在末尾）。例：<code className="rounded bg-muted/40 px-1">把下面内容改写为正式邮件语气：{"{{text}}"}</code>
      </p>

      {editing && (
        <div className="mb-3 flex flex-col gap-2 rounded-lg border border-border bg-muted/10 p-3">
          <div className="flex gap-2">
            <input
              className="w-16 rounded border border-border bg-background px-2 py-1.5 text-center text-sm"
              placeholder="✨"
              value={form.emoji}
              onChange={(e) => setForm({ ...form, emoji: e.target.value })}
            />
            <input
              className="flex-1 rounded border border-border bg-background px-2 py-1.5 text-sm"
              placeholder="动作名称（如：改写为正式语气）"
              value={form.label}
              onChange={(e) => setForm({ ...form, label: e.target.value })}
            />
          </div>
          <textarea
            className="min-h-[70px] rounded border border-border bg-background px-2 py-1.5 text-sm"
            placeholder="提示词模板，用 {{text}} 代表选中文字"
            value={form.prompt_template}
            onChange={(e) => setForm({ ...form, prompt_template: e.target.value })}
          />
          <div className="flex items-center justify-between">
            <label className="flex items-center gap-2 text-xs text-muted-foreground">
              <Switch checked={form.enabled} onCheckedChange={(v) => setForm({ ...form, enabled: v })} /> 启用
            </label>
            <div className="flex gap-2">
              <Button size="sm" variant="outline" onClick={() => { setForm(empty); setEditing(false); }}>取消</Button>
              <Button size="sm" onClick={() => void save()}>保存</Button>
            </div>
          </div>
        </div>
      )}

      {actions.length === 0 ? (
        !editing && <div className="rounded border border-dashed border-border px-3 py-3 text-center text-xs text-muted-foreground">还没有自定义动作。</div>
      ) : (
        <div className="flex flex-col gap-1.5">
          {actions.map((a) => (
            <div key={a.id} className="flex items-center gap-2 rounded-lg border border-border px-3 py-2">
              <span className="text-base">{a.emoji}</span>
              <div className="min-w-0 flex-1">
                <span className="text-sm font-medium">{a.label}</span>
                <div className="truncate text-[11px] text-muted-foreground" title={a.prompt_template}>{a.prompt_template}</div>
              </div>
              <Switch checked={a.enabled} onCheckedChange={(v) => { void quickActionApi.save({ id: a.id, label: a.label, emoji: a.emoji, promptTemplate: a.prompt_template, enabled: v, orderNum: a.order_num }).then(load); }} />
              <button onClick={() => edit(a)} title="编辑" className="rounded p-1 text-muted-foreground hover:bg-muted/30 hover:text-foreground"><Pencil className="h-3.5 w-3.5" /></button>
              <button onClick={() => void remove(a.id)} title="删除" className="rounded p-1 text-muted-foreground hover:bg-destructive/10 hover:text-destructive"><Trash2 className="h-3.5 w-3.5" /></button>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

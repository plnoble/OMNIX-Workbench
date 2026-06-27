import { useCallback, useEffect, useState } from "react";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Switch } from "@/components/ui/switch";
import { Webhook, Plus, Trash2, Play, Trash, ChevronDown, ChevronRight } from "lucide-react";

import { hooksApi, type Hook, type HookRun } from "@/lib/tauri-api";
import { toast } from "@/components/ui/sonner";

/**
 * HooksPanel — user-state hooks (Claude Code hooks inspired). Event → action
 * rules that fire on agent runtime events: a desktop notify, a shell command
 * (env: OMNIX_SESSION_ID / OMNIX_EVENT / OMNIX_EVENT_TEXT), or a log entry.
 * Self-contained; lives in the automation hub (CronTab).
 */
const EVENTS: { value: string; label: string }[] = [
  { value: "*", label: "任意事件" },
  { value: "session_started", label: "会话开始" },
  { value: "tool_started", label: "工具调用开始" },
  { value: "tool_completed", label: "工具调用完成" },
  { value: "approval_requested", label: "请求审批" },
  { value: "turn_completed", label: "一轮完成" },
  { value: "error", label: "出错" },
];
const ACTIONS: { value: string; label: string; hint: string }[] = [
  { value: "notify", label: "桌面通知", hint: "通知正文（留空则用事件内容）" },
  { value: "command", label: "执行命令", hint: "shell 命令，可用 $OMNIX_SESSION_ID / $OMNIX_EVENT / $OMNIX_EVENT_TEXT" },
  { value: "log", label: "记录日志", hint: "记录到下方运行日志的内容" },
];

const emptyForm = { id: undefined as string | undefined, name: "", event: "tool_completed", matcher: "", action_type: "notify", action_payload: "", enabled: true };

export function HooksPanel() {
  const [hooks, setHooks] = useState<Hook[]>([]);
  const [runs, setRuns] = useState<HookRun[]>([]);
  const [form, setForm] = useState(emptyForm);
  const [editing, setEditing] = useState(false);
  const [showRuns, setShowRuns] = useState(false);

  const load = useCallback(async () => {
    try {
      const [h, r] = await Promise.all([hooksApi.list(), hooksApi.runs(30)]);
      setHooks(h);
      setRuns(r);
    } catch (e) {
      toast.error("加载 Hooks 失败", { description: String(e) });
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  const save = async () => {
    if (!form.name.trim()) {
      toast.error("请填写 Hook 名称");
      return;
    }
    try {
      await hooksApi.save({
        id: form.id,
        name: form.name.trim(),
        event: form.event,
        matcher: form.matcher.trim(),
        action_type: form.action_type,
        action_payload: form.action_payload,
        enabled: form.enabled,
      });
      setForm(emptyForm);
      setEditing(false);
      await load();
      toast.success("已保存 Hook");
    } catch (e) {
      toast.error("保存失败", { description: String(e) });
    }
  };

  const edit = (h: Hook) => {
    setForm({ id: h.id, name: h.name, event: h.event, matcher: h.matcher, action_type: h.action_type, action_payload: h.action_payload, enabled: h.enabled });
    setEditing(true);
  };

  const remove = async (id: string) => {
    if (!window.confirm("删除该 Hook？")) return;
    try {
      await hooksApi.remove(id);
      await load();
    } catch (e) {
      toast.error("删除失败", { description: String(e) });
    }
  };

  const test = async (id: string) => {
    try {
      const detail = await hooksApi.test(id);
      toast.success("测试已触发", { description: detail });
      await load();
    } catch (e) {
      toast.error("测试失败", { description: String(e) });
    }
  };

  const actionHint = ACTIONS.find((a) => a.value === form.action_type)?.hint ?? "";

  return (
    <Card>
      <CardHeader className="mb-3 flex-row items-center justify-between">
        <CardTitle className="flex items-center gap-2 text-sm">
          <Webhook className="h-4 w-4" /> 事件 Hooks（自动化规则）
        </CardTitle>
        {!editing && (
          <Button size="sm" onClick={() => { setForm(emptyForm); setEditing(true); }}>
            <Plus className="h-3 w-3" /> 新建 Hook
          </Button>
        )}
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        {/* Editor */}
        {editing && (
          <div className="flex flex-col gap-2 rounded-lg border border-border bg-muted/10 p-3">
            <input
              className="rounded border border-border bg-background px-2 py-1.5 text-sm"
              placeholder="Hook 名称（如：部署完成提醒）"
              value={form.name}
              onChange={(e) => setForm({ ...form, name: e.target.value })}
            />
            <div className="grid grid-cols-2 gap-2">
              <label className="flex flex-col gap-1 text-[11px] text-muted-foreground">
                触发事件
                <select className="rounded border border-border bg-background px-2 py-1.5 text-sm text-foreground" value={form.event} onChange={(e) => setForm({ ...form, event: e.target.value })}>
                  {EVENTS.map((ev) => <option key={ev.value} value={ev.value}>{ev.label}</option>)}
                </select>
              </label>
              <label className="flex flex-col gap-1 text-[11px] text-muted-foreground">
                动作
                <select className="rounded border border-border bg-background px-2 py-1.5 text-sm text-foreground" value={form.action_type} onChange={(e) => setForm({ ...form, action_type: e.target.value })}>
                  {ACTIONS.map((a) => <option key={a.value} value={a.value}>{a.label}</option>)}
                </select>
              </label>
            </div>
            <input
              className="rounded border border-border bg-background px-2 py-1.5 text-sm"
              placeholder="文本匹配（可选，事件内容须包含此子串才触发）"
              value={form.matcher}
              onChange={(e) => setForm({ ...form, matcher: e.target.value })}
            />
            <textarea
              className="min-h-[60px] rounded border border-border bg-background px-2 py-1.5 font-mono text-xs"
              placeholder={actionHint}
              value={form.action_payload}
              onChange={(e) => setForm({ ...form, action_payload: e.target.value })}
            />
            <div className="flex items-center justify-between">
              <label className="flex items-center gap-2 text-xs text-muted-foreground">
                <Switch checked={form.enabled} onCheckedChange={(v) => setForm({ ...form, enabled: v })} /> 启用
              </label>
              <div className="flex gap-2">
                <Button size="sm" variant="outline" onClick={() => { setForm(emptyForm); setEditing(false); }}>取消</Button>
                <Button size="sm" onClick={() => void save()}>保存</Button>
              </div>
            </div>
            {form.action_type === "command" && (
              <p className="text-[10px] text-warning">⚠️ 命令动作会在你的机器上执行 shell 命令，请仅配置你信任的命令。</p>
            )}
          </div>
        )}

        {/* Hook list */}
        {hooks.length === 0 ? (
          <div className="rounded border border-dashed border-border px-3 py-4 text-center text-xs text-muted-foreground">
            还没有 Hook。新建后，当 Agent 会话触发对应事件时会自动执行动作（通知 / 命令 / 日志）。
          </div>
        ) : (
          <div className="flex flex-col gap-2">
            {hooks.map((h) => (
              <div key={h.id} className="flex items-center gap-2 rounded-lg border border-border px-3 py-2">
                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-2">
                    <span className="truncate text-sm font-medium">{h.name}</span>
                    <Badge variant="outline" className="shrink-0 text-[10px]">{EVENTS.find((e) => e.value === h.event)?.label ?? h.event}</Badge>
                    <Badge variant="outline" className="shrink-0 text-[10px]">{ACTIONS.find((a) => a.value === h.action_type)?.label ?? h.action_type}</Badge>
                  </div>
                  <div className="truncate text-[11px] text-muted-foreground">
                    {h.matcher && <span className="text-info">含「{h.matcher}」 · </span>}
                    {h.action_payload || "（无内容）"}
                    {h.fire_count > 0 && <span> · 已触发 {h.fire_count} 次</span>}
                  </div>
                </div>
                <Switch checked={h.enabled} onCheckedChange={(v) => { void hooksApi.toggle(h.id, v).then(load); }} />
                <button onClick={() => void test(h.id)} title="测试触发" className="rounded p-1 text-muted-foreground hover:bg-muted/30 hover:text-foreground">
                  <Play className="h-3.5 w-3.5" />
                </button>
                <button onClick={() => edit(h)} title="编辑" className="rounded px-1.5 py-1 text-[11px] text-muted-foreground hover:bg-muted/30 hover:text-foreground">编辑</button>
                <button onClick={() => void remove(h.id)} title="删除" className="rounded p-1 text-muted-foreground hover:bg-destructive/10 hover:text-destructive">
                  <Trash2 className="h-3.5 w-3.5" />
                </button>
              </div>
            ))}
          </div>
        )}

        {/* Run log */}
        <div>
          <button onClick={() => setShowRuns((s) => !s)} className="flex items-center gap-1 text-xs font-semibold text-muted-foreground hover:text-foreground">
            {showRuns ? <ChevronDown className="h-3.5 w-3.5" /> : <ChevronRight className="h-3.5 w-3.5" />} 运行日志 {runs.length > 0 && `(${runs.length})`}
          </button>
          {showRuns && (
            <div className="mt-2">
              {runs.length > 0 && (
                <div className="mb-2 flex justify-end">
                  <Button size="sm" variant="outline" onClick={() => void hooksApi.clearRuns().then(load)}>
                    <Trash className="h-3 w-3" /> 清空
                  </Button>
                </div>
              )}
              {runs.length === 0 ? (
                <div className="text-center text-[11px] text-muted-foreground">暂无触发记录</div>
              ) : (
                <div className="flex flex-col gap-1">
                  {runs.map((r) => (
                    <div key={r.id} className="flex items-center gap-2 border-b border-border pb-1 text-[11px]">
                      <Badge variant={r.ok ? "success" : "destructive"} className="shrink-0 text-[10px]">{r.ok ? "OK" : "ERR"}</Badge>
                      <span className="shrink-0 text-muted-foreground">{r.event}</span>
                      <span className="min-w-0 flex-1 truncate" title={r.detail}>{r.hook_name} · {r.detail}</span>
                      <span className="shrink-0 text-muted-foreground">{new Date(r.fired_at + "Z").toLocaleTimeString()}</span>
                    </div>
                  ))}
                </div>
              )}
            </div>
          )}
        </div>
      </CardContent>
    </Card>
  );
}

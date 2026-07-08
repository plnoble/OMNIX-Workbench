/**
 * AutopilotsTab — scheduled agent work (Multica-inspired).
 *
 * CRUD for autopilots. A due autopilot enqueues a run (backend scheduler); the
 * global useAutopilotRunner executes it through the runtime, producing a normal
 * reviewable conversation. This tab manages the definitions and can fire one now.
 */
import { useCallback, useEffect, useState } from "react";
import { Pause, Play, Plane, Plus, RefreshCw, Trash2, Zap } from "lucide-react";

import { Button } from "@/components/ui/button";
import { toast } from "@/components/ui/sonner";
import { cn } from "@/lib/utils";
import { AGENT_NAMES } from "@/lib/constants";
import { autopilotApi, type Autopilot } from "@/lib/tauri-api";

const SCHEDULE_PRESETS = ["every 30 minutes", "every 2 hours", "daily at 09:00"];
const PERMISSIONS = [
  { id: "ask_on_risk", label: "风险审批" },
  { id: "ask_every_time", label: "请求审批" },
  { id: "full_access", label: "完全访问" },
];
const WORK_MODES = [
  { id: "direct", label: "直接执行" },
  { id: "plan", label: "计划模式" },
];

interface FormState {
  id?: string;
  title: string;
  prompt: string;
  agentName: string;
  workspacePath: string;
  schedule: string;
  permission: string;
  workMode: string;
}

const EMPTY_FORM: FormState = {
  title: "",
  prompt: "",
  agentName: AGENT_NAMES[0],
  workspacePath: "",
  schedule: "every 2 hours",
  permission: "ask_on_risk",
  workMode: "direct",
};

export function AutopilotsTab() {
  const [autopilots, setAutopilots] = useState<Autopilot[]>([]);
  const [form, setForm] = useState<FormState | null>(null);
  const [busy, setBusy] = useState(false);

  const load = useCallback(async () => {
    try {
      setAutopilots(await autopilotApi.list());
    } catch (error) {
      toast.error(`读取 Autopilot 失败：${error}`);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  const save = async () => {
    if (!form) return;
    if (!form.title.trim() || !form.prompt.trim() || !form.workspacePath.trim()) {
      toast.error("请填写标题、任务提示和工作区");
      return;
    }
    setBusy(true);
    try {
      const payload = {
        title: form.title,
        prompt: form.prompt,
        agentName: form.agentName,
        workspacePath: form.workspacePath,
        schedule: form.schedule,
        permission: form.permission,
        workMode: form.workMode,
      };
      if (form.id) await autopilotApi.update({ id: form.id, ...payload });
      else await autopilotApi.create(payload);
      toast.success(form.id ? "已更新" : "已创建 Autopilot");
      setForm(null);
      await load();
    } catch (error) {
      toast.error(`保存失败：${error}`);
    } finally {
      setBusy(false);
    }
  };

  const toggle = async (ap: Autopilot) => {
    try {
      await autopilotApi.setEnabled(ap.id, !ap.enabled);
      await load();
    } catch (error) {
      toast.error(`切换失败：${error}`);
    }
  };

  const runNow = async (ap: Autopilot) => {
    try {
      await autopilotApi.runNow(ap.id);
      toast.message(`已手动触发：${ap.title}`, { description: "稍后会在会话列表出现" });
      await load();
    } catch (error) {
      toast.error(`触发失败：${error}`);
    }
  };

  const remove = async (ap: Autopilot) => {
    try {
      await autopilotApi.delete(ap.id);
      await load();
    } catch (error) {
      toast.error(`删除失败：${error}`);
    }
  };

  return (
    <div className="flex-1 overflow-y-auto p-6">
      <div className="mx-auto max-w-4xl">
        <div className="mb-4 flex items-center justify-between">
          <div>
            <h2 className="flex items-center gap-2 text-lg font-semibold">
              <Plane className="h-5 w-5 text-accent" /> 自动驾驶 Autopilot
            </h2>
            <p className="mt-0.5 text-sm text-muted-foreground">
              定时把一个任务派给 Agent，在工作区自动执行——每次运行都是一条可回看的会话。
            </p>
          </div>
          <div className="flex items-center gap-2">
            <button
              className="rounded p-2 text-muted-foreground hover:bg-muted/30 hover:text-foreground"
              title="刷新"
              onClick={() => void load()}
            >
              <RefreshCw className="h-4 w-4" />
            </button>
            <Button size="sm" onClick={() => setForm({ ...EMPTY_FORM })}>
              <Plus className="h-4 w-4" /> 新建
            </Button>
          </div>
        </div>

        {/* Create / edit form */}
        {form && (
          <div className="mb-5 space-y-3 rounded-lg border border-border bg-card/50 p-4">
            <input
              className="w-full rounded-md border border-border bg-background px-3 py-2 text-sm focus:border-accent focus:outline-none"
              placeholder="标题，例如「每日依赖安全巡检」"
              value={form.title}
              onChange={(e) => setForm({ ...form, title: e.target.value })}
            />
            <textarea
              className="w-full resize-y rounded-md border border-border bg-background px-3 py-2 text-sm leading-6 focus:border-accent focus:outline-none"
              rows={3}
              placeholder="任务提示（发给 Agent 的第一条消息），例如「检查 package.json 里过时或有漏洞的依赖，给出升级建议」"
              value={form.prompt}
              onChange={(e) => setForm({ ...form, prompt: e.target.value })}
            />
            <input
              className="w-full rounded-md border border-border bg-background px-3 py-2 text-sm focus:border-accent focus:outline-none"
              placeholder="工作区绝对路径，例如 D:\\Projects\\my-app"
              value={form.workspacePath}
              onChange={(e) => setForm({ ...form, workspacePath: e.target.value })}
            />
            <div className="grid grid-cols-2 gap-3 md:grid-cols-4">
              <label className="text-xs text-muted-foreground">
                Agent
                <select
                  className="mt-1 h-9 w-full rounded-md border border-border bg-background px-2 text-sm text-foreground"
                  value={form.agentName}
                  onChange={(e) => setForm({ ...form, agentName: e.target.value })}
                >
                  {AGENT_NAMES.map((n) => <option key={n} value={n}>{n}</option>)}
                </select>
              </label>
              <label className="text-xs text-muted-foreground">
                权限
                <select
                  className="mt-1 h-9 w-full rounded-md border border-border bg-background px-2 text-sm text-foreground"
                  value={form.permission}
                  onChange={(e) => setForm({ ...form, permission: e.target.value })}
                >
                  {PERMISSIONS.map((p) => <option key={p.id} value={p.id}>{p.label}</option>)}
                </select>
              </label>
              <label className="text-xs text-muted-foreground">
                模式
                <select
                  className="mt-1 h-9 w-full rounded-md border border-border bg-background px-2 text-sm text-foreground"
                  value={form.workMode}
                  onChange={(e) => setForm({ ...form, workMode: e.target.value })}
                >
                  {WORK_MODES.map((m) => <option key={m.id} value={m.id}>{m.label}</option>)}
                </select>
              </label>
              <label className="text-xs text-muted-foreground">
                计划
                <input
                  className="mt-1 h-9 w-full rounded-md border border-border bg-background px-2 text-sm text-foreground"
                  value={form.schedule}
                  onChange={(e) => setForm({ ...form, schedule: e.target.value })}
                />
              </label>
            </div>
            <div className="flex flex-wrap items-center gap-1.5">
              <span className="text-xs text-muted-foreground">常用：</span>
              {SCHEDULE_PRESETS.map((preset) => (
                <button
                  key={preset}
                  className="rounded-full border border-border px-2 py-0.5 text-xs text-muted-foreground hover:border-accent hover:text-accent"
                  onClick={() => setForm({ ...form, schedule: preset })}
                >
                  {preset}
                </button>
              ))}
            </div>
            {form.permission === "full_access" && (
              <p className="text-xs text-amber-500">
                完全访问会让 Agent 无人值守时绕过审批直接改动工作区，请仅在信任的任务上使用。
              </p>
            )}
            <div className="flex justify-end gap-2">
              <Button variant="outline" size="sm" onClick={() => setForm(null)}>取消</Button>
              <Button size="sm" disabled={busy} onClick={() => void save()}>
                {form.id ? "保存" : "创建"}
              </Button>
            </div>
          </div>
        )}

        {/* List */}
        <div className="space-y-2">
          {autopilots.length === 0 && !form && (
            <div className="rounded-lg border border-dashed border-border py-12 text-center text-sm text-muted-foreground">
              还没有 Autopilot。点「新建」定一个：定时把任务派给 Agent 自动跑。
            </div>
          )}
          {autopilots.map((ap) => (
            <div
              key={ap.id}
              className={cn(
                "rounded-lg border p-3",
                ap.enabled ? "border-border bg-card/40" : "border-border bg-muted/10 opacity-70",
              )}
            >
              <div className="flex items-start justify-between gap-2">
                <button className="min-w-0 flex-1 text-left" onClick={() => setForm({
                  id: ap.id,
                  title: ap.title,
                  prompt: ap.prompt,
                  agentName: ap.agent_name,
                  workspacePath: ap.workspace_path,
                  schedule: ap.schedule,
                  permission: ap.permission,
                  workMode: ap.work_mode,
                })}>
                  <div className="flex items-center gap-2">
                    <span className="truncate text-sm font-medium">{ap.title}</span>
                    <span className={cn("rounded-full px-1.5 py-0.5 text-[10px]", ap.enabled ? "bg-success/15 text-success" : "bg-muted text-muted-foreground")}>
                      {ap.enabled ? "启用" : "停用"}
                    </span>
                  </div>
                  <div className="mt-0.5 truncate text-xs text-muted-foreground">
                    {ap.agent_name} · {ap.schedule} · {ap.workspace_path.split(/[\\/]/).pop()}
                    {ap.last_run && ` · 上次 ${ap.last_run.slice(5, 16)}`}
                  </div>
                  <p className="mt-1 line-clamp-2 text-xs text-muted-foreground/80">{ap.prompt}</p>
                </button>
                <div className="flex shrink-0 items-center gap-0.5">
                  <button className="rounded p-1.5 text-muted-foreground hover:bg-muted/40 hover:text-accent" title="立即运行一次" onClick={() => void runNow(ap)}>
                    <Zap className="h-3.5 w-3.5" />
                  </button>
                  <button className="rounded p-1.5 text-muted-foreground hover:bg-muted/40" title={ap.enabled ? "停用" : "启用"} onClick={() => void toggle(ap)}>
                    {ap.enabled ? <Pause className="h-3.5 w-3.5" /> : <Play className="h-3.5 w-3.5" />}
                  </button>
                  <button className="rounded p-1.5 text-muted-foreground hover:bg-destructive/10 hover:text-destructive" title="删除" onClick={() => void remove(ap)}>
                    <Trash2 className="h-3.5 w-3.5" />
                  </button>
                </div>
              </div>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}

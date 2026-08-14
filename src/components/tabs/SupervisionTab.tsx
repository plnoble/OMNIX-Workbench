/**
 * SupervisionTab — 监督台（参照 TraceFence 的"总控台"形态，纯整合已有零件）。
 *
 * 一屏看全所有在跑的 agent 会话：状态、工作目录、最后活动；等待批准的请求
 * 汇总到一处，批/拒直接走既有的 runtime_respond_approval；停止走既有的
 * runtime_stop_session。轮询聚合接口（2s），不新造事件通道。
 */
import { useCallback, useState } from "react";
import { usePolling } from "@/hooks/usePolling";
import { Activity, CircleStop, Loader2, MonitorCog, ShieldAlert, ShieldCheck, X } from "lucide-react";
import { toast } from "sonner";

import { cn } from "@/lib/utils";
import { runtimeApi, supervisionApi, type SupervisedSession } from "@/lib/tauri-api";
import { QuotaStrip } from "@/components/QuotaStrip";

const STATUS_META: Record<string, { label: string; cls: string }> = {
  created: { label: "已创建", cls: "bg-muted/30 text-muted-foreground" },
  starting: { label: "启动中", cls: "bg-warning/15 text-warning" },
  running: { label: "运行中", cls: "bg-success/15 text-success" },
  awaiting_approval: { label: "等待批准", cls: "bg-warning/20 text-warning" },
  stopping: { label: "停止中", cls: "bg-muted/30 text-muted-foreground" },
  completed: { label: "已完成", cls: "bg-success/10 text-success" },
  failed: { label: "失败", cls: "bg-destructive/15 text-destructive" },
  cancelled: { label: "已取消", cls: "bg-muted/30 text-muted-foreground" },
};

/**
 * 拒绝时要补发的那条消息；不补则为空串。
 *
 * 抽出来是因为按钮文案和 `respond` 的行为必须同一个判断——写成两处迟早会漂：
 * 按钮显示「拒绝并说明」而实际什么都没发，是比不做还糟的那种假控件。
 *
 * 两条不变量：**批准一律不补**（那是拒绝路径专有的），空白草稿不算数。
 */
export function correctionToSend(approved: boolean, draft: string | undefined): string {
  return approved ? "" : (draft ?? "").trim();
}

/** 监控中心的「总控」视图（实时会话 + 审批 + 额度）。 */
export function SupervisionConsole() {
  const [sessions, setSessions] = useState<SupervisedSession[]>([]);
  const [recentDone, setRecentDone] = useState<SupervisedSession[]>([]);
  const [busy, setBusy] = useState("");
  const [loadedOnce, setLoadedOnce] = useState(false);
  /** 按会话存的「拒绝理由」草稿，见 respond 里的说明。 */
  const [correction, setCorrection] = useState<Record<string, string>>({});

  const load = useCallback(async () => {
    try {
      const overview = await supervisionApi.overview();
      setSessions(overview.sessions);
      setRecentDone(overview.recent_done);
      setLoadedOnce(true);
    } catch {
      /* transient */
    }
  }, []);

  usePolling(load, 2000);

  /**
   * 批准 / 拒绝，拒绝时可以顺带把「该怎么做」发回去。
   *
   * 两个协议都**没有**「改一改再放行」这种响应：ACP 只能从 agent 给的固定选项里
   * 挑一个 optionId（allow_once / reject_once …），Codex 只能对请求的权限整体
   * 授予或不授予。工具调用发生在别人的进程里，OMNIX 改不了它的参数。
   *
   * 所以纠正走的是「拒掉 + 补一条消息」——这是在这一层能做到的最接近的事，也
   * 省掉「拒绝完还得切回聊天页打字」这一步。拒绝已经生效之后消息才发，万一发
   * 失败也不会让人以为这次调用被放行了。
   */
  const respond = async (s: SupervisedSession, approved: boolean) => {
    if (!s.approval) return;
    const note = correctionToSend(approved, correction[s.session_id]);
    setBusy(s.session_id);
    try {
      await runtimeApi.respondApproval({
        sessionId: s.session_id,
        requestId: s.approval.request_id,
        approved,
        forSession: false,
        approvalMethod: s.approval.approval_method,
        requestedPermissions: s.approval.requested_permissions ?? undefined,
      });
      if (note) {
        try {
          await runtimeApi.sendMessage(s.session_id, note);
        } catch (e) {
          // 拒绝已经落地了，只是话没送到。草稿留着让人重试，别悄悄清空。
          toast.error(`已拒绝，但纠正没发出去：${String(e)}`);
          await load();
          return;
        }
      }
      setCorrection((prev) => {
        const next = { ...prev };
        delete next[s.session_id];
        return next;
      });
      toast.success(approved ? "已批准" : note ? "已拒绝并回话" : "已拒绝");
      await load();
    } catch (e) {
      toast.error(String(e));
    } finally {
      setBusy("");
    }
  };

  const stop = async (s: SupervisedSession) => {
    if (!window.confirm(`停止「${s.conversation_title}」的这个会话？`)) return;
    setBusy(s.session_id);
    try {
      await runtimeApi.stopSession(s.session_id);
      toast.success("已发送停止");
      await load();
    } catch (e) {
      toast.error(String(e));
    } finally {
      setBusy("");
    }
  };

  const awaiting = sessions.filter((s) => s.status === "awaiting_approval");
  const active = sessions.filter((s) => s.status !== "awaiting_approval");

  const card = (s: SupervisedSession, done?: boolean) => {
    const meta = STATUS_META[s.status] ?? { label: s.status, cls: "bg-muted/30 text-muted-foreground" };
    return (
      <div key={s.session_id} className={cn("rounded-xl border p-3", s.approval ? "border-warning/50 bg-warning/5" : "border-border glass-surface", done && "opacity-70")}>
        <div className="flex items-center gap-2">
          <span className="truncate text-sm font-medium">{s.conversation_title}</span>
          <span className={cn("shrink-0 rounded px-1.5 py-0.5 text-[10px]", meta.cls)}>{meta.label}</span>
          <span className="shrink-0 rounded border border-border px-1.5 py-0.5 text-[10px] text-muted-foreground">{s.agent_id}</span>
          {s.work_mode === "plan" && <span className="shrink-0 rounded border border-border px-1.5 py-0.5 text-[10px] text-muted-foreground">计划模式</span>}
          {!done && (
            <button
              className="ml-auto shrink-0 rounded border border-border p-1 text-muted-foreground hover:text-destructive"
              title="停止会话"
              disabled={busy === s.session_id}
              onClick={() => void stop(s)}
            >
              <CircleStop className="h-3.5 w-3.5" />
            </button>
          )}
        </div>
        <div className="mt-1 truncate text-[11px] text-muted-foreground" title={s.workspace_path}>
          {s.workspace_path || "（无工作目录）"} · 始于 {s.started_at}
          {s.last_event_at ? ` · 最后活动 ${s.last_event_at}` : ""}
        </div>
        {s.approval && (
          <div className="mt-2 rounded-lg border border-warning/40 bg-background p-2">
            <div className="flex items-start gap-1.5 text-xs">
              <ShieldAlert className="mt-0.5 h-3.5 w-3.5 shrink-0 text-warning" />
              <span className="min-w-0 flex-1 whitespace-pre-wrap break-words">{s.approval.summary}</span>
            </div>
            <textarea
              className="mt-2 w-full resize-none rounded-md border border-border bg-background px-2 py-1.5 text-xs placeholder:text-muted-foreground focus:outline-none focus:ring-1 focus:ring-warning/50"
              rows={2}
              placeholder="要拒绝的话，可以顺便说该怎么做（拒绝后作为下一条消息发过去）"
              value={correction[s.session_id] ?? ""}
              disabled={busy === s.session_id}
              onChange={(e) =>
                setCorrection((prev) => ({ ...prev, [s.session_id]: e.target.value }))
              }
            />
            <div className="mt-2 flex justify-end gap-2">
              <button
                className="inline-flex h-7 items-center gap-1 rounded-md border border-border px-2.5 text-xs hover:bg-muted/40"
                disabled={busy === s.session_id}
                onClick={() => void respond(s, false)}
              >
                <X className="h-3.5 w-3.5" />
                {correctionToSend(false, correction[s.session_id]) ? "拒绝并说明" : "拒绝"}
              </button>
              <button
                className="inline-flex h-7 items-center gap-1 rounded-md bg-primary px-2.5 text-xs font-medium text-primary-foreground hover:opacity-90"
                disabled={busy === s.session_id}
                onClick={() => void respond(s, true)}
              >
                {busy === s.session_id ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <ShieldCheck className="h-3.5 w-3.5" />}
                批准本次
              </button>
            </div>
          </div>
        )}
      </div>
    );
  };

  return (
    <div className="flex h-full flex-1 flex-col overflow-hidden bg-background">
      <div className="flex items-center gap-2 border-b border-border px-6 py-4">
        <MonitorCog className="h-5 w-5 text-primary" />
        <div>
          <div className="text-lg font-semibold">监督台</div>
          <p className="text-xs text-muted-foreground">
            所有在跑的 agent 会话一屏总控：等待批准的请求集中在这里批/拒，随时停止任何会话。每 2 秒刷新。
          </p>
        </div>
        <span className="ml-auto inline-flex items-center gap-1.5 text-xs text-muted-foreground">
          <Activity className="h-3.5 w-3.5" />
          {sessions.length} 个活动会话{awaiting.length > 0 ? ` · ${awaiting.length} 个待批准` : ""}
        </span>
      </div>

      <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto p-6">
        {/* 订阅额度：5 小时 / 周额度用量与重置，一眼看全 */}
        <QuotaStrip />
        {awaiting.length > 0 && (
          <section>
            <h3 className="mb-2 text-sm font-semibold text-warning">等待批准（{awaiting.length}）</h3>
            <div className="grid grid-cols-1 gap-3 xl:grid-cols-2">{awaiting.map((s) => card(s))}</div>
          </section>
        )}
        <section>
          <h3 className="mb-2 text-sm font-semibold text-muted-foreground">活动会话</h3>
          {active.length === 0 && awaiting.length === 0 ? (
            <div className="rounded-xl border border-dashed border-border p-10 text-center text-sm text-muted-foreground">
              {loadedOnce ? "现在没有在跑的 agent 会话。" : "加载中…"}
            </div>
          ) : (
            <div className="grid grid-cols-1 gap-3 xl:grid-cols-2">{active.map((s) => card(s))}</div>
          )}
        </section>
        {recentDone.length > 0 && (
          <section>
            <h3 className="mb-2 text-sm font-semibold text-muted-foreground">最近一小时结束</h3>
            <div className="grid grid-cols-1 gap-3 xl:grid-cols-2">{recentDone.map((s) => card(s, true))}</div>
          </section>
        )}
      </div>
    </div>
  );
}

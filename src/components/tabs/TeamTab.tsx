import { useEffect, useMemo, useState } from "react";
import {
  AlertTriangle,
  Check,
  ChevronRight,
  ClipboardList,
  FolderOpen,
  Loader2,
  Play,
  RefreshCw,
  RotateCcw,
  ShieldCheck,
  Square,
  Users,
  X,
} from "lucide-react";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";
import { toast } from "@/components/ui/sonner";
import { runtimeApi, shellApi, teamRunApi } from "@/lib/tauri-api";
import { cn } from "@/lib/utils";
import { TeamGraph } from "@/components/TeamGraph";
import type { DetectedAgent, TeamRunDetail, WorkspaceRun } from "@/types";

interface TeamTabProps {
  activeAgent: string;
  detectedAgents: DetectedAgent[];
  collabStdin: string;
  setActiveAgent: (name: string) => void;
  setCollabStdin: (value: string) => void;
}

const terminalStatuses = new Set(["completed", "failed", "validation_failed", "cancelled"]);

const statusLabels: Record<string, string> = {
  draft: "草稿",
  awaiting_plan_approval: "等待确认计划",
  approved: "计划已确认",
  running: "运行中",
  queued: "排队中",
  retrying: "重试中",
  awaiting_approval: "等待审批",
  completed: "已完成",
  failed: "失败",
  validation_failed: "验收未通过",
  blocked: "被依赖阻塞",
  cancelled: "已停止",
};

const STATUS_DOT: Record<string, string> = {
  completed: "#10b981",
  running: "#3b82f6",
  retrying: "#f59e0b",
  queued: "#6b7280",
  pending: "#6b7280",
  awaiting_approval: "#f59e0b",
  blocked: "#f97316",
  failed: "#ef4444",
  validation_failed: "#ef4444",
  cancelled: "#6b7280",
};

export function TeamTab({
  activeAgent,
  detectedAgents,
  collabStdin,
  setActiveAgent,
  setCollabStdin,
}: TeamTabProps) {
  const supported = useMemo(
    () => detectedAgents.filter((agent) => ["Claude Code", "Codex"].includes(agent.name) && agent.status === "installed"),
    [detectedAgents],
  );
  const [workspacePath, setWorkspacePath] = useState("");
  const [runs, setRuns] = useState<WorkspaceRun[]>([]);
  const [selectedRunId, setSelectedRunId] = useState("");
  const [detail, setDetail] = useState<TeamRunDetail | null>(null);
  const [concurrency, setConcurrency] = useState(2);
  const [busy, setBusy] = useState("");
  const [highlightedAssignment, setHighlightedAssignment] = useState("");

  const focusWorker = (assignmentId: string) => {
    setHighlightedAssignment(assignmentId);
    document.getElementById(`worker-${assignmentId}`)?.scrollIntoView({ behavior: "smooth", block: "center" });
  };

  const statusCounts = useMemo(() => {
    const counts: Record<string, number> = {};
    for (const worker of detail?.workers ?? []) {
      counts[worker.status] = (counts[worker.status] ?? 0) + 1;
    }
    return counts;
  }, [detail?.workers]);

  const loadRuns = async () => {
    const list = await teamRunApi.listRuns(false);
    setRuns(list);
    if (!selectedRunId && list[0]) setSelectedRunId(list[0].id);
  };

  const loadDetail = async (runId = selectedRunId) => {
    if (!runId) {
      setDetail(null);
      return;
    }
    setDetail(await teamRunApi.getDetail(runId));
  };

  useEffect(() => {
    loadRuns().catch((error) => toast.error(`读取 Team run 失败：${error}`));
  }, []);

  useEffect(() => {
    if (supported.length > 0 && !supported.some((agent) => agent.name === activeAgent)) {
      setActiveAgent(supported[0].name);
    }
  }, [supported, activeAgent, setActiveAgent]);

  useEffect(() => {
    loadDetail(selectedRunId).catch((error) => toast.error(`读取 Team 详情失败：${error}`));
  }, [selectedRunId]);

  useEffect(() => {
    if (!detail || terminalStatuses.has(detail.run.status) || detail.run.status === "awaiting_plan_approval") return;
    const timer = window.setInterval(() => {
      loadDetail(detail.run.id).catch(() => undefined);
      loadRuns().catch(() => undefined);
    }, 1500);
    return () => window.clearInterval(timer);
  }, [detail?.run.id, detail?.run.status]);

  const chooseWorkspace = async () => {
    const path = await shellApi.pickDirectory();
    if (path) setWorkspacePath(path);
  };

  const generatePlan = async () => {
    if (!collabStdin.trim() || !workspacePath || !activeAgent) {
      toast.warning("请填写团队目标，并选择工作区和已安装队长");
      return;
    }
    setBusy("planning");
    try {
      const created = await teamRunApi.generatePlan(collabStdin, workspacePath, activeAgent);
      setDetail(created);
      setSelectedRunId(created.run.id);
      await loadRuns();
      toast.success("队长计划已生成，请确认后再启动 Worker");
    } catch (error) {
      toast.error(`队长计划生成失败：${error}`);
    } finally {
      setBusy("");
    }
  };

  const approvePlan = async () => {
    if (!detail) return;
    setBusy("approve");
    try {
      await teamRunApi.approvePlan(detail.run.id);
      await loadDetail(detail.run.id);
    } finally {
      setBusy("");
    }
  };

  const startWorkers = async () => {
    if (!detail) return;
    setBusy("start");
    try {
      setDetail(await teamRunApi.startApproved(detail.run.id, concurrency));
      toast.success("Worker 已按依赖关系启动");
    } catch (error) {
      toast.error(`启动失败：${error}`);
    } finally {
      setBusy("");
    }
  };

  const stopRun = async () => {
    if (!detail) return;
    setBusy("stop");
    try {
      setDetail(await teamRunApi.stop(detail.run.id));
    } finally {
      setBusy("");
    }
  };

  const retryWorker = async (workerId: string) => {
    setBusy(workerId);
    try {
      setDetail(await teamRunApi.retryWorker(workerId));
    } catch (error) {
      toast.error(`重试失败：${error}`);
    } finally {
      setBusy("");
    }
  };

  const respondApproval = async (workerId: string, sessionId: string, approved: boolean) => {
    setBusy(workerId);
    try {
      const events = await runtimeApi.getEvents(sessionId);
      const request = [...events].reverse().find((event) => event.kind === "approval_requested" && event.request_id);
      if (!request?.request_id) throw new Error("没有找到待处理的审批请求");
      setDetail(await teamRunApi.respondWorkerApproval(workerId, request.request_id, approved, request.metadata.requested_permissions));
    } catch (error) {
      toast.error(`审批失败：${error}`);
    } finally {
      setBusy("");
    }
  };

  return (
    <div className="h-full overflow-y-auto bg-background">
      <div className="mx-auto max-w-7xl px-6 py-7">
        <header className="flex flex-wrap items-start justify-between gap-4 border-b border-border pb-5">
          <div>
            <div className="flex items-center gap-2">
              <Users className="h-5 w-5 text-primary" />
              <h2 className="text-xl font-semibold">团队协作</h2>
              <Badge variant="outline">真实调度</Badge>
            </div>
            <p className="mt-2 max-w-3xl text-sm leading-6 text-muted-foreground">
              队长先生成结构化计划。只有你确认后，Worker 才会按依赖和并发限制启动；运行、审批、重试与最终验收均保留记录。
            </p>
          </div>
          <div className="flex items-center gap-2 text-xs text-muted-foreground"><ShieldCheck className="h-4 w-4" /> 计划确认是强制步骤</div>
        </header>

        <div className="grid gap-7 py-6 xl:grid-cols-[minmax(0,1fr)_320px]">
          <main>
            <label className="text-sm font-semibold">团队目标</label>
            <Textarea value={collabStdin} onChange={(event) => setCollabStdin(event.target.value)} placeholder="描述目标、边界、涉及的模块和最终验收标准..." className="mt-2 min-h-40 resize-y leading-6" />
            <div className="mt-4 grid gap-3 md:grid-cols-[minmax(0,1fr)_180px]">
              <button type="button" onClick={chooseWorkspace} className="flex h-10 min-w-0 items-center gap-2 rounded-md border border-border px-3 text-left text-sm hover:bg-muted">
                <FolderOpen className="h-4 w-4 shrink-0" />
                <span className="truncate">{workspacePath || "选择工作区"}</span>
              </button>
              <select value={activeAgent} onChange={(event) => setActiveAgent(event.target.value)} className="h-10 rounded-md border border-border bg-background px-3 text-sm">
                <option value="">选择队长</option>
                {supported.map((agent) => <option key={agent.name} value={agent.name}>{agent.name}</option>)}
              </select>
            </div>
            <div className="mt-4 flex flex-wrap items-center gap-3">
              <Button onClick={generatePlan} disabled={busy === "planning" || supported.length === 0}>
                {busy === "planning" ? <Loader2 className="h-4 w-4 animate-spin" /> : <ClipboardList className="h-4 w-4" />}
                {busy === "planning" ? "队长正在规划" : "生成队长计划"}
              </Button>
              {supported.length === 0 && <span className="text-xs text-destructive">请先安装 Claude Code 或 Codex</span>}
            </div>

            {detail?.plan && (
              <section className="mt-7 border-t border-border pt-5">
                <div className="flex flex-wrap items-center justify-between gap-3">
                  <div>
                    <h3 className="font-semibold">队长计划</h3>
                    <p className="mt-1 text-xs text-muted-foreground">{detail.plan.goal}</p>
                  </div>
                  <Badge variant="outline">{statusLabels[detail.plan.status] || detail.plan.status}</Badge>
                </div>
                <div className="mt-4 divide-y divide-border border-y border-border">
                  {detail.plan.assignments.map((assignment, index) => (
                    <div key={assignment.id} className="grid gap-2 py-4 md:grid-cols-[36px_150px_minmax(0,1fr)]">
                      <span className="text-sm text-muted-foreground">{index + 1}</span>
                      <div><div className="text-sm font-medium">{assignment.agent_name}</div><div className="mt-1 text-xs text-muted-foreground">{assignment.id}</div></div>
                      <div>
                        <p className="text-sm leading-6">{assignment.task_title}</p>
                        <p className="mt-1 text-xs text-muted-foreground">依赖：{assignment.depends_on.join("、") || "无"}</p>
                        <p className="mt-1 text-xs text-muted-foreground">验收：{assignment.acceptance_criteria.join("；") || "由队长最终验收"}</p>
                      </div>
                    </div>
                  ))}
                </div>
                <div className="mt-4 flex flex-wrap justify-end gap-2">
                  {detail.plan.status === "proposed" && <Button onClick={approvePlan} disabled={busy === "approve"}><Check className="h-4 w-4" /> 确认计划</Button>}
                  {detail.plan.status === "approved" && detail.run.status !== "running" && (
                    <>
                      <label className="flex items-center gap-2 text-sm text-muted-foreground">并发
                        <select value={concurrency} onChange={(event) => setConcurrency(Number(event.target.value))} className="h-9 rounded-md border border-border bg-background px-2">
                          {[1, 2, 3, 4].map((value) => <option key={value} value={value}>{value}</option>)}
                        </select>
                      </label>
                      <Button onClick={startWorkers} disabled={busy === "start"}><Play className="h-4 w-4" /> 启动 Worker</Button>
                    </>
                  )}
                  {detail.run.status === "running" && <Button variant="outline" onClick={stopRun} disabled={busy === "stop"}><Square className="h-4 w-4" /> 停止</Button>}
                </div>
              </section>
            )}

            {detail && detail.workers.length > 0 && (
              <section className="mt-7 border-t border-border pt-5">
                <div className="flex items-center justify-between gap-3"><h3 className="font-semibold">协作看板</h3><Badge variant="outline">{statusLabels[detail.run.status] || detail.run.status}</Badge></div>

                {/* Status summary */}
                <div className="mt-3 flex flex-wrap gap-2">
                  {Object.entries(statusCounts).map(([status, count]) => (
                    <span key={status} className="inline-flex items-center gap-1.5 rounded-full border border-border bg-card/40 px-2.5 py-1 text-xs">
                      <span className="h-2 w-2 rounded-full" style={{ backgroundColor: STATUS_DOT[status] ?? "#6b7280" }} />
                      {statusLabels[status] || status}
                      <span className="text-muted-foreground">{count}</span>
                    </span>
                  ))}
                </div>

                {/* Dependency DAG */}
                <div className="mt-3">
                  <TeamGraph workers={detail.workers} selectedId={highlightedAssignment} onSelect={focusWorker} />
                </div>

                <div className="mt-5 space-y-3">
                  {detail.workers.map((worker) => (
                    <article
                      key={worker.id}
                      id={`worker-${worker.assignment_id}`}
                      className={cn(
                        "rounded-lg border border-border p-4 transition-shadow",
                        highlightedAssignment === worker.assignment_id && "ring-2 ring-primary"
                      )}
                    >
                      <div className="flex flex-wrap items-start justify-between gap-3">
                        <div><div className="text-sm font-semibold">{worker.assignment_id} · {worker.agent_name}</div><p className="mt-1 text-sm text-muted-foreground">{worker.task_title}</p></div>
                        <Badge variant="outline" className={cn(worker.status === "failed" && "border-destructive text-destructive")}>{statusLabels[worker.status] || worker.status}</Badge>
                      </div>
                      <div className="mt-3 flex flex-wrap gap-x-5 gap-y-1 text-xs text-muted-foreground"><span>依赖：{worker.dependencies.join("、") || "无"}</span><span>重试：{worker.retry_count}/{worker.max_retries}</span><span>验收：{worker.validation_status}</span></div>
                      {worker.result_summary && <pre className="mt-3 max-h-44 overflow-auto rounded-md bg-muted p-3 text-xs whitespace-pre-wrap">{worker.result_summary}</pre>}
                      <div className="mt-3 flex justify-end gap-2">
                        {worker.status === "awaiting_approval" && worker.session_id && (
                          <><Button variant="outline" onClick={() => respondApproval(worker.id, worker.session_id!, false)}><X className="h-4 w-4" /> 拒绝</Button><Button onClick={() => respondApproval(worker.id, worker.session_id!, true)}><ShieldCheck className="h-4 w-4" /> 批准本次</Button></>
                        )}
                        {["failed", "blocked"].includes(worker.status) && <Button variant="outline" onClick={() => retryWorker(worker.id)} disabled={busy === worker.id}><RotateCcw className="h-4 w-4" /> 重试</Button>}
                      </div>
                    </article>
                  ))}
                </div>
                {detail.run.summary && <div className={cn("mt-4 border-l-2 pl-4 text-sm leading-6", detail.run.status === "completed" ? "border-success" : "border-warning")}><strong>队长验收：</strong>{detail.run.summary}</div>}
              </section>
            )}
          </main>

          <aside className="border-l border-border pl-5">
            <div className="flex items-center justify-between gap-2"><h3 className="text-sm font-semibold">Team runs</h3><button type="button" title="刷新" onClick={() => loadRuns()} className="p-1 text-muted-foreground hover:text-foreground"><RefreshCw className="h-4 w-4" /></button></div>
            <div className="mt-3 divide-y divide-border">
              {runs.map((run) => (
                <button key={run.id} type="button" onClick={() => setSelectedRunId(run.id)} className={cn("flex w-full items-center gap-2 py-3 text-left", selectedRunId === run.id && "text-primary")}>
                  {run.status === "failed" || run.status === "validation_failed" ? <AlertTriangle className="h-4 w-4 shrink-0" /> : <ChevronRight className="h-4 w-4 shrink-0" />}
                  <span className="min-w-0 flex-1"><span className="block truncate text-sm font-medium">{run.title}</span><span className="mt-1 block text-xs text-muted-foreground">{statusLabels[run.status] || run.status} · {run.manager_agent}</span></span>
                </button>
              ))}
              {runs.length === 0 && <div className="py-10 text-center text-xs text-muted-foreground">尚未创建 Team run</div>}
            </div>
          </aside>
        </div>
      </div>
    </div>
  );
}

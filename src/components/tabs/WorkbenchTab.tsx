import { useEffect, useMemo, useRef, useState } from "react";
import {
  AlertTriangle,
  Bot,
  BookOpen,
  Brain,
  Check,
  Database,
  FolderOpen,
  ListChecks,
  MessageSquare,
  Play,
  Plug,
  RefreshCw,
  Search,
  Shield,
  Terminal,
  Users,
} from "lucide-react";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Textarea } from "@/components/ui/textarea";
import { toast } from "@/components/ui/sonner";
import { ChatTab, type ChatTabProps } from "@/components/tabs/ChatTab";
import { labsApi, workbenchApi } from "@/lib/tauri-api";
import { cn } from "@/lib/utils";
import type {
  AgentRun,
  LabFeature,
  TeamAssignmentInput,
  TeamPlan,
  WorkspaceRun,
} from "@/types";

const PRIMARY_AGENTS = ["Claude Code", "Codex", "Gemini CLI", "OpenCode"];

/** Local draft row: a TeamAssignmentInput plus a stable uid for React keys,
 *  so removing a middle row no longer shifts keys and remounts inputs. */
type AssignmentDraft = TeamAssignmentInput & { uid: number };

const DEFAULT_ASSIGNMENTS: AssignmentDraft[] = [
  { uid: 1, agent_name: "Claude Code", task_title: "担任队长，拆解重构步骤并守住用户确认点" },
  { uid: 2, agent_name: "Codex", task_title: "实现 Workbench、Run 状态流转和验证闭环" },
  { uid: 3, agent_name: "Gemini CLI", task_title: "补充架构审查、跨文件影响分析和文档整理" },
  { uid: 4, agent_name: "OpenCode", task_title: "验证技能同步适配和本地工具链兼容性" },
];

const RESOURCE_ITEMS = [
  { id: "models", label: "Models", title: "模型路由", icon: Database },
  { id: "knowledge", label: "Knowledge", title: "知识库", icon: BookOpen },
  { id: "memories", label: "Memory", title: "长期记忆", icon: Brain },
  { id: "mcp", label: "MCP", title: "工具服务", icon: Plug },
  { id: "search", label: "Search", title: "联网搜索", icon: Search },
];

interface WorkbenchTabProps extends ChatTabProps {
  onNavigate: (tab: string) => void;
}

export function WorkbenchTab(props: WorkbenchTabProps) {
  const {
    activeAgent,
    detectedAgents,
    chatWorkspace,
    onNavigate,
  } = props;

  const [runs, setRuns] = useState<WorkspaceRun[]>([]);
  const [selectedRun, setSelectedRun] = useState<WorkspaceRun | null>(null);
  const [plan, setPlan] = useState<TeamPlan | null>(null);
  const [agentRuns, setAgentRuns] = useState<AgentRun[]>([]);
  const [labs, setLabs] = useState<LabFeature[]>([]);
  const [title, setTitle] = useState("OMNIX 多 Agent 工作台重构");
  const [workspacePath, setWorkspacePath] = useState(chatWorkspace !== "direct" ? chatWorkspace : "");
  const [managerAgent, setManagerAgent] = useState(activeAgent || "Claude Code");
  const [goal, setGoal] = useState(
    "将 OMNIX 重构为以 Agent + Team + Skill 为主轴的多 Agent 开发工作台。"
  );
  const [assignmentDrafts, setAssignmentDrafts] = useState<AssignmentDraft[]>(DEFAULT_ASSIGNMENTS);
  const [busyAction, setBusyAction] = useState<string | null>(null);
  const draftUidRef = useRef(DEFAULT_ASSIGNMENTS.length);

  const installedAgents = useMemo(
    () => new Set(detectedAgents.filter((agent) => agent.status === "installed").map((agent) => agent.name)),
    [detectedAgents]
  );

  const managerOptions = useMemo(() => {
    const names = new Set<string>([activeAgent, ...PRIMARY_AGENTS, ...detectedAgents.map((agent) => agent.name)]);
    return Array.from(names).filter(Boolean);
  }, [activeAgent, detectedAgents]);

  useEffect(() => {
    loadRuns();
    labsApi.listFeatures().then(setLabs).catch(() => setLabs([]));
  }, []);

  useEffect(() => {
    if (!workspacePath && chatWorkspace !== "direct") {
      setWorkspacePath(chatWorkspace);
    }
  }, [chatWorkspace, workspacePath]);

  useEffect(() => {
    if (!managerAgent && activeAgent) {
      setManagerAgent(activeAgent);
    }
  }, [activeAgent, managerAgent]);

  async function loadRuns() {
    setBusyAction("load-runs");
    try {
      const nextRuns = await workbenchApi.listRuns(false);
      setRuns(nextRuns);
      if (!selectedRun && nextRuns.length > 0) {
        await selectRun(nextRuns[0]);
      }
    } catch (error) {
      toast.error(`加载 Workbench 任务失败：${String(error)}`);
    } finally {
      setBusyAction(null);
    }
  }

  async function hydrateRun(runId: string) {
    const [nextPlan, nextAgentRuns] = await Promise.all([
      workbenchApi.getPlan(runId).catch(() => null),
      workbenchApi.listAgentRuns(runId).catch(() => []),
    ]);
    setPlan(nextPlan);
    setAgentRuns(nextAgentRuns);
  }

  async function selectRun(run: WorkspaceRun) {
    setSelectedRun(run);
    setTitle(run.title);
    setWorkspacePath(run.workspace_path);
    setManagerAgent(run.manager_agent);
    await hydrateRun(run.id);
  }

  async function createRun(): Promise<WorkspaceRun | null> {
    const normalizedWorkspace = workspacePath.trim();
    if (!title.trim()) {
      toast.error("请先填写任务标题。");
      return null;
    }
    if (!normalizedWorkspace) {
      toast.error("请先填写 workspace 路径，或从现有项目会话载入。");
      return null;
    }

    setBusyAction("create-run");
    try {
      const run = await workbenchApi.createRun(title.trim(), normalizedWorkspace, managerAgent);
      setSelectedRun(run);
      setRuns((prev) => [run, ...prev.filter((item) => item.id !== run.id)]);
      setPlan(null);
      setAgentRuns([]);
      toast.success("已创建 Workbench run。");
      return run;
    } catch (error) {
      toast.error(`创建 run 失败：${String(error)}`);
      return null;
    } finally {
      setBusyAction(null);
    }
  }

  async function ensureRun(): Promise<WorkspaceRun | null> {
    if (selectedRun) return selectedRun;
    return createRun();
  }

  async function proposePlan() {
    const run = await ensureRun();
    if (!run) return;

    const assignments = assignmentDrafts
      .filter((assignment) => assignment.task_title.trim())
      .map(({ uid: _uid, ...rest }) => rest);
    if (!goal.trim() || assignments.length === 0) {
      toast.error("请填写目标，并至少保留一个 Worker 任务。");
      return;
    }

    setBusyAction("propose-plan");
    try {
      const nextPlan = await workbenchApi.proposePlan(run.id, goal.trim(), assignments);
      setPlan(nextPlan);
      const refreshed = await workbenchApi.getRun(run.id);
      setSelectedRun(refreshed);
      setRuns((prev) => prev.map((item) => (item.id === refreshed.id ? refreshed : item)));
      toast.success("队长计划已生成，等待确认。");
    } catch (error) {
      toast.error(`生成计划失败：${String(error)}`);
    } finally {
      setBusyAction(null);
    }
  }

  async function approvePlan() {
    if (!selectedRun || !plan) return;
    setBusyAction("approve-plan");
    try {
      const approved = await workbenchApi.approvePlan(selectedRun.id);
      setPlan(approved);
      const refreshed = await workbenchApi.getRun(selectedRun.id);
      setSelectedRun(refreshed);
      setRuns((prev) => prev.map((item) => (item.id === refreshed.id ? refreshed : item)));
      toast.success("计划已确认，可以登记 Worker 任务。");
    } catch (error) {
      toast.error(`确认计划失败：${String(error)}`);
    } finally {
      setBusyAction(null);
    }
  }

  async function registerWorkers() {
    if (!selectedRun || !plan || plan.status !== "approved") {
      toast.error("需要先确认队长计划，再启动 Worker。");
      return;
    }

    setBusyAction("register-workers");
    try {
      for (const assignment of plan.assignments) {
        const exists = agentRuns.some(
          (run) => run.agent_name === assignment.agent_name && run.task_title === assignment.task_title
        );
        if (!exists) {
          await workbenchApi.startAgentRun(
            selectedRun.id,
            assignment.agent_name,
            assignment.task_title,
            "queued"
          );
        }
      }
      const nextAgentRuns = await workbenchApi.listAgentRuns(selectedRun.id);
      setAgentRuns(nextAgentRuns);
      toast.success("Worker 任务已登记到 run。");
    } catch (error) {
      toast.error(`登记 Worker 失败：${String(error)}`);
    } finally {
      setBusyAction(null);
    }
  }

  function updateAssignment(index: number, field: keyof TeamAssignmentInput, value: string) {
    setAssignmentDrafts((prev) =>
      prev.map((assignment, currentIndex) =>
        currentIndex === index ? { ...assignment, [field]: value } : assignment
      )
    );
  }

  function addAssignment() {
    draftUidRef.current += 1;
    setAssignmentDrafts((prev) => [
      ...prev,
      { uid: draftUidRef.current, agent_name: managerOptions[0] || "Codex", task_title: "" },
    ]);
  }

  function removeAssignment(index: number) {
    setAssignmentDrafts((prev) => prev.filter((_, currentIndex) => currentIndex !== index));
  }

  const planApproved = plan?.status === "approved";
  const visibleLabs = labs.filter((feature) => feature.is_visible).slice(0, 4);

  return (
    <div className="flex-1 overflow-y-auto bg-background">
      <div className="mx-auto flex w-full max-w-[1680px] flex-col gap-4 p-4">
        <section className="grid gap-4 xl:grid-cols-[300px_minmax(0,1fr)_360px]">
          <RunList
            runs={runs}
            selectedRunId={selectedRun?.id}
            busy={busyAction === "load-runs"}
            onRefresh={loadRuns}
            onSelect={selectRun}
          />

          <div className="rounded-md border border-border bg-card/55 p-4">
            <div className="flex flex-wrap items-start justify-between gap-3 border-b border-border pb-3">
              <div>
                <div className="flex items-center gap-2 text-sm font-semibold">
                  <FolderOpen className="h-4 w-4 text-accent" />
                  Workbench Run
                  {selectedRun && <StatusBadge status={selectedRun.status} />}
                </div>
                <p className="mt-1 text-xs text-muted-foreground">
                  任务、workspace、队长计划和 Worker 状态在这里形成一条主线。
                </p>
              </div>
              <div className="flex gap-2">
                <Button size="sm" variant="outline" onClick={createRun} disabled={busyAction === "create-run"}>
                  <FolderOpen className="h-3.5 w-3.5" />
                  创建 Run
                </Button>
                <Button size="sm" onClick={proposePlan} disabled={busyAction === "propose-plan"}>
                  <ListChecks className="h-3.5 w-3.5" />
                  生成计划
                </Button>
              </div>
            </div>

            <div className="mt-4 grid gap-3 lg:grid-cols-[1fr_220px]">
              <div className="space-y-2">
                <Label>任务标题</Label>
                <Input value={title} onChange={(event) => setTitle(event.target.value)} />
              </div>
              <div className="space-y-2">
                <Label>队长 Agent</Label>
                <select
                  value={managerAgent}
                  onChange={(event) => setManagerAgent(event.target.value)}
                  className="h-9 w-full rounded-md border border-border bg-background/50 px-3 text-sm text-foreground"
                >
                  {managerOptions.map((name) => (
                    <option key={name} value={name}>
                      {name}
                    </option>
                  ))}
                </select>
              </div>
              <div className="space-y-2 lg:col-span-2">
                <Label>Workspace 路径</Label>
                <Input
                  value={workspacePath}
                  onChange={(event) => setWorkspacePath(event.target.value)}
                  placeholder="D:\Agent\Project\your-workspace"
                />
              </div>
              <div className="space-y-2 lg:col-span-2">
                <Label>队长目标</Label>
                <Textarea
                  value={goal}
                  onChange={(event) => setGoal(event.target.value)}
                  className="min-h-[88px]"
                />
              </div>
            </div>

            <div className="mt-4 rounded-md border border-border bg-background/25">
              <div className="flex items-center justify-between border-b border-border px-3 py-2">
                <div className="flex items-center gap-2 text-sm font-medium">
                  <Users className="h-4 w-4 text-accent" />
                  Worker 分工
                </div>
                <Button size="sm" variant="ghost" onClick={addAssignment}>
                  添加任务
                </Button>
              </div>
              <div className="divide-y divide-border">
                {assignmentDrafts.map((assignment, index) => (
                  <div key={assignment.uid} className="grid gap-2 p-3 md:grid-cols-[160px_minmax(0,1fr)_72px]">
                    <select
                      value={assignment.agent_name}
                      onChange={(event) => updateAssignment(index, "agent_name", event.target.value)}
                      className="h-9 rounded-md border border-border bg-background/50 px-2 text-sm text-foreground"
                    >
                      {managerOptions.map((name) => (
                        <option key={name} value={name}>
                          {name}
                        </option>
                      ))}
                    </select>
                    <Input
                      value={assignment.task_title}
                      onChange={(event) => updateAssignment(index, "task_title", event.target.value)}
                      placeholder="写清楚这个 Worker 应该完成的任务"
                    />
                    <Button size="sm" variant="outline" onClick={() => removeAssignment(index)}>
                      移除
                    </Button>
                  </div>
                ))}
              </div>
            </div>
          </div>

          <aside className="flex flex-col gap-4">
            <PlanPanel
              plan={plan}
              planApproved={planApproved}
              busyAction={busyAction}
              onApprove={approvePlan}
              onRegisterWorkers={registerWorkers}
            />
            <WorkerPanel agentRuns={agentRuns} installedAgents={installedAgents} />
            <ResourcePanel onNavigate={onNavigate} />
            <LabsPreview features={visibleLabs} onNavigate={onNavigate} />
          </aside>
        </section>

        <section className="min-h-[560px] overflow-hidden rounded-md border border-border bg-card/45">
          <div className="flex items-center justify-between border-b border-border px-4 py-2">
            <div className="flex items-center gap-2 text-sm font-semibold">
              <MessageSquare className="h-4 w-4 text-accent" />
              Agent 对话
            </div>
            <Badge variant="outline">已并入 Workbench</Badge>
          </div>
          <div className="h-[calc(100%-41px)] min-h-[520px]">
            <ChatTab {...props} />
          </div>
        </section>
      </div>
    </div>
  );
}

function RunList({
  runs,
  selectedRunId,
  busy,
  onRefresh,
  onSelect,
}: {
  runs: WorkspaceRun[];
  selectedRunId?: string;
  busy: boolean;
  onRefresh: () => void;
  onSelect: (run: WorkspaceRun) => void;
}) {
  return (
    <aside className="rounded-md border border-border bg-card/55">
      <div className="flex items-center justify-between border-b border-border px-3 py-2">
        <div className="flex items-center gap-2 text-sm font-semibold">
          <Terminal className="h-4 w-4 text-accent" />
          Runs
        </div>
        <Button size="icon" variant="ghost" onClick={onRefresh} title="刷新任务列表" aria-label="刷新任务列表">
          <RefreshCw className={cn("h-4 w-4", busy && "animate-spin")} />
        </Button>
      </div>
      <div className="max-h-[572px] overflow-y-auto p-2">
        {runs.length === 0 ? (
          <div className="rounded-md border border-dashed border-border px-3 py-8 text-center text-xs text-muted-foreground">
            暂无 run。填写右侧信息后创建。
          </div>
        ) : (
          runs.map((run) => (
            <button
              key={run.id}
              className={cn(
                "mb-2 w-full rounded-md border px-3 py-2 text-left transition-colors",
                selectedRunId === run.id
                  ? "border-accent/50 bg-accent/10"
                  : "border-border bg-background/20 hover:bg-muted/20"
              )}
              onClick={() => onSelect(run)}
            >
              <div className="flex items-center justify-between gap-2">
                <span className="truncate text-sm font-medium">{run.title}</span>
                <StatusBadge status={run.status} />
              </div>
              <div className="mt-1 truncate text-xs text-muted-foreground">{run.workspace_path}</div>
              <div className="mt-2 flex items-center gap-1.5 text-xs text-muted-foreground">
                <Bot className="h-3 w-3" />
                {run.manager_agent}
              </div>
            </button>
          ))
        )}
      </div>
    </aside>
  );
}

function PlanPanel({
  plan,
  planApproved,
  busyAction,
  onApprove,
  onRegisterWorkers,
}: {
  plan: TeamPlan | null;
  planApproved: boolean;
  busyAction: string | null;
  onApprove: () => void;
  onRegisterWorkers: () => void;
}) {
  return (
    <section className="rounded-md border border-border bg-card/55 p-3">
      <div className="flex items-center justify-between gap-2">
        <div className="flex items-center gap-2 text-sm font-semibold">
          <Shield className="h-4 w-4 text-accent" />
          队长计划
        </div>
        {plan && <StatusBadge status={plan.status} />}
      </div>
      {!plan ? (
        <div className="mt-3 rounded-md border border-dashed border-border p-3 text-xs text-muted-foreground">
          尚未生成计划。先创建 run，再让队长拆解任务。
        </div>
      ) : (
        <div className="mt-3 space-y-3">
          <p className="line-clamp-3 text-xs leading-5 text-muted-foreground">{plan.goal}</p>
          <div className="space-y-2">
            {plan.assignments.map((assignment) => (
              <div key={assignment.id} className="rounded-md border border-border bg-background/25 p-2">
                <div className="flex items-center justify-between gap-2">
                  <span className="text-xs font-semibold">{assignment.agent_name}</span>
                  <Badge variant="secondary">{assignment.status}</Badge>
                </div>
                <p className="mt-1 text-xs leading-5 text-muted-foreground">{assignment.task_title}</p>
              </div>
            ))}
          </div>
          <div className="grid grid-cols-2 gap-2">
            <Button size="sm" variant={planApproved ? "secondary" : "default"} onClick={onApprove} disabled={planApproved || busyAction === "approve-plan"}>
              <Check className="h-3.5 w-3.5" />
              确认计划
            </Button>
            <Button size="sm" variant="outline" onClick={onRegisterWorkers} disabled={!planApproved || busyAction === "register-workers"}>
              <Play className="h-3.5 w-3.5" />
              登记 Worker
            </Button>
          </div>
        </div>
      )}
    </section>
  );
}

function WorkerPanel({
  agentRuns,
  installedAgents,
}: {
  agentRuns: AgentRun[];
  installedAgents: Set<string>;
}) {
  return (
    <section className="rounded-md border border-border bg-card/55 p-3">
      <div className="flex items-center gap-2 text-sm font-semibold">
        <Play className="h-4 w-4 text-accent" />
        Worker 状态
      </div>
      <div className="mt-3 space-y-2">
        {agentRuns.length === 0 ? (
          <div className="rounded-md border border-dashed border-border p-3 text-xs text-muted-foreground">
            计划确认后会在这里登记 Worker 任务。
          </div>
        ) : (
          agentRuns.map((run) => (
            <div key={run.id} className="rounded-md border border-border bg-background/25 p-2">
              <div className="flex items-center justify-between gap-2">
                <div className="flex min-w-0 items-center gap-2">
                  <span
                    className={cn(
                      "h-2 w-2 rounded-full",
                      installedAgents.has(run.agent_name) ? "bg-success" : "bg-warning"
                    )}
                  />
                  <span className="truncate text-xs font-semibold">{run.agent_name}</span>
                </div>
                <StatusBadge status={run.status} />
              </div>
              <p className="mt-1 text-xs leading-5 text-muted-foreground">{run.task_title}</p>
              {run.session_id && <div className="mt-1 truncate text-[11px] text-muted-foreground">session: {run.session_id}</div>}
            </div>
          ))
        )}
      </div>
    </section>
  );
}

function ResourcePanel({ onNavigate }: { onNavigate: (tab: string) => void }) {
  return (
    <section className="rounded-md border border-border bg-card/55 p-3">
      <div className="flex items-center gap-2 text-sm font-semibold">
        <Database className="h-4 w-4 text-accent" />
        资源层
      </div>
      <div className="mt-3 grid grid-cols-2 gap-2">
        {RESOURCE_ITEMS.map((item) => {
          const Icon = item.icon;
          return (
            <button
              key={`${item.id}-${item.label}`}
              className="rounded-md border border-border bg-background/25 px-2 py-2 text-left text-xs transition-colors hover:bg-muted/20"
              onClick={() => onNavigate(item.id)}
            >
              <div className="flex items-center gap-1.5 font-medium">
                <Icon className="h-3.5 w-3.5 text-accent" />
                {item.label}
              </div>
              <div className="mt-1 text-muted-foreground">{item.title}</div>
            </button>
          );
        })}
      </div>
    </section>
  );
}

function LabsPreview({ features, onNavigate }: { features: LabFeature[]; onNavigate: (tab: string) => void }) {
  return (
    <section className="rounded-md border border-border bg-card/55 p-3">
      <div className="flex items-center justify-between gap-2">
        <div className="flex items-center gap-2 text-sm font-semibold">
          <AlertTriangle className="h-4 w-4 text-warning" />
          Labs
        </div>
        <Button size="sm" variant="ghost" onClick={() => onNavigate("labs")}>
          查看全部
        </Button>
      </div>
      <div className="mt-2 flex flex-wrap gap-1.5">
        {features.map((feature) => (
          <Badge key={feature.id} variant={feature.risk === "high" ? "warning" : "secondary"}>
            {feature.title}
          </Badge>
        ))}
      </div>
    </section>
  );
}

function StatusBadge({ status }: { status: string }) {
  const normalized = status.toLowerCase();
  const variant: "success" | "warning" | "secondary" =
    normalized === "approved" || normalized === "running" || normalized === "queued"
      ? "success"
      : normalized === "planning" || normalized === "proposed"
        ? "warning"
        : "secondary";

  const labelMap: Record<string, string> = {
    draft: "草稿",
    planning: "规划中",
    proposed: "待确认",
    approved: "已确认",
    queued: "已登记",
    running: "运行中",
    completed: "完成",
    failed: "失败",
  };

  return (
    <Badge variant={variant} className="shrink-0">
      {labelMap[normalized] ?? status}
    </Badge>
  );
}

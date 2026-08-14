/**
 * CronTab — 定时计划与执行监视器
 */

import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { useCronStore } from "@/store/AppStore";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Switch } from "@/components/ui/switch";
import { Plus, Zap, Edit, Trash2, Clock, Trash } from "lucide-react";
import type { CronRun, CronTask } from "@/types";

/**
 * 运行状态怎么显示。
 *
 * 以前只判 `success`，剩下**一律**画成「✗ FAILED」。而后端还会写
 * `skipped`（上一轮还在跑，这一轮主动不叠加——这是设计如此，不是失败）、
 * `timeout`（上一轮超时被收掉）和 `running`。把主动跳过标成失败，看的人会去
 * 排查一个根本不存在的故障。
 */
const RUN_BADGE: Record<CronRun["status"], { label: string; variant: "success" | "destructive" | "secondary" }> = {
  success: { label: "✓ SUCCESS", variant: "success" },
  failed: { label: "✗ FAILED", variant: "destructive" },
  running: { label: "… RUNNING", variant: "secondary" },
  skipped: { label: "⤼ SKIPPED", variant: "secondary" },
  timeout: { label: "⏱ TIMEOUT", variant: "destructive" },
};

export function CronTab() {
  const cron = useCronStore();
  const {
    cronTasks, cronRuns,
    deleteCronTask: onDeleteTask,
    toggleCronTask: onToggleTask,
    triggerCronTask: onTriggerTask,
    clearCronRuns: onClearRuns,
  } = cron;
  const onAddTask = () => cron.openCronModal();
  const onEditTask = (task: CronTask) => cron.openCronModal(task);
  return (
    <div className="flex flex-col h-full overflow-hidden flex-1">
      {/* Header */}
      <div className="px-4 py-4 border-b border-border flex justify-between items-center">
        <span className="text-sm font-semibold flex items-center gap-2">
          <Clock className="h-4 w-4" /> 系统已注册的计划任务列表
        </span>
        <Button size="sm" onClick={onAddTask}>
          <Plus className="h-3 w-3" /> 添加计划任务
        </Button>
      </div>

      {/* Content */}
      <div className="flex-1 overflow-y-auto p-4 flex flex-col gap-5">
        {/* Task Cards */}
        <div className="grid grid-cols-[repeat(auto-fit,minmax(280px,1fr))] gap-4">
          {cronTasks.length === 0 ? (
            <div className="col-span-full py-10 text-center">
              <Clock className="h-9 w-9 mx-auto mb-2 text-muted-foreground" />
              <span className="text-muted-foreground">暂无定时计划任务</span>
            </div>
          ) : (
            cronTasks.map((task) => (
              <Card key={task.id}>
                <CardContent className="p-4 flex flex-col gap-3">
                  <div className="flex justify-between items-start">
                    <div>
                      <span className="font-semibold text-sm block">{task.title}</span>
                      <span className="text-xs text-muted-foreground">
                        Agent: {task.agent_name} | Cron: <code>{task.schedule}</code>
                      </span>
                    </div>
                    <Switch
                      checked={task.is_active}
                      onCheckedChange={() => onToggleTask(task)}
                    />
                  </div>

                  <div className="text-xs bg-muted/5 p-2 rounded-md space-y-0.5">
                    <div><span className="text-muted-foreground">工作区:</span> {task.workspace_dir}</div>
                    {task.args && (
                      <div><span className="text-muted-foreground">指令参数:</span> <code>{task.args}</code></div>
                    )}
                  </div>

                  <div className="flex justify-end gap-2">
                    <Button size="sm" variant="outline" onClick={() => onTriggerTask(task.id)}>
                      <Zap className="h-3 w-3" /> 触发
                    </Button>
                    <Button size="sm" variant="outline" onClick={() => onEditTask(task)}>
                      <Edit className="h-3 w-3" /> 编辑
                    </Button>
                    <Button size="sm" variant="outline" onClick={() => onDeleteTask(task.id)}>
                      <Trash2 className="h-3 w-3 text-destructive" /> 删除
                    </Button>
                  </div>
                </CardContent>
              </Card>
            ))
          )}
        </div>

        {/* Recent Runs */}
        <Card>
          <CardHeader className="flex-row justify-between items-center mb-4">
            <CardTitle className="text-sm">📋 最近运行日志</CardTitle>
            {cronRuns.length > 0 && (
              <Button size="sm" variant="outline" onClick={onClearRuns}>
                <Trash className="h-3 w-3" /> 清空历史
              </Button>
            )}
          </CardHeader>
          <CardContent>
            {cronRuns.length === 0 ? (
              <div className="text-center text-muted-foreground text-xs py-5">无执行历史</div>
            ) : (
              <div className="flex flex-col gap-2">
                {cronRuns.slice(0, 10).map((run) => (
                  <div
                    key={run.id}
                    className="flex justify-between items-center text-xs border-b border-border pb-1.5"
                  >
                    <div className="min-w-0">
                      <Badge variant={RUN_BADGE[run.status]?.variant ?? "destructive"}>
                        {RUN_BADGE[run.status]?.label ?? `✗ ${run.status.toUpperCase()}`}
                      </Badge>
                      <span className="text-muted-foreground ml-2">
                        日志: <code>{run.log_path}</code>
                      </span>
                      {/* 这段摘要以前是写进库里就没人读了：查询没选、DTO 没有、
                          界面无从显示。它记的恰恰是这次定时运行往外发了什么。 */}
                      {run.action_summary && (
                        <div className="text-muted-foreground mt-0.5 truncate">
                          {run.action_summary}
                        </div>
                      )}
                    </div>
                    <span className="text-xs text-muted-foreground shrink-0 ml-2">
                      {new Date(run.started_at).toLocaleString()}
                    </span>
                  </div>
                ))}
              </div>
            )}
          </CardContent>
        </Card>
      </div>
    </div>
  );
}

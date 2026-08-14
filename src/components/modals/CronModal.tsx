/**
 * CronModal — 添加/编辑定时计划任务 Dialog
 */

import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogFooter, DialogDescription } from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import type { CronTask, DetectedAgent } from "@/types";

interface CronModalProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  editingCron: CronTask | null;
  cronForm: { title: string; schedule: string; agent_name: string; args: string; workspace_dir: string; is_active: boolean };
  detectedAgents: DetectedAgent[];
  onFormChange: (field: "title" | "schedule" | "agent_name" | "args" | "workspace_dir" | "is_active", value: string | boolean) => void;
  onSave: () => Promise<void>;
}

export function CronModal({
  open,
  onOpenChange,
  editingCron,
  cronForm,
  detectedAgents,
  onFormChange,
  onSave,
}: CronModalProps) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{editingCron ? "编辑定时任务" : "新增定时计划任务"}</DialogTitle>
          <DialogDescription>配置 Cron 调度表达式和执行参数</DialogDescription>
        </DialogHeader>

        <div className="flex flex-col gap-3">
          <div className="space-y-1.5">
            <Label>定时任务标题</Label>
            <Input
              value={cronForm.title}
              onChange={(e) => onFormChange("title", e.target.value)}
              placeholder="例如: 每 15 分钟自动增量 Git 备份"
            />
          </div>
          <div className="space-y-1.5">
            <Label>调度表达式</Label>
            <Input
              value={cronForm.schedule}
              onChange={(e) => onFormChange("schedule", e.target.value)}
              placeholder="如: */15 * * * * 或 every 30 minutes"
            />
            {/*
              这里必须写实话：匹配器每段只认三种写法（星号、星号斜杠 N、纯数字），
              标准 cron 的区间（1-5）和列表（1,3,5）不支持。以前提示里只给了一个
              五字段的例子，用户照标准 cron 写个「0 9 星星 1-5」就得到一条
              「已启用却永远不触发」的任务。现在后端保存那一步会直接拒绝。
            */}
            <p className="text-[11px] text-muted-foreground leading-relaxed">
              五字段 cron（分 时 日 月 周），每段只支持 <code>*</code>、<code>*/N</code> 或数字；
              <b>不支持</b> <code>1-5</code>、<code>1,3,5</code> 这类区间和列表。
              也可以写 <code>every N minutes</code>、<code>every N hours</code>、<code>daily at HH:MM</code>。
            </p>
          </div>
          <div className="grid grid-cols-2 gap-2.5">
            <div className="space-y-1.5">
              <Label>指定执行 Agent</Label>
              <Select value={cronForm.agent_name} onValueChange={(v) => onFormChange("agent_name", v)}>
                <SelectTrigger><SelectValue /></SelectTrigger>
                <SelectContent>
                  {detectedAgents.map((a) => (
                    <SelectItem key={a.name} value={a.name}>{a.name}</SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
            <div className="space-y-1.5">
              <Label>工作区路径</Label>
              <Input
                value={cronForm.workspace_dir}
                onChange={(e) => onFormChange("workspace_dir", e.target.value)}
              />
            </div>
          </div>
          <div className="space-y-1.5">
            <Label>执行附带参数 (JSON)</Label>
            <Input
              value={cronForm.args}
              onChange={(e) => onFormChange("args", e.target.value)}
              placeholder='例如: ["status"]'
            />
          </div>
        </div>

        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)}>取消</Button>
          <Button onClick={onSave}>保存任务</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

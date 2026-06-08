/**
 * AccountModal — 添加/编辑智能体账户凭证 Dialog
 */

import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogFooter, DialogDescription } from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import type { PlatformModel } from "@/types";

interface AccountModalProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  accFormId: string;
  accFormName: string;
  accFormKey: string;
  accFormHost: string;
  accFormModel: string;
  activeModels: PlatformModel[];
  onFieldChange: (field: string, value: string) => void;
  onSave: () => Promise<void>;
}

export function AccountModal({
  open,
  onOpenChange,
  accFormId,
  accFormName,
  accFormKey,
  accFormHost,
  accFormModel,
  activeModels,
  onFieldChange,
  onSave,
}: AccountModalProps) {
  const modelOptions = buildModelOptions(activeModels, accFormModel);

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{accFormId ? "编辑智能体账户" : "新增智能体账户"}</DialogTitle>
          <DialogDescription>配置 API 凭证和路由模型</DialogDescription>
        </DialogHeader>

        <div className="flex flex-col gap-3">
          <div className="space-y-1.5">
            <Label>账户显示名称</Label>
            <Input value={accFormName} onChange={(e) => onFieldChange("name", e.target.value)} placeholder="如: Claude Code 专属或 DeepSeek" />
          </div>
          <div className="space-y-1.5">
            <Label>API Key / 凭证</Label>
            <Input type="password" value={accFormKey} onChange={(e) => onFieldChange("key", e.target.value)} />
          </div>
          <div className="space-y-1.5">
            <Label>API 代理基底地址</Label>
            <Input value={accFormHost} onChange={(e) => onFieldChange("host", e.target.value)} placeholder="如: https://api.anthropic.com/v1" />
          </div>
          <div className="space-y-1.5">
            <Label>目标路由模型</Label>
            <Select value={accFormModel} onValueChange={(v) => onFieldChange("model", v)}>
              <SelectTrigger><SelectValue /></SelectTrigger>
              <SelectContent>
                {modelOptions.map((opt) => (
                  <SelectItem key={opt} value={opt}>{opt}</SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
        </div>

        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)}>取消</Button>
          <Button onClick={onSave}>保存配置</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function buildModelOptions(activeModels: PlatformModel[], current: string): string[] {
  const list = [...activeModels.map((m) => m.model_name)];
  const defaults = ["claude-3-5-sonnet", "deepseek-chat", "gpt-4o", "gemini-2.0-flash", "qwen-plus"];
  defaults.forEach((d) => { if (!list.includes(d)) list.push(d); });
  if (current && !list.includes(current)) list.push(current);
  return list;
}

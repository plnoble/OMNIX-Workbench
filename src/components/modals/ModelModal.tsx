/**
 * ModelModal — 添加自定义模型 Dialog
 *
 * Capabilities are auto-detected from the model name by the backend.
 * Users only need to provide the model ID; the system infers
 * vision/audio/reasoning/coding/etc. flags automatically.
 */

import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogFooter, DialogDescription } from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Sparkles } from "lucide-react";

interface ModelModalProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  modelForm: { model_name: string };
  onNameChange: (name: string) => void;
  onSave: () => Promise<void>;
}

export function ModelModal({
  open,
  onOpenChange,
  modelForm,
  onNameChange,
  onSave,
}: ModelModalProps) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>添加自定义模型</DialogTitle>
          <DialogDescription>输入模型标识名称，系统将自动检测模型能力</DialogDescription>
        </DialogHeader>

        <div className="flex flex-col gap-3">
          <div className="space-y-1.5">
            <Label>模型标识名称</Label>
            <Input
              value={modelForm.model_name}
              onChange={(e) => onNameChange(e.target.value)}
              placeholder="例如: deepseek-coder 或 qwen-2.5-coder"
            />
          </div>

          <div className="flex items-center gap-2 px-3 py-2.5 rounded-lg bg-muted/5 border border-border text-xs text-muted-foreground">
            <Sparkles className="h-4 w-4 text-primary shrink-0" />
            <span>
              模型的视觉、推理、编程等能力标识将由系统根据模型名称自动识别，
              无需手动设置。保存后可在模型列表中查看检测到的能力。
            </span>
          </div>
        </div>

        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)}>取消</Button>
          <Button onClick={onSave}>确定添加</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

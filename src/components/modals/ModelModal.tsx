/**
 * ModelModal — 添加自定义模型 Dialog
 */

import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogFooter, DialogDescription } from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Checkbox } from "@/components/ui/checkbox";
import { Eye, Mic, Brain, Code, Maximize2, Wrench, Layers, Zap } from "lucide-react";

interface ModelFormState {
  model_name: string;
  has_vision: boolean;
  has_audio: boolean;
  has_reasoning: boolean;
  has_coding: boolean;
  has_long_context: boolean;
  has_tool_use: boolean;
  has_embedding: boolean;
  has_speedy: boolean;
}

type CapabilityField = keyof Omit<ModelFormState, "model_name">;

interface ModelModalProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  modelForm: ModelFormState;
  onFormChange: (field: CapabilityField, value: boolean) => void;
  onNameChange: (name: string) => void;
  onSave: () => Promise<void>;
}

const CAPABILITIES: { field: CapabilityField; label: string; icon: React.ReactNode }[] = [
  { field: "has_vision", label: "视觉", icon: <Eye className="h-3.5 w-3.5 text-blue-400" /> },
  { field: "has_audio", label: "音频", icon: <Mic className="h-3.5 w-3.5 text-purple-400" /> },
  { field: "has_reasoning", label: "推理", icon: <Brain className="h-3.5 w-3.5 text-amber-400" /> },
  { field: "has_coding", label: "编程", icon: <Code className="h-3.5 w-3.5 text-green-400" /> },
  { field: "has_long_context", label: "长上下文", icon: <Maximize2 className="h-3.5 w-3.5 text-cyan-400" /> },
  { field: "has_tool_use", label: "工具调用", icon: <Wrench className="h-3.5 w-3.5 text-orange-400" /> },
  { field: "has_embedding", label: "嵌入", icon: <Layers className="h-3.5 w-3.5 text-pink-400" /> },
  { field: "has_speedy", label: "快速", icon: <Zap className="h-3.5 w-3.5 text-yellow-400" /> },
];

export function ModelModal({
  open,
  onOpenChange,
  modelForm,
  onFormChange,
  onNameChange,
  onSave,
}: ModelModalProps) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>添加自定义模型</DialogTitle>
          <DialogDescription>指定模型名称和能力标记</DialogDescription>
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

          <div className="grid grid-cols-2 gap-2.5 mt-2.5">
            {CAPABILITIES.map(({ field, label, icon }) => (
              <label key={field} className="flex items-center gap-2 cursor-pointer text-xs">
                <Checkbox checked={modelForm[field] as boolean} onCheckedChange={(v) => onFormChange(field, !!v)} />
                {icon} {label}
              </label>
            ))}
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

/**
 * PlatformModal — 添加/编辑模型平台 Dialog
 */

import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogFooter, DialogDescription } from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { toast } from "@/components/ui/sonner";
import type { ModelPlatform, ProviderType } from "@/types";
import { PROVIDER_TYPES } from "@/lib/constants";

interface PlatformModalProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  editingPlatform: ModelPlatform | null;
  platformForm: { name: string; api_type: ProviderType; api_key: string; api_address: string };
  onFormChange: (field: "name" | "api_type" | "api_key" | "api_address", value: string) => void;
  onSave: () => Promise<void>;
}

export function PlatformModal({
  open,
  onOpenChange,
  editingPlatform,
  platformForm,
  onFormChange,
  onSave,
}: PlatformModalProps) {
  const handleSave = async () => {
    try {
      await onSave();
    } catch (e) {
      toast.error("保存失败：" + e);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{editingPlatform ? "编辑模型平台" : "添加模型平台"}</DialogTitle>
          <DialogDescription>配置模型提供商的连接参数</DialogDescription>
        </DialogHeader>

        <div className="flex flex-col gap-3">
          <div className="space-y-1.5">
            <Label>平台显示名称</Label>
            <Input
              value={platformForm.name}
              onChange={(e) => onFormChange("name", e.target.value)}
              placeholder="例如: Ollama 或 DeepSeek"
            />
          </div>
          <div className="space-y-1.5">
            <Label>平台协议类型</Label>
            <Select value={platformForm.api_type} onValueChange={(v) => onFormChange("api_type", v)}>
              <SelectTrigger><SelectValue /></SelectTrigger>
              <SelectContent>
                {Object.entries(PROVIDER_TYPES).map(([value, label]) => (
                  <SelectItem key={value} value={value}>{label}</SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
          <div className="space-y-1.5">
            <Label>API Key / 密钥 (本地 Ollama 可不填)</Label>
            <Input
              type="password"
              value={platformForm.api_key}
              onChange={(e) => onFormChange("api_key", e.target.value)}
            />
          </div>
          <div className="space-y-1.5">
            <Label>API 基准地址 (Endpoint URL)</Label>
            <Input
              value={platformForm.api_address}
              onChange={(e) => onFormChange("api_address", e.target.value)}
              placeholder="例如: http://localhost:11434"
            />
          </div>
        </div>

        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)}>取消</Button>
          <Button onClick={handleSave}>保存</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

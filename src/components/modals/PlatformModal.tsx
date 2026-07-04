/**
 * PlatformModal — 添加/编辑模型平台 Dialog
 *
 * API Key management: multi-key with encrypted storage.
 * Each platform can have multiple API keys; one is active at a time.
 */

import { useState, useEffect } from "react";
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogFooter, DialogDescription } from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { toast } from "@/components/ui/sonner";
import { Eye, EyeOff, Plus, Trash2, Check, Copy } from "lucide-react";
import type { ModelPlatform, ProviderType, PlatformApiKey } from "@/types";
import { PROVIDER_TYPES } from "@/lib/constants";
import { apiKeyApi } from "@/lib/tauri-api";

// Quick-add presets (ZCF inspired): correct protocol + endpoint so users don't
// mis-configure (e.g. Volcano as Anthropic). The user only adds the API Key.
const PROVIDER_PRESETS: { id: string; label: string; api_type: ProviderType; api_address: string }[] = [
  { id: "agnes", label: "Agnes AI（多模态：文本/生图/生视频）", api_type: "openai", api_address: "https://apihub.agnes-ai.com/v1" },
  { id: "deepseek", label: "DeepSeek", api_type: "openai", api_address: "https://api.deepseek.com/v1" },
  { id: "volcengine", label: "火山引擎（OpenAI 兼容）", api_type: "openai", api_address: "https://ark.cn-beijing.volces.com/api/v3" },
  { id: "openai", label: "OpenAI", api_type: "openai", api_address: "https://api.openai.com/v1" },
  { id: "anthropic", label: "Anthropic（Claude）", api_type: "anthropic", api_address: "https://api.anthropic.com" },
  { id: "openrouter", label: "OpenRouter", api_type: "openai", api_address: "https://openrouter.ai/api/v1" },
  { id: "siliconflow", label: "硅基流动 SiliconFlow", api_type: "openai", api_address: "https://api.siliconflow.cn/v1" },
  { id: "zhipu", label: "智谱 GLM", api_type: "openai", api_address: "https://open.bigmodel.cn/api/paas/v4" },
  { id: "moonshot", label: "月之暗面 Kimi", api_type: "openai", api_address: "https://api.moonshot.cn/v1" },
  { id: "bailian", label: "阿里百炼（Qwen）", api_type: "openai", api_address: "https://dashscope.aliyuncs.com/compatible-mode/v1" },
  { id: "ollama", label: "Ollama（本地）", api_type: "ollama", api_address: "http://localhost:11434" },
  { id: "lmstudio", label: "LM Studio（本地）", api_type: "openai", api_address: "http://localhost:1234/v1" },
];

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
  const [apiKeys, setApiKeys] = useState<PlatformApiKey[]>([]);
  const [newKey, setNewKey] = useState("");
  const [newKeyLabel, setNewKeyLabel] = useState("");
  const [revealedKeyId, setRevealedKeyId] = useState<string | null>(null);
  const [revealedKeyValue, setRevealedKeyValue] = useState("");

  // Load keys when editing an existing platform
  useEffect(() => {
    if (open && editingPlatform?.id) {
      apiKeyApi.list(editingPlatform.id).then(setApiKeys).catch(() => setApiKeys([]));
    } else {
      setApiKeys([]);
    }
    setNewKey("");
    setNewKeyLabel("");
    setRevealedKeyId(null);
  }, [open, editingPlatform?.id]);

  const handleAddKey = async () => {
    if (!editingPlatform?.id || !newKey.trim()) return;
    try {
      const added = await apiKeyApi.add(editingPlatform.id, newKey.trim(), newKeyLabel.trim() || undefined);
      setApiKeys((prev) => [...prev, added]);
      setNewKey("");
      setNewKeyLabel("");
      toast.success("API Key 已添加");
    } catch (e) {
      toast.error("添加失败：" + e);
    }
  };

  const handleSelectKey = async (keyId: string) => {
    try {
      await apiKeyApi.select(keyId);
      setApiKeys((prev) => prev.map((k) => ({ ...k, is_active: k.id === keyId })));
      toast.success("已切换活跃 Key");
    } catch (e) {
      toast.error("切换失败：" + e);
    }
  };

  const handleDeleteKey = async (keyId: string) => {
    try {
      await apiKeyApi.delete(keyId);
      if (editingPlatform?.id) {
        const updated = await apiKeyApi.list(editingPlatform.id);
        setApiKeys(updated);
      }
      toast.success("Key 已删除");
    } catch (e) {
      toast.error("删除失败：" + e);
    }
  };

  const handleCopyKey = async (keyId: string) => {
    try {
      const full = await apiKeyApi.reveal(keyId);
      await navigator.clipboard.writeText(full);
      toast.success("已复制到剪贴板");
    } catch (e) {
      toast.error("复制失败：" + e);
    }
  };

  const handleRevealKey = async (keyId: string) => {
    if (revealedKeyId === keyId) {
      setRevealedKeyId(null);
      setRevealedKeyValue("");
      return;
    }
    try {
      const full = await apiKeyApi.reveal(keyId);
      setRevealedKeyId(keyId);
      setRevealedKeyValue(full);
    } catch (e) {
      toast.error("解密失败：" + e);
    }
  };

  const handleSave = async () => {
    try {
      await onSave();
    } catch (e) {
      toast.error("保存失败：" + e);
    }
  };

  const isOllama = platformForm.api_type === "ollama";

  const applyPreset = (id: string) => {
    const preset = PROVIDER_PRESETS.find((p) => p.id === id);
    if (!preset) return;
    onFormChange("name", preset.label);
    onFormChange("api_type", preset.api_type);
    onFormChange("api_address", preset.api_address);
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-md">
        <DialogHeader>
          <DialogTitle>{editingPlatform ? "编辑模型平台" : "添加模型平台"}</DialogTitle>
          <DialogDescription>配置模型提供商的连接参数</DialogDescription>
        </DialogHeader>

        <div className="flex flex-col gap-3">
          {!editingPlatform && (
            <div className="space-y-1.5">
              <Label>快速预设（可选）</Label>
              <Select value="" onValueChange={applyPreset}>
                <SelectTrigger><SelectValue placeholder="选择一个常用供应商，自动填好协议和地址" /></SelectTrigger>
                <SelectContent>
                  {PROVIDER_PRESETS.map((preset) => (
                    <SelectItem key={preset.id} value={preset.id}>{preset.label}</SelectItem>
                  ))}
                </SelectContent>
              </Select>
              <p className="text-xs text-muted-foreground">选预设后只需填 API Key。第三方模型对 Codex 建议用 OpenAI 协议。</p>
            </div>
          )}
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

          {/* API Key Management */}
          {!isOllama && (
            <div className="space-y-2">
              <Label className="text-sm font-medium">API Key 管理</Label>

              {/* Existing keys list */}
              {apiKeys.length > 0 && (
                <div className="flex flex-col gap-1.5">
                  {apiKeys.map((k) => (
                    <div key={k.id} className="flex items-center gap-2 px-2.5 py-1.5 rounded-md bg-muted/5 border border-border text-xs">
                      <button
                        onClick={() => handleSelectKey(k.id)}
                        className={`shrink-0 w-4 h-4 rounded-full border-2 flex items-center justify-center transition-colors ${
                          k.is_active ? "border-primary bg-primary/20" : "border-muted-foreground/30 hover:border-muted-foreground/60"
                        }`}
                        title={k.is_active ? "当前活跃 Key" : "点击设为活跃"}
                      >
                        {k.is_active && <Check className="h-2.5 w-2.5 text-primary" />}
                      </button>
                      <span className="font-mono text-muted-foreground flex-1 truncate">
                        {revealedKeyId === k.id ? revealedKeyValue : k.masked_key}
                      </span>
                      {k.label && <span className="text-muted-foreground/60 shrink-0">({k.label})</span>}
                      <span
                        className={`h-2 w-2 shrink-0 rounded-full ${k.last_status === "success" ? "bg-success" : k.last_status === "error" ? "bg-destructive" : "bg-muted-foreground/40"}`}
                        title={k.last_checked_at ? `${k.last_status} · ${k.latency_ms ?? "-"} ms${k.last_error ? ` · ${k.last_error}` : ""}` : "尚未参与运行请求"}
                      />
                      <button onClick={() => handleRevealKey(k.id)} className="shrink-0 text-muted-foreground hover:text-foreground" title="显示/隐藏">
                        {revealedKeyId === k.id ? <EyeOff className="h-3 w-3" /> : <Eye className="h-3 w-3" />}
                      </button>
                      <button onClick={() => handleCopyKey(k.id)} className="shrink-0 text-muted-foreground hover:text-foreground" title="复制">
                        <Copy className="h-3 w-3" />
                      </button>
                      <button onClick={() => handleDeleteKey(k.id)} className="shrink-0 text-muted-foreground hover:text-destructive" title="删除">
                        <Trash2 className="h-3 w-3" />
                      </button>
                    </div>
                  ))}
                </div>
              )}

              {/* Add new key */}
              {editingPlatform?.id && (
                <div className="flex gap-1.5">
                  <Input
                    type="password"
                    value={newKey}
                    onChange={(e) => setNewKey(e.target.value)}
                    placeholder="输入 API Key"
                    className="flex-1 text-xs h-8"
                  />
                  <Input
                    value={newKeyLabel}
                    onChange={(e) => setNewKeyLabel(e.target.value)}
                    placeholder="标签"
                    className="w-20 text-xs h-8"
                  />
                  <Button size="sm" variant="outline" onClick={handleAddKey} disabled={!newKey.trim()} className="h-8 px-2">
                    <Plus className="h-3 w-3" />
                  </Button>
                </div>
              )}

              {/* For new platforms (not yet saved), show simple key input */}
              {!editingPlatform?.id && (
                <Input
                  type="password"
                  value={platformForm.api_key}
                  onChange={(e) => onFormChange("api_key", e.target.value)}
                  placeholder="输入 API Key（保存后可添加更多）"
                />
              )}
            </div>
          )}

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

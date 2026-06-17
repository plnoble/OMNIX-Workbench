import { useEffect, useState } from "react";
import { FolderOpen, Loader2 } from "lucide-react";

import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { projectProtocolApi, shellApi } from "@/lib/tauri-api";
import type { ProtocolInitPreview } from "@/lib/tauri-api";
import { toast } from "@/components/ui/sonner";

interface WorkspaceModalProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  workspaceFormPath: string;
  onPathChange: (path: string) => void;
  onSave: (options?: { enableProjectProtocol: boolean }) => Promise<void>;
}

export function WorkspaceModal({
  open,
  onOpenChange,
  workspaceFormPath,
  onPathChange,
  onSave,
}: WorkspaceModalProps) {
  const [isPicking, setIsPicking] = useState(false);
  const [isSaving, setIsSaving] = useState(false);
  const [enableProjectProtocol, setEnableProjectProtocol] = useState(false);
  const [preview, setPreview] = useState<ProtocolInitPreview | null>(null);
  const [isPreviewing, setIsPreviewing] = useState(false);

  useEffect(() => {
    if (!open) {
      setPreview(null);
      setEnableProjectProtocol(false);
      setIsSaving(false);
    }
  }, [open]);

  useEffect(() => {
    if (!enableProjectProtocol || !workspaceFormPath.trim()) {
      setPreview(null);
      return;
    }

    let cancelled = false;
    setIsPreviewing(true);
    projectProtocolApi
      .previewInit(workspaceFormPath)
      .then((result) => {
        if (!cancelled) setPreview(result);
      })
      .catch((error) => {
        if (!cancelled) {
          setPreview(null);
          toast.error("协议初始化预览失败：" + error);
        }
      })
      .finally(() => {
        if (!cancelled) setIsPreviewing(false);
      });
    return () => {
      cancelled = true;
    };
  }, [enableProjectProtocol, workspaceFormPath]);

  const chooseFolder = async () => {
    setIsPicking(true);
    try {
      const selected = await shellApi.pickDirectory();
      if (selected) onPathChange(selected);
    } catch (error) {
      toast.error("打开文件夹选择器失败：" + error);
    } finally {
      setIsPicking(false);
    }
  };

  const save = async () => {
    setIsSaving(true);
    try {
      await onSave({ enableProjectProtocol });
    } finally {
      setIsSaving(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-2xl">
        <DialogHeader>
          <DialogTitle>选择工作区</DialogTitle>
          <DialogDescription>选择一个本地项目文件夹，后续 Agent 会在这个目录中工作。</DialogDescription>
        </DialogHeader>

        <div className="space-y-2">
          <Label>工作区文件夹</Label>
          <div className="flex gap-2">
            <Input
              value={workspaceFormPath}
              readOnly
              placeholder="请选择电脑中的项目文件夹"
              className="flex-1"
            />
            <Button type="button" variant="outline" onClick={chooseFolder} disabled={isPicking}>
              {isPicking ? <Loader2 className="h-4 w-4 animate-spin" /> : <FolderOpen className="h-4 w-4" />}
              选择
            </Button>
          </div>
          <p className="text-xs text-muted-foreground">不需要手动输入路径，点击“选择”打开系统文件夹选择器。</p>
        </div>

        <div className="mt-4 rounded-md border border-border bg-card/50 p-3">
          <label className="flex cursor-pointer items-start gap-3">
            <Checkbox
              checked={enableProjectProtocol}
              onCheckedChange={(checked) => setEnableProjectProtocol(checked === true)}
              disabled={!workspaceFormPath.trim()}
            />
            <span className="min-w-0">
              <span className="block text-sm font-medium">启用项目协议</span>
              <span className="mt-1 block text-xs leading-5 text-muted-foreground">
                创建 AGENTS.md、.omx/development 记录和本地 Claude skill 桥接文件。已有文件只跳过，不覆盖。
              </span>
            </span>
          </label>

          {enableProjectProtocol && (
            <div className="mt-3 rounded-md border border-border bg-background/70 p-3">
              {isPreviewing ? (
                <div className="flex items-center gap-2 text-sm text-muted-foreground">
                  <Loader2 className="h-4 w-4 animate-spin" />
                  正在生成初始化预览...
                </div>
              ) : preview ? (
                <div>
                  <div className="mb-2 text-xs text-muted-foreground">
                    将创建 {preview.will_create_count} 个，跳过 {preview.will_skip_count} 个。
                  </div>
                  <div className="max-h-48 space-y-1 overflow-y-auto pr-1">
                    {preview.files.map((file) => (
                      <div key={file.path} className="flex items-center justify-between gap-3 rounded border border-border px-2 py-1.5 text-xs">
                        <span className="min-w-0 truncate" title={file.path}>{file.path}</span>
                        <span className={file.action === "create" ? "text-primary" : "text-muted-foreground"}>
                          {file.action === "create" ? "创建" : "跳过"}
                        </span>
                      </div>
                    ))}
                  </div>
                </div>
              ) : (
                <div className="text-sm text-muted-foreground">选择工作区后显示预览。</div>
              )}
            </div>
          )}
        </div>

        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)}>
            取消
          </Button>
          <Button onClick={save} disabled={!workspaceFormPath.trim() || isSaving || isPreviewing}>
            {isSaving ? <Loader2 className="h-4 w-4 animate-spin" /> : null}
            {enableProjectProtocol ? "确认并载入" : "载入工作区"}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

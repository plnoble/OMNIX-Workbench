/**
 * WorkspaceModal — 载入项目开发工作区 Dialog
 */

import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogFooter, DialogDescription } from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";

interface WorkspaceModalProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  workspaceFormPath: string;
  onPathChange: (path: string) => void;
  onSave: () => Promise<void>;
}

export function WorkspaceModal({
  open,
  onOpenChange,
  workspaceFormPath,
  onPathChange,
  onSave,
}: WorkspaceModalProps) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>载入项目开发工作区</DialogTitle>
          <DialogDescription>指定本地项目目录作为智能体的工作环境</DialogDescription>
        </DialogHeader>

        <div className="space-y-1.5">
          <Label>工作区绝对路径</Label>
          <Input
            value={workspaceFormPath}
            onChange={(e) => onPathChange(e.target.value)}
            placeholder="如: d:/Agent/Project/MyWebDemo"
          />
        </div>

        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)}>取消</Button>
          <Button onClick={onSave}>确认载入</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

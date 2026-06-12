/**
 * PreviewPane — Live preview sidebar for workspace files
 */

import { Button } from "@/components/ui/button";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { X, RefreshCw, Search } from "lucide-react";
import type { PreviewType } from "@/types";

interface PreviewPaneProps {
  previewFiles: string[];
  selectedPreviewFile: string;
  previewType: PreviewType;
  previewHtmlUrl: string;
  previewTextContent: string;
  previewImageBase64: string;
  chatWorkspace: string;
  onSelectFile: (file: string) => void;
  onRefreshFiles: () => void;
  onLoadGitDiff: () => void;
  onClose: () => void;
}

export function PreviewPane({
  previewFiles,
  selectedPreviewFile,
  previewType,
  previewHtmlUrl,
  previewTextContent,
  previewImageBase64,
  onSelectFile,
  onRefreshFiles,
  onLoadGitDiff,
  onClose,
}: PreviewPaneProps) {
  return (
    <aside className="w-[min(420px,30vw)] border-l border-border glass-panel flex flex-col h-full">
      {/* Header */}
      <div className="p-4 border-b border-border flex justify-between items-center">
        <span className="text-sm font-semibold">👁️ 实时预览</span>
        <button onClick={onClose} className="bg-transparent border-none text-muted-foreground cursor-pointer hover:text-foreground" aria-label="关闭预览">
          <X className="h-4 w-4" />
        </button>
      </div>

      {/* File Selector */}
      <div className="p-3 flex flex-col gap-2.5 border-b border-border">
        <div className="flex gap-1.5">
          <Select value={selectedPreviewFile} onValueChange={onSelectFile}>
            <SelectTrigger className="flex-1">
              <SelectValue placeholder="-- 选择预览文件 --" />
            </SelectTrigger>
            <SelectContent>
              {previewFiles.map((f) => (
                <SelectItem key={f} value={f}>{f}</SelectItem>
              ))}
            </SelectContent>
          </Select>
          <Button size="sm" variant="outline" onClick={onRefreshFiles}>
            <RefreshCw className="h-3 w-3" />
          </Button>
        </div>
        <Button size="sm" variant="outline" className="w-full" onClick={onLoadGitDiff}>
          <Search className="h-3 w-3" /> 查看当前工作区 Git Diff
        </Button>
      </div>

      {/* Content */}
      <div className="flex-1 overflow-auto p-4">
        {selectedPreviewFile ? (
          <PreviewContent
            previewType={previewType}
            previewHtmlUrl={previewHtmlUrl}
            previewTextContent={previewTextContent}
            previewImageBase64={previewImageBase64}
          />
        ) : (
          <div className="text-center py-10 text-muted-foreground text-xs">
            请在上方选择一个文件或查看 Git Diff
          </div>
        )}
      </div>
    </aside>
  );
}

function PreviewContent({
  previewType,
  previewHtmlUrl,
  previewTextContent,
  previewImageBase64,
}: {
  previewType: PreviewType;
  previewHtmlUrl: string;
  previewTextContent: string;
  previewImageBase64: string;
}) {
  if (previewType === "html" && previewHtmlUrl) {
    return <iframe src={previewHtmlUrl} className="w-full h-full border-none bg-white" title="HTML Preview" />;
  }

  if (previewType === "image" && previewImageBase64) {
    return (
      <div className="text-center">
        <img src={`data:image/png;base64,${previewImageBase64}`} className="max-w-full max-h-full" alt="preview" />
      </div>
    );
  }

  if (previewType === "diff") {
    return (
      <pre className="text-xs font-mono text-foreground whitespace-pre-wrap m-0">
        {previewTextContent.split("\n").map((line, i) => {
          const color = line.startsWith("+") ? "text-emerald-500" : line.startsWith("-") ? "text-red-500" : line.startsWith("@@") ? "text-purple-500" : "text-foreground";
          return <div key={i} className={color}>{line}</div>;
        })}
      </pre>
    );
  }

  // markdown / text
  return (
    <div className="text-sm leading-relaxed">
      <pre className="whitespace-pre-wrap break-all">{previewTextContent}</pre>
    </div>
  );
}

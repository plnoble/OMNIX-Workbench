import { useEffect, useState } from "react";
import ReactMarkdown from "react-markdown";
import { openPath } from "@tauri-apps/plugin-opener";
import { FileCode2, Image as ImageIcon, FileText, ExternalLink, X, Loader2 } from "lucide-react";

import { workspaceApi, type FilePreview } from "@/lib/tauri-api";
import { toast } from "@/components/ui/sonner";

/**
 * FilePreviewPanel — in-app preview of a workspace file (R2 / file preview
 * panel, AionUi inspired). Renders code/text as monospaced text, Markdown
 * rendered, images and PDF inline; binary/Office files offer "open with system".
 */
interface Props {
  workspacePath: string;
  relativePath: string;
  onClose: () => void;
}

function humanSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

export function FilePreviewPanel({ workspacePath, relativePath, onClose }: Props) {
  const [preview, setPreview] = useState<FilePreview | null>(null);
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    let active = true;
    setLoading(true);
    setError("");
    setPreview(null);
    workspaceApi
      .readFile(workspacePath, relativePath)
      .then((result) => {
        if (active) setPreview(result);
      })
      .catch((e) => {
        if (active) setError(String(e));
      })
      .finally(() => {
        if (active) setLoading(false);
      });
    return () => {
      active = false;
    };
  }, [workspacePath, relativePath]);

  const openExternal = () => {
    const sep = workspacePath.includes("\\") ? "\\" : "/";
    openPath(`${workspacePath}${sep}${relativePath.replace(/[\\/]/g, sep)}`).catch((e) =>
      toast.error(`无法打开：${e}`),
    );
  };

  const icon =
    preview?.kind === "image" ? <ImageIcon className="h-4 w-4" /> :
    preview?.kind === "markdown" || preview?.kind === "text" ? <FileText className="h-4 w-4" /> :
    <FileCode2 className="h-4 w-4" />;

  return (
    <div className="absolute inset-y-0 right-0 z-40 flex w-[min(48rem,94vw)] flex-col border-l border-border bg-background shadow-2xl">
      <div className="flex items-center gap-2 border-b border-border px-4 py-3">
        {icon}
        <span className="min-w-0 flex-1 truncate text-sm font-medium" title={relativePath}>{relativePath}</span>
        {preview && <span className="shrink-0 text-xs text-muted-foreground">{humanSize(preview.size)}</span>}
        <button onClick={openExternal} title="用系统应用打开" className="rounded p-1 text-muted-foreground hover:bg-muted/20 hover:text-foreground">
          <ExternalLink className="h-4 w-4" />
        </button>
        <button onClick={onClose} title="关闭预览" className="rounded p-1 text-muted-foreground hover:bg-muted/20 hover:text-foreground">
          <X className="h-4 w-4" />
        </button>
      </div>

      <div className="min-h-0 flex-1 overflow-auto">
        {loading ? (
          <div className="flex h-full items-center justify-center gap-2 text-sm text-muted-foreground">
            <Loader2 className="h-4 w-4 animate-spin" /> 读取文件…
          </div>
        ) : error ? (
          <div className="p-6 text-sm text-destructive">{error}</div>
        ) : !preview ? null : preview.kind === "image" ? (
          <div className="flex h-full items-center justify-center p-4">
            <img src={preview.content} alt={relativePath} className="max-h-full max-w-full object-contain" />
          </div>
        ) : preview.kind === "pdf" ? (
          <iframe src={preview.content} title={relativePath} className="h-full w-full border-0" />
        ) : preview.kind === "binary" ? (
          <div className="flex h-full flex-col items-center justify-center gap-3 p-6 text-center">
            <FileCode2 className="h-10 w-10 text-muted-foreground" />
            <p className="text-sm text-muted-foreground">二进制文件，无法在应用内预览。</p>
            <button onClick={openExternal} className="flex items-center gap-2 rounded-md border border-border px-3 py-1.5 text-sm hover:bg-muted/20">
              <ExternalLink className="h-4 w-4" /> 用系统应用打开
            </button>
          </div>
        ) : preview.kind === "markdown" ? (
          <div className="prose prose-sm prose-invert max-w-none p-5">
            <ReactMarkdown>{preview.content}</ReactMarkdown>
          </div>
        ) : (
          <pre className="overflow-auto p-4 text-xs leading-5">
            <code>{preview.content}</code>
          </pre>
        )}
        {preview?.truncated && (
          <div className="border-t border-border bg-warning/10 px-4 py-2 text-xs text-warning">文件较大，仅预览前 512 KB。</div>
        )}
      </div>
    </div>
  );
}

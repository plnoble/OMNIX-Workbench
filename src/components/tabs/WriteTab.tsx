/**
 * WriteTab — Markdown writing workspace (DeepSeek-GUI Write mode inspired).
 *
 * A "space" is a folder of `.md` files. Left: space selector + file tree.
 * Center: a Source / Split / Preview editor with save + HTML export. Selecting
 * text reveals an inline writing assistant (润色 / 续写 / 精简) that runs the
 * current chat model on the selection and edits the document in place.
 */
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import ReactMarkdown from "react-markdown";
import {
  Columns2,
  Download,
  Eye,
  FilePlus2,
  FileText,
  FolderPlus,
  Loader2,
  Pencil,
  Save,
  Sparkles,
  Trash2,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { toast } from "@/components/ui/sonner";
import { cn } from "@/lib/utils";
import { writeApi, modelApi, settingsApi, qaApi, shellApi, type WriteSpace, type WriteFile } from "@/lib/tauri-api";

type EditorMode = "source" | "split" | "preview";

const ASSIST_ACTIONS: { id: string; label: string; build: (text: string) => string; mode: "replace" | "append" }[] = [
  { id: "polish", label: "润色", mode: "replace", build: (t) => `请在保持原意与语气的前提下润色下面这段文字，只返回润色后的文本，不要任何解释：\n\n${t}` },
  { id: "continue", label: "续写", mode: "append", build: (t) => `请顺着下面这段文字自然地续写一段，只返回续写的新内容，不要重复原文：\n\n${t}` },
  { id: "shorten", label: "精简", mode: "replace", build: (t) => `请精简下面这段文字，保留关键信息，只返回精简后的文本：\n\n${t}` },
];

function buildExportHtml(title: string, bodyHtml: string): string {
  return `<!doctype html><html lang="zh"><head><meta charset="utf-8"><title>${title}</title>
<style>body{max-width:760px;margin:40px auto;padding:0 20px;font:16px/1.7 -apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif;color:#1a1a1a}
h1,h2,h3{line-height:1.3}pre{background:#f5f5f5;padding:12px;border-radius:6px;overflow:auto}
code{background:#f0f0f0;padding:2px 5px;border-radius:4px}blockquote{border-left:4px solid #ddd;margin:0;padding-left:16px;color:#555}
table{border-collapse:collapse}td,th{border:1px solid #ddd;padding:6px 10px}img{max-width:100%}</style></head>
<body>${bodyHtml}</body></html>`;
}

export function WriteTab() {
  const [spaces, setSpaces] = useState<WriteSpace[]>([]);
  const [activeSpace, setActiveSpace] = useState<string>("");
  const [files, setFiles] = useState<WriteFile[]>([]);
  const [activeFile, setActiveFile] = useState<string>("");
  const [content, setContent] = useState("");
  const [savedContent, setSavedContent] = useState("");
  const [mode, setMode] = useState<EditorMode>("split");
  const [chatModel, setChatModel] = useState("");
  const [assistBusy, setAssistBusy] = useState(false);
  const [selection, setSelection] = useState<{ start: number; end: number }>({ start: 0, end: 0 });

  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const previewRef = useRef<HTMLDivElement>(null);

  const dirty = content !== savedContent;
  const hasSelection = selection.end > selection.start;

  // ── Load spaces + resolve a chat model for the inline assistant ──
  useEffect(() => {
    void (async () => {
      try {
        const list = await writeApi.listSpaces();
        setSpaces(list);
        setActiveSpace((current) => current || list[0]?.path || "");
      } catch (error) {
        toast.error(`读取写作空间失败：${error}`);
      }
      try {
        const [qaModel, targetModel] = await Promise.all([
          settingsApi.get("quick_assistant_model"),
          settingsApi.get("target_model"),
        ]);
        let model = qaModel || targetModel || "";
        if (!model) model = (await modelApi.getAvailableNames())[0] || "";
        setChatModel(model);
      } catch {
        /* assistant just stays disabled until a model exists */
      }
    })();
  }, []);

  const loadFiles = useCallback(async (spacePath: string) => {
    if (!spacePath) return;
    try {
      setFiles(await writeApi.listFiles(spacePath));
    } catch (error) {
      toast.error(`读取文件失败：${error}`);
    }
  }, []);

  useEffect(() => {
    void loadFiles(activeSpace);
  }, [activeSpace, loadFiles]);

  const openFile = useCallback(async (spacePath: string, rel: string) => {
    try {
      const text = await writeApi.readFile(spacePath, rel);
      setActiveFile(rel);
      setContent(text);
      setSavedContent(text);
    } catch (error) {
      toast.error(`打开失败：${error}`);
    }
  }, []);

  const save = useCallback(async () => {
    if (!activeFile || !dirty) return;
    try {
      await writeApi.saveFile(activeSpace, activeFile, content);
      setSavedContent(content);
      void loadFiles(activeSpace);
    } catch (error) {
      toast.error(`保存失败：${error}`);
    }
  }, [activeFile, activeSpace, content, dirty, loadFiles]);

  // Ctrl/Cmd+S saves.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "s") {
        e.preventDefault();
        void save();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [save]);

  const newFile = async () => {
    const name = window.prompt("新文件名（无需 .md）");
    if (!name?.trim()) return;
    try {
      const rel = await writeApi.createFile(activeSpace, name.trim());
      await loadFiles(activeSpace);
      await openFile(activeSpace, rel);
    } catch (error) {
      toast.error(`新建失败：${error}`);
    }
  };

  const renameFile = async (file: WriteFile) => {
    const name = window.prompt("重命名为（无需 .md）", file.name.replace(/\.md$/, ""));
    if (!name?.trim()) return;
    try {
      const rel = await writeApi.renameFile(activeSpace, file.relative_path, name.trim());
      await loadFiles(activeSpace);
      if (activeFile === file.relative_path) setActiveFile(rel);
    } catch (error) {
      toast.error(`重命名失败：${error}`);
    }
  };

  const deleteFile = async (file: WriteFile) => {
    if (!window.confirm(`删除「${file.name}」？此操作不可恢复。`)) return;
    try {
      await writeApi.deleteFile(activeSpace, file.relative_path);
      if (activeFile === file.relative_path) {
        setActiveFile("");
        setContent("");
        setSavedContent("");
      }
      await loadFiles(activeSpace);
    } catch (error) {
      toast.error(`删除失败：${error}`);
    }
  };

  const addSpace = async () => {
    try {
      const picked = await shellApi.pickDirectory();
      if (!picked) return;
      const space = await writeApi.addSpace(picked);
      setSpaces(await writeApi.listSpaces());
      setActiveSpace(space.path);
    } catch (error) {
      toast.error(`添加空间失败：${error}`);
    }
  };

  const exportHtml = async () => {
    if (!activeFile) return;
    const body = previewRef.current?.innerHTML;
    if (!body) {
      toast.error("没有可导出的内容");
      return;
    }
    try {
      const title = activeFile.replace(/\.md$/, "");
      const path = await writeApi.exportHtml(activeSpace, activeFile, buildExportHtml(title, body));
      toast.success("已导出 HTML", { description: path });
    } catch (error) {
      toast.error(`导出失败：${error}`);
    }
  };

  const syncSelection = () => {
    const el = textareaRef.current;
    if (el) setSelection({ start: el.selectionStart, end: el.selectionEnd });
  };

  const runAssist = async (action: (typeof ASSIST_ACTIONS)[number]) => {
    if (!hasSelection || assistBusy) return;
    if (!chatModel) {
      toast.error("未找到可用模型，请先到「模型」页启用一个");
      return;
    }
    const selected = content.slice(selection.start, selection.end);
    setAssistBusy(true);
    try {
      const res = await qaApi.query({ query: action.build(selected), useKb: false, chatModel });
      const result = res.answer.trim();
      if (!result) {
        toast.error("模型没有返回内容");
        return;
      }
      const next =
        action.mode === "replace"
          ? content.slice(0, selection.start) + result + content.slice(selection.end)
          : content.slice(0, selection.end) + "\n\n" + result + content.slice(selection.end);
      setContent(next);
      toast.success(`已${action.label}选中文本`);
    } catch (error) {
      toast.error(`${action.label}失败：${error}`);
    } finally {
      setAssistBusy(false);
    }
  };

  const preview = useMemo(
    () => (
      <div ref={previewRef} className="prose-plan max-w-none text-sm leading-7 text-foreground/90">
        <ReactMarkdown>{content || "*（空文档）*"}</ReactMarkdown>
      </div>
    ),
    [content],
  );

  return (
    <div className="flex h-full flex-1 overflow-hidden">
      {/* File tree */}
      <aside className="flex w-56 shrink-0 flex-col border-r border-border bg-background/60 min-[1500px]:w-64">
        <div className="flex items-center gap-1 border-b border-border p-2">
          <select
            className="h-8 min-w-0 flex-1 rounded-md border border-border bg-background px-2 text-xs"
            value={activeSpace}
            onChange={(e) => setActiveSpace(e.target.value)}
          >
            {spaces.map((s) => (
              <option key={s.path} value={s.path}>{s.name}</option>
            ))}
          </select>
          <button className="rounded p-1.5 text-muted-foreground hover:bg-muted/30 hover:text-foreground" title="添加写作空间" onClick={() => void addSpace()}>
            <FolderPlus className="h-4 w-4" />
          </button>
          <button className="rounded p-1.5 text-muted-foreground hover:bg-muted/30 hover:text-foreground" title="新建文件" onClick={() => void newFile()}>
            <FilePlus2 className="h-4 w-4" />
          </button>
        </div>
        <div className="min-h-0 flex-1 overflow-y-auto p-2">
          {files.length === 0 ? (
            <div className="rounded-md border border-dashed border-border px-2 py-8 text-center text-xs text-muted-foreground">
              还没有文档，点右上「+」新建。
            </div>
          ) : (
            files.map((file) => (
              <div
                key={file.relative_path}
                className={cn(
                  "group flex cursor-pointer items-center gap-1.5 rounded px-2 py-1.5 text-sm",
                  activeFile === file.relative_path ? "bg-primary/12 text-primary" : "text-foreground hover:bg-muted/20",
                )}
                onClick={() => void openFile(activeSpace, file.relative_path)}
              >
                <FileText className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
                <span className="min-w-0 flex-1 truncate">{file.name.replace(/\.md$/, "")}</span>
                <button className="rounded p-0.5 text-muted-foreground opacity-0 hover:text-foreground group-hover:opacity-100" title="重命名" onClick={(e) => { e.stopPropagation(); void renameFile(file); }}>
                  <Pencil className="h-3 w-3" />
                </button>
                <button className="rounded p-0.5 text-muted-foreground opacity-0 hover:text-destructive group-hover:opacity-100" title="删除" onClick={(e) => { e.stopPropagation(); void deleteFile(file); }}>
                  <Trash2 className="h-3 w-3" />
                </button>
              </div>
            ))
          )}
        </div>
      </aside>

      {/* Editor */}
      <div className="flex min-w-0 flex-1 flex-col">
        <div className="flex items-center gap-2 border-b border-border px-4 py-2">
          <span className="truncate text-sm font-medium">
            {activeFile ? activeFile.replace(/\.md$/, "") : "未选择文档"}
            {dirty && <span className="ml-1.5 text-amber-500" title="未保存">●</span>}
          </span>
          <div className="ml-auto flex items-center gap-1">
            <div className="flex rounded-md border border-border p-0.5">
              {([["source", FileText, "源码"], ["split", Columns2, "分屏"], ["preview", Eye, "预览"]] as const).map(([m, Icon, label]) => (
                <button
                  key={m}
                  className={cn("flex items-center gap-1 rounded px-2 py-1 text-xs", mode === m ? "bg-accent text-accent-foreground" : "text-muted-foreground hover:text-foreground")}
                  onClick={() => setMode(m)}
                  title={label}
                >
                  <Icon className="h-3.5 w-3.5" />
                </button>
              ))}
            </div>
            <Button variant="outline" size="sm" disabled={!activeFile} onClick={() => void exportHtml()} title="导出为 HTML">
              <Download className="h-3.5 w-3.5" /> HTML
            </Button>
            <Button size="sm" disabled={!activeFile || !dirty} onClick={() => void save()}>
              <Save className="h-3.5 w-3.5" /> 保存
            </Button>
          </div>
        </div>

        {/* Inline writing assistant bar (appears on selection) */}
        {mode !== "preview" && hasSelection && activeFile && (
          <div className="flex items-center gap-2 border-b border-border bg-accent/5 px-4 py-1.5 text-xs">
            <Sparkles className="h-3.5 w-3.5 text-accent" />
            <span className="text-muted-foreground">对选中文本：</span>
            {ASSIST_ACTIONS.map((action) => (
              <button
                key={action.id}
                disabled={assistBusy}
                onClick={() => void runAssist(action)}
                className="rounded-full border border-border px-2 py-0.5 text-muted-foreground hover:border-accent hover:text-accent disabled:opacity-50"
              >
                {action.label}
              </button>
            ))}
            {assistBusy && <Loader2 className="h-3.5 w-3.5 animate-spin text-accent" />}
          </div>
        )}

        {!activeFile ? (
          <div className="flex flex-1 items-center justify-center text-sm text-muted-foreground">
            从左侧选择或新建一个文档开始写作。
          </div>
        ) : (
          <div className="flex min-h-0 flex-1">
            {mode !== "preview" && (
              <textarea
                ref={textareaRef}
                value={content}
                onChange={(e) => setContent(e.target.value)}
                onSelect={syncSelection}
                onKeyUp={syncSelection}
                onMouseUp={syncSelection}
                placeholder="开始写作…（Markdown；选中文字可调用写作助手；Ctrl+S 保存）"
                className={cn(
                  "min-h-0 resize-none border-0 bg-transparent p-5 font-mono text-sm leading-7 focus:outline-none",
                  mode === "split" ? "w-1/2 border-r border-border" : "w-full",
                )}
              />
            )}
            <div
              className={cn(
                "min-h-0 overflow-y-auto p-5",
                mode === "preview" ? "w-full" : mode === "split" ? "w-1/2" : "hidden",
              )}
            >
              {preview}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

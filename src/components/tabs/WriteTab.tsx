/**
 * WriteTab — Markdown writing workspace.
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
import { writeApi, modelApi, settingsApi, qaApi, shellApi, officeApi, slidesApi, type WriteSpace, type WriteFile, type Brand, type WriteSection } from "@/lib/tauri-api";

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
  // Word/长文/批量（P1）
  const [docxBusy, setDocxBusy] = useState(false);
  const [brands, setBrands] = useState<Brand[]>([]);
  const [brandName, setBrandName] = useState("");
  const [longOpen, setLongOpen] = useState(false);
  const [longTopic, setLongTopic] = useState("");
  const [longSections, setLongSections] = useState<WriteSection[]>([]);
  const [longBusy, setLongBusy] = useState<"" | "outline" | "expand">("");
  const [mergeOpen, setMergeOpen] = useState(false);
  const [mergeTemplate, setMergeTemplate] = useState("");
  const [mergeData, setMergeData] = useState("");
  const [mergeKey, setMergeKey] = useState("");
  const [merging, setMerging] = useState(false);
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

  useEffect(() => {
    slidesApi.listBrands().then(setBrands).catch(() => {});
  }, []);

  /** P1：Markdown → 带样式 Word（可选品牌母版，母版来自演示的母版库）。 */
  const exportDocx = async () => {
    if (!activeFile) return;
    setDocxBusy(true);
    try {
      const title = activeFile.replace(/\.md$/i, "").split(/[\\/]/).pop() ?? "文档";
      const path = await officeApi.exportDocx(content, title, brandName || undefined);
      toast.success(`已导出 Word：${path}`);
    } catch (e) {
      toast.error(`导出失败：${e}`);
    } finally {
      setDocxBusy(false);
    }
  };

  /** P1：现有 docx → Markdown 存进当前写作空间。 */
  const importDocx = async () => {
    if (!activeSpace) {
      toast.error("先选择一个写作空间");
      return;
    }
    try {
      const path = await shellApi.pickFile();
      if (!path) return;
      if (!path.toLowerCase().endsWith(".docx")) {
        toast.error("请选择 .docx 文件");
        return;
      }
      const md = await officeApi.importDocx(path);
      const base = (path.split(/[\\/]/).pop() ?? "导入文档").replace(/\.docx$/i, "");
      const rel = await writeApi.createFile(activeSpace, `${base}.md`);
      await writeApi.saveFile(activeSpace, rel, md);
      await loadFiles(activeSpace);
      setActiveFile(rel);
      setContent(md);
      toast.success("已导入为 Markdown，可继续编辑后再导出 Word");
    } catch (e) {
      toast.error(`导入失败：${e}`);
    }
  };

  /** P1：AI 长文两阶段——先大纲后逐章并行展开，写进当前文件。 */
  const longOutline = async () => {
    if (!longTopic.trim() || !chatModel) {
      toast.error(!chatModel ? "请先启用一个对话模型" : "先描述要写什么");
      return;
    }
    setLongBusy("outline");
    try {
      setLongSections(await officeApi.writeOutline(longTopic.trim(), chatModel));
    } catch (e) {
      toast.error(String(e));
    } finally {
      setLongBusy("");
    }
  };

  const longExpand = async () => {
    if (!activeFile || longSections.length === 0) return;
    setLongBusy("expand");
    try {
      const parts = await Promise.all(
        longSections.map((sec) =>
          officeApi.writeExpand(longTopic.trim(), sec, chatModel).catch(() => `## ${sec.title}\n\n（本章生成失败，可重试）`),
        ),
      );
      const assembled = `# ${longTopic.trim()}\n\n${parts.join("\n\n")}\n`;
      setContent(assembled);
      await writeApi.saveFile(activeSpace, activeFile, assembled);
      setLongOpen(false);
      setLongSections([]);
      toast.success(`已生成 ${parts.length} 章，写入当前文件`);
    } catch (e) {
      toast.error(String(e));
    } finally {
      setLongBusy("");
    }
  };

  /** P1：merge 批量生成（模板 {{key}} + JSON 数组 → 一条一份）。 */
  const runMerge = async () => {
    if (!mergeTemplate || !mergeData.trim()) {
      toast.error("先选模板并粘贴 JSON 数据");
      return;
    }
    setMerging(true);
    try {
      const outputs = await officeApi.mergeBatch(mergeTemplate, mergeData.trim(), mergeKey.trim() || undefined);
      toast.success(`批量生成完成：${outputs.length} 份`, { description: outputs[0] });
      setMergeOpen(false);
    } catch (e) {
      toast.error(`批量生成失败：${e}`);
    } finally {
      setMerging(false);
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
            {brands.length > 0 && (
              <select
                className="h-8 rounded-md border border-border bg-background px-1.5 text-xs"
                value={brandName}
                onChange={(e) => setBrandName(e.target.value)}
                title="导出 Word 用的品牌母版（来自演示的母版库）"
              >
                <option value="">无母版</option>
                {brands.map((b) => (
                  <option key={b.name} value={b.name}>{b.name}</option>
                ))}
              </select>
            )}
            <Button variant="outline" size="sm" disabled={!activeFile || docxBusy} onClick={() => void exportDocx()} title="导出为带样式的 Word（.docx）">
              <Download className="h-3.5 w-3.5" /> {docxBusy ? "导出中…" : "Word"}
            </Button>
            <Button variant="outline" size="sm" onClick={() => void importDocx()} title="导入现有 .docx 为 Markdown">
              <Download className="h-3.5 w-3.5 rotate-180" /> 导入
            </Button>
            <Button variant="outline" size="sm" disabled={!activeFile} onClick={() => setLongOpen(true)} title="AI 长文：先出大纲，确认后逐章展开">
              <Sparkles className="h-3.5 w-3.5" /> AI 长文
            </Button>
            <Button variant="outline" size="sm" onClick={() => setMergeOpen(true)} title="模板 {{key}} + JSON 数据 → 批量生成文档">
              <Columns2 className="h-3.5 w-3.5" /> 批量
            </Button>
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

      {/* AI 长文：先大纲后展开（复用演示的两阶段模式） */}
      {longOpen && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-8" onClick={() => setLongOpen(false)}>
          <div className="flex max-h-full w-full max-w-xl flex-col gap-3 rounded-xl border border-border bg-background p-4 shadow-2xl" onClick={(e) => e.stopPropagation()}>
            <div className="text-sm font-semibold">AI 长文（先大纲，确认后逐章展开）</div>
            <textarea
              value={longTopic}
              onChange={(e) => setLongTopic(e.target.value)}
              placeholder="要写什么？例如：给团队写一份《远程办公安全规范》，面向非技术同事，1500 字左右"
              className="h-20 resize-none rounded-lg border border-border bg-background p-2 text-sm focus:outline-none"
            />
            <div className="flex gap-2">
              <Button size="sm" disabled={longBusy !== ""} onClick={() => void longOutline()}>
                {longBusy === "outline" ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <Sparkles className="h-3.5 w-3.5" />} 出大纲
              </Button>
              {longSections.length > 0 && (
                <Button size="sm" variant="outline" disabled={longBusy !== ""} onClick={() => void longExpand()}>
                  {longBusy === "expand" ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : null}
                  按大纲展开（{longSections.length} 章并行，写入当前文件）
                </Button>
              )}
            </div>
            {longSections.length > 0 && (
              <div className="flex max-h-64 flex-col gap-1.5 overflow-y-auto">
                {longSections.map((sec, i) => (
                  <div key={i} className="flex items-center gap-2 rounded-lg border border-border p-2">
                    <input
                      value={sec.title}
                      onChange={(e) => setLongSections((prev) => prev.map((x, j) => (j === i ? { ...x, title: e.target.value } : x)))}
                      className="w-44 shrink-0 rounded border border-border bg-background px-1.5 py-0.5 text-xs"
                    />
                    <input
                      value={sec.brief}
                      onChange={(e) => setLongSections((prev) => prev.map((x, j) => (j === i ? { ...x, brief: e.target.value } : x)))}
                      className="min-w-0 flex-1 rounded border border-border bg-background px-1.5 py-0.5 text-xs"
                    />
                    <button onClick={() => setLongSections((prev) => prev.filter((_, j) => j !== i))}>
                      <Trash2 className="h-3.5 w-3.5 text-muted-foreground hover:text-destructive" />
                    </button>
                  </div>
                ))}
              </div>
            )}
            <p className="text-[11px] text-muted-foreground">展开会覆盖当前文件内容；写完可直接「Word」导出带样式文档。</p>
          </div>
        </div>
      )}

      {/* 批量生成：模板 {{key}} + JSON 数组 */}
      {mergeOpen && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-8" onClick={() => setMergeOpen(false)}>
          <div className="flex max-h-full w-full max-w-xl flex-col gap-3 rounded-xl border border-border bg-background p-4 shadow-2xl" onClick={(e) => e.stopPropagation()}>
            <div className="text-sm font-semibold">批量生成（模板 {"{{key}}"} 占位 + JSON 数据）</div>
            <div className="flex items-center gap-2">
              <Button size="sm" variant="outline" onClick={() => void shellApi.pickFile().then((p) => p && setMergeTemplate(p))}>
                选模板（docx/xlsx/pptx）
              </Button>
              <span className="truncate text-xs text-muted-foreground">{mergeTemplate || "未选择"}</span>
            </div>
            <textarea
              value={mergeData}
              onChange={(e) => setMergeData(e.target.value)}
              placeholder={'JSON 数组，每条一份产物：\n[{"name":"张三","amount":"1200"},{"name":"李四","amount":"800"}]'}
              className="h-32 resize-none rounded-lg border border-border bg-background p-2 font-mono text-xs focus:outline-none"
            />
            <div className="flex items-center gap-2 text-xs">
              <span className="text-muted-foreground">命名字段（可选）：</span>
              <input
                value={mergeKey}
                onChange={(e) => setMergeKey(e.target.value)}
                placeholder="如 name（缺省用序号）"
                className="w-40 rounded border border-border bg-background px-1.5 py-0.5"
              />
              <Button size="sm" className="ml-auto" disabled={merging} onClick={() => void runMerge()}>
                {merging ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : null} 开始生成
              </Button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

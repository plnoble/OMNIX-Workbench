/**
 * NotesTab — local Markdown notes (笔记, R5). Lightweight: a searchable list on
 * the left, a title + Markdown editor with live preview on the right. Notes can
 * also be created from the Quick Assistant ("存为笔记"). Stored locally in SQLite.
 */
import { useCallback, useEffect, useState } from "react";
import ReactMarkdown from "react-markdown";
import { StickyNote, Plus, Search, Trash2, Eye, Pencil, Save, FolderOpen, Plug } from "lucide-react";

import { notesApi, mcpApi, mcpSyncApi, type Note } from "@/lib/tauri-api";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { toast } from "@/components/ui/sonner";
import { cn } from "@/lib/utils";

export function NotesTab() {
  const [notes, setNotes] = useState<Note[]>([]);
  const [query, setQuery] = useState("");
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [title, setTitle] = useState("");
  const [content, setContent] = useState("");
  const [tags, setTags] = useState("");
  const [dirty, setDirty] = useState(false);
  const [preview, setPreview] = useState(false);

  const load = useCallback(async (q?: string) => {
    try {
      setNotes(await notesApi.list(q ?? query));
    } catch (e) {
      toast.error("加载笔记失败", { description: String(e) });
    }
  }, [query]);

  useEffect(() => {
    void load("");
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  const selectNote = (note: Note) => {
    if (dirty && !window.confirm("当前笔记有未保存改动，切换将丢失。继续？")) return;
    setSelectedId(note.id);
    setTitle(note.title);
    setContent(note.content);
    setTags(note.tags);
    setDirty(false);
    setPreview(false);
  };

  const newNote = () => {
    if (dirty && !window.confirm("当前笔记有未保存改动，新建将丢失。继续？")) return;
    setSelectedId(null);
    setTitle("");
    setContent("");
    setTags("");
    setDirty(false);
    setPreview(false);
  };

  const save = async () => {
    if (!title.trim() && !content.trim()) {
      toast.error("空笔记，已忽略");
      return;
    }
    try {
      const saved = await notesApi.save({ id: selectedId ?? undefined, title, content, tags });
      setSelectedId(saved.id);
      setDirty(false);
      await load();
      toast.success("已保存");
    } catch (e) {
      toast.error("保存失败", { description: String(e) });
    }
  };

  // Expose the notes folder to agents as an MCP "notes tool" (a filesystem
  // server scoped to ~/.omnix/notes) so an agent can list / read / write notes.
  const connectToAgent = async () => {
    if (!window.confirm("把笔记接入 Agent？将注册一个指向笔记文件夹的 MCP 工具(文件系统服务，需要本机有 Node/npx)，写入 Claude Code 与 Codex 的配置，Agent 即可读写你的笔记。")) return;
    try {
      const dir = await notesApi.dir();
      await mcpApi.save({
        id: "omnix-notes",
        name: "omnix-notes",
        command: "npx",
        args: `-y @modelcontextprotocol/server-filesystem ${dir}`,
        env: "",
        url: "",
        server_type: "stdio",
        is_enabled: true,
      });
      const reports = await mcpSyncApi.syncToAgents(["Claude Code", "Codex"], ["omnix-notes"]);
      const ok = reports.filter((r) => r.synced.includes("omnix-notes")).map((r) => r.agent);
      toast.success("已接入 Agent", { description: ok.length ? `已写入：${ok.join("、")}。在 Agent 里即可读写 ~/.omnix/notes` : "已注册 MCP，请在 Agent 中启用" });
    } catch (e) {
      toast.error("接入失败", { description: String(e) });
    }
  };

  const remove = async (id: string) => {
    if (!window.confirm("删除这条笔记？")) return;
    try {
      await notesApi.remove(id);
      if (selectedId === id) newNote();
      await load();
    } catch (e) {
      toast.error("删除失败", { description: String(e) });
    }
  };

  return (
    <div className="flex h-full w-full overflow-hidden">
      {/* List */}
      <div className="flex w-72 shrink-0 flex-col border-r border-border">
        <div className="flex items-center gap-2 border-b border-border px-3 py-3">
          <StickyNote className="h-4 w-4" />
          <span className="text-sm font-semibold">笔记</span>
          <button
            onClick={() => void connectToAgent()}
            title="把笔记接入 Agent（注册 MCP 工具，Agent 可读写笔记）"
            className="ml-auto rounded p-1.5 text-muted-foreground hover:bg-muted/30 hover:text-foreground"
          >
            <Plug className="h-4 w-4" />
          </button>
          <button
            onClick={() => notesApi.openFolder().catch((e) => toast.error(`无法打开：${e}`))}
            title="打开笔记文件夹（~/.omnix/notes，每条笔记是一个 .md 文件）"
            className="rounded p-1.5 text-muted-foreground hover:bg-muted/30 hover:text-foreground"
          >
            <FolderOpen className="h-4 w-4" />
          </button>
          <Button size="sm" className="h-7 gap-1" onClick={newNote}>
            <Plus className="h-3 w-3" /> 新建
          </Button>
        </div>
        <div className="border-b border-border p-2">
          <div className="relative">
            <Search className="absolute left-2 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground" />
            <Input
              value={query}
              onChange={(e) => { setQuery(e.target.value); void load(e.target.value); }}
              placeholder="搜索标题/正文/标签"
              className="h-8 pl-7 text-xs"
            />
          </div>
        </div>
        <div className="flex-1 overflow-y-auto">
          {notes.length === 0 ? (
            <div className="p-4 text-center text-xs text-muted-foreground">暂无笔记</div>
          ) : (
            notes.map((note) => (
              <button
                key={note.id}
                onClick={() => selectNote(note)}
                className={cn(
                  "flex w-full flex-col items-start gap-0.5 border-b border-border px-3 py-2 text-left hover:bg-muted/20",
                  selectedId === note.id && "bg-muted/30",
                )}
              >
                <span className="w-full truncate text-sm font-medium">{note.title || "无标题"}</span>
                <span className="w-full truncate text-[11px] text-muted-foreground">{note.content.replace(/[#>*`\n]/g, " ").slice(0, 48) || "（空）"}</span>
                <div className="flex w-full items-center justify-between text-[10px] text-muted-foreground">
                  <span>{new Date(note.updated_at + "Z").toLocaleString()}</span>
                  {note.source && <span className="rounded bg-muted/40 px-1">{note.source}</span>}
                </div>
              </button>
            ))
          )}
        </div>
      </div>

      {/* Editor */}
      <div className="flex min-w-0 flex-1 flex-col">
        <div className="flex items-center gap-2 border-b border-border px-4 py-2.5">
          <Input
            value={title}
            onChange={(e) => { setTitle(e.target.value); setDirty(true); }}
            placeholder="标题"
            className="h-8 flex-1 border-0 text-base font-semibold focus-visible:ring-0"
          />
          <button onClick={() => setPreview((p) => !p)} className="flex items-center gap-1 rounded px-2 py-1 text-xs text-muted-foreground hover:bg-muted/30 hover:text-foreground" title={preview ? "编辑" : "预览"}>
            {preview ? <Pencil className="h-3.5 w-3.5" /> : <Eye className="h-3.5 w-3.5" />} {preview ? "编辑" : "预览"}
          </button>
          {selectedId && (
            <button onClick={() => void remove(selectedId)} className="rounded p-1.5 text-muted-foreground hover:bg-destructive/10 hover:text-destructive" title="删除">
              <Trash2 className="h-3.5 w-3.5" />
            </button>
          )}
          <Button size="sm" className="h-8 gap-1" onClick={() => void save()} disabled={!dirty && !!selectedId}>
            <Save className="h-3.5 w-3.5" /> 保存
          </Button>
        </div>
        <div className="border-b border-border px-4 py-1.5">
          <Input
            value={tags}
            onChange={(e) => { setTags(e.target.value); setDirty(true); }}
            placeholder="标签（逗号分隔，可选）"
            className="h-7 border-0 text-xs focus-visible:ring-0"
          />
        </div>
        <div className="min-h-0 flex-1 overflow-y-auto">
          {preview ? (
            <div className="prose prose-sm prose-invert max-w-none p-5">
              <ReactMarkdown>{content || "*（空）*"}</ReactMarkdown>
            </div>
          ) : (
            <textarea
              value={content}
              onChange={(e) => { setContent(e.target.value); setDirty(true); }}
              placeholder="用 Markdown 写点什么…"
              className="h-full w-full resize-none border-0 bg-transparent p-5 font-mono text-sm leading-6 outline-none"
            />
          )}
        </div>
      </div>
    </div>
  );
}

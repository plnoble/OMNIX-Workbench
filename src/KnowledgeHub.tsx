/**
 * KnowledgeHub — 知识库 RAG 管理 UI
 *
 * 三栏布局:
 * - 左栏: 文档列表 + 导入按钮 + 嵌入控制
 * - 中栏: Tab 切换 (Chunks / Search / RAG Chat)
 * - 右栏: 详情面板
 *
 * 所有状态和业务逻辑由 useKnowledgeBase hook 管理。
 */

import { useKnowledgeBase } from "@/hooks/useKnowledgeBase";
import { useRef, useState } from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { Badge } from "@/components/ui/badge";
import { Card, CardContent } from "@/components/ui/card";
import { Separator } from "@/components/ui/separator";
import { toast } from "@/components/ui/sonner";
import { cn } from "@/lib/utils";
import { shellApi, knowledgeApi } from "@/lib/tauri-api";
import {
  BookOpen, Search, Upload, Trash2, FileText, Code, Hash,
  Sparkles, Loader2, MessageSquare, Send,
  ChevronRight, File, Brain, Zap, FolderOpen, FolderPlus, Quote, Download,
} from "lucide-react";

// ── Status Badge ────────────────────────────────────────

function EmbeddingStatusBadge({ status }: { status: string }) {
  const variants: Record<string, { className: string; label: string }> = {
    pending: { className: "bg-yellow-500/20 text-yellow-400 border-yellow-500/30", label: "待嵌入" },
    in_progress: { className: "bg-blue-500/20 text-blue-400 border-blue-500/30", label: "嵌入中" },
    completed: { className: "bg-green-500/20 text-green-400 border-green-500/30", label: "已完成" },
    failed: { className: "bg-red-500/20 text-red-400 border-red-500/30", label: "失败" },
  };
  const v = variants[status] || variants.pending;
  return <Badge variant="outline" className={cn("text-xs px-1.5 py-0", v.className)}>{v.label}</Badge>;
}

function FileTypeIcon({ fileType }: { fileType: string }) {
  switch (fileType) {
    case "markdown": case "md": return <FileText className="h-4 w-4 text-cyan-400" />;
    case "code": return <Code className="h-4 w-4 text-green-400" />;
    default: return <File className="h-4 w-4 text-zinc-400" />;
  }
}

// ── Main Component ──────────────────────────────────────

export function KnowledgeHub() {
  const kb = useKnowledgeBase();
  const [showBaseForm, setShowBaseForm] = useState(false);
  const [newBaseName, setNewBaseName] = useState("");

  // ── Import handler ──────────────────────────────────

  const handleImport = async () => {
    try {
      await kb.importDocument();
      toast.success("文档导入成功！");
    } catch (e) {
      toast.error(String(e));
    }
  };

  const handleImportFile = async () => {
    const filePath = await shellApi.pickFile();
    if (!filePath) return;
    try {
      await kb.importFile(filePath);
      toast.success("文件导入成功！");
    } catch (e) {
      toast.error(String(e));
    }
  };

  const handleImportDirectory = async () => {
    const dirPath = await shellApi.pickDirectory();
    if (!dirPath) return;
    try {
      await kb.importDirectory(dirPath);
      toast.success("目录导入完成！");
    } catch (e) {
      toast.error(String(e));
    }
  };

  const handleDelete = async (docId: string) => {
    try {
      await kb.deleteDocument(docId);
      toast.success("文档已删除");
    } catch (e) {
      toast.error("删除失败：" + e);
    }
  };

  const handleCreateBase = async () => {
    if (!newBaseName.trim()) return;
    try {
      await kb.createKnowledgeBase(newBaseName.trim());
      setNewBaseName("");
      setShowBaseForm(false);
      toast.success("知识库已创建");
    } catch (error) {
      toast.error(String(error));
    }
  };

  const handleDeleteBase = async () => {
    try {
      await kb.deleteKnowledgeBase(kb.selectedBaseId);
      toast.success("知识库已删除");
    } catch (error) {
      toast.error(String(error));
    }
  };

  const kbFileInputRef = useRef<HTMLInputElement>(null);

  // Export the current KB (documents + chunks + embeddings) to a portable file.
  const handleExportBase = async () => {
    try {
      const base = kb.knowledgeBases.find((b) => b.id === kb.selectedBaseId);
      const json = await knowledgeApi.exportBase(kb.selectedBaseId);
      const blob = new Blob([json], { type: "application/json" });
      const url = URL.createObjectURL(blob);
      const link = document.createElement("a");
      link.href = url;
      link.download = `${(base?.name || "knowledge-base").replace(/[\\/:*?"<>|]/g, "_")}.omnixkb.json`;
      link.click();
      URL.revokeObjectURL(url);
      toast.success("知识库已导出（含嵌入向量）");
    } catch (error) {
      toast.error("导出失败", { description: String(error) });
    }
  };

  const handleImportBase = async (file: File) => {
    try {
      const data = await file.text();
      const imported = await knowledgeApi.importBase(data);
      await kb.loadKnowledgeBases();
      kb.selectKnowledgeBase(imported.id);
      toast.success(`已导入知识库「${imported.name}」`, { description: "若用不同的嵌入模型，关键词(BM25)搜索可用，向量搜索建议重新嵌入" });
    } catch (error) {
      toast.error("导入失败", { description: String(error) });
    }
  };

  const handleGenerateEmbeddings = async () => {
    try {
      const progress = await kb.generateEmbeddings();
      if (progress) {
        toast.success(`嵌入完成: ${progress.embedded_chunks}/${progress.total_chunks} chunks`);
      }
    } catch (e) {
      toast.error(String(e));
    }
  };

  const handleBatchEmbed = async () => {
    try {
      await kb.batchEmbedAll();
      toast.success("批量嵌入完成！");
    } catch (e) {
      toast.error(String(e));
    }
  };

  const handleSearch = async () => {
    try {
      await kb.hybridSearch();
    } catch (e) {
      toast.error(String(e));
    }
  };

  const handleRagQuery = async () => {
    try {
      await kb.sendRagQuery();
    } catch (e) {
      toast.error(String(e));
    }
  };

  // ── Computed ────────────────────────────────────────

  const pendingCount = kb.documents.filter(d => d.embedding_status === "pending" || d.embedding_status === "failed").length;
  const selectedDoc = kb.documents.find(d => d.id === kb.selectedDocId);

  // ── Render ──────────────────────────────────────────

  return (
    <div className="flex h-full w-full min-w-0">
      {/* ── Left Panel: Document List ── */}
      <div className="w-48 sm:w-64 border-r border-border flex flex-col glass-surface">
        <div className="p-3 border-b border-border flex items-center justify-between">
          <h3 className="text-sm font-semibold text-foreground flex items-center gap-1.5">
            <BookOpen className="h-4 w-4 text-cyan-400" />
            知识库
          </h3>
          <div className="flex items-center gap-0.5">
            <Button
              size="sm"
              variant="ghost"
              className="h-6 w-6 p-0"
              title="粘贴文本导入"
              onClick={() => kb.setShowImportForm(!kb.showImportForm)}
            >
              <Upload className="h-3.5 w-3.5" />
            </Button>
            <Button
              size="sm"
              variant="ghost"
              className="h-6 w-6 p-0"
              title="选择文件导入"
              onClick={handleImportFile}
              disabled={kb.isImporting}
            >
              <FileText className="h-3.5 w-3.5" />
            </Button>
            <Button
              size="sm"
              variant="ghost"
              className="h-6 w-6 p-0"
              title="导入整个目录"
              onClick={handleImportDirectory}
              disabled={kb.isImporting}
            >
              <FolderOpen className="h-3.5 w-3.5" />
            </Button>
          </div>
        </div>

        <div className="border-b border-border p-2">
          <div className="flex items-center gap-1">
            <select
              value={kb.selectedBaseId}
              onChange={(event) => kb.selectKnowledgeBase(event.target.value)}
              className="h-8 min-w-0 flex-1 rounded-md border border-border bg-background px-2 text-xs"
            >
              {kb.knowledgeBases.map((base) => (
                <option key={base.id} value={base.id}>{base.name} ({base.document_count})</option>
              ))}
            </select>
            <Button
              size="sm"
              variant="ghost"
              className="h-8 w-8 p-0"
              onClick={() => setShowBaseForm((show) => !show)}
              title="新建知识库"
            >
              <FolderPlus className="h-4 w-4" />
            </Button>
            <Button
              size="sm"
              variant="ghost"
              className="h-8 w-8 p-0"
              onClick={() => void handleExportBase()}
              title="导出当前知识库（含嵌入，可迁移到别的电脑）"
            >
              <Download className="h-4 w-4" />
            </Button>
            <Button
              size="sm"
              variant="ghost"
              className="h-8 w-8 p-0"
              onClick={() => kbFileInputRef.current?.click()}
              title="导入知识库文件（.omnixkb.json）"
            >
              <Upload className="h-4 w-4" />
            </Button>
            {kb.selectedBaseId !== "default" && (
              <Button
                size="sm"
                variant="ghost"
                className="h-8 w-8 p-0 text-destructive"
                onClick={handleDeleteBase}
                title="删除当前知识库"
              >
                <Trash2 className="h-4 w-4" />
              </Button>
            )}
          </div>
          <input
            ref={kbFileInputRef}
            type="file"
            accept="application/json,.json"
            className="hidden"
            onChange={(e) => {
              const file = e.target.files?.[0];
              if (file) void handleImportBase(file);
              e.target.value = "";
            }}
          />
          {showBaseForm && (
            <div className="mt-2 flex items-center gap-1">
              <Input
                value={newBaseName}
                onChange={(event) => setNewBaseName(event.target.value)}
                onKeyDown={(event) => event.key === "Enter" && void handleCreateBase()}
                placeholder="知识库名称"
                className="h-8 text-xs"
              />
              <Button size="sm" className="h-8" onClick={handleCreateBase}>创建</Button>
            </div>
          )}
        </div>

        {/* Import Form */}
        {kb.showImportForm && (
          <div className="p-3 border-b border-border space-y-2">
            <Input
              placeholder="文档标题"
              value={kb.importForm.title}
              onChange={e => kb.updateImportForm("title", e.target.value)}
              className="h-7 text-xs"
            />
            <div className="flex gap-1">
              {(["markdown", "code", "text"] as const).map(t => (
                <Button
                  key={t}
                  size="sm"
                  variant={kb.importForm.fileType === t ? "default" : "ghost"}
                  className="h-6 text-xs px-2"
                  onClick={() => kb.updateImportForm("fileType", t)}
                >
                  {t === "markdown" ? "MD" : t === "code" ? "Code" : "Text"}
                </Button>
              ))}
            </div>
            <Textarea
              placeholder="粘贴文档内容..."
              value={kb.importForm.content}
              onChange={e => kb.updateImportForm("content", e.target.value)}
              className="min-h-[80px] text-xs"
            />
            <div className="flex gap-1">
              <Button size="sm" className="h-6 text-xs flex-1" onClick={handleImport} disabled={kb.isImporting}>
                {kb.isImporting ? <Loader2 className="h-3 w-3 animate-spin mr-1" /> : <Upload className="h-3 w-3 mr-1" />}
                导入
              </Button>
              <Button size="sm" variant="ghost" className="h-6 text-xs" onClick={() => kb.setShowImportForm(false)}>
                取消
              </Button>
            </div>
          </div>
        )}

        {/* Document List */}
        <div className="flex-1 overflow-y-auto p-2 space-y-1">
          {kb.documents.length === 0 && !kb.showImportForm && (
            <div className="text-center text-muted-foreground text-xs mt-8">
              <BookOpen className="h-8 w-8 mx-auto mb-2 opacity-30" />
              <p>暂无文档</p>
              <p className="text-xs mt-1">点击 ↑ 导入文档开始</p>
            </div>
          )}
          {kb.documents.map(doc => (
            <div
              key={doc.id}
              className={cn(
                "p-2 rounded-md cursor-pointer text-xs group transition-colors",
                kb.selectedDocId === doc.id
                  ? "bg-cyan-500/15 border border-cyan-500/30"
                  : "hover:bg-muted/50 border border-transparent",
              )}
              onClick={() => kb.selectDocument(doc.id)}
            >
              <div className="flex items-center justify-between">
                <div className="flex items-center gap-1.5 min-w-0 flex-1">
                  <FileTypeIcon fileType={doc.file_type} />
                  <span className="truncate font-medium">{doc.title}</span>
                </div>
                <Button
                  size="sm"
                  variant="ghost"
                  className="h-5 w-5 p-0 opacity-0 group-hover:opacity-100 transition-opacity"
                  onClick={e => { e.stopPropagation(); handleDelete(doc.id); }}
                >
                  <Trash2 className="h-3 w-3 text-red-400" />
                </Button>
              </div>
              <div className="flex items-center gap-2 mt-1 text-muted-foreground">
                <span className="text-xs">{doc.chunk_count} chunks</span>
                <EmbeddingStatusBadge status={doc.embedding_status} />
              </div>
            </div>
          ))}
        </div>

        {/* Embedding Section */}
        <div className="p-3 border-t border-border space-y-2">
          <div className="flex items-center gap-1.5">
            <Brain className="h-3.5 w-3.5 text-purple-400" />
            <span className="text-xs font-medium text-muted-foreground">嵌入模型</span>
          </div>
          <select
            className="w-full h-7 rounded-md bg-background border border-border text-xs px-2"
            value={kb.selectedEmbedModel}
            onChange={e => kb.setSelectedEmbedModel(e.target.value)}
          >
            {kb.embeddingModels.length === 0 && <option value="">无可用模型</option>}
            {kb.embeddingModels.map(m => (
              <option key={`${m.platform_id}:${m.model_name}`} value={m.model_name}>
                {m.model_name} ({m.platform_name})
              </option>
            ))}
          </select>
          <Button
            size="sm"
            className="w-full h-7 text-xs"
            onClick={handleGenerateEmbeddings}
            disabled={!kb.selectedDocId || kb.isEmbedding || !kb.selectedEmbedModel}
          >
            {kb.isEmbedding ? <Loader2 className="h-3 w-3 animate-spin mr-1" /> : <Sparkles className="h-3 w-3 mr-1" />}
            生成嵌入
          </Button>
          {pendingCount > 0 && (
            <Button
              size="sm"
              variant="outline"
              className="w-full h-7 text-xs"
              onClick={handleBatchEmbed}
              disabled={kb.isEmbedding || !kb.selectedEmbedModel}
            >
              <Zap className="h-3 w-3 mr-1" />
              批量嵌入 ({pendingCount})
            </Button>
          )}
        </div>
      </div>

      {/* ── Center Panel ── */}
      <div className="flex-1 flex flex-col min-w-0">
        {/* Sub-tab bar */}
        <div className="flex items-center border-b border-border px-4 py-1.5 gap-1">
          {([
            { id: "chunks" as const, label: "分块浏览", icon: <Hash className="h-3.5 w-3.5" /> },
            { id: "search" as const, label: "混合搜索", icon: <Search className="h-3.5 w-3.5" /> },
            { id: "rag" as const, label: "RAG 问答", icon: <MessageSquare className="h-3.5 w-3.5" /> },
          ]).map(tab => (
            <Button
              key={tab.id}
              size="sm"
              variant={kb.activeSubTab === tab.id ? "secondary" : "ghost"}
              className="h-7 text-xs gap-1"
              onClick={() => kb.setActiveSubTab(tab.id)}
            >
              {tab.icon}
              {tab.label}
            </Button>
          ))}
        </div>

        {/* Sub-tab content */}
        <div className="flex-1 overflow-hidden">
          {/* ── Chunks Tab ── */}
          {kb.activeSubTab === "chunks" && (
            <div className="h-full overflow-y-auto p-4">
              {!kb.selectedDocId ? (
                <div className="text-center text-muted-foreground text-sm mt-16">
                  <FileText className="h-12 w-12 mx-auto mb-3 opacity-20" />
                  <p>选择左侧文档查看分块</p>
                </div>
              ) : kb.chunks.length === 0 ? (
                <div className="text-center text-muted-foreground text-sm mt-16">加载中…</div>
              ) : (
                <div className="space-y-2">
                  <h4 className="text-sm font-semibold text-foreground mb-3">
                    {selectedDoc?.title} — {kb.chunks.length} 分块
                  </h4>
                  {kb.chunks.map(chunk => (
                    <Card
                      key={chunk.id}
                      className={cn(
                        "cursor-pointer transition-colors",
                        kb.selectedChunkId === chunk.id ? "border-cyan-500/50 bg-cyan-500/5" : "hover:border-border",
                      )}
                      onClick={() => kb.selectChunk(chunk.id)}
                    >
                      <CardContent className="p-3">
                        <div className="flex items-center gap-2 mb-1">
                          <Badge variant="outline" className="text-xs h-5">#{chunk.chunk_index}</Badge>
                          <span className="text-xs text-muted-foreground">
                            {chunk.char_start}–{chunk.char_end} chars
                          </span>
                          {chunk.has_embedding && (
                            <Badge variant="outline" className="text-xs h-5 bg-green-500/10 text-green-400 border-green-500/30">
                              已嵌入
                            </Badge>
                          )}
                          {typeof chunk.metadata?.heading === "string" && (
                            <Badge variant="outline" className="text-xs h-5 bg-purple-500/10 text-purple-400 border-purple-500/30">
                              {chunk.metadata.heading}
                            </Badge>
                          )}
                        </div>
                        <p className="text-xs text-muted-foreground line-clamp-3">{chunk.content}</p>
                      </CardContent>
                    </Card>
                  ))}
                </div>
              )}
            </div>
          )}

          {/* ── Search Tab ── */}
          {kb.activeSubTab === "search" && (
            <div className="h-full flex flex-col">
              <div className="p-4 border-b border-border flex items-center gap-2">
                <Input
                  placeholder="输入搜索关键词..."
                  value={kb.searchQuery}
                  onChange={e => kb.setSearchQuery(e.target.value)}
                  className="flex-1 h-8 text-sm"
                  onKeyDown={e => e.key === "Enter" && handleSearch()}
                />
                <Button size="sm" className="h-8" onClick={handleSearch} disabled={kb.isSearching || !kb.selectedEmbedModel}>
                  {kb.isSearching ? <Loader2 className="h-4 w-4 animate-spin" /> : <Search className="h-4 w-4" />}
                </Button>
              </div>
              <div className="flex-1 overflow-y-auto p-4 space-y-2">
                {kb.searchResults.length === 0 && !kb.isSearching && (
                  <div className="text-center text-muted-foreground text-sm mt-16">
                    <Search className="h-12 w-12 mx-auto mb-3 opacity-20" />
                    <p>输入关键词进行 BM25+向量混合搜索</p>
                  </div>
                )}
                {kb.searchResults.map(result => (
                  <Card
                    key={result.chunk_id}
                    className={cn(
                      "cursor-pointer transition-colors",
                      kb.selectedChunkId === result.chunk_id ? "border-cyan-500/50 bg-cyan-500/5" : "hover:border-border",
                    )}
                    onClick={() => kb.selectChunk(result.chunk_id)}
                  >
                    <CardContent className="p-3">
                      <div className="flex items-center gap-2 mb-1">
                        <Badge variant="outline" className="text-xs h-5">#{result.rank}</Badge>
                        {result.bm25_score !== null && (
                          <Badge variant="outline" className="text-xs h-5 bg-blue-500/10 text-blue-400 border-blue-500/30">
                            BM25: {result.bm25_score.toFixed(4)}
                          </Badge>
                        )}
                        {result.vector_score !== null && (
                          <Badge variant="outline" className="text-xs h-5 bg-purple-500/10 text-purple-400 border-purple-500/30">
                            Vec: {result.vector_score.toFixed(4)}
                          </Badge>
                        )}
                        <Badge variant="outline" className="text-xs h-5 bg-cyan-500/10 text-cyan-400 border-cyan-500/30">
                          RRF: {result.rrf_score.toFixed(6)}
                        </Badge>
                        <button
                          className="ml-auto flex items-center gap-1 rounded px-1.5 py-0.5 text-xs text-muted-foreground hover:bg-muted/30 hover:text-foreground"
                          title="复制为带出处的引用"
                          onClick={(e) => {
                            e.stopPropagation();
                            const citation = `> ${result.content}\n>\n> — ${result.knowledge_base_name} / ${result.document_title}`;
                            navigator.clipboard.writeText(citation).then(
                              () => toast.success("引用已复制（含出处）"),
                              () => toast.error("复制失败"),
                            );
                          }}
                        >
                          <Quote className="h-3 w-3" /> 引用
                        </button>
                      </div>
                      <p className="text-xs text-muted-foreground line-clamp-3">{result.content}</p>
                      <p className="mt-1 text-xs text-muted-foreground">
                        {result.knowledge_base_name} / {result.document_title}
                      </p>
                    </CardContent>
                  </Card>
                ))}
              </div>
            </div>
          )}

          {/* ── RAG Chat Tab ── */}
          {kb.activeSubTab === "rag" && (
            <div className="h-full flex flex-col">
              {/* Config bar */}
              <div className="p-3 border-b border-border flex items-center gap-2">
                <span className="text-xs text-muted-foreground">对话模型:</span>
                <Input
                  placeholder="deepseek-chat"
                  value={kb.ragChatModel}
                  onChange={e => kb.setRagChatModel(e.target.value)}
                  className="h-7 w-40 text-xs"
                />
              </div>

              {/* Messages */}
              <div className="flex-1 overflow-y-auto p-4 space-y-4">
                {kb.ragMessages.length === 0 && (
                  <div className="text-center text-muted-foreground text-sm mt-16">
                    <MessageSquare className="h-12 w-12 mx-auto mb-3 opacity-20" />
                    <p>基于知识库回答问题</p>
                    <p className="text-xs mt-1">需要先导入文档并生成嵌入</p>
                  </div>
                )}
                {kb.ragMessages.map((msg, i) => (
                  <div key={i} className={cn("rounded-lg p-3 text-sm", msg.role === "user" ? "bg-cyan-500/10 ml-12" : "bg-muted/50 mr-12")}>
                    <p className="text-xs text-muted-foreground mb-1">{msg.role === "user" ? "👤 你" : "🤖 助手"}</p>
                    <p className="whitespace-pre-wrap text-xs leading-relaxed">{msg.content}</p>
                    {msg.sources && msg.sources.length > 0 && (
                      <details className="mt-2">
                        <summary className="text-xs text-cyan-400 cursor-pointer">查看来源 ({msg.sources.length})</summary>
                        <div className="mt-1 space-y-1">
                          {msg.sources.map((s, j) => (
                            <p key={j} className="text-xs text-muted-foreground line-clamp-2">
                              [{j + 1}] {s.knowledge_base_name} / {s.document_title}: {s.content.slice(0, 100)}…
                            </p>
                          ))}
                        </div>
                      </details>
                    )}
                  </div>
                ))}
                {kb.isRagLoading && (
                  <div className="bg-muted/50 mr-12 rounded-lg p-3">
                    <Loader2 className="h-4 w-4 animate-spin text-cyan-400" />
                    <span className="text-xs text-muted-foreground ml-2">检索并生成回答…</span>
                  </div>
                )}
              </div>

              {/* Input */}
              <div className="p-3 border-t border-border flex items-center gap-2">
                <Input
                  placeholder="输入问题..."
                  value={kb.ragQuery}
                  onChange={e => kb.setRagQuery(e.target.value)}
                  className="flex-1 h-8 text-sm"
                  onKeyDown={e => e.key === "Enter" && !e.shiftKey && handleRagQuery()}
                  disabled={kb.isRagLoading || !kb.selectedEmbedModel}
                />
                <Button size="sm" className="h-8" onClick={handleRagQuery} disabled={kb.isRagLoading || !kb.selectedEmbedModel}>
                  <Send className="h-4 w-4" />
                </Button>
              </div>
            </div>
          )}
        </div>
      </div>

      {/* ── Right Panel: Detail ── */}
      <div className="hidden lg:flex lg:w-80 border-l border-border flex-col glass-surface">
        <div className="p-3 border-b border-border">
          <h3 className="text-sm font-semibold text-foreground flex items-center gap-1.5">
            <ChevronRight className="h-4 w-4 text-cyan-400" />
            分块详情
          </h3>
        </div>
        <div className="flex-1 overflow-y-auto p-3">
          {!kb.selectedChunkId ? (
            <div className="text-center text-muted-foreground text-xs mt-8">
              <FileText className="h-8 w-8 mx-auto mb-2 opacity-20" />
              <p>选择分块查看详情</p>
            </div>
          ) : (() => {
            const chunk = kb.chunks.find(c => c.id === kb.selectedChunkId);
            const searchResult = kb.searchResults.find(r => r.chunk_id === kb.selectedChunkId);
            return (
              <div className="space-y-3">
                {chunk && (
                  <>
                    <div>
                      <p className="text-xs text-muted-foreground">ID</p>
                      <p className="text-xs font-mono break-all">{chunk.id}</p>
                    </div>
                    <div>
                      <p className="text-xs text-muted-foreground">位置</p>
                      <p className="text-xs">{chunk.char_start} – {chunk.char_end} chars</p>
                    </div>
                    <Separator />
                    <div>
                      <p className="text-xs text-muted-foreground mb-1">内容</p>
                      <pre className="text-xs whitespace-pre-wrap bg-muted/50 rounded p-2 max-h-[400px] overflow-y-auto">
                        {chunk.content}
                      </pre>
                    </div>
                    {chunk.metadata && Object.keys(chunk.metadata).length > 0 && (
                      <div>
                        <p className="text-xs text-muted-foreground mb-1">元数据</p>
                        <pre className="text-xs whitespace-pre-wrap bg-muted/50 rounded p-2">
                          {JSON.stringify(chunk.metadata, null, 2)}
                        </pre>
                      </div>
                    )}
                  </>
                )}
                {searchResult && (
                  <>
                    <Separator />
                    <div>
                      <p className="text-xs text-muted-foreground">搜索分数</p>
                      <div className="space-y-1 mt-1">
                        {searchResult.bm25_score !== null && (
                          <div className="flex justify-between text-xs">
                            <span className="text-blue-400">BM25</span>
                            <span>{searchResult.bm25_score.toFixed(4)}</span>
                          </div>
                        )}
                        {searchResult.vector_score !== null && (
                          <div className="flex justify-between text-xs">
                            <span className="text-purple-400">向量</span>
                            <span>{searchResult.vector_score.toFixed(4)}</span>
                          </div>
                        )}
                        <div className="flex justify-between text-xs font-medium">
                          <span className="text-cyan-400">RRF</span>
                          <span>{searchResult.rrf_score.toFixed(6)}</span>
                        </div>
                      </div>
                    </div>
                  </>
                )}
              </div>
            );
          })()}
        </div>
      </div>
    </div>
  );
}

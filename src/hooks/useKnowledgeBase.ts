/**
 * useKnowledgeBase — Knowledge Base RAG state management
 *
 * Manages all KB state: documents, chunks, embedding models,
 * search, RAG chat. Follows the useCron pattern.
 */

import { useState, useCallback, useEffect } from "react";
import { knowledgeApi, settingsApi } from "@/lib/tauri-api";
import type {
  KnowledgeBase, KbDocument, KbChunk, SearchResult,
  EmbeddingModelInfo,
} from "@/types";

// ── Import Form State ───────────────────────────────────

export interface ImportFormState {
  title: string;
  content: string;
  fileType: string;
  sourcePath: string;
}

export const EMPTY_IMPORT_FORM: ImportFormState = {
  title: "",
  content: "",
  fileType: "markdown",
  sourcePath: "manual_input",
};

// ── RAG Message ─────────────────────────────────────────

export interface RagMessage {
  role: "user" | "assistant";
  content: string;
  sources?: SearchResult[];
}

// ── Return Type ─────────────────────────────────────────

export interface UseKnowledgeBaseReturn {
  // Data
  knowledgeBases: KnowledgeBase[];
  documents: KbDocument[];
  chunks: KbChunk[];
  embeddingModels: EmbeddingModelInfo[];

  // Selection
  selectedBaseId: string;
  selectedDocId: string | null;
  selectedChunkId: string | null;

  // Embedding
  selectedEmbedModel: string;
  isEmbedding: boolean;

  // Search
  searchQuery: string;
  searchResults: SearchResult[];
  isSearching: boolean;

  // RAG
  ragQuery: string;
  ragChatModel: string;
  ragMessages: RagMessage[];
  isRagLoading: boolean;

  // Import
  showImportForm: boolean;
  isImporting: boolean;
  importForm: ImportFormState;

  // Sub-tab
  activeSubTab: "chunks" | "search" | "rag";

  // Actions — Data loading
  loadDocuments: () => Promise<void>;
  loadKnowledgeBases: () => Promise<void>;
  loadChunks: (docId: string) => Promise<void>;
  loadEmbeddingModels: () => Promise<void>;

  // Actions — Selection
  selectDocument: (id: string | null) => void;
  selectKnowledgeBase: (id: string) => void;
  createKnowledgeBase: (name: string, description?: string) => Promise<void>;
  deleteKnowledgeBase: (id: string) => Promise<void>;
  selectChunk: (id: string | null) => void;

  // Actions — Import
  importDocument: () => Promise<void>;
  importFile: (filePath: string) => Promise<void>;
  importDirectory: (dirPath: string, extensions?: string) => Promise<void>;
  deleteDocument: (id: string) => Promise<void>;
  setShowImportForm: (show: boolean) => void;
  updateImportForm: (field: keyof ImportFormState, value: string) => void;

  // Actions — Embedding
  generateEmbeddings: () => Promise<import("@/types").EmbeddingProgress | undefined>;
  batchEmbedAll: () => Promise<void>;
  setSelectedEmbedModel: (model: string) => void;

  // Actions — Search
  hybridSearch: () => Promise<void>;
  setSearchQuery: (q: string) => void;

  // Actions — RAG
  sendRagQuery: () => Promise<void>;
  setRagQuery: (q: string) => void;
  setRagChatModel: (m: string) => void;

  // Actions — Sub-tab
  setActiveSubTab: (tab: "chunks" | "search" | "rag") => void;
}

// ── Hook Implementation ────────────────────────────────

export function useKnowledgeBase(): UseKnowledgeBaseReturn {
  // Data state
  const [knowledgeBases, setKnowledgeBases] = useState<KnowledgeBase[]>([]);
  const [documents, setDocuments] = useState<KbDocument[]>([]);
  const [chunks, setChunks] = useState<KbChunk[]>([]);
  const [embeddingModels, setEmbeddingModels] = useState<EmbeddingModelInfo[]>([]);

  // Selection state
  const [selectedBaseId, setSelectedBaseId] = useState("default");
  const [selectedDocId, setSelectedDocId] = useState<string | null>(null);
  const [selectedChunkId, setSelectedChunkId] = useState<string | null>(null);

  // Embedding state
  const [selectedEmbedModel, setSelectedEmbedModel] = useState("");
  const [isEmbedding, setIsEmbedding] = useState(false);

  // Search state
  const [searchQuery, setSearchQuery] = useState("");
  const [searchResults, setSearchResults] = useState<SearchResult[]>([]);
  const [isSearching, setIsSearching] = useState(false);

  // RAG state
  const [ragQuery, setRagQuery] = useState("");
  const [ragChatModel, setRagChatModel] = useState("");
  const [ragMessages, setRagMessages] = useState<RagMessage[]>([]);
  const [isRagLoading, setIsRagLoading] = useState(false);

  // Import state
  const [showImportForm, setShowImportForm] = useState(false);
  const [isImporting, setIsImporting] = useState(false);
  const [importForm, setImportForm] = useState<ImportFormState>(EMPTY_IMPORT_FORM);

  // Sub-tab
  const [activeSubTab, setActiveSubTab] = useState<"chunks" | "search" | "rag">("chunks");

  // ── Data loading ────────────────────────────────────

  const loadKnowledgeBases = useCallback(async () => {
    try {
      const bases = await knowledgeApi.listBases();
      setKnowledgeBases(bases);
      setSelectedBaseId((current) =>
        bases.some((base) => base.id === current) ? current : bases[0]?.id || "default"
      );
    } catch (e) {
      console.error("[useKnowledgeBase] Failed to load knowledge bases:", e);
    }
  }, []);

  const loadDocuments = useCallback(async () => {
    try {
      const docs = await knowledgeApi.listDocuments(selectedBaseId);
      setDocuments(docs);
    } catch (e) {
      console.error("[useKnowledgeBase] Failed to load documents:", e);
    }
  }, [selectedBaseId]);

  const loadChunks = useCallback(async (docId: string) => {
    try {
      const c = await knowledgeApi.getChunks(docId);
      setChunks(c);
    } catch (e) {
      console.error("[useKnowledgeBase] Failed to load chunks:", e);
    }
  }, []);

  const loadEmbeddingModels = useCallback(async () => {
    try {
      const models = await knowledgeApi.getEmbeddingModels();
      setEmbeddingModels(models);
      // Default to the persisted app-wide embedding model (shared with the
      // evolution/memory vectors) so the choice is fixed across restarts; fall
      // back to the first available model only if nothing is saved yet.
      const saved = await settingsApi.get("embedding_model").catch(() => null);
      setSelectedEmbedModel((prev) => prev || saved || models[0]?.model_name || "");
    } catch (e) {
      console.error("[useKnowledgeBase] Failed to load embedding models:", e);
    }
  }, []);

  // Persisting setter: the UI's model dropdown calls this, so the user's choice
  // sticks and stays consistent with the memory/workspace vectors.
  const chooseEmbedModel = useCallback((model: string) => {
    setSelectedEmbedModel(model);
    void settingsApi.set("embedding_model", model).catch(() => {});
  }, []);

  // Auto-load on mount
  useEffect(() => {
    loadKnowledgeBases();
    loadDocuments();
    loadEmbeddingModels();
  }, [loadKnowledgeBases, loadDocuments, loadEmbeddingModels]);

  // Load chunks when document selection changes
  useEffect(() => {
    if (selectedDocId) {
      loadChunks(selectedDocId);
    } else {
      setChunks([]);
    }
  }, [selectedDocId, loadChunks]);

  // ── Selection ───────────────────────────────────────

  const selectDocument = useCallback((id: string | null) => {
    setSelectedDocId(id);
    setSelectedChunkId(null);
    if (id) setActiveSubTab("chunks");
  }, []);

  const selectKnowledgeBase = useCallback((id: string) => {
    setSelectedBaseId(id);
    setSelectedDocId(null);
    setSelectedChunkId(null);
    setChunks([]);
    setSearchResults([]);
    setRagMessages([]);
  }, []);

  const createKnowledgeBase = useCallback(async (name: string, description = "") => {
    const created = await knowledgeApi.createBase(name, description);
    await loadKnowledgeBases();
    setSelectedBaseId(created.id);
  }, [loadKnowledgeBases]);

  const deleteKnowledgeBase = useCallback(async (id: string) => {
    await knowledgeApi.deleteBase(id);
    if (selectedBaseId === id) setSelectedBaseId("default");
    await loadKnowledgeBases();
  }, [loadKnowledgeBases, selectedBaseId]);

  const selectChunk = useCallback((id: string | null) => {
    setSelectedChunkId(id);
  }, []);

  // ── Import ──────────────────────────────────────────

  const updateImportForm = useCallback((field: keyof ImportFormState, value: string) => {
    setImportForm(prev => ({ ...prev, [field]: value }));
  }, []);

  const importDocument = useCallback(async () => {
    if (!importForm.title.trim() || !importForm.content.trim()) {
      throw new Error("请填写标题和内容");
    }
    setIsImporting(true);
    try {
      await knowledgeApi.importDocument({
        knowledgeBaseId: selectedBaseId,
        title: importForm.title.trim(),
        sourcePath: importForm.sourcePath || "manual_input",
        fileType: importForm.fileType,
        content: importForm.content,
      });
      setImportForm(EMPTY_IMPORT_FORM);
      setShowImportForm(false);
      await loadDocuments();
    } finally {
      setIsImporting(false);
    }
      await loadKnowledgeBases();
  }, [importForm, loadDocuments, loadKnowledgeBases, selectedBaseId]);

  const importFile = useCallback(async (filePath: string) => {
    setIsImporting(true);
    try {
      await knowledgeApi.importFile({ filePath, knowledgeBaseId: selectedBaseId });
      setShowImportForm(false);
      await loadDocuments();
    } finally {
      setIsImporting(false);
    }
      await loadKnowledgeBases();
  }, [loadDocuments, loadKnowledgeBases, selectedBaseId]);

  const importDirectory = useCallback(async (dirPath: string, extensions?: string) => {
    setIsImporting(true);
    try {
      await knowledgeApi.importDirectory({ directoryPath: dirPath, extensions, knowledgeBaseId: selectedBaseId });
      setShowImportForm(false);
      await loadDocuments();
    } finally {
      setIsImporting(false);
    }
      await loadKnowledgeBases();
  }, [loadDocuments, loadKnowledgeBases, selectedBaseId]);

  const deleteDocument = useCallback(async (id: string) => {
    try {
      await knowledgeApi.deleteDocument(id);
      if (selectedDocId === id) {
        setSelectedDocId(null);
        setChunks([]);
      }
      await loadDocuments();
      await loadKnowledgeBases();
    } catch (e) {
      console.error("[useKnowledgeBase] Failed to delete document:", e);
      throw e; // re-throw so callers can show user feedback
    }
  }, [selectedDocId, loadDocuments, loadKnowledgeBases]);

  // ── Embedding ───────────────────────────────────────

  const generateEmbeddings = useCallback(async () => {
    if (!selectedDocId || !selectedEmbedModel) {
      throw new Error("请选择文档和嵌入模型");
    }
    setIsEmbedding(true);
    try {
      const progress = await knowledgeApi.generateEmbeddings({
        documentId: selectedDocId,
        modelName: selectedEmbedModel,
      });
      await loadDocuments();
      await loadChunks(selectedDocId);
      return progress;
    } finally {
      setIsEmbedding(false);
    }
  }, [selectedDocId, selectedEmbedModel, loadDocuments, loadChunks]);

  const batchEmbedAll = useCallback(async () => {
    const pendingDocs = documents.filter(d => d.embedding_status === "pending" || d.embedding_status === "failed");
    if (pendingDocs.length === 0) {
      throw new Error("没有待嵌入的文档");
    }
    if (!selectedEmbedModel) {
      throw new Error("请先选择嵌入模型");
    }
    setIsEmbedding(true);
    try {
      for (const doc of pendingDocs) {
        try {
          await knowledgeApi.generateEmbeddings({
            documentId: doc.id,
            modelName: selectedEmbedModel,
          });
        } catch (e) {
          console.error(`[useKnowledgeBase] Embedding failed for ${doc.id}:`, e);
          // Continue with next document
        }
      }
      await loadDocuments();
      if (selectedDocId) await loadChunks(selectedDocId);
    } finally {
      setIsEmbedding(false);
    }
  }, [documents, selectedEmbedModel, selectedDocId, loadDocuments, loadChunks]);

  // ── Search ──────────────────────────────────────────

  const hybridSearch = useCallback(async () => {
    if (!searchQuery.trim() || !selectedEmbedModel) {
      throw new Error("请输入查询并选择嵌入模型");
    }
    setIsSearching(true);
    try {
      const results = await knowledgeApi.hybridSearch({
        query: searchQuery.trim(),
        embeddingModel: selectedEmbedModel,
        limit: 10,
        knowledgeBaseIds: [selectedBaseId],
      });
      setSearchResults(results);
    } finally {
      setIsSearching(false);
    }
  }, [searchQuery, selectedEmbedModel, selectedBaseId]);

  // ── RAG ─────────────────────────────────────────────

  const sendRagQuery = useCallback(async () => {
    if (!ragQuery.trim() || !selectedEmbedModel) {
      throw new Error("请输入问题并选择嵌入模型");
    }
    setIsRagLoading(true);
    const userMsg = ragQuery.trim();
    setRagQuery("");
    setRagMessages(prev => [...prev, { role: "user", content: userMsg }]);

    try {
      const response = await knowledgeApi.ragQuery({
        query: userMsg,
        embeddingModel: selectedEmbedModel,
        chatModel: ragChatModel || "deepseek-chat",
        topK: 5,
        knowledgeBaseIds: [selectedBaseId],
      });
      setRagMessages(prev => [...prev, {
        role: "assistant",
        content: response.answer,
        sources: response.sources,
      }]);
    } catch (e) {
      setRagMessages(prev => [...prev, { role: "assistant", content: `❌ 错误: ${e}` }]);
    } finally {
      setIsRagLoading(false);
    }
  }, [ragQuery, selectedEmbedModel, ragChatModel, selectedBaseId]);

  // ── Return ──────────────────────────────────────────

  return {
    knowledgeBases, documents, chunks, embeddingModels,
    selectedBaseId, selectedDocId, selectedChunkId,
    selectedEmbedModel, isEmbedding,
    searchQuery, searchResults, isSearching,
    ragQuery, ragChatModel, ragMessages, isRagLoading,
    showImportForm, isImporting, importForm,
    activeSubTab,

    loadDocuments, loadKnowledgeBases, loadChunks, loadEmbeddingModels,
    selectDocument, selectKnowledgeBase, createKnowledgeBase, deleteKnowledgeBase, selectChunk,
    importDocument, importFile, importDirectory, deleteDocument, setShowImportForm, updateImportForm,
    generateEmbeddings, batchEmbedAll, setSelectedEmbedModel: chooseEmbedModel,
    hybridSearch, setSearchQuery,
    sendRagQuery, setRagQuery, setRagChatModel,
    setActiveSubTab,
  };
}

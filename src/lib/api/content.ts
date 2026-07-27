/** Auto-split from tauri-api.ts — domain: content. Import via "@/lib/tauri-api". */
import { invoke } from "@tauri-apps/api/core";
import type {
  KnowledgeBase,
  KbDocument,
  KbChunk,
  SearchResult,
  RagResponse,
  EmbeddingModelInfo,
  EmbeddingProgress,
  ChunkConfig,
  QaResponse,
  SelectionCaptureResult,
  SelectionHistoryEntry,
  TranslateResponse,
  TranslateHistoryEntry,
  SearchProvider,
  WebSearchResult,
  SearchHistoryEntry,
  McpServer,
} from "@/types";

// ── Knowledge Base ─────────────────────────────────────

export const knowledgeApi = {
  listBases: () => invoke<KnowledgeBase[]>("kb_list_bases"),
  createBase: (name: string, description = "") =>
    invoke<KnowledgeBase>("kb_create_base", { name, description }),
  updateBase: (knowledgeBaseId: string, name: string, description = "") =>
    invoke("kb_update_base", { knowledgeBaseId, name, description }),
  deleteBase: (knowledgeBaseId: string) => invoke("kb_delete_base", { knowledgeBaseId }),
  exportBase: (knowledgeBaseId: string) => invoke<string>("kb_export_base", { knowledgeBaseId }),
  importBase: (data: string) => invoke<KnowledgeBase>("kb_import_base", { data }),
  listDocuments: (knowledgeBaseId?: string) =>
    invoke<KbDocument[]>("kb_list_documents", { knowledgeBaseId }),
  importDocument: (params: { knowledgeBaseId?: string; title: string; sourcePath: string; fileType: string; content: string; chunkConfig?: ChunkConfig }) =>
    invoke<KbDocument>("kb_import_document", params),
  importFile: (params: { filePath: string; knowledgeBaseId?: string; chunkConfig?: ChunkConfig }) =>
    invoke<KbDocument>("kb_import_file", params),
  importDirectory: (params: { directoryPath: string; extensions?: string; knowledgeBaseId?: string }) =>
    invoke<KbDocument[]>("kb_import_directory", params),
  deleteDocument: (documentId: string) => invoke("kb_delete_document", { documentId }),
  getChunks: (documentId: string) => invoke<KbChunk[]>("kb_get_chunks", { documentId }),
  generateEmbeddings: (params: { documentId: string; modelName: string }) =>
    invoke<EmbeddingProgress>("kb_generate_embeddings", params),
  hybridSearch: (params: { query: string; embeddingModel: string; limit?: number; knowledgeBaseIds?: string[] }) =>
    invoke<SearchResult[]>("kb_hybrid_search", params),
  ragQuery: (params: { query: string; embeddingModel: string; chatModel: string; topK?: number; knowledgeBaseIds?: string[] }) =>
    invoke<RagResponse>("kb_rag_query", params),
  getEmbeddingModels: () => invoke<EmbeddingModelInfo[]>("kb_get_embedding_models"),
};

// ── Quick Assistant ────────────────────────────────────

export const qaApi = {
  toggle: (visible: boolean) => invoke("toggle_quick_assistant", { visible }),
  showWithText: (text: string) => invoke("show_quick_assistant_with_text", { text }),
  query: (params: { query: string; useKb: boolean; chatModel: string; embeddingModel?: string }) =>
    invoke<QaResponse>("qa_query", params),
  queryStream: (params: { query: string; useKb: boolean; chatModel: string; embeddingModel?: string }) =>
    invoke<string>("qa_query_stream", params),
};

// ── Selection Assistant ──────────────────────────────────

export const selectionApi = {
  captureAndShow: () => invoke("capture_selection_and_show"),
  getText: () => invoke<string>("get_selection_text"),
  getWithContext: () => invoke<SelectionCaptureResult>("get_selection_with_context"),
  getHistory: (limit?: number) =>
    invoke<SelectionHistoryEntry[]>("get_selection_history", { limit: limit ?? 50 }),
  deleteHistoryItem: (id: string) => invoke("delete_selection_history_item", { id }),
  clearHistory: () => invoke("clear_selection_history"),
  toggleAutoCapture: (enabled: boolean) => invoke<boolean>("toggle_selection_auto_capture", { enabled }),
};

// ── Translation ──────────────────────────────────────────

export const translationApi = {
  translate: (params: { text: string; targetLang: string; sourceLang?: string; chatModel?: string; prompt?: string }) =>
    invoke<TranslateResponse>("translate_text", params),
  detectLanguage: (params: { text: string; chatModel?: string }) =>
    invoke<string>("detect_language", params),
  getHistory: (limit?: number) =>
    invoke<TranslateHistoryEntry[]>("get_translation_history", { limit: limit ?? 50 }),
  deleteHistoryItem: (id: string) =>
    invoke("delete_translation_history_item", { id }),
  clearHistory: () => invoke("clear_translation_history"),
};

// ── Web Search ──────────────────────────────────────────

export const searchApi = {
  listProviders: () => invoke<SearchProvider[]>("get_search_providers"),
  saveProvider: (provider: SearchProvider) => invoke("save_search_provider", { provider }),
  deleteProvider: (id: string) => invoke("delete_search_provider", { id }),
  search: (query: string, providerId?: string, limit?: number) =>
    invoke<WebSearchResult[]>("web_search", { query, providerId, limit }),
  getHistory: (limit?: number) =>
    invoke<SearchHistoryEntry[]>("get_search_history", { limit: limit ?? 50 }),
  deleteHistoryItem: (id: string) => invoke("delete_search_history_item", { id }),
  clearHistory: () => invoke("clear_search_history"),
};

// ── MCP Servers ─────────────────────────────────────────

export const mcpApi = {
  list: () => invoke<McpServer[]>("get_mcp_servers"),
  save: (server: McpServer) => invoke("save_mcp_server", { server }),
  delete: (id: string) => invoke("delete_mcp_server", { id }),
};


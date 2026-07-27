/** Auto-split from tauri-api.ts — domain: office. Import via "@/lib/tauri-api". */
import { invoke } from "@tauri-apps/api/core";

// ── Presentations / PPT panel (结构化幻灯模型，preview == export) ──

export type SlideLayout =
  | "cover" | "section" | "bullets" | "content"
  | "two-column" | "quote" | "image" | "image-left";

export interface SlideColumn {
  title?: string;
  bullets?: string[];
  body?: string;
}
export interface Slide {
  layout: SlideLayout | string;
  title?: string;
  subtitle?: string;
  bullets?: string[];
  body?: string;
  columns?: SlideColumn[];
  image?: string;
  notes?: string;
}
export interface Brand {
  name: string;
  primary: string;
  accent: string;
  background: string;
  text: string;
  font: string;
  logo: string;
  footer: string;
}
export interface Deck {
  id: string;
  title: string;
  theme: string;
  slides: Slide[];
  brand?: Brand | null;
}
export interface OutlineItem {
  layout: string;
  title: string;
  points: string[];
}
export interface Outline {
  title: string;
  theme: string;
  items: OutlineItem[];
}
export interface DeckMeta {
  id: string;
  title: string;
  theme: string;
  slide_count: number;
  updated_at: string;
}
export interface DeckRecord {
  id: string;
  title: string;
  theme: string;
  model_json: string;
}
export interface DeckVersion {
  id: number;
  label: string;
  created_at: string;
}
export const DECK_THEMES = ["midnight", "minimal", "corporate", "sunset"] as const;

export const slidesApi = {
  list: () => invoke<DeckMeta[]>("list_decks"),
  get: (id: string) => invoke<DeckRecord>("get_deck", { id }),
  create: (title: string, theme: string) =>
    invoke<DeckRecord>("create_deck", { title, theme }),
  save: (id: string, modelJson: string) =>
    invoke<DeckRecord>("save_deck", { id, modelJson }),
  remove: (id: string) => invoke<void>("delete_deck", { id }),
  render: (modelJson: string, slideIndex?: number | null, print = false) =>
    invoke<string>("render_deck", {
      modelJson,
      slideIndex: slideIndex ?? null,
      print,
    }),
  generate: (topic: string, chatModel: string, slideCount?: number) =>
    invoke<DeckRecord>("generate_deck", {
      topic,
      chatModel,
      slideCount: slideCount ?? null,
    }),
  editAi: (id: string, instruction: string, chatModel: string) =>
    invoke<DeckRecord>("edit_deck_ai", { id, instruction, chatModel }),
  exportHtml: (modelJson: string) =>
    invoke<string>("export_deck_html", { modelJson }),
  exportPdf: (modelJson: string) =>
    invoke<string>("export_deck_pdf", { modelJson }),
  // E: real PowerPoint from the same JSON model, QA'd by OfficeCLI on the way out
  exportPptx: (modelJson: string) =>
    invoke<PptxExportResult>("export_deck_pptx", { modelJson }),
  // A: two-stage generation (outline → expand)
  generateOutline: (topic: string, chatModel: string, slideCount?: number) =>
    invoke<Outline>("generate_outline", { topic, chatModel, slideCount: slideCount ?? null }),
  expandOutline: (outline: Outline, chatModel: string) =>
    invoke<DeckRecord>("expand_outline", { outline, chatModel }),
  // B: single-slide diff edit
  editSlide: (id: string, slideIndex: number, instruction: string, chatModel: string) =>
    invoke<DeckRecord>("edit_slide_ai", { id, slideIndex, instruction, chatModel }),
  // C: auto illustration
  suggestImagePrompt: (modelJson: string, slideIndex: number) =>
    invoke<string>("suggest_slide_image_prompt", { modelJson, slideIndex }),
  generateImage: (
    id: string,
    slideIndex: number,
    platformId: string,
    model: string,
    prompt: string,
    size?: string,
  ) =>
    invoke<DeckRecord>("generate_slide_image", {
      id, slideIndex, platformId, model, prompt, size: size ?? null,
    }),
  // Version history — every AI mutation is undoable
  listVersions: (id: string) => invoke<DeckVersion[]>("list_deck_versions", { id }),
  restoreVersion: (id: string, versionId?: number) =>
    invoke<DeckRecord>("restore_deck_version", { id, versionId: versionId ?? null }),
  // D: reusable brand masters
  listBrands: () => invoke<Brand[]>("list_brands"),
  saveBrand: (brand: Brand) => invoke<void>("save_brand", { brand }),
  deleteBrand: (name: string) => invoke<void>("delete_brand", { name }),
};

// ── Write (Markdown writing workspace)──

export interface WriteSpace {
  name: string;
  path: string;
  is_default: boolean;
}
export interface WriteFile {
  name: string;
  relative_path: string;
  updated_at: string;
}
export const writeApi = {
  listSpaces: () => invoke<WriteSpace[]>("write_list_spaces"),
  addSpace: (path: string) => invoke<WriteSpace>("write_add_space", { path }),
  removeSpace: (path: string) => invoke("write_remove_space", { path }),
  listFiles: (spacePath: string) => invoke<WriteFile[]>("write_list_files", { spacePath }),
  readFile: (spacePath: string, relativePath: string) =>
    invoke<string>("write_read_file", { spacePath, relativePath }),
  saveFile: (spacePath: string, relativePath: string, content: string) =>
    invoke("write_save_file", { spacePath, relativePath, content }),
  createFile: (spacePath: string, name: string) =>
    invoke<string>("write_create_file", { spacePath, name }),
  renameFile: (spacePath: string, relativePath: string, newName: string) =>
    invoke<string>("write_rename_file", { spacePath, relativePath, newName }),
  deleteFile: (spacePath: string, relativePath: string) =>
    invoke("write_delete_file", { spacePath, relativePath }),
  exportHtml: (spacePath: string, relativePath: string, html: string) =>
    invoke<string>("write_export_html", { spacePath, relativePath, html }),
};


export interface PptxQa {
  ran: boolean;
  schema_ok: boolean;
  issue_count: number;
  detail: string[];
}
export interface PptxExportResult { path: string; qa: PptxQa; }
export interface OfficeStatus {
  installed: boolean;
  path: string | null;
  kind: "managed" | "system" | null;
  version: string | null;
  pinned_version: string;
  update_available: boolean;
  skill_pool: string | null;
  skill_reviewed: boolean;
}
export interface WriteSection { title: string; brief: string; }
export const officeApi = {
  status: () => invoke<OfficeStatus>("office_status"),
  install: () => invoke<string>("office_install"),
  importPptx: (filePath: string) => invoke<DeckRecord>("import_pptx_deck", { filePath }),
  // P1 Word
  exportDocx: (markdown: string, title: string, brandName?: string) =>
    invoke<string>("export_write_docx", { markdown, title, brandName: brandName ?? null }),
  importDocx: (filePath: string) => invoke<string>("import_docx_markdown", { filePath }),
  mergeBatch: (template: string, dataJson: string, nameKey?: string) =>
    invoke<string[]>("office_merge_batch", { template, dataJson, nameKey: nameKey ?? null }),
  writeOutline: (topic: string, chatModel: string) =>
    invoke<WriteSection[]>("write_outline_ai", { topic, chatModel }),
  writeExpand: (topic: string, section: WriteSection, chatModel: string) =>
    invoke<string>("write_expand_ai", { topic, section, chatModel }),
  // P2 Excel + 统一预览
  previewHtml: (filePath: string) => invoke<string>("office_preview_html", { filePath }),
  excelNew: (title: string) => invoke<string>("excel_new", { title }),
  excelAiEdit: (filePath: string, instruction: string, chatModel: string) =>
    invoke<string>("excel_ai_edit", { filePath, instruction, chatModel }),
  excelImportCsv: (filePath: string, csvPath: string) =>
    invoke<void>("excel_import_csv", { filePath, csvPath }),
};

// 监督台 — live console over all running agent sessions.

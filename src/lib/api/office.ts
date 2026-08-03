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
/** P3 结构化条目：一张表喂全部数据类版式（图表/指标/流程/时间线/风险…）。 */
export interface SlideItem {
  label?: string;
  value?: number;
  /** 第二个数值，目前只有甘特用（条长） */
  span?: number;
  detail?: string;
  group?: string;
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
  /** P1 页面角色（cover/metric/risk…） */
  role?: string;
  /** P2 控件值。键取自版式目录，脏值由后端回落——前端不做二次校验。 */
  params?: Record<string, unknown>;
  items?: SlideItem[];
}

// ── P2 版式目录：控件契约由后端单点定义，前端只负责画 ──

export type ControlKind = "range" | "toggle" | "select";
export interface LayoutControl {
  key: string;
  label: string;
  kind: ControlKind;
  min?: number;
  max?: number;
  default: number | boolean | string;
  options?: [string, string][];
  desc: string;
}
export interface PageRole {
  key: string;
  label: string;
  layouts: string[];
  intent: string;
}
export interface LayoutInfo {
  key: string;
  label: string;
  fields_hint: string;
  controls: LayoutControl[];
}
export interface LayoutCatalog {
  roles: PageRole[];
  layouts: LayoutInfo[];
}
export interface SlideCandidate {
  label: string;
  kind: "template" | "ai";
  slide_json: string;
  html: string;
}

// ── P0 体检：报出渲染时会静默咽下去的问题 ──

export type LintSeverity = "error" | "warning" | "info";
export interface LintFinding {
  /** 稳定的机器可读码，如 "bullets-over-budget" */
  code: string;
  severity: LintSeverity;
  /** 0 起的页码；缺省 = 整份演示层面的问题 */
  slide?: number;
  message: string;
}
export interface LintReport {
  ok: boolean;
  errors: number;
  warnings: number;
  infos: number;
  findings: LintFinding[];
}

/** 演讲者视图窗口与放映主窗之间的同步事件。 */
export const SPEAKER_EVENT = {
  /** 主窗 → 演讲者视图：当前状态 */
  state: "slides-present-state",
  /** 演讲者视图 → 主窗：翻页 */
  nav: "slides-present-nav",
  /** 演讲者视图 → 主窗：刚开窗，把状态再发一遍 */
  hello: "slides-present-hello",
} as const;

export interface SpeakerState {
  deckJson: string;
  index: number;
  /** 主窗还在放映吗——退出放映时演讲者视图要知道 */
  presenting: boolean;
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
  /** P1 页面角色：先定「这页干什么」，版式由角色推导 */
  role?: string;
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
  // P3: 导出的 HTML 里嵌了 deck JSON，能原样读回来继续编辑
  importHtml: (filePath: string) =>
    invoke<DeckRecord>("import_deck_html", { filePath }),
  // P0: 体检
  lint: (modelJson: string) => invoke<LintReport>("lint_deck", { modelJson }),
  // P2: 演讲者视图（另开一个窗口——备注只能给讲的人看）
  openSpeakerView: () => invoke<void>("open_speaker_view"),
  closeSpeakerView: () => invoke<void>("close_speaker_view"),
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
  // B: single-slide diff edit. lockTemplate（默认 true）= 只换文案，
  // 版式/角色/控件参数/图片槽由后端确定性还原，不指望模型守规矩。
  editSlide: (
    id: string,
    slideIndex: number,
    instruction: string,
    chatModel: string,
    lockTemplate = true,
  ) =>
    invoke<DeckRecord>("edit_slide_ai", {
      id, slideIndex, instruction, chatModel, lockTemplate,
    }),
  // P2: 版式目录（页面角色 + 每个版式的控件契约）
  layoutCatalog: () => invoke<LayoutCatalog>("slides_layout_catalog"),
  // 每页多候选：模板候选本地算，只有 AI 那个花调用
  candidates: (id: string, slideIndex: number, chatModel: string, includeAi = true) =>
    invoke<SlideCandidate[]>("slide_candidates", {
      id, slideIndex, chatModel, includeAi,
    }),
  applyCandidate: (id: string, slideIndex: number, slideJson: string) =>
    invoke<DeckRecord>("apply_slide_candidate", { id, slideIndex, slideJson }),
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

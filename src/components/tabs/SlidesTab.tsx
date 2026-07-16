/**
 * SlidesTab — PPT 演示工作台。
 *
 * 结构化 JSON 幻灯模型是唯一真源：AI 生成/修改的是模型字段，渲染是确定性的
 * （预览 == 导出），所以“细微修改”精准可控，而且随时可手动编辑，不是一张图。
 */
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  ArrowDown,
  ArrowLeft,
  ArrowUp,
  FileDown,
  FileText,
  FileUp,
  Image as ImageIcon,
  ListTree,
  Loader2,
  Palette,
  Play,
  Plus,
  Presentation,
  Sparkles,
  Trash2,
  Undo2,
  Wand2,
  X,
} from "lucide-react";
import { toast } from "sonner";

import { cn } from "@/lib/utils";
import type { PlatformModel, ModelPlatform } from "@/types";
import {
  DECK_THEMES,
  modelApi,
  platformApi,
  officeApi,
  shellApi,
  slidesApi,
  type Brand,
  type Deck,
  type DeckMeta,
  type DeckVersion,
  type Outline,
  type Slide,
} from "@/lib/tauri-api";

const THEME_LABEL: Record<string, string> = {
  midnight: "午夜蓝",
  minimal: "极简白",
  corporate: "商务蓝",
  sunset: "落日紫",
};

const LAYOUTS: { id: string; label: string }[] = [
  { id: "cover", label: "封面" },
  { id: "section", label: "章节页" },
  { id: "bullets", label: "要点" },
  { id: "content", label: "图文" },
  { id: "two-column", label: "双栏" },
  { id: "quote", label: "引用" },
  { id: "image", label: "大图" },
  { id: "image-left", label: "左图右文" },
];

function slideSummary(s: Slide): string {
  return s.title || s.body?.slice(0, 24) || s.bullets?.[0] || "（空白页）";
}

export function SlidesTab() {
  const [decks, setDecks] = useState<DeckMeta[]>([]);
  const [deck, setDeck] = useState<Deck | null>(null);
  const [selected, setSelected] = useState(0);
  const [previewHtml, setPreviewHtml] = useState("");
  const [models, setModels] = useState<PlatformModel[]>([]);
  const [chatModel, setChatModel] = useState("");
  const [topic, setTopic] = useState("");
  const [slideCount, setSlideCount] = useState(10);
  const [instruction, setInstruction] = useState("");
  const [generating, setGenerating] = useState(false);
  const [aiEditing, setAiEditing] = useState(false);
  const [exporting, setExporting] = useState(false);
  const [saveState, setSaveState] = useState<"saved" | "saving" | "dirty">("saved");
  const [scale, setScale] = useState(0.5);

  // A: 大纲两阶段
  const [outline, setOutline] = useState<Outline | null>(null);
  const [expanding, setExpanding] = useState(false);
  // B: 单页 vs 整份
  const [editScope, setEditScope] = useState<"slide" | "deck">("slide");
  // C: 配图
  const [imgOpen, setImgOpen] = useState(false);
  const [imgPrompt, setImgPrompt] = useState("");
  const [imgPlatforms, setImgPlatforms] = useState<ModelPlatform[]>([]);
  const [imgPlatform, setImgPlatform] = useState("");
  const [imgModel, setImgModel] = useState("gpt-image-1");
  const [imaging, setImaging] = useState(false);
  // D: 母版
  const [brandOpen, setBrandOpen] = useState(false);
  const [brands, setBrands] = useState<Brand[]>([]);
  // 版本历史（AI 改动可撤销）
  const [versions, setVersions] = useState<DeckVersion[]>([]);
  // 放映
  const [presenting, setPresenting] = useState(false);
  const [presentHtml, setPresentHtml] = useState("");

  const previewBoxRef = useRef<HTMLDivElement | null>(null);
  const saveTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const deckRef = useRef<Deck | null>(null);
  deckRef.current = deck;

  const loadDecks = useCallback(async () => {
    try {
      setDecks(await slidesApi.list());
    } catch (e) {
      toast.error(`读取演示列表失败：${e}`);
    }
  }, []);

  useEffect(() => {
    void loadDecks();
    modelApi
      .getActive()
      .then((list) => {
        const usable = list.filter(
          (m) =>
            !m.model_name.toLowerCase().includes("embedding") &&
            !m.model_name.toLowerCase().includes("rerank"),
        );
        setModels(usable);
        if (usable.length > 0) {
          setChatModel(`${usable[0].platform_id}:${usable[0].model_name}`);
        }
      })
      .catch(() => {});
  }, [loadDecks]);

  // ── preview: deterministic re-render of the selected slide ──
  useEffect(() => {
    if (!deck || deck.slides.length === 0) {
      setPreviewHtml("");
      return;
    }
    const idx = Math.min(selected, deck.slides.length - 1);
    let cancelled = false;
    slidesApi
      .render(JSON.stringify(deck), idx, false)
      .then((html) => {
        if (!cancelled) setPreviewHtml(html);
      })
      .catch((e) => console.error("render failed:", e));
    return () => {
      cancelled = true;
    };
  }, [deck, selected]);

  // ── scale the fixed 1280×720 canvas into the preview box ──
  useEffect(() => {
    const el = previewBoxRef.current;
    if (!el) return;
    const update = () => {
      // 1328 = slide 1280 + body padding 24×2
      setScale(Math.min((el.clientWidth - 16) / 1328, (el.clientHeight - 16) / 792));
    };
    update();
    const ro = new ResizeObserver(update);
    ro.observe(el);
    return () => ro.disconnect();
  }, [deck]);

  // ── autosave (debounced) ──
  const scheduleSave = useCallback((next: Deck) => {
    setSaveState("dirty");
    if (saveTimer.current) clearTimeout(saveTimer.current);
    saveTimer.current = setTimeout(async () => {
      setSaveState("saving");
      try {
        await slidesApi.save(next.id, JSON.stringify(next));
        setSaveState("saved");
        void loadDecks();
      } catch (e) {
        setSaveState("dirty");
        toast.error(`保存失败：${e}`);
      }
    }, 800);
  }, [loadDecks]);

  const mutateDeck = useCallback(
    (fn: (d: Deck) => Deck) => {
      setDeck((prev) => {
        if (!prev) return prev;
        const next = fn(structuredClone(prev));
        scheduleSave(next);
        return next;
      });
    },
    [scheduleSave],
  );

  const mutateSlide = useCallback(
    (fn: (s: Slide) => void) => {
      mutateDeck((d) => {
        const idx = Math.min(selected, d.slides.length - 1);
        fn(d.slides[idx]);
        return d;
      });
    },
    [mutateDeck, selected],
  );

  const openDeck = async (id: string) => {
    try {
      const rec = await slidesApi.get(id);
      setDeck(JSON.parse(rec.model_json) as Deck);
      setSelected(0);
      setSaveState("saved");
      void refreshVersions(id);
    } catch (e) {
      toast.error(`打开演示失败：${e}`);
    }
  };

  const handleCreate = async () => {
    try {
      const rec = await slidesApi.create("未命名演示", "midnight");
      await loadDecks();
      await openDeck(rec.id);
    } catch (e) {
      toast.error(`新建失败：${e}`);
    }
  };

  /** 导入现有 pptx：OfficeCLI 提取文本+备注 → 结构化模型 → 全套编辑流水线。 */
  const handleImportPptx = async () => {
    try {
      const path = await shellApi.pickFile();
      if (!path) return;
      if (!path.toLowerCase().endsWith(".pptx")) {
        toast.error("请选择 .pptx 文件");
        return;
      }
      toast.info("正在导入，OfficeCLI 提取内容中…");
      const rec = await officeApi.importPptx(path);
      toast.success("导入完成", { description: "版式为推断结果，图片未迁移——可用「配图」补" });
      await loadDecks();
      await openDeck(rec.id);
    } catch (e) {
      toast.error(`导入失败：${e}`);
    }
  };

  const handleGenerate = async () => {
    if (!topic.trim()) {
      toast.error("先描述一下要做什么演示");
      return;
    }
    if (!chatModel) {
      toast.error("请先在「模型中心」启用一个对话模型");
      return;
    }
    setGenerating(true);
    try {
      const rec = await slidesApi.generate(topic.trim(), chatModel, slideCount);
      toast.success("演示已生成");
      setTopic("");
      await loadDecks();
      await openDeck(rec.id);
    } catch (e) {
      toast.error(`生成失败：${e}`);
    } finally {
      setGenerating(false);
    }
  };

  // ── A：先出大纲，确认后再展开 ──
  const handleOutline = async () => {
    if (!topic.trim()) {
      toast.error("先描述一下要做什么演示");
      return;
    }
    if (!chatModel) {
      toast.error("请先在「模型中心」启用一个对话模型");
      return;
    }
    setGenerating(true);
    try {
      setOutline(await slidesApi.generateOutline(topic.trim(), chatModel, slideCount));
    } catch (e) {
      toast.error(`生成大纲失败：${e}`);
    } finally {
      setGenerating(false);
    }
  };

  const handleExpand = async () => {
    if (!outline || outline.items.length === 0) return;
    setExpanding(true);
    try {
      const rec = await slidesApi.expandOutline(outline, chatModel);
      toast.success(`已按大纲生成 ${outline.items.length} 页`);
      setOutline(null);
      setTopic("");
      await loadDecks();
      await openDeck(rec.id);
    } catch (e) {
      toast.error(`展开失败：${e}`);
    } finally {
      setExpanding(false);
    }
  };

  // ── B：单页精修（默认）/ 整份修改 ──
  const handleAiEdit = async () => {
    if (!deck || !instruction.trim()) return;
    if (!chatModel) {
      toast.error("请先在「模型中心」启用一个对话模型");
      return;
    }
    setAiEditing(true);
    try {
      // Flush any pending manual edits first so the AI sees the latest deck.
      if (saveTimer.current) clearTimeout(saveTimer.current);
      await slidesApi.save(deck.id, JSON.stringify(deck));
      const rec =
        editScope === "slide"
          ? await slidesApi.editSlide(deck.id, selected, instruction.trim(), chatModel)
          : await slidesApi.editAi(deck.id, instruction.trim(), chatModel);
      const next = JSON.parse(rec.model_json) as Deck;
      setDeck(next);
      setSelected((i) => Math.min(i, next.slides.length - 1));
      setSaveState("saved");
      setInstruction("");
      toast.success(editScope === "slide" ? `第 ${selected + 1} 页已修改` : "整份已修改");
      void refreshVersions(deck.id);
      void loadDecks();
    } catch (e) {
      toast.error(`AI 修改失败：${e}`);
    } finally {
      setAiEditing(false);
    }
  };

  // ── C：自动配图 ──
  const openImageDialog = async () => {
    if (!deck) return;
    setImgOpen(true);
    try {
      const [prompt, plats] = await Promise.all([
        slidesApi.suggestImagePrompt(JSON.stringify(deck), selected),
        imgPlatforms.length > 0 ? Promise.resolve(imgPlatforms) : platformApi.list(),
      ]);
      setImgPrompt(prompt);
      setImgPlatforms(plats);
      if (!imgPlatform && plats.length > 0) setImgPlatform(plats[0].id);
    } catch (e) {
      toast.error(String(e));
    }
  };

  const handleGenImage = async () => {
    if (!deck) return;
    if (!imgPlatform) {
      toast.error("请先在「模型中心」添加一个支持生图的供应商");
      return;
    }
    setImaging(true);
    try {
      const rec = await slidesApi.generateImage(
        deck.id, selected, imgPlatform, imgModel, imgPrompt,
      );
      setDeck(JSON.parse(rec.model_json) as Deck);
      setSaveState("saved");
      setImgOpen(false);
      toast.success("配图已插入本页");
      void refreshVersions(deck.id);
    } catch (e) {
      toast.error(`生图失败：${e}`);
    } finally {
      setImaging(false);
    }
  };

  // ── 撤销 AI 改动 ──
  const refreshVersions = useCallback(async (deckId: string) => {
    try {
      setVersions(await slidesApi.listVersions(deckId));
    } catch {
      setVersions([]);
    }
  }, []);

  const handleUndo = async () => {
    if (!deck || versions.length === 0) return;
    try {
      const rec = await slidesApi.restoreVersion(deck.id);
      const next = JSON.parse(rec.model_json) as Deck;
      setDeck(next);
      setSelected((i) => Math.min(i, next.slides.length - 1));
      setSaveState("saved");
      toast.success(`已撤销：${versions[0].label}`);
      void refreshVersions(deck.id);
      void loadDecks();
    } catch (e) {
      toast.error(String(e));
    }
  };

  // ── 本地图片（AI 生图之外的另一半）──
  const pickLocalImage = async () => {
    const file = await shellApi.pickFile();
    if (!file) return;
    mutateSlide((s) => {
      s.image = file;
      if (s.layout === "bullets" || s.layout === "cover" || s.layout === "section") {
        s.layout = "image-left";
      }
    });
    toast.success("已插入本地图片");
  };

  // ── 放映 ──
  const startPresent = async () => {
    if (!deck) return;
    try {
      setPresentHtml(await slidesApi.render(JSON.stringify(deck), null, false));
      setPresenting(true);
    } catch (e) {
      toast.error(String(e));
    }
  };

  // ── D：母版 ──
  const openBrandDialog = async () => {
    setBrandOpen(true);
    try {
      setBrands(await slidesApi.listBrands());
    } catch { /* 母版列表为空不是错误 */ }
  };

  const applyBrand = (brand: Brand | null) => {
    mutateDeck((d) => { d.brand = brand; return d; });
    toast.success(brand ? `已应用母版「${brand.name}」` : "已清除母版");
  };

  const saveCurrentBrand = async () => {
    if (!deck?.brand?.name?.trim()) {
      toast.error("给母版起个名字再保存");
      return;
    }
    try {
      await slidesApi.saveBrand(deck.brand);
      setBrands(await slidesApi.listBrands());
      toast.success(`母版「${deck.brand.name}」已保存，可在其它演示复用`);
    } catch (e) {
      toast.error(String(e));
    }
  };

  const handleExport = async (kind: "html" | "pdf" | "pptx") => {
    if (!deck) return;
    setExporting(true);
    try {
      const json = JSON.stringify(deck);
      if (kind === "pptx") {
        const result = await slidesApi.exportPptx(json);
        // 质检门：OfficeCLI schema + 内容扫描的判决随导出一起给出。
        if (!result.qa.ran) {
          toast.success(`已导出：${result.path}`, {
            description: "未质检（OfficeCLI 未安装）",
            action: {
              label: "安装 OfficeCLI",
              onClick: () => {
                toast.info("正在下载并校验 OfficeCLI…");
                void officeApi
                  .install()
                  .then((p) => toast.success("OfficeCLI 已就绪，下次导出自动质检", { description: p }))
                  .catch((err) => toast.error(`安装失败：${err}`));
              },
            },
            duration: 12000,
          });
        } else if (result.qa.schema_ok && result.qa.issue_count === 0) {
          toast.success(`已导出并通过质检：${result.path}`, { description: "schema 校验通过 · 0 问题" });
        } else {
          toast.warning(`已导出，但质检发现问题：${result.path}`, {
            description: result.qa.detail.slice(0, 4).join("；"),
            duration: 12000,
          });
        }
      } else {
        const path =
          kind === "html" ? await slidesApi.exportHtml(json) : await slidesApi.exportPdf(json);
        toast.success(`已导出：${path}`);
      }
    } catch (e) {
      toast.error(String(e));
    } finally {
      setExporting(false);
    }
  };

  const handleDeleteDeck = async (id: string) => {
    if (!window.confirm("确定删除这个演示？不可恢复。")) return;
    try {
      await slidesApi.remove(id);
      if (deck?.id === id) setDeck(null);
      void loadDecks();
    } catch (e) {
      toast.error(`删除失败：${e}`);
    }
  };

  const slide = deck?.slides[Math.min(selected, (deck?.slides.length ?? 1) - 1)];

  const modelOptions = useMemo(
    () =>
      models.map((m) => ({
        ref: `${m.platform_id}:${m.model_name}`,
        label: m.model_name,
      })),
    [models],
  );

  // ─────────────────────────── gallery ───────────────────────────
  if (!deck) {
    return (
      <div className="flex h-full flex-1 flex-col overflow-hidden bg-background">
        <div className="border-b border-border px-6 py-4">
          <div className="flex items-center gap-2 text-lg font-semibold">
            <Presentation className="h-5 w-5 text-primary" /> 演示 · PPT
          </div>
          <p className="mt-1 max-w-3xl text-sm leading-6 text-muted-foreground">
            用 AI 生成结构化幻灯，随时手动编辑，导出 PDF / HTML。每一页都是可改的字段，不是图片。
          </p>
        </div>

        <div className="flex flex-col gap-6 overflow-y-auto p-6">
          {/* generate form */}
          <div className="rounded-xl border border-border bg-card/40 p-5">
            <div className="flex items-center gap-2 text-sm font-semibold">
              <Sparkles className="h-4 w-4 text-primary" /> AI 生成演示
            </div>
            <textarea
              value={topic}
              onChange={(e) => setTopic(e.target.value)}
              placeholder="描述主题和受众，比如：给管理层汇报 Q2 项目进展，重点讲上线成果、风险和下一步计划"
              rows={3}
              className="mt-3 w-full resize-none rounded-lg border border-border bg-background px-3 py-2 text-sm outline-none focus:border-primary"
            />
            <div className="mt-3 flex flex-wrap items-center gap-3">
              <select
                value={chatModel}
                onChange={(e) => setChatModel(e.target.value)}
                className="h-9 rounded-lg border border-border bg-background px-2 text-sm"
              >
                {modelOptions.length === 0 && <option value="">（无可用模型）</option>}
                {modelOptions.map((m) => (
                  <option key={m.ref} value={m.ref}>{m.label}</option>
                ))}
              </select>
              <label className="flex items-center gap-2 text-sm text-muted-foreground">
                页数
                <input
                  type="number"
                  min={3}
                  max={30}
                  value={slideCount}
                  onChange={(e) => setSlideCount(Number(e.target.value) || 10)}
                  className="h-9 w-16 rounded-lg border border-border bg-background px-2 text-sm"
                />
              </label>
              <div className="ml-auto flex items-center gap-2">
                <button
                  onClick={handleGenerate}
                  disabled={generating || expanding}
                  title="跳过大纲，直接一次性生成整份"
                  className="inline-flex h-9 items-center gap-2 rounded-lg border border-border px-3 text-sm hover:bg-muted/40 disabled:opacity-50"
                >
                  <Wand2 className="h-4 w-4" /> 直接生成
                </button>
                <button
                  onClick={() => void handleOutline()}
                  disabled={generating || expanding}
                  title="先出大纲，你确认/改完再展开成正式内容（推荐）"
                  className="inline-flex h-9 items-center gap-2 rounded-lg bg-primary px-4 text-sm font-medium text-primary-foreground hover:opacity-90 disabled:opacity-50"
                >
                  {generating ? <Loader2 className="h-4 w-4 animate-spin" /> : <ListTree className="h-4 w-4" />}
                  {generating ? "规划中…" : "先出大纲"}
                </button>
              </div>
            </div>
          </div>

          {/* A: 大纲确认 —— 跑偏时改 3 秒大纲，不用重生成整份 */}
          {outline && (
            <div className="rounded-xl border border-primary/40 bg-primary/5 p-5">
              <div className="flex flex-wrap items-center gap-2">
                <ListTree className="h-4 w-4 text-primary" />
                <input
                  value={outline.title}
                  onChange={(e) => setOutline({ ...outline, title: e.target.value })}
                  className="h-8 min-w-52 flex-1 rounded-md border border-transparent bg-transparent px-2 text-sm font-semibold outline-none hover:border-border focus:border-primary"
                />
                <select
                  value={outline.theme}
                  onChange={(e) => setOutline({ ...outline, theme: e.target.value })}
                  className="h-8 rounded-md border border-border bg-background px-2 text-xs"
                >
                  {DECK_THEMES.map((t) => (
                    <option key={t} value={t}>{THEME_LABEL[t]}</option>
                  ))}
                </select>
                <span className="text-xs text-muted-foreground">{outline.items.length} 页</span>
              </div>
              <p className="mt-1 text-xs text-muted-foreground">
                先确认结构再展开——不满意就直接改这里，比重新生成整份快得多。
              </p>
              <div className="mt-3 flex flex-col gap-1.5">
                {outline.items.map((item, i) => (
                  <div key={i} className="flex items-start gap-2 rounded-lg border border-border bg-background/60 p-2">
                    <span className="mt-1.5 w-5 shrink-0 text-center font-mono text-[10px] text-muted-foreground">{i + 1}</span>
                    <select
                      value={item.layout}
                      onChange={(e) => {
                        const items = [...outline.items];
                        items[i] = { ...item, layout: e.target.value };
                        setOutline({ ...outline, items });
                      }}
                      className="h-7 shrink-0 rounded border border-border bg-background px-1 text-[11px]"
                    >
                      {LAYOUTS.map((l) => (
                        <option key={l.id} value={l.id}>{l.label}</option>
                      ))}
                    </select>
                    <div className="flex min-w-0 flex-1 flex-col gap-1">
                      <input
                        value={item.title}
                        onChange={(e) => {
                          const items = [...outline.items];
                          items[i] = { ...item, title: e.target.value };
                          setOutline({ ...outline, items });
                        }}
                        placeholder="这一页的标题"
                        className="h-7 w-full rounded border border-border bg-background px-2 text-xs font-medium outline-none focus:border-primary"
                      />
                      <textarea
                        value={item.points.join("\n")}
                        onChange={(e) => {
                          const items = [...outline.items];
                          items[i] = { ...item, points: e.target.value.split("\n").filter((p) => p.trim()) };
                          setOutline({ ...outline, items });
                        }}
                        rows={Math.max(1, item.points.length)}
                        placeholder="要点提纲（每行一条）"
                        className="w-full resize-none rounded border border-border bg-background px-2 py-1 text-[11px] leading-5 outline-none focus:border-primary"
                      />
                    </div>
                    <div className="flex shrink-0 flex-col gap-0.5">
                      <button
                        onClick={() => {
                          const items = [...outline.items];
                          if (i > 0) { [items[i - 1], items[i]] = [items[i], items[i - 1]]; setOutline({ ...outline, items }); }
                        }}
                        disabled={i === 0}
                        className="rounded p-0.5 text-muted-foreground hover:text-foreground disabled:opacity-30"
                      >
                        <ArrowUp className="h-3 w-3" />
                      </button>
                      <button
                        onClick={() => {
                          const items = [...outline.items];
                          if (i < items.length - 1) { [items[i], items[i + 1]] = [items[i + 1], items[i]]; setOutline({ ...outline, items }); }
                        }}
                        disabled={i === outline.items.length - 1}
                        className="rounded p-0.5 text-muted-foreground hover:text-foreground disabled:opacity-30"
                      >
                        <ArrowDown className="h-3 w-3" />
                      </button>
                      <button
                        onClick={() => setOutline({ ...outline, items: outline.items.filter((_, j) => j !== i) })}
                        className="rounded p-0.5 text-muted-foreground hover:text-destructive"
                      >
                        <Trash2 className="h-3 w-3" />
                      </button>
                    </div>
                  </div>
                ))}
              </div>
              <div className="mt-3 flex items-center gap-2">
                <button
                  onClick={() =>
                    setOutline({
                      ...outline,
                      items: [...outline.items, { layout: "bullets", title: "新页面", points: [] }],
                    })
                  }
                  className="inline-flex h-8 items-center gap-1 rounded-lg border border-dashed border-border px-3 text-xs text-muted-foreground hover:border-primary hover:text-foreground"
                >
                  <Plus className="h-3 w-3" /> 加一页
                </button>
                <button
                  onClick={() => setOutline(null)}
                  className="h-8 rounded-lg border border-border px-3 text-xs hover:bg-muted/40"
                >
                  放弃
                </button>
                <button
                  onClick={() => void handleExpand()}
                  disabled={expanding || outline.items.length === 0}
                  className="ml-auto inline-flex h-9 items-center gap-2 rounded-lg bg-primary px-4 text-sm font-medium text-primary-foreground hover:opacity-90 disabled:opacity-50"
                >
                  {expanding ? <Loader2 className="h-4 w-4 animate-spin" /> : <Sparkles className="h-4 w-4" />}
                  {expanding ? `逐页生成中（${outline.items.length} 页并行）…` : "按大纲展开成正式内容"}
                </button>
              </div>
            </div>
          )}

          {/* deck list */}
          <div>
            <div className="mb-3 flex items-center justify-between">
              <h3 className="text-sm font-semibold text-muted-foreground">我的演示</h3>
              <div className="flex gap-2">
                <button
                  onClick={() => void handleImportPptx()}
                  className="inline-flex h-8 items-center gap-1.5 rounded-lg border border-border px-3 text-sm hover:bg-muted/40"
                  title="导入现有 .pptx（提取文本与备注进入编辑流水线）"
                >
                  <FileUp className="h-4 w-4" /> 导入 PPTX
                </button>
                <button
                  onClick={handleCreate}
                  className="inline-flex h-8 items-center gap-1.5 rounded-lg border border-border px-3 text-sm hover:bg-muted/40"
                >
                  <Plus className="h-4 w-4" /> 空白新建
                </button>
              </div>
            </div>
            {decks.length === 0 ? (
              <div className="rounded-xl border border-dashed border-border p-10 text-center text-sm text-muted-foreground">
                还没有演示——用上面的 AI 生成一个，或空白新建。
              </div>
            ) : (
              <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-3">
                {decks.map((d) => (
                  <div
                    key={d.id}
                    className="group cursor-pointer rounded-xl border border-border bg-card/40 p-4 transition hover:border-primary/60"
                    onClick={() => void openDeck(d.id)}
                  >
                    <div className="flex items-start justify-between gap-2">
                      <div className="truncate text-sm font-semibold" title={d.title}>{d.title}</div>
                      <button
                        onClick={(e) => { e.stopPropagation(); void handleDeleteDeck(d.id); }}
                        className="rounded p-1 text-muted-foreground opacity-0 transition hover:text-destructive group-hover:opacity-100"
                        title="删除"
                      >
                        <Trash2 className="h-3.5 w-3.5" />
                      </button>
                    </div>
                    <div className="mt-2 flex items-center gap-2 text-xs text-muted-foreground">
                      <span>{d.slide_count} 页</span>
                      <span>·</span>
                      <span>{THEME_LABEL[d.theme] ?? d.theme}</span>
                      <span>·</span>
                      <span>{d.updated_at.slice(0, 16)}</span>
                    </div>
                  </div>
                ))}
              </div>
            )}
          </div>
        </div>
      </div>
    );
  }

  // ─────────────────────────── editor ───────────────────────────
  return (
    <div className="flex h-full flex-1 flex-col overflow-hidden bg-background">
      {/* header */}
      <div className="flex items-center gap-3 border-b border-border px-4 py-2.5">
        <button
          onClick={() => { setDeck(null); void loadDecks(); }}
          className="inline-flex h-8 items-center gap-1 rounded-lg border border-border px-2.5 text-sm hover:bg-muted/40"
        >
          <ArrowLeft className="h-4 w-4" /> 返回
        </button>
        <input
          value={deck.title}
          onChange={(e) => mutateDeck((d) => { d.title = e.target.value; return d; })}
          className="h-8 w-64 rounded-lg border border-transparent bg-transparent px-2 text-sm font-semibold outline-none hover:border-border focus:border-primary"
        />
        <select
          value={deck.theme}
          onChange={(e) => mutateDeck((d) => { d.theme = e.target.value; return d; })}
          className="h-8 rounded-lg border border-border bg-background px-2 text-sm"
          title="主题"
        >
          {DECK_THEMES.map((t) => (
            <option key={t} value={t}>{THEME_LABEL[t]}</option>
          ))}
        </select>
        <span className={cn(
          "text-xs",
          saveState === "saved" ? "text-muted-foreground" : saveState === "saving" ? "text-warning" : "text-warning",
        )}>
          {saveState === "saved" ? "已保存" : saveState === "saving" ? "保存中…" : "有改动…"}
        </span>
        <div className="ml-auto flex items-center gap-2">
          <button
            onClick={() => void handleUndo()}
            disabled={versions.length === 0}
            title={versions.length > 0 ? `撤销：${versions[0].label}` : "没有可撤销的 AI 改动"}
            className="inline-flex h-8 items-center gap-1.5 rounded-lg border border-border px-2.5 text-sm hover:bg-muted/40 disabled:opacity-40"
          >
            <Undo2 className="h-4 w-4" />
            撤销{versions.length > 0 ? ` (${versions.length})` : ""}
          </button>
          <button
            onClick={() => void startPresent()}
            title="全屏放映（方向键翻页，Esc 退出）"
            className="inline-flex h-8 items-center gap-1.5 rounded-lg border border-border px-2.5 text-sm hover:bg-muted/40"
          >
            <Play className="h-4 w-4" /> 放映
          </button>
          <button
            onClick={() => void openBrandDialog()}
            title="母版：主色/字体/Logo/页脚，可保存复用"
            className="inline-flex h-8 items-center gap-1.5 rounded-lg border border-border px-3 text-sm hover:bg-muted/40"
          >
            <Palette className="h-4 w-4" /> 母版
          </button>
          <button
            onClick={() => void handleExport("html")}
            disabled={exporting}
            className="inline-flex h-8 items-center gap-1.5 rounded-lg border border-border px-3 text-sm hover:bg-muted/40 disabled:opacity-50"
          >
            <FileText className="h-4 w-4" /> HTML
          </button>
          <button
            onClick={() => void handleExport("pptx")}
            disabled={exporting}
            title="导出真正的 PowerPoint 文件（同事可直接编辑）"
            className="inline-flex h-8 items-center gap-1.5 rounded-lg border border-border px-3 text-sm hover:bg-muted/40 disabled:opacity-50"
          >
            <Presentation className="h-4 w-4" /> PPTX
          </button>
          <button
            onClick={() => void handleExport("pdf")}
            disabled={exporting}
            className="inline-flex h-8 items-center gap-1.5 rounded-lg bg-primary px-3 text-sm font-medium text-primary-foreground hover:opacity-90 disabled:opacity-50"
          >
            {exporting ? <Loader2 className="h-4 w-4 animate-spin" /> : <FileDown className="h-4 w-4" />}
            导出 PDF
          </button>
        </div>
      </div>

      <div className="flex flex-1 overflow-hidden">
        {/* thumbnails */}
        <div className="flex w-52 shrink-0 flex-col border-r border-border">
          <div className="flex-1 overflow-y-auto p-2">
            {deck.slides.map((s, i) => (
              <div
                key={i}
                onClick={() => setSelected(i)}
                className={cn(
                  "group mb-1.5 cursor-pointer rounded-lg border p-2.5 text-xs transition",
                  i === selected
                    ? "border-primary bg-primary/10"
                    : "border-border bg-card/30 hover:border-primary/40",
                )}
              >
                <div className="flex items-center justify-between">
                  <span className="font-mono text-[10px] text-muted-foreground">{i + 1}</span>
                  <span className="rounded bg-muted/60 px-1 py-0.5 text-[10px] text-muted-foreground">
                    {LAYOUTS.find((l) => l.id === s.layout)?.label ?? s.layout}
                  </span>
                </div>
                <div className="mt-1 truncate font-medium">{slideSummary(s)}</div>
                <div className="mt-1 hidden items-center gap-1 group-hover:flex">
                  <button
                    title="上移"
                    disabled={i === 0}
                    onClick={(e) => {
                      e.stopPropagation();
                      mutateDeck((d) => {
                        [d.slides[i - 1], d.slides[i]] = [d.slides[i], d.slides[i - 1]];
                        return d;
                      });
                      setSelected(i - 1);
                    }}
                    className="rounded p-0.5 text-muted-foreground hover:text-foreground disabled:opacity-30"
                  >
                    <ArrowUp className="h-3 w-3" />
                  </button>
                  <button
                    title="下移"
                    disabled={i === deck.slides.length - 1}
                    onClick={(e) => {
                      e.stopPropagation();
                      mutateDeck((d) => {
                        [d.slides[i], d.slides[i + 1]] = [d.slides[i + 1], d.slides[i]];
                        return d;
                      });
                      setSelected(i + 1);
                    }}
                    className="rounded p-0.5 text-muted-foreground hover:text-foreground disabled:opacity-30"
                  >
                    <ArrowDown className="h-3 w-3" />
                  </button>
                  <button
                    title="删除本页"
                    disabled={deck.slides.length <= 1}
                    onClick={(e) => {
                      e.stopPropagation();
                      mutateDeck((d) => {
                        d.slides.splice(i, 1);
                        return d;
                      });
                      setSelected((cur) => Math.max(0, Math.min(cur, deck.slides.length - 2)));
                    }}
                    className="ml-auto rounded p-0.5 text-muted-foreground hover:text-destructive disabled:opacity-30"
                  >
                    <Trash2 className="h-3 w-3" />
                  </button>
                </div>
              </div>
            ))}
          </div>
          <button
            onClick={() => {
              mutateDeck((d) => {
                d.slides.splice(selected + 1, 0, { layout: "bullets", title: "新页面", bullets: ["要点…"] });
                return d;
              });
              setSelected(selected + 1);
            }}
            className="m-2 inline-flex h-8 items-center justify-center gap-1 rounded-lg border border-dashed border-border text-xs text-muted-foreground hover:border-primary hover:text-foreground"
          >
            <Plus className="h-3.5 w-3.5" /> 加一页
          </button>
        </div>

        {/* preview */}
        <div ref={previewBoxRef} className="relative flex flex-1 items-center justify-center overflow-hidden bg-muted/20 p-2">
          {previewHtml ? (
            <div
              style={{ width: 1328 * scale, height: 792 * scale }}
              className="overflow-hidden rounded-xl"
            >
              <iframe
                title="slide-preview"
                sandbox=""
                srcDoc={previewHtml}
                style={{
                  width: 1328,
                  height: 792,
                  transform: `scale(${scale})`,
                  transformOrigin: "top left",
                  border: "none",
                  pointerEvents: "none",
                }}
              />
            </div>
          ) : (
            <div className="text-sm text-muted-foreground">渲染中…</div>
          )}
        </div>

        {/* field editor */}
        {slide && (
          <div className="flex w-80 shrink-0 flex-col gap-3 overflow-y-auto border-l border-border p-4">
            <label className="text-xs font-medium text-muted-foreground">
              版式
              <select
                value={slide.layout}
                onChange={(e) => mutateSlide((s) => { s.layout = e.target.value; })}
                className="mt-1 h-9 w-full rounded-lg border border-border bg-background px-2 text-sm text-foreground"
              >
                {LAYOUTS.map((l) => (
                  <option key={l.id} value={l.id}>{l.label}</option>
                ))}
              </select>
            </label>
            <label className="text-xs font-medium text-muted-foreground">
              标题
              <input
                value={slide.title ?? ""}
                onChange={(e) => mutateSlide((s) => { s.title = e.target.value; })}
                className="mt-1 h-9 w-full rounded-lg border border-border bg-background px-2 text-sm text-foreground"
              />
            </label>
            <label className="text-xs font-medium text-muted-foreground">
              副标题{slide.layout === "quote" ? "（引用出处）" : ""}
              <input
                value={slide.subtitle ?? ""}
                onChange={(e) => mutateSlide((s) => { s.subtitle = e.target.value; })}
                className="mt-1 h-9 w-full rounded-lg border border-border bg-background px-2 text-sm text-foreground"
              />
            </label>
            {slide.layout !== "quote" && (
              <label className="text-xs font-medium text-muted-foreground">
                要点（每行一条，**词** 可加粗）
                <textarea
                  value={(slide.bullets ?? []).join("\n")}
                  onChange={(e) =>
                    mutateSlide((s) => {
                      s.bullets = e.target.value.split("\n").filter((l) => l.trim().length > 0);
                    })
                  }
                  rows={5}
                  className="mt-1 w-full resize-none rounded-lg border border-border bg-background px-2 py-1.5 text-sm text-foreground"
                />
              </label>
            )}
            <label className="text-xs font-medium text-muted-foreground">
              正文{slide.layout === "quote" ? "（引文）" : ""}
              <textarea
                value={slide.body ?? ""}
                onChange={(e) => mutateSlide((s) => { s.body = e.target.value; })}
                rows={4}
                className="mt-1 w-full resize-none rounded-lg border border-border bg-background px-2 py-1.5 text-sm text-foreground"
              />
            </label>
            {slide.layout === "two-column" && (
              <div className="flex flex-col gap-2">
                {[0, 1].map((ci) => {
                  const col = slide.columns?.[ci] ?? {};
                  return (
                    <div key={ci} className="rounded-lg border border-border p-2">
                      <input
                        placeholder={`第 ${ci + 1} 栏标题`}
                        value={col.title ?? ""}
                        onChange={(e) =>
                          mutateSlide((s) => {
                            s.columns = s.columns ?? [{}, {}];
                            while (s.columns.length < 2) s.columns.push({});
                            s.columns[ci] = { ...s.columns[ci], title: e.target.value };
                          })
                        }
                        className="h-8 w-full rounded border border-border bg-background px-2 text-sm text-foreground"
                      />
                      <textarea
                        placeholder="要点（每行一条）"
                        value={(col.bullets ?? []).join("\n")}
                        onChange={(e) =>
                          mutateSlide((s) => {
                            s.columns = s.columns ?? [{}, {}];
                            while (s.columns.length < 2) s.columns.push({});
                            s.columns[ci] = {
                              ...s.columns[ci],
                              bullets: e.target.value.split("\n").filter((l) => l.trim().length > 0),
                            };
                          })
                        }
                        rows={3}
                        className="mt-1.5 w-full resize-none rounded border border-border bg-background px-2 py-1 text-sm text-foreground"
                      />
                    </div>
                  );
                })}
              </div>
            )}
            {(slide.layout === "image" || slide.layout === "image-left" || slide.layout === "content") && (
              <label className="text-xs font-medium text-muted-foreground">
                图片（URL 或本地路径）
                <div className="mt-1 flex gap-1">
                  <input
                    value={slide.image ?? ""}
                    onChange={(e) => mutateSlide((s) => { s.image = e.target.value; })}
                    placeholder="https://… 或 D:\\pics\\a.png"
                    className="h-9 min-w-0 flex-1 rounded-lg border border-border bg-background px-2 text-sm text-foreground"
                  />
                  <button
                    onClick={() => void pickLocalImage()}
                    title="选择本地图片"
                    className="inline-flex h-9 shrink-0 items-center rounded-lg border border-border px-2 text-xs hover:bg-muted/40"
                  >
                    <ImageIcon className="h-3.5 w-3.5" />
                  </button>
                </div>
              </label>
            )}
            <label className="text-xs font-medium text-muted-foreground">
              演讲备注（不上幻灯片）
              <textarea
                value={slide.notes ?? ""}
                onChange={(e) => mutateSlide((s) => { s.notes = e.target.value; })}
                rows={3}
                className="mt-1 w-full resize-none rounded-lg border border-border bg-background px-2 py-1.5 text-sm text-foreground"
              />
            </label>
          </div>
        )}
      </div>

      {/* AI instruction bar */}
      <div className="flex items-center gap-2 border-t border-border px-4 py-2.5">
        <Sparkles className="h-4 w-4 shrink-0 text-primary" />
        {/* B: 默认只改这一页——快 5-10 倍且不会误伤别页 */}
        <div className="flex shrink-0 overflow-hidden rounded-lg border border-border">
          {(
            [
              ["slide", `本页 ${selected + 1}`],
              ["deck", "整份"],
            ] as ["slide" | "deck", string][]
          ).map(([id, label]) => (
            <button
              key={id}
              onClick={() => setEditScope(id)}
              className={cn(
                "px-2.5 py-1.5 text-xs",
                editScope === id ? "bg-primary text-primary-foreground" : "hover:bg-muted/40",
              )}
            >
              {label}
            </button>
          ))}
        </div>
        <button
          onClick={() => void openImageDialog()}
          title="为这一页自动配图"
          className="inline-flex h-9 shrink-0 items-center gap-1 rounded-lg border border-border px-2.5 text-xs hover:bg-muted/40"
        >
          <ImageIcon className="h-3.5 w-3.5" /> 配图
        </button>
        <input
          value={instruction}
          onChange={(e) => setInstruction(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && !e.nativeEvent.isComposing) void handleAiEdit();
          }}
          placeholder={
            editScope === "slide"
              ? `只改第 ${selected + 1} 页，例如：要点压缩成 3 条 / 换成双栏对比`
              : "改整份，例如：整体口吻更正式；统一术语"
          }
          className="h-9 flex-1 rounded-lg border border-border bg-background px-3 text-sm outline-none focus:border-primary"
          disabled={aiEditing}
        />
        <select
          value={chatModel}
          onChange={(e) => setChatModel(e.target.value)}
          className="h-9 max-w-44 rounded-lg border border-border bg-background px-2 text-sm"
          title="使用的模型"
        >
          {modelOptions.length === 0 && <option value="">（无可用模型）</option>}
          {modelOptions.map((m) => (
            <option key={m.ref} value={m.ref}>{m.label}</option>
          ))}
        </select>
        <button
          onClick={() => void handleAiEdit()}
          disabled={aiEditing || !instruction.trim()}
          className="inline-flex h-9 items-center gap-1.5 rounded-lg bg-primary px-4 text-sm font-medium text-primary-foreground hover:opacity-90 disabled:opacity-50"
        >
          {aiEditing ? <Loader2 className="h-4 w-4 animate-spin" /> : <Wand2 className="h-4 w-4" />}
          {aiEditing ? "修改中…" : "AI 修改"}
        </button>
      </div>

      {/* 放映：整份 HTML 全屏滚动吸附，方向键翻页 / Esc 退出 */}
      {presenting && (
        <div className="fixed inset-0 z-[60] bg-black">
          <iframe
            title="present"
            sandbox="allow-same-origin"
            srcDoc={presentHtml}
            className="h-full w-full border-0"
            ref={(el) => {
              if (!el) return;
              // 让每页占满一屏并支持滚动吸附（放映体验）
              el.onload = () => {
                const d = el.contentDocument;
                if (!d) return;
                const st = d.createElement("style");
                st.textContent =
                  "body{padding:0;gap:0;scroll-snap-type:y mandatory;height:100vh;overflow-y:auto}" +
                  ".slide{scroll-snap-align:start;border-radius:0;box-shadow:none;flex:0 0 auto;" +
                  "transform-origin:center;zoom:min(calc(100vw/1280),calc(100vh/720));}";
                d.head.appendChild(st);
                const goto = (n: number) => d.querySelectorAll(".slide")[n]?.scrollIntoView();
                goto(selected);
                d.addEventListener("keydown", (e) => {
                  const ev = e as KeyboardEvent;
                  if (ev.key === "Escape") setPresenting(false);
                  if (["ArrowRight", "ArrowDown", " ", "PageDown"].includes(ev.key)) {
                    ev.preventDefault();
                    d.defaultView?.scrollBy({ top: d.defaultView.innerHeight, behavior: "smooth" });
                  }
                  if (["ArrowLeft", "ArrowUp", "PageUp"].includes(ev.key)) {
                    ev.preventDefault();
                    d.defaultView?.scrollBy({ top: -d.defaultView.innerHeight, behavior: "smooth" });
                  }
                });
                d.body.tabIndex = 0;
                d.body.focus();
              };
            }}
          />
          <button
            onClick={() => setPresenting(false)}
            className="absolute right-4 top-4 rounded-lg bg-white/10 px-3 py-1.5 text-xs text-white backdrop-blur hover:bg-white/20"
          >
            退出放映 (Esc)
          </button>
        </div>
      )}

      {/* C: 配图 */}
      {imgOpen && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-8" onClick={() => !imaging && setImgOpen(false)}>
          <div className="flex w-full max-w-xl flex-col gap-3 rounded-xl border border-border bg-background p-4 shadow-2xl" onClick={(e) => e.stopPropagation()}>
            <div className="flex items-center justify-between">
              <span className="text-sm font-semibold">
                <ImageIcon className="mr-1 inline h-4 w-4 text-primary" /> 为第 {selected + 1} 页配图
              </span>
              <button onClick={() => setImgOpen(false)} disabled={imaging}>
                <X className="h-4 w-4 text-muted-foreground hover:text-foreground" />
              </button>
            </div>
            <p className="text-xs text-muted-foreground">提示词已按本页内容自动拟好，可以改。生成后图片存进本地媒体库并插入本页。</p>
            <textarea
              value={imgPrompt}
              onChange={(e) => setImgPrompt(e.target.value)}
              rows={4}
              className="w-full resize-none rounded-lg border border-border bg-background px-3 py-2 text-sm outline-none focus:border-primary"
            />
            <div className="flex flex-wrap items-center gap-2">
              <select
                value={imgPlatform}
                onChange={(e) => setImgPlatform(e.target.value)}
                className="h-9 rounded-lg border border-border bg-background px-2 text-sm"
              >
                {imgPlatforms.length === 0 && <option value="">（无供应商）</option>}
                {imgPlatforms.map((p) => (
                  <option key={p.id} value={p.id}>{p.name}</option>
                ))}
              </select>
              <input
                value={imgModel}
                onChange={(e) => setImgModel(e.target.value)}
                placeholder="生图模型"
                className="h-9 w-44 rounded-lg border border-border bg-background px-2 text-sm outline-none focus:border-primary"
              />
              <button
                onClick={() => void handleGenImage()}
                disabled={imaging || !imgPrompt.trim()}
                className="ml-auto inline-flex h-9 items-center gap-2 rounded-lg bg-primary px-4 text-sm font-medium text-primary-foreground hover:opacity-90 disabled:opacity-50"
              >
                {imaging ? <Loader2 className="h-4 w-4 animate-spin" /> : <Sparkles className="h-4 w-4" />}
                {imaging ? "生成中…" : "生成并插入"}
              </button>
            </div>
          </div>
        </div>
      )}

      {/* D: 母版 */}
      {brandOpen && deck && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-8" onClick={() => setBrandOpen(false)}>
          <div className="flex max-h-full w-full max-w-lg flex-col gap-3 overflow-y-auto rounded-xl border border-border bg-background p-4 shadow-2xl" onClick={(e) => e.stopPropagation()}>
            <div className="flex items-center justify-between">
              <span className="text-sm font-semibold">
                <Palette className="mr-1 inline h-4 w-4 text-primary" /> 母版 / 品牌
              </span>
              <button onClick={() => setBrandOpen(false)}>
                <X className="h-4 w-4 text-muted-foreground hover:text-foreground" />
              </button>
            </div>
            <p className="text-xs text-muted-foreground">在主题之上覆盖品牌样式，留空的项沿用主题默认值。保存后可在其它演示一键复用。</p>

            {brands.length > 0 && (
              <div className="flex flex-wrap items-center gap-1.5">
                <span className="text-xs text-muted-foreground">已存母版：</span>
                {brands.map((b) => (
                  <span key={b.name} className="inline-flex items-center overflow-hidden rounded border border-border">
                    <button onClick={() => applyBrand(b)} className="px-2 py-0.5 text-xs hover:bg-muted/40">{b.name}</button>
                    <button
                      onClick={async () => {
                        await slidesApi.deleteBrand(b.name).catch(() => {});
                        setBrands(await slidesApi.listBrands());
                      }}
                      className="border-l border-border px-1 py-0.5 text-muted-foreground hover:text-destructive"
                    >
                      <X className="h-3 w-3" />
                    </button>
                  </span>
                ))}
              </div>
            )}

            {(
              [
                ["name", "母版名称", "如：公司蓝"],
                ["primary", "主色（标题）", "#2f6fed"],
                ["accent", "强调色（项目符号）", "#4dd0e1"],
                ["background", "背景", "#0b1020 或 linear-gradient(...)"],
                ["text", "正文颜色", "#aab8dd"],
                ["font", "字体", "'Inter','Microsoft YaHei',sans-serif"],
                ["logo", "Logo 图片路径/URL", "D:\\brand\\logo.png"],
                ["footer", "页脚文字", "内部资料 · 2026"],
              ] as [keyof Brand, string, string][]
            ).map(([key, label, ph]) => (
              <label key={key} className="text-xs font-medium text-muted-foreground">
                {label}
                <input
                  value={deck.brand?.[key] ?? ""}
                  onChange={(e) =>
                    mutateDeck((d) => {
                      const base: Brand = d.brand ?? {
                        name: "", primary: "", accent: "", background: "",
                        text: "", font: "", logo: "", footer: "",
                      };
                      d.brand = { ...base, [key]: e.target.value };
                      return d;
                    })
                  }
                  placeholder={ph}
                  className="mt-1 h-8 w-full rounded-md border border-border bg-background px-2 font-mono text-xs text-foreground outline-none focus:border-primary"
                />
              </label>
            ))}

            <div className="flex items-center gap-2">
              <button
                onClick={() => applyBrand(null)}
                className="h-8 rounded-lg border border-border px-3 text-xs hover:bg-muted/40"
              >
                清除母版
              </button>
              <button
                onClick={() => void saveCurrentBrand()}
                className="ml-auto inline-flex h-8 items-center gap-1.5 rounded-lg bg-primary px-4 text-xs font-medium text-primary-foreground hover:opacity-90"
              >
                保存为可复用母版
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

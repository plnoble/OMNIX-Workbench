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
  Loader2,
  Plus,
  Presentation,
  Sparkles,
  Trash2,
  Wand2,
} from "lucide-react";
import { toast } from "sonner";

import { cn } from "@/lib/utils";
import type { PlatformModel } from "@/types";
import {
  DECK_THEMES,
  modelApi,
  slidesApi,
  type Deck,
  type DeckMeta,
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
      const rec = await slidesApi.editAi(deck.id, instruction.trim(), chatModel);
      const next = JSON.parse(rec.model_json) as Deck;
      setDeck(next);
      setSelected((i) => Math.min(i, next.slides.length - 1));
      setSaveState("saved");
      setInstruction("");
      toast.success("AI 修改完成");
      void loadDecks();
    } catch (e) {
      toast.error(`AI 修改失败：${e}`);
    } finally {
      setAiEditing(false);
    }
  };

  const handleExport = async (kind: "html" | "pdf") => {
    if (!deck) return;
    setExporting(true);
    try {
      const path =
        kind === "html"
          ? await slidesApi.exportHtml(JSON.stringify(deck))
          : await slidesApi.exportPdf(JSON.stringify(deck));
      toast.success(`已导出：${path}`);
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
              <button
                onClick={handleGenerate}
                disabled={generating}
                className="ml-auto inline-flex h-9 items-center gap-2 rounded-lg bg-primary px-4 text-sm font-medium text-primary-foreground hover:opacity-90 disabled:opacity-50"
              >
                {generating ? <Loader2 className="h-4 w-4 animate-spin" /> : <Wand2 className="h-4 w-4" />}
                {generating ? "生成中…" : "生成"}
              </button>
            </div>
          </div>

          {/* deck list */}
          <div>
            <div className="mb-3 flex items-center justify-between">
              <h3 className="text-sm font-semibold text-muted-foreground">我的演示</h3>
              <button
                onClick={handleCreate}
                className="inline-flex h-8 items-center gap-1.5 rounded-lg border border-border px-3 text-sm hover:bg-muted/40"
              >
                <Plus className="h-4 w-4" /> 空白新建
              </button>
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
            onClick={() => void handleExport("html")}
            disabled={exporting}
            className="inline-flex h-8 items-center gap-1.5 rounded-lg border border-border px-3 text-sm hover:bg-muted/40 disabled:opacity-50"
          >
            <FileText className="h-4 w-4" /> HTML
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
                图片 URL
                <input
                  value={slide.image ?? ""}
                  onChange={(e) => mutateSlide((s) => { s.image = e.target.value; })}
                  placeholder="https://…"
                  className="mt-1 h-9 w-full rounded-lg border border-border bg-background px-2 text-sm text-foreground"
                />
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
        <input
          value={instruction}
          onChange={(e) => setInstruction(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && !e.nativeEvent.isComposing) void handleAiEdit();
          }}
          placeholder={`用自然语言修改整个演示，例如：第 ${selected + 1} 页要点压缩成 3 条；或：整体口吻更正式，换成商务蓝主题`}
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
    </div>
  );
}

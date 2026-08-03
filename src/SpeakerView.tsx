/**
 * 演讲者视图（P2）——独立窗口，拖到第二块屏。
 *
 * 为什么必须另开窗口：备注只能给讲的人看。挤在同一块屏上的"演讲者视图"等于
 * 把小抄投给全场，那样的功能不如不做。
 *
 * 这个窗口**不持有状态**：页码归放映的主窗所有，这里只显示收到的东西、并把
 * 翻页请求发回去。两边各存一份索引迟早会对不上，而讲到一半版面对不上是没法
 * 现场修的。
 */
import { useCallback, useEffect, useRef, useState } from "react";
import { emit, listen } from "@tauri-apps/api/event";

import { SPEAKER_EVENT, slidesApi, type Deck, type SpeakerState } from "@/lib/tauri-api";

/** 把秒数显示成 mm:ss / h:mm:ss */
function clock(sec: number): string {
  const s = Math.max(0, Math.floor(sec));
  const mm = String(Math.floor(s / 60) % 60).padStart(2, "0");
  const ss = String(s % 60).padStart(2, "0");
  const h = Math.floor(s / 3600);
  return h > 0 ? `${h}:${mm}:${ss}` : `${mm}:${ss}`;
}

/** 一页的等比缩放预览。父容器给多大就铺多大。 */
function SlideFrame({ html, title }: { html: string; title: string }) {
  const boxRef = useRef<HTMLDivElement | null>(null);
  const [scale, setScale] = useState(0.4);

  useEffect(() => {
    const el = boxRef.current;
    if (!el) return;
    const update = () => {
      const { width, height } = el.getBoundingClientRect();
      // 1328×792 = 幻灯画布 1280×720 加上渲染器的外边距
      setScale(Math.max(0.05, Math.min(width / 1328, height / 792)));
    };
    update();
    const ro = new ResizeObserver(update);
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  return (
    <div ref={boxRef} className="flex h-full w-full items-center justify-center overflow-hidden">
      {html ? (
        <div style={{ width: 1328 * scale, height: 792 * scale }} className="overflow-hidden rounded-lg">
          <iframe
            title={title}
            sandbox=""
            srcDoc={html}
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
        <span className="text-sm text-white/40">—</span>
      )}
    </div>
  );
}

export default function SpeakerView() {
  const [deck, setDeck] = useState<Deck | null>(null);
  const [index, setIndex] = useState(0);
  const [presenting, setPresenting] = useState(true);
  const [cur, setCur] = useState("");
  const [next, setNext] = useState("");
  const [started, setStarted] = useState(() => Date.now());
  const [now, setNow] = useState(() => Date.now());
  const [paused, setPaused] = useState(false);
  const pausedAt = useRef<number | null>(null);

  // ── 接主窗的状态 ──
  useEffect(() => {
    const un = listen<SpeakerState>(SPEAKER_EVENT.state, (e) => {
      setDeck(JSON.parse(e.payload.deckJson) as Deck);
      setIndex(e.payload.index);
      setPresenting(e.payload.presenting);
    });
    // 开窗时主窗可能已经在放映了，讨一次当前状态
    void emit(SPEAKER_EVENT.hello);
    return () => {
      void un.then((f) => f());
    };
  }, []);

  // ── 渲染当前页与下一页 ──
  useEffect(() => {
    if (!deck || deck.slides.length === 0) return;
    let cancelled = false;
    const json = JSON.stringify(deck);
    void slidesApi.render(json, index, false).then((h) => !cancelled && setCur(h)).catch(() => {});
    const n = index + 1;
    if (n < deck.slides.length) {
      void slidesApi.render(json, n, false).then((h) => !cancelled && setNext(h)).catch(() => {});
    } else {
      setNext("");
    }
    return () => {
      cancelled = true;
    };
  }, [deck, index]);

  // ── 计时 ──
  useEffect(() => {
    if (paused) return;
    const t = setInterval(() => setNow(Date.now()), 500);
    return () => clearInterval(t);
  }, [paused]);

  const go = useCallback((delta: number) => {
    void emit(SPEAKER_EVENT.nav, { delta });
  }, []);

  const toggleTimer = () => {
    setPaused((p) => {
      if (p) {
        // 续上：把暂停期间的时间从起点里补回去，读数不跳
        if (pausedAt.current !== null) setStarted((s) => s + (Date.now() - pausedAt.current!));
        pausedAt.current = null;
      } else {
        pausedAt.current = Date.now();
        setNow(Date.now());
      }
      return !p;
    });
  };

  // ── 键盘：这个窗口也能翻页，讲的人不用去点另一块屏 ──
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (["ArrowRight", "ArrowDown", " ", "PageDown"].includes(e.key)) {
        e.preventDefault();
        go(1);
      } else if (["ArrowLeft", "ArrowUp", "PageUp"].includes(e.key)) {
        e.preventDefault();
        go(-1);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [go]);

  const slide = deck?.slides[index];
  const total = deck?.slides.length ?? 0;
  const elapsed = ((paused ? (pausedAt.current ?? now) : now) - started) / 1000;

  return (
    <div className="flex h-screen w-screen flex-col bg-[#0a0b10] text-white">
      {/* 顶栏：页码 + 计时 */}
      <div className="flex shrink-0 items-center justify-between border-b border-white/10 px-5 py-2.5">
        <span className="text-sm text-white/60">
          第 <span className="text-lg font-semibold text-white">{total ? index + 1 : 0}</span> / {total} 页
          {!presenting && <span className="ml-3 text-amber-400">主窗已退出放映</span>}
        </span>
        <div className="flex items-center gap-2">
          <span className="font-mono text-2xl tabular-nums">{clock(elapsed)}</span>
          <button
            onClick={toggleTimer}
            className="rounded border border-white/15 px-2 py-1 text-xs hover:bg-white/10"
          >
            {paused ? "继续" : "暂停"}
          </button>
          <button
            onClick={() => {
              setStarted(Date.now());
              setNow(Date.now());
              pausedAt.current = paused ? Date.now() : null;
            }}
            className="rounded border border-white/15 px-2 py-1 text-xs hover:bg-white/10"
          >
            归零
          </button>
        </div>
      </div>

      <div className="flex min-h-0 flex-1">
        {/* 当前页 */}
        <div className="flex min-w-0 flex-[3] flex-col gap-1.5 p-4">
          <span className="text-xs uppercase tracking-wide text-white/40">当前</span>
          <div className="min-h-0 flex-1">
            <SlideFrame html={cur} title="speaker-current" />
          </div>
        </div>

        {/* 右栏：下一页 + 备注 */}
        <div className="flex min-w-0 flex-[2] flex-col gap-3 border-l border-white/10 p-4">
          <div className="flex min-h-0 flex-[2] flex-col gap-1.5">
            <span className="text-xs uppercase tracking-wide text-white/40">
              {index + 1 < total ? "下一页" : "已是最后一页"}
            </span>
            <div className="min-h-0 flex-1">
              <SlideFrame html={next} title="speaker-next" />
            </div>
          </div>
          <div className="flex min-h-0 flex-[3] flex-col gap-1.5">
            <span className="text-xs uppercase tracking-wide text-white/40">备注</span>
            <div className="min-h-0 flex-1 overflow-y-auto whitespace-pre-wrap rounded-lg bg-white/5 p-3 text-[15px] leading-relaxed">
              {slide?.notes?.trim() ? (
                slide.notes
              ) : (
                <span className="text-white/35">这一页没有备注</span>
              )}
            </div>
          </div>
        </div>
      </div>

      {/* 底部翻页 */}
      <div className="flex shrink-0 items-center justify-center gap-3 border-t border-white/10 px-5 py-2.5">
        <button
          onClick={() => go(-1)}
          disabled={index <= 0}
          className="rounded-lg border border-white/15 px-5 py-1.5 text-sm hover:bg-white/10 disabled:opacity-35"
        >
          上一页
        </button>
        <button
          onClick={() => go(1)}
          disabled={index >= total - 1}
          className="rounded-lg bg-white/90 px-5 py-1.5 text-sm font-medium text-black hover:bg-white disabled:opacity-35"
        >
          下一页
        </button>
        <span className="ml-3 text-xs text-white/35">方向键也能翻</span>
      </div>
    </div>
  );
}

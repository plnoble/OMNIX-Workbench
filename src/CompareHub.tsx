/**
 * CompareHub — AI 专家比对与最佳结论熔炼炉
 *
 * 一次提问并排发给多个**网关模型**，每个模型各自保留上下文可持续追问，最后把
 * 各家的最新回答熔炼成一份结论。
 *
 * 这里曾经还有两条路，都已删除：
 *
 * - **Web 网页原生比对**：往 DeepSeek / ChatGPT / 豆包 / Gemini / 元宝 的网页里
 *   嵌 webview 并注入硬编码 CSS 选择器自动填词、点发送。对方改一次 UI 就断，
 *   而且必须先在各家登录（区域墙一挡就全空），并排的 webview 又被压成一条窄带，
 *   什么都看不清。维护成本远大于价值。
 * - **按账号比对**：拿 `agent_accounts` 当比对源。那张表是「外部 CLI 挂到 OMNIX
 *   网关时用哪个上游」的覆盖配置，不是模型清单；一个账号还只对应一个
 *   `target_model`。要比的本来就是模型，按模型选就是对的轴。
 */
import React, { useState, useEffect, useRef } from "react";
import { cn } from "@/lib/utils";
import { toast } from "@/components/ui/sonner";
import { DEFAULT_PROXY_PORT } from "@/lib/constants";
import { modelApi } from "@/lib/tauri-api";
import type { PlatformModel } from "@/types";

/** One message in a per-model conversation thread (多模型同对话). */
interface TurnMsg {
  role: "user" | "assistant";
  content: string;
  loading?: boolean;
  error?: string;
  startTime?: number;
  latencyMs?: number;
  tokenCount?: number;
}

const GATEWAY_URL = `http://localhost:${DEFAULT_PROXY_PORT}/v1/chat/completions`;

/** 读一条 OpenAI 兼容的 SSE 流，每收到一段增量就回调一次。 */
async function streamChat(
  model: string,
  body: unknown,
  signal: AbortSignal,
  onDelta: (accumulated: string) => void,
): Promise<string> {
  const response = await fetch(GATEWAY_URL, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      Authorization: "Bearer local-proxy",
      // 这条路由不看 body 里的 `model`（它给外部 CLI 用，要按用户配置改写）。
      // 不带这个头，每一列都会打到同一个全局 target_model 上——「按模型比对」
      // 就成了同一个模型自己和自己比。
      "x-omnix-model": model,
    },
    body: JSON.stringify(body),
    signal,
  });
  if (!response.ok) {
    // 网关现在回 OpenAI 形状的错误信封，把 message 拎出来；拎不到再退回原文。
    const raw = await response.text();
    let detail = raw;
    try {
      const parsed = JSON.parse(raw);
      if (typeof parsed?.error?.message === "string") detail = parsed.error.message;
    } catch { /* 非 JSON，原样显示 */ }
    throw new Error(`HTTP ${response.status}: ${detail}`);
  }
  const reader = response.body?.getReader();
  if (!reader) throw new Error("上游没有返回响应体");

  const decoder = new TextDecoder();
  let accumulated = "";
  let done = false;
  while (!done) {
    const { value, done: doneReading } = await reader.read();
    done = doneReading;
    if (!value) continue;
    for (const line of decoder.decode(value, { stream: true }).split("\n")) {
      if (!line.trim()) continue;
      if (line.startsWith("data: [DONE]")) { done = true; break; }
      if (!line.startsWith("data: ")) continue;
      try {
        const delta = JSON.parse(line.slice(6))?.choices?.[0]?.delta?.content || "";
        accumulated += delta;
        onDelta(accumulated);
      } catch {
        // 流式切分会把一行劈成两半，半行解析失败是正常的，等下一块拼上。
      }
    }
  }
  return accumulated;
}

export const CompareHub: React.FC = () => {
  const [prompt, setPrompt] = useState("");
  const [models, setModels] = useState<PlatformModel[]>([]);
  const [selectedModelRefs, setSelectedModelRefs] = useState<string[]>([]);
  const [threads, setThreads] = useState<{ [modelRef: string]: TurnMsg[] }>({});

  const [fusionContent, setFusionContent] = useState("");
  const [fusionLoading, setFusionLoading] = useState(false);

  // 出结果之后把「选模型 + 提问框 + 模板」整块收起来。这三块常驻要占掉三百多
  // 像素，而看比对结果时它们一个都用不上——纵向立刻多出接近一屏。
  const [composerOpen, setComposerOpen] = useState(true);
  // 聚焦某一个模型：它撑满整宽，其余收成上方的一排标签。三列并排时每列只有
  // 四百来像素，中文一行二十几个字，读长回答基本靠猜。
  const [focusedModel, setFocusedModel] = useState<string | null>(null);
  // 熔炼用哪个模型。空 = 跟随 Auto 路由。
  //
  // 熔炼是「交叉评审 + 取长补短」，一次只跑一趟，值得选准；而 Auto 是关键词
  // 启发式，平局时还由平台优先级决定——不该让它替你决定谁来当评审。
  const [fusionModel, setFusionModel] = useState("");

  const abortControllersRef = useRef<AbortController[]>([]);
  const fusionAbortControllerRef = useRef<AbortController | null>(null);

  const modelInfo = (ref: string): { name: string; platform: string } => {
    const [platform, ...rest] = ref.split(":");
    return { name: rest.join(":") || ref, platform };
  };
  const anyStreaming = Object.values(threads).some(
    (msgs) => msgs.length > 0 && msgs[msgs.length - 1].loading,
  );

  useEffect(() => {
    modelApi
      .getActive()
      .then((active) => {
        // 嵌入 / 重排模型不会聊天，列出来只会让人选错。
        const chatModels = active.filter(
          (m) =>
            !m.model_name.toLowerCase().includes("embedding") &&
            !m.model_name.toLowerCase().includes("rerank"),
        );
        setModels(chatModels);
        setSelectedModelRefs(chatModels.slice(0, 3).map((m) => `${m.platform_id}:${m.model_name}`));
      })
      .catch((e) => toast.error("读取已启用模型失败", { description: String(e) }));

    return () => {
      abortControllersRef.current.forEach((controller) => controller.abort());
      fusionAbortControllerRef.current?.abort();
    };
  }, []);

  /** Abort all in-flight comparison streams. */
  const stopAll = () => {
    abortControllersRef.current.forEach((c) => c.abort());
    abortControllersRef.current = [];
    setThreads((prev) => {
      const next: typeof prev = {};
      for (const [id, msgs] of Object.entries(prev)) {
        const arr = [...msgs];
        const last = arr[arr.length - 1];
        if (last && last.role === "assistant" && last.loading) {
          arr[arr.length - 1] = { ...last, loading: false, error: last.content ? undefined : "已停止" };
        }
        next[id] = arr;
      }
      return next;
    });
  };

  /** Update the trailing (in-flight) assistant message of one model's thread. */
  const patchLastAssistant = (modelRef: string, patch: Partial<TurnMsg>) => {
    setThreads((prev) => {
      const arr = [...(prev[modelRef] || [])];
      const idx = arr.length - 1;
      if (idx >= 0 && arr[idx].role === "assistant") arr[idx] = { ...arr[idx], ...patch };
      return { ...prev, [modelRef]: arr };
    });
  };

  // 多轮：每次提交都把**该模型自己的完整历史**再发一遍，所以追问对每个模型
  // 都是接着上一轮说的，而不是各自独立的一次性提问。
  const handleCompareSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    const targets = selectedModelRefs;
    if (!prompt.trim() || targets.length === 0) return;

    abortControllersRef.current.forEach((controller) => controller.abort());
    abortControllersRef.current = [];

    const submitted = prompt.trim();
    const historyByModel: Record<string, { role: string; content: string }[]> = {};
    targets.forEach((ref) => {
      const prior = (threads[ref] || [])
        .filter((m) => !m.loading && !m.error)
        .map((m) => ({ role: m.role, content: m.content }));
      historyByModel[ref] = [...prior, { role: "user", content: submitted }];
    });

    setThreads((prev) => {
      const next = { ...prev };
      targets.forEach((ref) => {
        const arr = next[ref] ? [...next[ref]] : [];
        arr.push({ role: "user", content: submitted });
        arr.push({ role: "assistant", content: "", loading: true, startTime: Date.now() });
        next[ref] = arr;
      });
      return next;
    });
    setPrompt("");
    setFusionContent("");
    setComposerOpen(false);

    await Promise.allSettled(
      targets.map(async (ref) => {
        const controller = new AbortController();
        abortControllersRef.current.push(controller);
        const startedAt = Date.now();
        try {
          const text = await streamChat(
            ref,
            { model: ref, messages: historyByModel[ref], stream: true },
            controller.signal,
            (accumulated) => patchLastAssistant(ref, { content: accumulated, loading: true }),
          );
          patchLastAssistant(ref, {
            content: text,
            loading: false,
            latencyMs: Date.now() - startedAt,
            tokenCount: text.length,
          });
        } catch (err: unknown) {
          const error = err instanceof Error ? err : new Error(String(err));
          if (error.name === "AbortError") return;
          patchLastAssistant(ref, { loading: false, error: error.message || "请求失败" });
        } finally {
          abortControllersRef.current = abortControllersRef.current.filter((c) => c !== controller);
        }
      }),
    );
  };

  const handleNewConversation = () => {
    abortControllersRef.current.forEach((controller) => controller.abort());
    abortControllersRef.current = [];
    setThreads({});
    setFusionContent("");
    setComposerOpen(true);
    setFocusedModel(null);
  };

  /** Latest assistant answer per model (for the fusion furnace). */
  const latestAnswers = (): { [name: string]: string } => {
    const out: { [name: string]: string } = {};
    Object.entries(threads).forEach(([ref, msgs]) => {
      const last = [...msgs].reverse().find((m) => m.role === "assistant" && m.content.trim());
      if (last) out[modelInfo(ref).name] = last.content;
    });
    return out;
  };

  const handleFusionSummary = async () => {
    const answers = latestAnswers();
    if (Object.keys(answers).length === 0) {
      toast.warning("还没有可熔炼的回答", { description: "等各模型回答生成完再点。" });
      return;
    }

    // 熔炼用的问题是最后一轮问的那个，不是输入框里的（那已经清空了）。
    const lastQuestion =
      Object.values(threads)
        .map((msgs) => [...msgs].reverse().find((m) => m.role === "user")?.content)
        .find(Boolean) || "";

    const sources = Object.entries(answers)
      .map(([name, text]) => `【${name} 的回答】：\n${text.slice(0, 3500)}`)
      .join("\n\n");
    // ⚠️ 这段提示词的措辞会影响**路由**：Auto 靠关键词判断请求需要什么能力，
    // 原文里的「代码」一词会让每一次熔炼都命中 need_coding，把评审工作固定交给
    // 代码专用模型——哪怕你比对的是散文或产品问题。改动这里时别把
    // 「代码 / algorithm / 算法 / 死锁 / 图片」这类词写回来。
    const fusionPrompt = `以下是同一个问题、多个 AI 分别给出的回答。请你作为中立的评审，综合比较它们：核对事实、指出分歧与各自的强弱，去重去错，取长补短，融合出一份最全面、准确、可信的答案。若涉及工程实现，一并留意常见反模式与安全、并发方面的隐患。用与问题相同的语言作答。

【问题】：${lastQuestion}

${sources}

【融合后的最佳答案】：`;

    fusionAbortControllerRef.current?.abort();
    const controller = new AbortController();
    fusionAbortControllerRef.current = controller;
    setFusionLoading(true);
    setFusionContent("");

    try {
      const judge = fusionModel || "Auto";
      await streamChat(
        judge,
        { model: judge, messages: [{ role: "user", content: fusionPrompt }], stream: true },
        controller.signal,
        setFusionContent,
      );
    } catch (e: unknown) {
      const error = e instanceof Error ? e : new Error(String(e));
      if (error.name === "AbortError") return;
      setFusionContent(`熔炼失败：${error.message}`);
    } finally {
      if (fusionAbortControllerRef.current === controller) fusionAbortControllerRef.current = null;
      setFusionLoading(false);
    }
  };

  const handleCopyText = (text: string) => {
    navigator.clipboard.writeText(text).then(
      () => toast.success("已复制到剪贴板"),
      () => toast.error("复制失败，请手动复制"),
    );
  };

  const hasThreads = Object.keys(threads).length > 0;
  // 聚焦的模型被取消勾选后要自动解除，否则结果区会整片空掉。
  const activeFocus = focusedModel && selectedModelRefs.includes(focusedModel) ? focusedModel : null;
  const visibleModels = activeFocus ? [activeFocus] : selectedModelRefs;

  return (
    // `min-h-0` 是关键：没有它，flex 子项的 `flex-1` 撑不开，结果区仍会被内容
    // 高度决定，输入区收起来腾出的空间就白腾了。
    <div className="flex h-full min-h-0 flex-col gap-5 overflow-y-auto p-5">
      <div>
        <h2 className="m-0 flex items-center gap-2 text-lg">⚖️ AI 专家比对与最佳结论熔炼炉</h2>
        <span className="text-xs text-muted-foreground">
          一次提问并排发给多个模型，每个模型各自保留上下文可持续追问，再熔炼出最佳结论。全部走 OMNIX 网关。
        </span>
      </div>

      {/* 收起态：一行搞定「继续追问」，不必展开整块。 */}
      {!composerOpen && (
        <form onSubmit={handleCompareSubmit} className="card flex items-center gap-2 p-2.5">
          <span className="shrink-0 text-xs text-muted-foreground">
            {selectedModelRefs.length} 个模型
          </span>
          <input
            className="min-w-0 flex-1 rounded-md border border-border bg-muted/10 px-3 py-2 text-sm outline-none focus:border-accent"
            placeholder="继续追问…（Enter 发送）"
            value={prompt}
            onChange={(e) => setPrompt(e.target.value)}
          />
          {anyStreaming ? (
            <button type="button" className="btn btn-secondary shrink-0 px-3 py-2 text-sm" onClick={stopAll}>
              ⏹ 停止
            </button>
          ) : (
            <button type="submit" className="btn btn-primary shrink-0 px-3 py-2 text-sm" disabled={!prompt.trim()}>
              发送
            </button>
          )}
          <button
            type="button"
            className="shrink-0 rounded-md border border-border px-2.5 py-2 text-xs text-muted-foreground hover:text-foreground"
            onClick={() => setComposerOpen(true)}
            title="展开：换模型、用模板、写长提示词"
          >
            ⇕ 展开
          </button>
          <button
            type="button"
            className="shrink-0 rounded-md border border-border px-2.5 py-2 text-xs text-muted-foreground hover:text-foreground"
            onClick={handleNewConversation}
            title="清空所有模型的对话，开始新一轮"
          >
            🆕
          </button>
        </form>
      )}

      {/* 比对目标：已启用的网关模型 */}
      {composerOpen && (
      <div className="card p-4">
        <div className="mb-2 flex items-center justify-between gap-2">
          <span className="text-sm font-semibold text-secondary-foreground">选择要比对的模型</span>
          <span className="text-xs text-muted-foreground">已选 {selectedModelRefs.length} 个</span>
        </div>
        <div className="flex flex-wrap gap-2.5">
          {models.length === 0 && (
            <span className="text-xs text-muted-foreground">
              没有已启用的模型 —— 先到「模型」页启用几个带 API Key 的供应商。
            </span>
          )}
          {models.map((m) => {
            const ref = `${m.platform_id}:${m.model_name}`;
            const on = selectedModelRefs.includes(ref);
            return (
              <label
                key={m.id}
                className={cn(
                  "checkbox-label flex cursor-pointer items-center gap-2 rounded-lg px-3 py-2",
                  on ? "checked border border-purple-500 bg-purple-500/12" : "border border-border bg-muted/5",
                )}
                title={m.platform_id}
              >
                <input
                  type="checkbox"
                  checked={on}
                  onChange={(e) =>
                    setSelectedModelRefs((prev) =>
                      e.target.checked ? [...prev, ref] : prev.filter((r) => r !== ref),
                    )
                  }
                  className="cursor-pointer"
                />
                <div>
                  <span className="block text-sm font-medium">{m.model_name}</span>
                  <span className="text-xs text-muted-foreground">{m.platform_id}</span>
                </div>
              </label>
            );
          })}
        </div>
      </div>
      )}

      {/* 提问 */}
      {composerOpen && (
      <form onSubmit={handleCompareSubmit} className="card flex flex-col gap-4 p-5">
        <div className="form-group">
          <label className="mb-2 flex items-center justify-between">
            <span className="text-sm font-semibold text-foreground">📝 输入提问 / System Prompt</span>
            <span className="flex items-center gap-3 text-xs text-muted-foreground">
              <span>⚡ Ctrl/⌘+Enter 发送 · 模板见下方</span>
              {hasThreads && (
                <button
                  type="button"
                  className="rounded border border-border px-2 py-0.5 hover:text-foreground"
                  onClick={() => setComposerOpen(false)}
                  title="收起，把版面让给回答"
                >
                  ⇕ 收起
                </button>
              )}
            </span>
          </label>
          <textarea
            className="w-full resize-y rounded-lg border border-border bg-muted/10 px-3.5 py-2.5 font-mono text-sm leading-relaxed text-foreground placeholder:text-muted-foreground focus:border-accent focus:outline-none focus:ring-1 focus:ring-accent/30"
            rows={6}
            style={{ minHeight: "140px", maxHeight: "400px" }}
            placeholder="输入要同时问多个模型的问题…（Ctrl/⌘+Enter 发送）"
            value={prompt}
            onChange={(e) => setPrompt(e.target.value)}
            onKeyDown={(e) => {
              if ((e.metaKey || e.ctrlKey) && e.key === "Enter") {
                e.preventDefault();
                e.currentTarget.form?.requestSubmit();
              }
            }}
            required
          />
        </div>

        <div className="flex gap-2.5">
          {anyStreaming ? (
            <button
              type="button"
              className="btn btn-secondary flex flex-1 items-center justify-center gap-1.5 py-2.5"
              onClick={stopAll}
            >
              ⏹ 停止生成
            </button>
          ) : (
            <button
              type="submit"
              className="btn btn-primary flex flex-1 items-center justify-center gap-1.5 py-2.5"
              disabled={selectedModelRefs.length === 0}
            >
              {hasThreads ? "💬 继续追问（多模型同对话）" : "🎯 开始并行比对"}
            </button>
          )}
          {hasThreads && (
            <button
              type="button"
              className="btn btn-secondary flex items-center justify-center gap-1.5 px-4 py-2.5"
              onClick={handleNewConversation}
              title="清空所有模型的对话，开始新一轮"
            >
              🆕 新对话
            </button>
          )}
        </div>

        <div className="flex flex-wrap gap-2 border-t border-border pt-1">
          <span className="mr-1 self-center text-xs text-muted-foreground">模板：</span>
          {[
            ["CORS OPTIONS 预检", "如何解决 Node.js 跨域请求（CORS）中首发 OPTIONS 预检请求抛出的 403 跨域失败错误？"],
            ["Tokio 异步死锁", "分析以下 Rust 代码在使用 tokio::sync::Mutex 时为什么在多路 select 中造成死锁，如何用 std 或 ParkingLot 锁修复？"],
            ["高并发线程安全缓存", "编写一个用 Rust 泛型实现的高并发 Thread-Safe LruCache 缓存模块，要求附带生命周期淘汰逻辑与单元测试用例。"],
          ].map(([label, text]) => (
            <button
              key={label}
              type="button"
              className="cursor-pointer rounded-full border border-border bg-muted/10 px-2.5 py-1 text-xs text-foreground transition-colors hover:border-accent/40 hover:bg-accent/10 hover:text-accent"
              onClick={() => setPrompt(text)}
            >
              {label}
            </button>
          ))}
        </div>
      </form>
      )}

      {/* 聚焦切换条：并排读不下去时，点一个模型让它独占整宽。 */}
      {hasThreads && selectedModelRefs.length > 1 && (
        <div className="flex flex-wrap items-center gap-1.5">
          <span className="mr-1 text-xs text-muted-foreground">视图：</span>
          <button
            type="button"
            onClick={() => setFocusedModel(null)}
            className={cn(
              "rounded-full border px-3 py-1 text-xs transition-colors",
              activeFocus === null
                ? "border-purple-500 bg-purple-500/12 text-foreground"
                : "border-border text-muted-foreground hover:text-foreground",
            )}
          >
            并排（{selectedModelRefs.length}）
          </button>
          {selectedModelRefs.map((ref) => (
            <button
              key={ref}
              type="button"
              onClick={() => setFocusedModel(ref)}
              title={ref}
              className={cn(
                "max-w-[14rem] truncate rounded-full border px-3 py-1 text-xs transition-colors",
                activeFocus === ref
                  ? "border-purple-500 bg-purple-500/12 text-foreground"
                  : "border-border text-muted-foreground hover:text-foreground",
              )}
            >
              {modelInfo(ref).name}
            </button>
          ))}
        </div>
      )}

      {/* 结果区。并排时少于 4 个铺满、多了横向滚动；聚焦时单列独占整宽。
          高度吃满剩余空间——上面的输入区收起来腾出的地方，要真的用上。 */}
      {hasThreads && (
        <div className="flex min-h-0 flex-1 gap-[15px] overflow-x-auto pb-1">
          {visibleModels.map((ref) => {
            const info = modelInfo(ref);
            const msgs = threads[ref] || [];
            const lastAssistant = [...msgs].reverse().find((m) => m.role === "assistant");
            return (
              <div
                key={ref}
                className={cn(
                  "card glass-card flex min-h-0 flex-col p-4",
                  activeFocus
                    ? "w-full"
                    : visibleModels.length <= 3
                      ? "min-w-[320px] flex-1"
                      : "w-[340px] shrink-0",
                )}
              >
                <div className="mb-3 flex items-center justify-between border-b border-border pb-2.5">
                  <button
                    type="button"
                    className="min-w-0 cursor-pointer border-none bg-transparent p-0 text-left"
                    onClick={() => setFocusedModel(activeFocus === ref ? null : ref)}
                    title={activeFocus === ref ? "点击回到并排" : "点击让这个模型独占整宽"}
                  >
                    <strong className="block truncate text-sm">{info.name}</strong>
                    <span className="text-xs text-muted-foreground">{info.platform}</span>
                    {lastAssistant?.latencyMs && !lastAssistant.loading && (
                      <span className="ml-2 text-xs text-cyan-400">
                        ⏱ {(lastAssistant.latencyMs / 1000).toFixed(1)}s · ~{Math.ceil((lastAssistant.tokenCount || 0) / 4)} tokens
                      </span>
                    )}
                  </button>
                  {lastAssistant?.loading ? (
                    <span className="pulse-dot active" title="正在生成实时流..." />
                  ) : lastAssistant?.content ? (
                    <button
                      className="btn-icon cursor-pointer border-none bg-transparent text-sm"
                      onClick={() => handleCopyText(lastAssistant.content)}
                      title="复制最新回答"
                      aria-label="复制最新回答"
                    >
                      📋
                    </button>
                  ) : null}
                </div>

                <div className="flex min-h-[220px] flex-1 flex-col gap-2.5 overflow-y-auto text-sm leading-relaxed">
                  {msgs.map((m, i) =>
                    m.role === "user" ? (
                      <div key={i} className="max-w-[90%] self-end whitespace-pre-wrap rounded-lg bg-accent/15 px-2.5 py-1.5 text-xs text-foreground">
                        {m.content}
                      </div>
                    ) : (
                      <div key={i} className="whitespace-pre-wrap text-foreground">
                        {m.error ? (
                          <span className="text-red-500">🚫 错误: {m.error}</span>
                        ) : (
                          m.content || <span className="text-muted-foreground">等待回答流生成中...</span>
                        )}
                      </div>
                    ),
                  )}
                </div>
              </div>
            );
          })}
        </div>
      )}

      {/* 熔炼炉 */}
      {hasThreads && (
        <div
          className="card flex flex-col gap-4 p-5"
          style={{
            background: "linear-gradient(135deg, rgba(168, 85, 247, 0.08) 0%, rgba(236, 72, 153, 0.08) 100%)",
            border: "1px solid rgba(168, 85, 247, 0.25)",
            boxShadow: "0 4px 20px rgba(168,85,247,0.15)",
          }}
        >
          <div className="flex flex-wrap items-center justify-between gap-3">
            <div className="min-w-0">
              <strong className="block text-base text-foreground">🔮 最佳结论熔炼炉</strong>
              <span className="text-xs text-muted-foreground">
                取各模型的最新一条回答，交叉评审后融合出一份结论。
              </span>
            </div>
            <div className="flex flex-wrap items-center gap-2">
              <label className="flex items-center gap-1.5 text-xs text-muted-foreground">
                评审模型
                <select
                  value={fusionModel}
                  onChange={(e) => setFusionModel(e.target.value)}
                  className="h-8 max-w-[16rem] rounded-md border border-border bg-background px-2 text-xs text-foreground"
                  title="熔炼只跑一趟，值得选准。留空则交给 Auto 路由按关键词猜。"
                >
                  <option value="">Auto（自动挑选）</option>
                  {models.map((m) => {
                    const ref = `${m.platform_id}:${m.model_name}`;
                    return (
                      <option key={m.id} value={ref}>
                        {m.model_name} · {m.platform_id}
                      </option>
                    );
                  })}
                </select>
              </label>
              <button
                className="btn btn-primary flex items-center gap-1.5 px-5 py-2"
                onClick={handleFusionSummary}
                disabled={fusionLoading || anyStreaming}
                title={anyStreaming ? "等所有模型回答完再熔炼" : undefined}
              >
                {fusionLoading ? "熔炼中..." : "🔥 开始点火熔炼"}
              </button>
            </div>
          </div>

          {(fusionLoading || fusionContent) && (
            <div className="relative min-h-[100px] whitespace-pre-wrap rounded-lg border border-purple-500/15 bg-muted/10 p-4 text-sm leading-relaxed text-foreground">
              {fusionContent ? (
                <>
                  <div className="mb-2 flex justify-end border-b border-border pb-1.5">
                    <button
                      className="btn-icon cursor-pointer border-none bg-transparent text-sm"
                      onClick={() => handleCopyText(fusionContent)}
                      title="复制熔炼方案"
                    >
                      📋 复制方案
                    </button>
                  </div>
                  {fusionContent}
                </>
              ) : (
                <div className="py-5 text-center">
                  <span className="pulse-dot active mr-2.5 inline-block" />
                  <span className="text-muted-foreground">正在交叉评审各模型的回答，熔炼结论中...</span>
                </div>
              )}
            </div>
          )}
        </div>
      )}
    </div>
  );
};

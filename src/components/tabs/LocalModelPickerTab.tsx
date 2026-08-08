/**
 * LocalModelPickerTab — 本地模型选型。
 *
 * 自动读本机 CPU / 内存 / 显卡（nvidia-smi），按显存和内存排出能跑的开源模型，
 * 给出建议量化档，并**直接下载**（`ollama pull`，带进度、可取消、装完标已安装）。
 *
 * 这一页原先用的是 `local_models.rs` 那份薄目录：只有名字/参数量/家族，没有显卡
 * 检测（要用户手输显存），也没有安装命令。而项目里早就有一份更全的
 * `model_knowledge.rs`——显卡数据库、显卡模拟、证据分级、`ollama pull` 命令一应
 * 俱全，命令注册了、前端也绑定了，**却没有任何界面调用**。这里接的就是那一份。
 */
import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { Cpu, Download, HardDrive, MonitorCog, RefreshCw, Sparkles, X } from "lucide-react";

import { cn } from "@/lib/utils";
import { toast } from "@/components/ui/sonner";
import {
  cookbookApi,
  type GpuSpec,
  type HardwareInfo,
  type ModelRecommendation,
} from "@/lib/tauri-api";

const FIT_META: Record<ModelRecommendation["overall_fit"], { label: string; cls: string }> = {
  perfect: { label: "可流畅运行", cls: "text-success border-success/40 bg-success/10" },
  tight: { label: "勉强能跑", cls: "text-warning border-warning/40 bg-warning/10" },
  impossible: { label: "跑不动", cls: "text-muted-foreground border-border bg-muted/10" },
};

interface PullEvent {
  model: string;
  line: string;
  done: boolean;
  ok: boolean;
}

/**
 * 从 `ollama pull` 的进度行里抠出百分比。
 *
 * 它长这样：`pulling 8934d96d3f08: 45% ▕███ ▏ 1.2 GB/2.7 GB`。
 * 抠不出来就返回 null（校验、解包这些阶段本来就没有百分比）。
 */
export function pullPercent(line: string): number | null {
  const match = line.match(/(\d{1,3})\s*%/);
  if (!match) return null;
  const value = Number(match[1]);
  return Number.isFinite(value) ? Math.min(100, Math.max(0, value)) : null;
}

/**
 * 「跑不动 / 勉强能跑」到底卡在哪：显存还是内存。
 *
 * 后端一直分别算了 `fits_vram` 和 `fits_ram`，但这一页只显示合并后的结论，
 * 于是看到「跑不动」也不知道该加内存还是换显卡——而这正是唯一有用的信息。
 */
export function fitReason(rec: ModelRecommendation): string {
  if (rec.overall_fit === "perfect") return "";
  const short: string[] = [];
  if (!rec.fits_vram) short.push("显存不够");
  if (!rec.fits_ram) short.push("内存不够");
  if (short.length === 0) return "余量很小";
  return short.join(" · ");
}

export function LocalModelPickerTab() {
  const [hw, setHw] = useState<HardwareInfo | null>(null);
  const [recs, setRecs] = useState<ModelRecommendation[]>([]);
  const [installed, setInstalled] = useState<Set<string>>(new Set());
  const [loading, setLoading] = useState(false);

  // 假想显卡：想知道「换张 4090 能跑什么」时用，不影响真实检测结果。
  const [gpus, setGpus] = useState<GpuSpec[]>([]);
  const [simulatedGpu, setSimulatedGpu] = useState("");

  // 下载进度：模型名 → 最近一行输出。
  const [pulling, setPulling] = useState<Record<string, string>>({});
  const pullingRef = useRef(pulling);
  pullingRef.current = pulling;

  const [filter, setFilter] = useState<"runnable" | "installed" | "all">("runnable");

  const loadInstalled = useCallback(async () => {
    // 没装 Ollama 是正常情况——推荐照看，只是不能一键下载。
    setInstalled(new Set(await cookbookApi.installedOllamaModels().catch(() => [])));
  }, []);

  const loadReal = useCallback(async () => {
    setLoading(true);
    try {
      const result = await cookbookApi.getRecommendations();
      setHw(result.hardware);
      setRecs(result.recommendations);
      setSimulatedGpu("");
    } catch (error) {
      toast.error("读取硬件推荐失败", { description: String(error) });
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void loadReal();
    void loadInstalled();
    cookbookApi.getGpuDatabase().then(setGpus).catch(() => setGpus([]));
  }, [loadReal, loadInstalled]);

  // 下载进度事件。装完自动刷新「已安装」，这样按钮状态不用手动同步。
  useEffect(() => {
    const unlisten = listen<PullEvent>("local-model-pull", (event) => {
      const { model, line, done, ok } = event.payload;
      setPulling((prev) => {
        if (!done) return { ...prev, [model]: line };
        const next = { ...prev };
        delete next[model];
        return next;
      });
      if (done) {
        if (ok) {
          toast.success(`${model} 下载完成`);
          void loadInstalled();
        } else if (line !== "已取消") {
          toast.error(`${model} 下载失败`, { description: line });
        }
      }
    });
    return () => { void unlisten.then((off) => off()); };
  }, [loadInstalled]);

  const simulate = async (gpuName: string) => {
    setSimulatedGpu(gpuName);
    if (!gpuName) { void loadReal(); return; }
    setLoading(true);
    try {
      const result = await cookbookApi.recommendForGpu(gpuName);
      setRecs(result.recommendations);
    } catch (error) {
      toast.error("模拟失败", { description: String(error) });
    } finally {
      setLoading(false);
    }
  };

  // 排序：能跑的在前，同档按有效质量降序——先看到的就是这台机器上最好的选择。
  // 后端给的是目录顺序，跟这台机器无关。
  const visible = recs
    .filter((rec) => {
      if (filter === "all") return true;
      const tag = ollamaTag(rec);
      if (filter === "installed") return tag ? installed.has(tag) : false;
      return rec.overall_fit !== "impossible";
    })
    .slice()
    .sort((a, b) => {
      const rank = { perfect: 0, tight: 1, impossible: 2 } as const;
      return rank[a.overall_fit] - rank[b.overall_fit] || b.effective_quality - a.effective_quality;
    });

  const startPull = async (rec: ModelRecommendation) => {
    const model = ollamaTag(rec);
    if (!model) {
      toast.error("这个模型没有给出 Ollama 拉取命令");
      return;
    }
    setPulling((prev) => ({ ...prev, [model]: "准备中…" }));
    try {
      await cookbookApi.pull(model);
    } catch (error) {
      // 「已取消」由事件那边处理过了，这里只报真正的启动失败。
      const message = String(error);
      if (!message.includes("已取消")) toast.error("下载失败", { description: message });
      setPulling((prev) => { const next = { ...prev }; delete next[model]; return next; });
    }
  };

  return (
    <div className="flex h-full flex-1 flex-col overflow-hidden bg-background">
      <div className="border-b border-border px-6 py-4">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div className="min-w-0">
            <div className="flex items-center gap-2 text-lg font-semibold">
              <Sparkles className="h-5 w-5 text-primary" /> 本地模型选型
            </div>
            <p className="mt-1 max-w-3xl text-sm leading-6 text-muted-foreground">
              自动识别本机显卡与内存，排出能跑的开源模型和建议量化档，可直接下载。装好的模型会出现在「模型中心」的 Ollama 供应商下。
            </p>
          </div>
          <button
            onClick={() => { void loadReal(); void loadInstalled(); }}
            className="flex items-center gap-1.5 rounded-md border border-border px-3 py-1.5 text-sm text-muted-foreground hover:bg-muted/20 hover:text-foreground"
          >
            <RefreshCw className={cn("h-4 w-4", loading && "animate-spin")} /> 重新检测
          </button>
        </div>
      </div>

      <div className="flex flex-col gap-5 overflow-y-auto p-6">
        {/* 硬件 */}
        <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
          <div className="rounded-lg border border-border glass-surface p-4">
            <div className="flex items-center gap-2 text-xs text-muted-foreground"><Cpu className="h-3.5 w-3.5" /> CPU</div>
            <div className="mt-1 text-sm font-medium">{hw ? `${hw.cpu_cores} 核` : "检测中…"}</div>
            <div className="text-xs text-muted-foreground">{hw?.os}</div>
          </div>
          <div className="rounded-lg border border-border glass-surface p-4">
            <div className="flex items-center gap-2 text-xs text-muted-foreground"><HardDrive className="h-3.5 w-3.5" /> 内存</div>
            <div className="mt-1 text-sm font-medium">{hw ? `${(hw.ram_mb / 1024).toFixed(1)} GB` : "检测中…"}</div>
          </div>
          <div className="rounded-lg border border-border glass-surface p-4">
            <div className="flex items-center gap-2 text-xs text-muted-foreground"><MonitorCog className="h-3.5 w-3.5" /> 显卡</div>
            <div className="mt-1 truncate text-sm font-medium" title={hw?.gpu?.name}>
              {hw?.gpu ? hw.gpu.name : "未检测到独显（按内存跑 CPU 推理）"}
            </div>
            {hw?.gpu && <div className="text-xs text-muted-foreground">{(hw.gpu.vram_mb / 1024).toFixed(1)} GB 显存</div>}
          </div>
        </div>

        {/* 换张显卡试试 */}
        <div className="flex flex-wrap items-center gap-2 rounded-lg border border-border glass-surface px-4 py-3">
          <span className="text-xs text-muted-foreground">假设换成：</span>
          <select
            value={simulatedGpu}
            onChange={(e) => void simulate(e.target.value)}
            className="h-8 max-w-[18rem] rounded-md border border-border bg-background px-2 text-xs"
          >
            <option value="">本机实际硬件</option>
            {gpus.map((g) => (
              <option key={g.name} value={g.name}>
                {g.name}（{(g.vram_mb / 1024).toFixed(0)} GB · {g.generation}）
              </option>
            ))}
          </select>
          {simulatedGpu && <span className="text-xs text-warning">模拟中，下方为该显卡的推荐</span>}
        </div>

        {/* 推荐列表 */}
        <div className="rounded-lg border border-border glass-surface p-4">
          <div className="mb-3 flex flex-wrap items-center justify-between gap-2">
            <div className="text-sm font-semibold">
              推荐（{recs.length} 个候选{installed.size > 0 && ` · 本机已装 ${installed.size} 个`}）
            </div>
            {/* 「跑不动」的条目留着有用（想知道换硬件能跑什么），但挑模型时是噪声。 */}
            <div className="flex gap-1">
              {([
                { id: "runnable" as const, label: "能跑的" },
                { id: "installed" as const, label: "已安装" },
                { id: "all" as const, label: "全部" },
              ]).map((f) => (
                <button
                  key={f.id}
                  onClick={() => setFilter(f.id)}
                  className={cn(
                    "rounded-md border px-2 py-0.5 text-xs transition",
                    filter === f.id
                      ? "border-primary/40 bg-primary/10 text-primary"
                      : "border-border text-muted-foreground hover:text-foreground"
                  )}
                >
                  {f.label}
                </button>
              ))}
            </div>
          </div>
          <div className="space-y-1.5">
            {visible.length === 0 && (
              <div className="py-6 text-center text-xs text-muted-foreground">
                {loading
                  ? "计算中…"
                  : recs.length === 0
                    ? "没有候选——检查一下硬件检测是否成功。"
                    : "这个筛选下没有条目。"}
              </div>
            )}
            {visible.map((rec) => {
              const meta = FIT_META[rec.overall_fit];
              const tag = ollamaTag(rec);
              const isInstalled = tag ? installed.has(tag) : false;
              const progress = tag ? pulling[tag] : undefined;
              const percent = progress ? pullPercent(progress) : null;
              const reason = fitReason(rec);
              return (
                <div key={rec.model.name} className="flex items-center gap-3 rounded-md border border-border/60 px-3 py-2">
                  <div className="min-w-0 flex-1">
                    <div className="flex flex-wrap items-center gap-2">
                      <span className="text-sm font-medium">{rec.model.display_name}</span>
                      {/* 「本机已装但不在目录里」的条目没有体积/质量数据（全是 0），
                          照原样显示会变成「0 GB · 质量 0/10」，看着像坏了。 */}
                      {rec.model.family === "local" ? (
                        <span className="text-xs text-muted-foreground">本机已有 · 不在推荐目录中</span>
                      ) : (
                        <span className="text-xs text-muted-foreground">
                          {rec.model.size_gb} GB · {rec.model.family} · 质量 {rec.model.quality}/10 · {rec.model.speed_rating}
                        </span>
                      )}
                      {/* 「这个模型是干什么的」是挑选时的第一个问题。目录里一直带着
                          categories，只是从没显示过。 */}
                      {rec.model.categories.map((category) => (
                        <span key={category} className="rounded border border-border px-1.5 py-0.5 text-[10px] text-muted-foreground">
                          {category}
                        </span>
                      ))}
                      {rec.model.is_moe && (
                        <span className="rounded border border-primary/30 bg-primary/10 px-1.5 py-0.5 text-[10px] text-primary"
                              title={rec.model.active_params_gb ? `MoE：每次推理只激活约 ${rec.model.active_params_gb}B 参数` : "混合专家模型"}>
                          MoE
                        </span>
                      )}
                      {isInstalled && (
                        <span className="rounded border border-success/40 bg-success/10 px-1.5 py-0.5 text-[11px] text-success">已安装</span>
                      )}
                    </div>
                    <div className="truncate text-xs text-muted-foreground" title={rec.model.description}>
                      {rec.model.description}
                    </div>
                    <div className="mt-0.5 text-[11px] text-muted-foreground">
                      {rec.model.family !== "local" && <>需 {rec.model.min_vram_gb} GB 显存 · </>}
                      置信度 {rec.confidence_label}
                      {tag && <> · <code className="text-foreground">{rec.install_cmd || `ollama pull ${tag}`}</code></>}
                    </div>
                    {progress !== undefined && (
                      <div className="mt-1.5">
                        {/* 几 GB 的下载只给一行文字太难熬。有百分比就画条，
                            没有（校验、解包阶段）就照原样显示那行输出。 */}
                        {percent !== null && (
                          <div className="mb-1 h-1.5 overflow-hidden rounded-full bg-muted/40">
                            <div className="h-full rounded-full bg-primary transition-all" style={{ width: `${percent}%` }} />
                          </div>
                        )}
                        <div className="truncate font-mono text-[11px] text-primary" title={progress}>
                          {percent !== null && <span className="mr-1 font-semibold">{percent}%</span>}
                          {progress}
                        </div>
                      </div>
                    )}
                  </div>

                  <div className="flex shrink-0 flex-col items-end gap-0.5">
                    <span className={cn("rounded border px-2 py-0.5 text-xs font-medium", meta.cls)}>{meta.label}</span>
                    {/* 卡在显存还是内存，决定的是「换显卡」还是「加内存」。 */}
                    {reason && <span className="text-[10px] text-muted-foreground">{reason}</span>}
                  </div>

                  {progress !== undefined ? (
                    <button
                      onClick={() => tag && void cookbookApi.cancelPull(tag)}
                      className="flex shrink-0 items-center gap-1 rounded-md border border-border px-2 py-1 text-xs text-muted-foreground hover:text-destructive"
                      title="取消下载"
                    >
                      <X className="h-3 w-3" /> 取消
                    </button>
                  ) : (
                    <button
                      onClick={() => void startPull(rec)}
                      disabled={isInstalled || !tag || rec.overall_fit === "impossible"}
                      title={
                        isInstalled ? "本机已经装了" :
                        !tag ? "这个模型没有给出 Ollama 拉取命令" :
                        rec.overall_fit === "impossible" ? "这台机器跑不动" : "下载到本机"
                      }
                      className="flex shrink-0 items-center gap-1 rounded-md border border-border px-2 py-1 text-xs text-muted-foreground hover:bg-muted/20 hover:text-foreground disabled:opacity-30"
                    >
                      <Download className="h-3 w-3" /> 下载
                    </button>
                  )}
                </div>
              );
            })}
          </div>
        </div>
      </div>
    </div>
  );
}

/** 从 `ollama pull xxx` 里取出模型标签。取不出就说明这条没有安装命令。 */
function ollamaTag(rec: ModelRecommendation): string {
  const source = rec.install_cmd || rec.model.ollama_cmd || "";
  const match = source.match(/ollama\s+pull\s+(\S+)/);
  return match ? match[1] : "";
}

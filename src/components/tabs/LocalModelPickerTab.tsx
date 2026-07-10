/**
 * LocalModelPickerTab — 本地模型选型 (whichllm inspired).
 *
 * Detects the machine's CPU/RAM, then ranks which open-weight models fit a
 * memory budget (RAM for CPU inference, or type your GPU's VRAM to simulate GPU
 * inference). Recommends only — pairs with the deferred local-model installer.
 */
import { useCallback, useEffect, useState } from "react";
import { Cpu, HardDrive, RefreshCw, Sparkles } from "lucide-react";

import { cn } from "@/lib/utils";
import { localModelApi, type HardwareInfo, type ModelRecommendation } from "@/lib/tauri-api";

const FIT_META: Record<ModelRecommendation["fit"], { label: string; cls: string }> = {
  fits: { label: "可流畅运行", cls: "text-success border-success/40 bg-success/10" },
  tight: { label: "勉强能跑", cls: "text-warning border-warning/40 bg-warning/10" },
  wont_run: { label: "跑不动", cls: "text-muted-foreground border-border bg-muted/10" },
};

export function LocalModelPickerTab() {
  const [hw, setHw] = useState<HardwareInfo | null>(null);
  const [budget, setBudget] = useState<number>(16);
  const [models, setModels] = useState<ModelRecommendation[]>([]);
  const [loading, setLoading] = useState(false);

  const load = useCallback(async (b: number) => {
    setLoading(true);
    try {
      setModels(await localModelApi.recommend(b));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    localModelApi.detectHardware().then((info) => {
      setHw(info);
      const ram = Math.max(1, Math.round(info.ram_gb));
      setBudget(ram);
      void load(ram);
    }).catch(() => void load(16));
  }, [load]);

  return (
    <div className="flex h-full flex-1 flex-col overflow-hidden bg-background">
      <div className="border-b border-border px-6 py-4">
        <div className="flex items-center gap-2 text-lg font-semibold">
          <Sparkles className="h-5 w-5 text-primary" /> 本地模型选型
        </div>
        <p className="mt-1 max-w-3xl text-sm leading-6 text-muted-foreground">
          按本机内存/显存推荐能跑的开源大模型与合适的量化档。仅推荐——装模型请配合本地模型安装（规划中）。
        </p>
      </div>

      <div className="flex flex-col gap-5 overflow-y-auto p-6">
        {/* Hardware + budget */}
        <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
          <div className="rounded-lg border border-border bg-card/40 p-4">
            <div className="flex items-center gap-2 text-xs text-muted-foreground"><Cpu className="h-3.5 w-3.5" /> CPU</div>
            <div className="mt-1 text-sm font-medium">{hw ? `${hw.cpu_cores} 核` : "检测中…"}</div>
            <div className="truncate text-xs text-muted-foreground" title={hw?.cpu_brand}>{hw?.cpu_brand}</div>
          </div>
          <div className="rounded-lg border border-border bg-card/40 p-4">
            <div className="flex items-center gap-2 text-xs text-muted-foreground"><HardDrive className="h-3.5 w-3.5" /> 内存</div>
            <div className="mt-1 text-sm font-medium">{hw ? `${hw.ram_gb} GB` : "检测中…"}</div>
          </div>
          <div className="rounded-lg border border-border bg-card/40 p-4">
            <label className="flex items-center gap-2 text-xs text-muted-foreground">显存/预算 (GB)</label>
            <div className="mt-1 flex items-center gap-2">
              <input
                type="number"
                min={1}
                value={budget}
                onChange={(e) => setBudget(Number(e.target.value) || 1)}
                className="h-8 w-20 rounded-md border border-border bg-background px-2 text-sm"
              />
              <button
                onClick={() => void load(budget)}
                className="flex items-center gap-1 rounded-md border border-border px-2 py-1 text-xs text-muted-foreground hover:bg-muted/20 hover:text-foreground"
              >
                <RefreshCw className={cn("h-3 w-3", loading && "animate-spin")} /> 重算
              </button>
            </div>
            <div className="mt-1 text-xs text-muted-foreground">默认用内存；有独显就填显存模拟 GPU 推理。</div>
          </div>
        </div>

        {/* Ranked models */}
        <div className="rounded-lg border border-border bg-card/40 p-4">
          <div className="mb-3 text-sm font-semibold">按 {budget} GB 预算的推荐（共 {models.length} 个候选）</div>
          <div className="space-y-1.5">
            {models.map((m) => {
              const meta = FIT_META[m.fit];
              return (
                <div key={m.name} className="flex items-center gap-3 rounded-md border border-border/60 px-3 py-2">
                  <div className="min-w-0 flex-1">
                    <div className="flex items-center gap-2">
                      <span className="text-sm font-medium">{m.name}</span>
                      <span className="text-xs text-muted-foreground">{m.params_b}B · {m.family}</span>
                    </div>
                    <div className="text-xs text-muted-foreground">建议量化 {m.best_quant} · 约需 {m.needed_gb} GB</div>
                  </div>
                  <span className={cn("shrink-0 rounded border px-2 py-0.5 text-xs font-medium", meta.cls)}>{meta.label}</span>
                </div>
              );
            })}
          </div>
        </div>
      </div>
    </div>
  );
}

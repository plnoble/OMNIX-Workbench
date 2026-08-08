import { describe, expect, it } from "vitest";

import { fitReason, pullPercent } from "./LocalModelPickerTab";
import type { ModelRecommendation } from "@/lib/tauri-api";

/** 只关心这三个字段，其余用最小占位补齐。 */
function rec(fit: ModelRecommendation["overall_fit"], vram: boolean, ram: boolean): ModelRecommendation {
  return {
    model: {
      name: "x", display_name: "X", size_gb: 1, min_vram_gb: 1, categories: [], quality: 5,
      description: "", ollama_cmd: "", speed_rating: "fast", family: "X", generation: 1,
      evidence_tier: "Direct", confidence: 1, is_moe: false, active_params_gb: null,
    },
    fits_vram: vram,
    fits_ram: ram,
    overall_fit: fit,
    install_cmd: "",
    effective_quality: 5,
    confidence_label: "高",
  } as ModelRecommendation;
}

describe("pullPercent", () => {
  it("从 ollama 的进度行里抠出百分比", () => {
    expect(pullPercent("pulling 8934d96d3f08:  45% ▕███ ▏ 1.2 GB/2.7 GB")).toBe(45);
    expect(pullPercent("pulling manifest: 100%")).toBe(100);
    expect(pullPercent("pulling 0%")).toBe(0);
  });

  it("没有百分比的阶段返回 null，而不是假装 0%", () => {
    // 校验和解包这两步真的没有百分比。返回 0 会让进度条倒退回起点。
    for (const line of ["verifying sha256 digest", "writing manifest", "success", "准备中…"]) {
      expect(pullPercent(line)).toBeNull();
    }
  });

  it("越界的数字被夹回 0–100", () => {
    expect(pullPercent("999%")).toBe(100);
  });
});

describe("fitReason", () => {
  it("跑得动就不说废话", () => {
    expect(fitReason(rec("perfect", true, true))).toBe("");
  });

  it("分别点出卡在显存还是内存——这决定的是换显卡还是加内存", () => {
    expect(fitReason(rec("impossible", false, true))).toBe("显存不够");
    expect(fitReason(rec("impossible", true, false))).toBe("内存不够");
    expect(fitReason(rec("impossible", false, false))).toBe("显存不够 · 内存不够");
  });

  it("两项都过但仍标 tight 时，说的是余量小而不是编一个假原因", () => {
    expect(fitReason(rec("tight", true, true))).toBe("余量很小");
  });
});

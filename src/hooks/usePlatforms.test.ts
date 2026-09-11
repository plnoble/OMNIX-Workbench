import { describe, expect, it } from "vitest";

import { platformModelsResponseIsCurrent } from "./usePlatforms";

describe("platformModelsResponseIsCurrent", () => {
  it("快速 A→B 时，A 的慢列表不能覆盖 B", () => {
    expect(platformModelsResponseIsCurrent("plat-a", 1, "plat-b", 2)).toBe(false);
  });

  it("批量检测期间切走平台，结果不回写", () => {
    expect(platformModelsResponseIsCurrent("plat-a", 4, "plat-b", 5)).toBe(false);
  });

  it("当前平台的当前请求可以写入", () => {
    expect(platformModelsResponseIsCurrent("plat-b", 5, "plat-b", 5)).toBe(true);
  });
});

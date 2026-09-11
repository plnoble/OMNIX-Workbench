import { describe, expect, it } from "vitest";

import { previewRequestIsCurrent } from "./usePreview";

describe("previewRequestIsCurrent", () => {
  it("A→B 切换后，A 的晚到结果不能写入 B", () => {
    expect(previewRequestIsCurrent("ws-a", 1, "ws-b", 2)).toBe(false);
  });

  it("同一工作区的当前序号才能写入", () => {
    expect(previewRequestIsCurrent("ws-b", 2, "ws-b", 2)).toBe(true);
  });

  it("空工作区不保留旧序号", () => {
    expect(previewRequestIsCurrent("ws-a", 3, "direct", 4)).toBe(false);
    expect(previewRequestIsCurrent("", 1, "", 1)).toBe(true);
  });
});

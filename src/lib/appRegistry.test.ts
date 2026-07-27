import { describe, expect, it } from "vitest";

import { APP_ENTRIES, DEFAULT_NAVIGATION_LAYOUT, normalizeNavigationLayout } from "./appRegistry";

describe("normalizeNavigationLayout", () => {
  it("空布局回落到注册表默认 placement，且核心入口 work/chat 必在 pinned", () => {
    const layout = normalizeNavigationLayout(null);
    expect(layout.pinned).toContain("chat");
    expect(layout.pinned).toContain("work");
    // 每个注册的应用恰好出现一次（pinned ∪ launcher，无隐藏层）。
    const all = [...layout.pinned, ...layout.launcher];
    expect(new Set(all).size).toBe(all.length);
    expect(all.sort()).toEqual(APP_ENTRIES.map((e) => e.id).sort());
    expect(layout.hidden).toEqual([]);
  });

  it("未知 id 丢弃、重复去重", () => {
    const layout = normalizeNavigationLayout({
      pinned: ["chat", "chat", "ghost-app", "work"],
      launcher: ["team", "team"],
    });
    expect(layout.pinned.filter((id) => id === "chat")).toHaveLength(1);
    expect([...layout.pinned, ...layout.launcher]).not.toContain("ghost-app");
    expect(layout.launcher.filter((id) => id === "team")).toHaveLength(1);
  });

  it("旧版 hidden 层折进宫格，不丢应用", () => {
    const layout = normalizeNavigationLayout({ pinned: ["work", "chat"], launcher: [], hidden: ["notes"] });
    expect(layout.launcher).toContain("notes");
    expect(layout.hidden).toEqual([]);
  });

  it("work/chat 被挪进宫格时强制拉回 pinned 首位", () => {
    const layout = normalizeNavigationLayout({ pinned: ["team"], launcher: ["work", "chat"] });
    expect(layout.pinned[0]).toBe("chat");
    expect(layout.pinned).toContain("work");
    expect(layout.launcher).not.toContain("work");
    expect(layout.launcher).not.toContain("chat");
  });

  it("默认布局自身归一化后保持稳定（无意外迁移）", () => {
    const layout = normalizeNavigationLayout(DEFAULT_NAVIGATION_LAYOUT);
    expect(layout.pinned).toEqual(DEFAULT_NAVIGATION_LAYOUT.pinned);
  });
});

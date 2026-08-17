import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { pollWhileVisible, type VisibilitySource } from "./usePolling";

/** 手动可控的可见性来源。 */
function fakeVisibility(startHidden = false) {
  let hidden = startHidden;
  const handlers = new Set<() => void>();
  return {
    source: {
      isHidden: () => hidden,
      subscribe: (handler: () => void) => {
        handlers.add(handler);
        return () => handlers.delete(handler);
      },
    } satisfies VisibilitySource,
    set(next: boolean) {
      hidden = next;
      handlers.forEach((h) => { h(); });
    },
    get subscriberCount() {
      return handlers.size;
    },
  };
}

beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

describe("pollWhileVisible", () => {
  it("可见时立刻拉一次，然后按周期反复拉", () => {
    const run = vi.fn();
    const vis = fakeVisibility();
    const stop = pollWhileVisible(run, 1000, vis.source);

    expect(run).toHaveBeenCalledTimes(1);
    vi.advanceTimersByTime(3000);
    expect(run).toHaveBeenCalledTimes(4);
    stop();
  });

  it("窗口隐藏后一次都不再拉", () => {
    const run = vi.fn();
    const vis = fakeVisibility();
    const stop = pollWhileVisible(run, 1000, vis.source);
    vi.advanceTimersByTime(2000);
    const before = run.mock.calls.length;

    vis.set(true);
    vi.advanceTimersByTime(60_000);

    expect(run).toHaveBeenCalledTimes(before);
    stop();
  });

  it("重新可见时立刻补一次，不用干等一个周期", () => {
    const run = vi.fn();
    const vis = fakeVisibility();
    const stop = pollWhileVisible(run, 10_000, vis.source);
    vis.set(true);
    const paused = run.mock.calls.length;

    vis.set(false);

    expect(run).toHaveBeenCalledTimes(paused + 1);
    // 而且周期要重新跑起来
    vi.advanceTimersByTime(10_000);
    expect(run).toHaveBeenCalledTimes(paused + 2);
    stop();
  });

  it("启动时就是隐藏的话，一次都不拉", () => {
    const run = vi.fn();
    const vis = fakeVisibility(true);
    const stop = pollWhileVisible(run, 1000, vis.source);

    expect(run).not.toHaveBeenCalled();
    vi.advanceTimersByTime(5000);
    expect(run).not.toHaveBeenCalled();
    stop();
  });

  it("连着两次「可见」不会开出两个定时器", () => {
    const run = vi.fn();
    const vis = fakeVisibility();
    const stop = pollWhileVisible(run, 1000, vis.source);
    run.mockClear();

    vis.set(false); // 本来就可见，再来一次
    vis.set(false);
    vi.advanceTimersByTime(1000);

    expect(run).toHaveBeenCalledTimes(1);
    stop();
  });

  it("清理时定时器和监听都撤掉", () => {
    const run = vi.fn();
    const vis = fakeVisibility();
    const stop = pollWhileVisible(run, 1000, vis.source);
    expect(vis.subscriberCount).toBe(1);

    stop();
    vi.advanceTimersByTime(10_000);

    expect(run).toHaveBeenCalledTimes(1); // 只有挂载那一次
    expect(vis.subscriberCount).toBe(0);
    // 撤掉之后再切可见性也不该复活
    vis.set(true);
    vis.set(false);
    vi.advanceTimersByTime(10_000);
    expect(run).toHaveBeenCalledTimes(1);
  });
});

describe("接线守卫", () => {
  /**
   * 新的轮询必须走 `usePolling`。
   *
   * 这条守的是它诞生的原因：同一个「立刻拉一次 + setInterval + 清理」的模式
   * 曾在 8 个组件里各写一遍，八份都没管窗口是不是还看得见。谁再手写一个，
   * 那份就又不会在托盘里停下——而且照样一声不吭。
   */
  it("组件里不再手写 setInterval", async () => {
    const fs = await import("node:fs");
    const path = await import("node:path");
    const root = path.resolve(__dirname, "..");

    /** 故意不走 usePolling 的，写在这里并说明理由。 */
    const EXEMPT: Record<string, string> = {
      "hooks/useAutopilotRunner.ts":
        "自动任务是「不看着也要按时跑」的后台工作，停掉等于把功能关了",
      "SpeakerView.tsx":
        "演讲者视图的秒表，不拉任何数据；它已按 paused 自行门控",
      "hooks/usePolling.ts": "就是它本身",
    };

    const offenders: string[] = [];
    const walk = (dir: string) => {
      for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
        const full = path.join(dir, entry.name);
        if (entry.isDirectory()) {
          walk(full);
          continue;
        }
        if (!/\.tsx?$/.test(entry.name) || entry.name.endsWith(".test.ts")) continue;
        const rel = path.relative(root, full).split(path.sep).join("/");
        if (rel in EXEMPT) continue;
        if (/setInterval\s*\(/.test(fs.readFileSync(full, "utf8"))) offenders.push(rel);
      }
    };
    walk(root);

    expect(offenders, "这些文件手写了 setInterval，改用 usePolling（或在 EXEMPT 里写明理由）").toEqual([]);
  });
});

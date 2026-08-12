import { describe, expect, it } from "vitest";

import {
  capLog,
  conversationIsWork,
  MAX_TERMINAL_LOG_CHARS,
  pickConversationForSurface,
} from "./useConversations";

/** 造一条会话记录，只填判断用得上的字段。 */
function conv(id: string, agent: string, workspace: string | null, createdAt: string) {
  return { id, active_agent: agent, workspace_path: workspace, created_at: createdAt };
}

describe("capLog", () => {
  it("没超上限就原样返回——不该无端改写正常日志", () => {
    const short = "line1\nline2\n";
    expect(capLog(short)).toBe(short);
  });

  it("超限时保留最近内容并标出省略", () => {
    const text = "x".repeat(MAX_TERMINAL_LOG_CHARS + 5000);
    const capped = capLog(text);
    expect(capped.length).toBeLessThanOrEqual(MAX_TERMINAL_LOG_CHARS + 64);
    expect(capped).toContain("较早的日志已省略");
  });

  it("从换行处切，不把一行截成半句", () => {
    // 头部塞满，尾部是几行完整日志。
    const tail = "第一行完整\n第二行完整\n第三行完整\n";
    const text = "A".repeat(MAX_TERMINAL_LOG_CHARS) + "\n" + tail;
    const capped = capLog(text);
    // 省略提示之后的第一行必须是完整的一行，不能是被切了一半的 "AAAA…"
    const firstRealLine = capped.split("\n")[1];
    expect(firstRealLine.startsWith("A")).toBe(false);
  });
});

describe("conversationIsWork", () => {
  it('"direct" 和空值都算普通对话，不是工作会话', () => {
    expect(conversationIsWork({ workspace_path: "direct" })).toBe(false);
    expect(conversationIsWork({ workspace_path: "" })).toBe(false);
    expect(conversationIsWork({ workspace_path: null })).toBe(false);
    expect(conversationIsWork({})).toBe(false);
  });

  it("绑了真实工作区才算工作会话", () => {
    expect(conversationIsWork({ workspace_path: "D:/repo" })).toBe(true);
  });
});

describe("pickConversationForSurface", () => {
  const list = [
    conv("c_old", "claude", "direct", "2026-08-01T00:00:00Z"),
    conv("c_new", "claude", "direct", "2026-08-05T00:00:00Z"),
    conv("w1", "claude", "D:/repo", "2026-08-06T00:00:00Z"),
    conv("other", "codex", "direct", "2026-08-07T00:00:00Z"),
  ];

  it("当前会话已经合适就什么都不动", () => {
    expect(
      pickConversationForSurface({ agent: "claude", surface: "chat", conversations: list, currentConvId: "c_new" })
    ).toEqual({ kind: "keep" });
  });

  it("切到对话时恢复该 Agent 最近的一条普通会话", () => {
    expect(
      pickConversationForSurface({ agent: "claude", surface: "chat", conversations: list, currentConvId: "" })
    ).toEqual({ kind: "select", id: "c_new" });
  });

  it("不会串到别的 Agent 的会话上——每个 Agent 的历史是独立的", () => {
    const pick = pickConversationForSurface({
      agent: "gemini",
      surface: "chat",
      conversations: list,
      currentConvId: "",
    });
    expect(pick).toEqual({ kind: "blank" });
  });

  it("工作界面永远从干净的工作区选择开始，不悄悄重开上一个工作区", () => {
    expect(
      pickConversationForSurface({ agent: "claude", surface: "work", conversations: list, currentConvId: "" })
    ).toEqual({ kind: "blank" });
  });

  /**
   * 这条守的是那个真实 bug：切个页签回来历史就没了。
   *
   * 刚发出第一条消息就切走，新会话还没回填到列表里——「id 有值但列表里没有」
   * 只说明列表没刷新，不说明会话不存在。当成不存在就会清空用户正在看的对话。
   */
  it("当前会话还没回填进列表时，绝不清空", () => {
    const pick = pickConversationForSurface({
      agent: "claude",
      surface: "chat",
      conversations: list,
      currentConvId: "conv_just_created_not_in_list_yet",
    });
    expect(pick).toEqual({ kind: "keep" });
  });

  it("从工作切回对话时，工作会话不算数", () => {
    // 当前停在工作会话 w1，要切到「对话」→ 得换成最近的普通会话。
    expect(
      pickConversationForSurface({ agent: "claude", surface: "chat", conversations: list, currentConvId: "w1" })
    ).toEqual({ kind: "select", id: "c_new" });
  });

  it("列表为空且没有当前会话时开一个空的", () => {
    expect(
      pickConversationForSurface({ agent: "claude", surface: "chat", conversations: [], currentConvId: "" })
    ).toEqual({ kind: "blank" });
  });
});

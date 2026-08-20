import { describe, expect, it } from "vitest";
import type { ConversationMessage, MessagesDelta } from "../types";
import { lastPersistedMessageId, mergeMessagesDelta, prependOlderMessages } from "./useConversations";

/**
 * 增量合并。
 *
 * 替代的是「每收一个 agent 事件就把整个会话重拉一遍并整体替换」——一轮对话会走到
 * 那里几十次。改成增量之后，这里有三件容易错的事，而它们错了**都只表现为界面上
 * 消息重了一条，不会报任何错**。所以逻辑抽成纯函数，专门钉住。
 */

const msg = (id: string, role: ConversationMessage["role"] = "assistant"): ConversationMessage => ({
  id,
  conversation_id: "c1",
  role,
  content: id,
  timestamp: "2026-01-01 10:00:00",
});

const delta = (messages: ConversationMessage[], is_full = false): MessagesDelta =>
  ({ messages, is_full });

describe("增量游标", () => {
  it("跳过乐观气泡，取最后一条持久化消息", () => {
    const list = [msg("msg_agent_1"), msg("msg_agent_2"), msg("msg_u_999", "user")];
    expect(lastPersistedMessageId(list)).toBe("msg_agent_2");
  });

  it("全是乐观气泡时返回 null（让后端给全量）", () => {
    expect(lastPersistedMessageId([msg("msg_u_1", "user")])).toBeNull();
    expect(lastPersistedMessageId([])).toBeNull();
  });

  it("拿乐观气泡当游标的话后端一定查不到——那就等于没做增量", () => {
    // 这条是上一条的「为什么」：msg_u_* 从不入库。
    const list = [msg("msg_agent_1"), msg("msg_u_999", "user")];
    expect(lastPersistedMessageId(list)).not.toContain("msg_u_");
  });
});

describe("增量合并", () => {
  it("is_full 是替换，不是追加", () => {
    const current = [msg("a"), msg("b")];
    const out = mergeMessagesDelta(current, delta([msg("x")], true));
    expect(out.map((m) => m.id)).toEqual(["x"]);
  });

  it("全量能让列表**变短**——压缩删过消息之后就是这样", () => {
    // 这条是上一条的补充，专挑「按 id 去重也救不回来」的场景：压缩把旧消息删了，
    // 全量回来的是更短的一段。当成追加的话被删掉的那些还留在界面上，用户会以为
    // 压缩没生效。
    //
    // （第一版这里写的是「同一批消息以 is_full 回来，长度不能翻倍」——那个断言
    //  恒真：去重本来就会挡掉重复，忽略 is_full 也照样绿。反向验证当场抓到了。）
    const current = [msg("old1"), msg("old2"), msg("recent")];
    const out = mergeMessagesDelta(current, delta([msg("summary"), msg("recent")], true));
    expect(out.map((m) => m.id)).toEqual(["summary", "recent"]);
  });

  it("空增量不动当前列表", () => {
    const current = [msg("a")];
    expect(mergeMessagesDelta(current, delta([]))).toBe(current);
  });

  it("普通增量追加在后面", () => {
    const out = mergeMessagesDelta([msg("a")], delta([msg("b"), msg("c")]));
    expect(out.map((m) => m.id)).toEqual(["a", "b", "c"]);
  });

  it("增量里出现 user 消息时，乐观气泡被清掉——否则同一句话显示两遍", () => {
    const current = [msg("msg_agent_1"), msg("msg_u_999", "user")];
    const out = mergeMessagesDelta(
      current,
      delta([msg("msg_agent_2", "user"), msg("msg_agent_3")]),
    );
    expect(out.map((m) => m.id)).toEqual(["msg_agent_1", "msg_agent_2", "msg_agent_3"]);
    expect(out.some((m) => m.id.startsWith("msg_u_"))).toBe(false);
  });

  it("增量里没有 user 消息时，乐观气泡留着（这一轮还没落库）", () => {
    const current = [msg("msg_agent_1"), msg("msg_u_999", "user")];
    const out = mergeMessagesDelta(current, delta([msg("msg_agent_2")]));
    expect(out.map((m) => m.id)).toEqual(["msg_agent_1", "msg_u_999", "msg_agent_2"]);
  });

  it("按 id 去重：REPLACE 过的消息会重新排到游标之后", () => {
    const current = [msg("a"), msg("b")];
    const out = mergeMessagesDelta(current, delta([msg("b"), msg("c")]));
    expect(out.map((m) => m.id)).toEqual(["a", "b", "c"]);
  });
});

describe("往回翻页拼接", () => {
  it("新拿到的一页拼在顶部（更早的在前）", () => {
    const out = prependOlderMessages([msg("c")], [msg("a"), msg("b")]);
    expect(out.map((m) => m.id)).toEqual(["a", "b", "c"]);
  });

  it("并发点两次「加载更早」不会拼出重复", () => {
    const current = [msg("b"), msg("c")];
    const out = prependOlderMessages(current, [msg("a"), msg("b")]);
    expect(out.map((m) => m.id)).toEqual(["a", "b", "c"]);
  });

  it("整页都已经有了就不动原数组（避免无谓重渲染）", () => {
    const current = [msg("a"), msg("b")];
    expect(prependOlderMessages(current, [msg("a")])).toBe(current);
  });

  it("空页不动原数组", () => {
    const current = [msg("a")];
    expect(prependOlderMessages(current, [])).toBe(current);
  });
});

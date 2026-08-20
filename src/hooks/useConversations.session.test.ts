import { describe, expect, it } from "vitest";
import { shouldHandoff, shouldTryResume } from "./useConversations";

/**
 * 「几天后回来还能不能接上」的判定。
 *
 * 这里出错的后果**全是静默的**：用户看到的还是那条连续的对话，只会觉得
 * 「它怎么把刚才说的都忘了」，不会有任何报错。
 */

describe("先试 resume", () => {
  it("重启后被收敛的会话，只要 CLI 那边还留着 session 就先 resume", () => {
    expect(shouldTryResume({
      sessionDead: true, hasExternalSessionId: true, configChanged: false,
    })).toBe(true);
  });

  it("没有 external_session_id 就没得 resume", () => {
    expect(shouldTryResume({
      sessionDead: true, hasExternalSessionId: false, configChanged: false,
    })).toBe(false);
  });

  it("配置变了不能 resume——resume 会按旧配置重新拉起", () => {
    expect(shouldTryResume({
      sessionDead: true, hasExternalSessionId: true, configChanged: true,
    })).toBe(false);
  });

  it("会话还活着就不用 resume", () => {
    expect(shouldTryResume({
      sessionDead: false, hasExternalSessionId: true, configChanged: false,
    })).toBe(false);
  });
});

describe("要不要交接上下文", () => {
  const base = {
    handoffEnabled: true,
    agent: "Claude Code",
    resumed: false,
    hadSession: true,
    sessionDead: false,
    configChanged: false,
  };

  it("换了 agent 要交接", () => {
    expect(shouldHandoff({ ...base, priorAgent: "Codex" })).toBe(true);
  });

  /**
   * 这条是那个回归本身。启动收敛把遗留会话标成 failed 之后，同一个 agent
   * 起新会话时 `priorAgent === agent`，旧逻辑判定「不用交接」——新会话完全空白，
   * 而原文一直躺在 OMNIX 自己的库里。
   */
  it("同一个 agent、旧会话接不上、起了新会话 —— 也必须交接", () => {
    expect(shouldHandoff({
      ...base, priorAgent: "Claude Code", sessionDead: true,
    })).toBe(true);
  });

  it("resume 成功就不用交接——agent 自己的上下文还在", () => {
    expect(shouldHandoff({
      ...base, priorAgent: "Claude Code", sessionDead: true, resumed: true,
    })).toBe(false);
  });

  it("配置变了另起会话不算「接不上」，同 agent 时不重复注入", () => {
    expect(shouldHandoff({
      ...base, priorAgent: "Claude Code", sessionDead: true, configChanged: true,
    })).toBe(false);
  });

  it("全新对话（此前没有会话）没什么可交接", () => {
    expect(shouldHandoff({ ...base, hadSession: false, sessionDead: true })).toBe(false);
  });

  it("用户关掉了交接开关就不交接", () => {
    expect(shouldHandoff({
      ...base, handoffEnabled: false, priorAgent: "Codex",
    })).toBe(false);
  });
});

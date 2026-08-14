import { describe, expect, it } from "vitest";

import { correctionToSend } from "./SupervisionTab";

/**
 * 监督台的「拒绝并说明」。
 *
 * ACP / Codex 两个协议都没有「改一改再放行」的响应形态，所以纠正只能走
 * 「拒掉 + 补一条消息」。这里守的是那个判断——它同时决定**按钮文案**和
 * **要不要真的发消息**，两边共用一处才不会漂成假控件。
 */
describe("correctionToSend", () => {
  it("拒绝时把草稿原样交出去", () => {
    expect(correctionToSend(false, "别删那个文件，先备份")).toBe("别删那个文件，先备份");
  });

  it("批准时一律不发——补充说明是拒绝路径专有的", () => {
    // 批准了还把草稿甩过去，等于用户没按的按钮替他按了。
    expect(correctionToSend(true, "别删那个文件")).toBe("");
  });

  it("只有空白的草稿不算数", () => {
    expect(correctionToSend(false, "   \n\t ")).toBe("");
  });

  it("没填过的会话是 undefined，不是崩溃", () => {
    // correction 是按 session_id 存的稀疏字典，没碰过的会话取出来就是 undefined。
    expect(correctionToSend(false, undefined)).toBe("");
  });

  it("前后空白裁掉，中间的留着", () => {
    expect(correctionToSend(false, "  改用 git mv\n保留历史  ")).toBe("改用 git mv\n保留历史");
  });
});

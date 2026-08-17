import fs from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";

/**
 * 预览 agent 生成的 HTML 时，那个 iframe 必须是**关死的沙箱**。
 *
 * 这条守的不是「功能对不对」，是「安全属性有没有被人顺手去掉」——而去掉它的动机
 * 很现实：沙箱一关，页面里的脚本和样式表就不跑了，看起来「预览坏了」，最省事的
 * 改法就是加 `allow-scripts allow-same-origin`。那两个一旦同时给出，iframe 就能
 * 拿到本应用的 origin：读写 localStorage、带着远程面板的会话 Cookie 去打
 * `/api/remote/*`。而这份 HTML 是 agent 写的，内容可能来自它读到的任意网页或文件。
 *
 * 扫源码而不是渲染组件：这个仓库没有 jsdom/testing-library，惯例是把纯逻辑导出来
 * 单测；而这里要守的恰恰是 JSX 属性本身，扫文本是唯一能直接钉住它的办法。
 */

const PANE = path.resolve(__dirname, "PreviewPane.tsx");

describe("HTML 预览的沙箱", () => {
  const src = fs.readFileSync(PANE, "utf8");

  it("iframe 用 srcDoc 而不是 src", () => {
    // src 指向 URL 意味着又要有人去提供那个 URL。上一次就是这么坏的：指向
    // `localhost:1421/preview/...`，而网关根本没有那条路由，点开就是 404 白屏。
    expect(src).toMatch(/srcDoc=/);
    expect(src).not.toMatch(/<iframe[^>]*\ssrc=/);
  });

  it("sandbox 存在且不放行同源或脚本", () => {
    const m = src.match(/sandbox="([^"]*)"/);
    expect(m, "HTML 预览的 iframe 必须带 sandbox 属性").not.toBeNull();
    const value = m![1];
    expect(value, "sandbox 不能放行 allow-same-origin").not.toContain("allow-same-origin");
    expect(value, "sandbox 不能放行 allow-scripts").not.toContain("allow-scripts");
  });
});

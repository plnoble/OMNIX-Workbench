import { describe, expect, it } from "vitest";

import { buildDecisionReply, parseDecisionParts } from "./decisionBlock";

const VALID_BLOCK = `\`\`\`omnix-decision
{"question":"用哪种存储？","multi":false,"options":[{"label":"SQLite","description":"零运维","recommended":true},{"label":"Postgres","description":"并发写强"}]}
\`\`\``;

describe("parseDecisionParts", () => {
  it("拆出 正文 + 决策块 + 后文", () => {
    const parts = parseDecisionParts(`分析如下。\n${VALID_BLOCK}\n选完继续。`);
    expect(parts.map((p) => p.type)).toEqual(["text", "decision", "text"]);
    const decision = parts[1];
    if (decision.type !== "decision") throw new Error("expected decision part");
    expect(decision.spec.question).toBe("用哪种存储？");
    expect(decision.spec.options.map((o) => o.label)).toEqual(["SQLite", "Postgres"]);
  });

  it("流式未闭合的 fence 原样保留为文本（不吞内容）", () => {
    const streaming = '前文```omnix-decision\n{"question":"未完';
    const parts = parseDecisionParts(streaming);
    expect(parts).toEqual([{ type: "text", content: streaming }]);
  });

  it("非法 JSON / 选项不足 2 个时整块回退为文本", () => {
    const bad = "```omnix-decision\n{not json}\n```";
    expect(parseDecisionParts(bad).every((p) => p.type === "text")).toBe(true);
    const single = '```omnix-decision\n{"question":"q","options":[{"label":"唯一"}]}\n```';
    expect(parseDecisionParts(single).every((p) => p.type === "text")).toBe(true);
  });

  it("同一消息里的多个决策块都解析", () => {
    const parts = parseDecisionParts(`${VALID_BLOCK}\n中场\n${VALID_BLOCK}`);
    expect(parts.filter((p) => p.type === "decision")).toHaveLength(2);
  });
});

describe("buildDecisionReply", () => {
  it("带上所选项与补充说明", () => {
    const { display, agent } = buildDecisionReply(
      { question: "用哪种存储？", options: [{ label: "SQLite" }, { label: "Postgres" }] },
      ["SQLite"],
      "先跑通再说"
    );
    expect(display).toContain("SQLite");
    expect(display).toContain("先跑通再说");
    expect(agent).toContain("用哪种存储？");
    expect(agent).toContain("请基于这个选择继续");
  });
});

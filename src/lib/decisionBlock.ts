/**
 * omnix-decision 决策块协议 (#2 方案抉择框).
 *
 * Any agent (OMNIX gateway models, Claude Code, Codex, …) can emit a fenced
 * block and the chat renders it as selectable option cards; the user's choice
 * is sent back as the next turn:
 *
 * ```omnix-decision
 * {"question":"用哪种存储？","multi":false,
 *  "options":[{"label":"SQLite","description":"零运维","recommended":true},
 *             {"label":"Postgres","description":"并发写强"}]}
 * ```
 */

export interface DecisionOption {
  label: string;
  description?: string;
  recommended?: boolean;
}

export interface DecisionSpec {
  question: string;
  multi?: boolean;
  options: DecisionOption[];
}

export type MessagePart =
  | { type: "text"; content: string }
  | { type: "decision"; spec: DecisionSpec; raw: string };

const FENCE_OPEN = "```omnix-decision";
const FENCE_CLOSE = "```";

/**
 * Split message text into plain-text and decision parts. Incomplete fences
 * (mid-stream) and unparsable JSON stay as text so nothing is ever swallowed.
 */
export function parseDecisionParts(content: string): MessagePart[] {
  const parts: MessagePart[] = [];
  let remaining = content;
  while (remaining.length > 0) {
    const open = remaining.indexOf(FENCE_OPEN);
    if (open === -1) {
      parts.push({ type: "text", content: remaining });
      break;
    }
    const bodyStart = open + FENCE_OPEN.length;
    const close = remaining.indexOf(FENCE_CLOSE, bodyStart);
    if (close === -1) {
      // Fence not finished yet (still streaming) — keep as text for now.
      parts.push({ type: "text", content: remaining });
      break;
    }
    if (open > 0) parts.push({ type: "text", content: remaining.slice(0, open) });
    const raw = remaining.slice(bodyStart, close).trim();
    const spec = tryParseSpec(raw);
    if (spec) {
      parts.push({ type: "decision", spec, raw });
    } else {
      parts.push({ type: "text", content: remaining.slice(open, close + FENCE_CLOSE.length) });
    }
    remaining = remaining.slice(close + FENCE_CLOSE.length);
  }
  return parts;
}

function tryParseSpec(raw: string): DecisionSpec | null {
  try {
    const parsed = JSON.parse(raw) as DecisionSpec;
    if (
      typeof parsed.question === "string" &&
      Array.isArray(parsed.options) &&
      parsed.options.length >= 2 &&
      parsed.options.every((o) => typeof o.label === "string" && o.label.length > 0)
    ) {
      return parsed;
    }
    return null;
  } catch {
    return null;
  }
}

/** The format contract handed to models so their output parses every time. */
export const DECISION_FORMAT_SPEC = `输出一个决策块让用户选择。格式（必须严格遵守）：
\`\`\`omnix-decision
{"question": "<向用户提出的问题>", "multi": false, "options": [{"label": "<方案名>", "description": "<一两句说明，含利弊>", "recommended": true}, {"label": "<方案名2>", "description": "<说明>"}]}
\`\`\`
规则：options 2-4 个；每个 label 简短（3-8字）；description 一两句说清做法和利弊；最多一个 recommended:true；如果允许多选把 multi 设为 true；JSON 必须合法（双引号、无尾逗号）。决策块前可以有简短分析文字，但选项内容只放在决策块里，不要在正文重复罗列。`;

/** Build the /方案 prompt: analyze the requirement → propose schemes as a decision block. */
export function buildProposalPrompt(requirement: string): string {
  return `我有一个需求，请帮我提出 2-4 个可行方案让我抉择。

需求：
${requirement}

要求：
1. 先用三五句话说明你对需求的理解和关键权衡点。
2. 然后${DECISION_FORMAT_SPEC}
3. 等我选择后，你再基于我选的方案继续细化/执行，不要现在就展开全部细节。`;
}

/** Build the send-back message after the user picks option(s). */
export function buildDecisionReply(spec: DecisionSpec, chosen: string[], note: string): {
  display: string;
  agent: string;
} {
  const picks = chosen.join("、");
  const noteSuffix = note.trim() ? `\n补充说明：${note.trim()}` : "";
  return {
    display: `✅ 我的选择：${picks}${noteSuffix}`,
    agent: `关于「${spec.question}」，我选择了：${picks}。${noteSuffix}\n请基于这个选择继续。`,
  };
}

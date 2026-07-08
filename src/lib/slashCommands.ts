/**
 * Composer slash-command parsers — ported from DeepSeek-GUI's
 * `floating-composer-commands.ts` (behavior contract preserved).
 *
 * Only `/goal` and `/btw` are handled here; both are intercepted at send time
 * and never forwarded to the agent as a normal message.
 */

export type GoalCommand =
  | { action: "menu" }
  | { action: "set"; objective: string }
  | { action: "pause" | "resume" | "clear" | "complete" };

/**
 * Parses a `/goal` command. Returns `null` when the input is not `/goal`.
 * - `/goal` → open the menu (show current goal / usage)
 * - `/goal <objective>` → set the objective
 * - `/goal pause|resume|clear|complete` → status control
 * The `/goal` token must be standalone (followed by whitespace or end).
 */
export function parseGoalCommand(input: string): GoalCommand | null {
  const trimmed = input.trim();
  if (!trimmed.startsWith("/goal")) return null;
  const rest = trimmed.slice(5);
  if (rest.length > 0 && !/^\s/.test(rest)) return null;
  const body = rest.trim();
  if (!body) return { action: "menu" };
  const lowered = body.toLowerCase();
  if (lowered === "pause") return { action: "pause" };
  if (lowered === "resume") return { action: "resume" };
  if (lowered === "clear") return { action: "clear" };
  if (lowered === "complete" || lowered === "done") return { action: "complete" };
  return { action: "set", objective: body };
}

/**
 * Parses a `/btw <question>` command (open a side conversation that inherits
 * the current context). Returns `null` when not a `/btw` command, or
 * `{ question }` with `question === null` when `/btw` was given with no text.
 * The `/btw` token must be standalone (followed by whitespace or end).
 */
export function parseBtwCommand(input: string): { question: string | null } | null {
  const trimmed = input.trim();
  if (!trimmed.startsWith("/btw")) return null;
  const rest = trimmed.slice(4);
  if (rest.length > 0 && !/^\s/.test(rest)) return null;
  const question = rest.trim();
  return { question: question.length > 0 ? question : null };
}

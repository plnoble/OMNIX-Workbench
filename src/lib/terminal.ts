/**
 * OMNIX DevFlow — Terminal Stream Processing Utilities
 *
 * Pure functions for ANSI stripping, block-character filtering,
 * and interactive prompt detection from PTY output.
 */

import type { PromptType } from "@/types";

/**
 * Strip ANSI escape codes from terminal output.
 * Handles CSI, OSC, and other common escape sequences.
 */
export function stripAnsi(text: string): string {
  return text.replace(
    /[][[()#;?]*(?:[0-9]{1,4}(?:;[0-9]{0,4})*)?[0-9A-ORZcf-nqry=><]/g,
    ""
  );
}

/**
 * Process raw PTY output: strip ANSI, remove Braille spinners,
 * filter block-art/loading spinners by character ratio heuristic.
 */
export function processTerminalStream(text: string): string {
  // 1. Strip ANSI escape codes
  let cleaned = stripAnsi(text);

  // 2. Clear Unicode Braille animation spinners
  cleaned = cleaned.replace(/[⠀-⣿]\s*/g, "");

  // 3. Line-by-line filter for block art / loading spinners
  const BLOCK_CHARS = new Set([
    "■", "⬝", "▄", "▀", "█", "░", "▒", "▓",
    "│", "┌", "┐", "└", "┘", "├", "┤", "┬", "┴", "┼", "─",
  ]);

  const lines = cleaned.split("\n");
  const filteredLines = lines.filter((line) => {
    let blockCount = 0;
    for (const char of line) {
      if (BLOCK_CHARS.has(char)) blockCount++;
    }

    const totalChars = line.trim().length;
    if (totalChars === 0) return true;

    // Heuristic: ratio > 25% or count > 15 → likely block art
    return blockCount / totalChars <= 0.25 && blockCount <= 15;
  });

  return filteredLines.join("\n");
}

/**
 * Detect interactive prompt type from accumulated terminal logs.
 * Returns the detected prompt type as a pure value (no side effects).
 */
export function detectInteractivePrompts(logs: string): PromptType {
  const lower = logs.toLowerCase();

  if (lower.includes("safety check") || lower.includes("trust")) {
    return "trust";
  }
  if (lower.includes("new release") || lower.includes("update now")) {
    return "update";
  }
  if (
    lower.includes("navigate") ||
    lower.includes("terms of service") ||
    lower.includes("↑/↓") ||
    lower.includes("tab to")
  ) {
    return "menu";
  }
  if (
    lower.includes("shift+tab") ||
    lower.includes("accept edits") ||
    lower.includes("type your message")
  ) {
    return "editor";
  }

  return "none";
}

// ── Automatic Mistake Detection ──────────────────────────

/**
 * A development mistake detected from terminal output.
 * Categorized by type and severity for prioritized memory injection.
 */
export interface DetectedMistake {
  /** Error category */
  category:
    | "compile_error"
    | "test_failure"
    | "runtime_crash"
    | "lint_warning"
    | "api_error"
    | "privacy_leak";
  /** high / medium / low */
  severity: "high" | "medium" | "low";
  /** Extracted error message text */
  message: string;
  /** File name if parseable */
  file?: string;
  /** Line number if parseable */
  line?: number;
  /** The original matched line for dedup */
  raw_line: string;
}

interface MistakePattern {
  regex: RegExp;
  category: DetectedMistake["category"];
  severity: DetectedMistake["severity"];
  extract: (match: RegExpMatchArray) => Partial<DetectedMistake>;
}

const MISTAKE_PATTERNS: MistakePattern[] = [
  // ── Rust compile errors ──
  {
    regex: /^error\[E(\d{4})\]:\s*(.+)/,
    category: "compile_error",
    severity: "high",
    extract: (m) => ({ message: `Rust E${m[1]}: ${m[2]}` }),
  },
  {
    regex: /^error:\s*(.+)/,
    category: "compile_error",
    severity: "high",
    extract: (m) => ({ message: m[1] }),
  },
  // ── TypeScript compile errors ──
  {
    regex: /^(.+\.ts\(\d+,\d+\)):\s*error TS(\d+):\s*(.+)/,
    category: "compile_error",
    severity: "high",
    extract: (m) => ({ file: m[1], message: `TS${m[2]}: ${m[3]}` }),
  },
  {
    regex: /error TS(\d+):\s*(.+)/,
    category: "compile_error",
    severity: "high",
    extract: (m) => ({ message: `TS${m[1]}: ${m[2]}` }),
  },
  {
    regex: /Cannot find name\s+['"](.+)['"]/,
    category: "compile_error",
    severity: "high",
    extract: (m) => ({ message: `Cannot find name '${m[1]}'` }),
  },
  // ── Test failures ──
  {
    regex: /^FAILED\s+(.+)/,
    category: "test_failure",
    severity: "high",
    extract: (m) => ({ message: `Test FAILED: ${m[1]}` }),
  },
  {
    regex: /(\d+)\s+test(s)?\s+failed/,
    category: "test_failure",
    severity: "high",
    extract: (m) => ({ message: `${m[1]} test(s) failed` }),
  },
  {
    regex: /panic!|panicked at/i,
    category: "test_failure",
    severity: "high",
    extract: (m) => ({ message: m[0] }),
  },
  {
    regex: /AssertionError|assert!?\s*\(/,
    category: "test_failure",
    severity: "high",
    extract: (m) => ({ message: m[0] }),
  },
  // ── Runtime crashes ──
  {
    regex: /SIGSEGV|segmentation fault/i,
    category: "runtime_crash",
    severity: "high",
    extract: (m) => ({ message: m[0] }),
  },
  {
    regex: /Uncaught\s+(TypeError|ReferenceError|RangeError|SyntaxError):\s*(.+)/,
    category: "runtime_crash",
    severity: "high",
    extract: (m) => ({ message: `${m[1]}: ${m[2]}` }),
  },
  // ── Lint warnings ──
  {
    regex: /^warning:\s*(.+)/,
    category: "lint_warning",
    severity: "medium",
    extract: (m) => ({ message: m[1] }),
  },
  {
    regex: /eslint.*warning.*['"](.+)['"]/i,
    category: "lint_warning",
    severity: "medium",
    extract: (m) => ({ message: `eslint: ${m[1]}` }),
  },
  // ── API errors ──
  {
    regex: /(429|401|403|500|502|503)\s+(Too Many Requests|Unauthorized|Forbidden|Internal Server Error|Bad Gateway|Service Unavailable)/,
    category: "api_error",
    severity: "medium",
    extract: (m) => ({ message: `${m[1]} ${m[2]}` }),
  },
  {
    regex: /API key invalid|Invalid API key|incorrect api key/i,
    category: "api_error",
    severity: "high",
    extract: (m) => ({ message: m[0] }),
  },
  // ── Privacy leaks ──
  {
    regex: /api_?[Kk]ey\s*[=:]\s*["'][sS][kK]-/,
    category: "privacy_leak",
    severity: "high",
    extract: (_m) => ({ message: "Hardcoded API key detected" }),
  },
  {
    regex: /password\s*[=:]\s*["'][^"']{3,}/,
    category: "privacy_leak",
    severity: "high",
    extract: (_m) => ({ message: "Hardcoded password detected" }),
  },
  {
    regex: /secret\s*[=:]\s*["'][^"']{8,}/,
    category: "privacy_leak",
    severity: "high",
    extract: (_m) => ({ message: "Hardcoded secret detected" }),
  },
  {
    regex: /Bearer\s+[sS][kK]-/,
    category: "privacy_leak",
    severity: "high",
    extract: (_m) => ({ message: "API key exposed in Authorization header" }),
  },
  {
    regex: /console\.log\(.*api_?[Kk]ey/i,
    category: "privacy_leak",
    severity: "medium",
    extract: (_m) => ({ message: "API key logged to console" }),
  },
];

/**
 * Scan terminal output for development mistakes.
 * Returns a list of detected issues with category, severity, and extracted metadata.
 * Pure function — no side effects.
 */
export function detectMistakes(text: string): DetectedMistake[] {
  const lines = text.split("\n");
  const mistakes: DetectedMistake[] = [];

  for (const line of lines) {
    const trimmed = line.trim();
    if (!trimmed || trimmed.length < 5) continue;

    for (const pattern of MISTAKE_PATTERNS) {
      const match = trimmed.match(pattern.regex);
      if (match) {
        const extra = pattern.extract(match);
        // Try to extract file:line from the line itself
        const fileMatch = trimmed.match(/(.+\.\w+):(\d+)/);
        mistakes.push({
          category: pattern.category,
          severity: pattern.severity,
          message: extra.message || trimmed.slice(0, 200),
          file: extra.file || fileMatch?.[1],
          line: extra.line || (fileMatch?.[2] ? parseInt(fileMatch[2], 10) : undefined),
          raw_line: trimmed.slice(0, 300),
        });
        break; // one match per line, first pattern wins
      }
    }
  }

  return mistakes;
}

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

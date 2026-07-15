/**
 * agentRegistry — single frontend source for "which agents can run and how".
 *
 * The authoritative registry lives in the Rust backend (`agent_definition` in
 * runtime.rs, surfaced via the `runtime_get_agent_catalog` command). This
 * module loads it once at startup and answers display-name → runtime-id /
 * adapter lookups everywhere in the UI, so registering a new agent in the
 * backend automatically propagates here — no per-component hardcoded maps.
 *
 * A static fallback covers the window before the catalog resolves (and any
 * catalog failure) so agent selection never breaks.
 */

import { runtimeApi } from "@/lib/tauri-api";
import type { RuntimeAgentCatalogEntry, RuntimeAgentId } from "@/types";

/** Fallback mirror of the backend registry (serde snake_case wire ids). */
const STATIC_RUNTIME_AGENT_IDS: Record<string, RuntimeAgentId> = {
  "Claude Code": "claude_code",
  "Codex": "codex",
  "Gemini CLI": "gemini_cli",
  "Qwen Code": "qwen_code",
  "OpenCode": "open_code",
  "GitHub Copilot CLI": "copilot_cli",
  "Grok Build": "grok",
};

let catalog: RuntimeAgentCatalogEntry[] | null = null;
let loading: Promise<void> | null = null;

/** Loads the backend agent catalog once; safe to call repeatedly. */
export function loadAgentRegistry(): Promise<void> {
  if (!loading) {
    loading = runtimeApi
      .getAgentCatalog()
      .then((entries) => {
        catalog = entries;
      })
      .catch(() => {
        // Keep the static fallback; a later explicit reload may retry.
        loading = null;
      });
  }
  return loading;
}

/** The runtime wire id for an agent display name, or null if not runnable. */
export function getRuntimeAgentId(agentName: string): RuntimeAgentId | null {
  const entry = catalog?.find((candidate) => candidate.name === agentName);
  if (entry) return entry.id as RuntimeAgentId;
  return STATIC_RUNTIME_AGENT_IDS[agentName] ?? null;
}

/** The runtime adapter for an agent display name ("acp" etc.), if known. */
export function getAgentAdapter(agentName: string): string | null {
  return catalog?.find((candidate) => candidate.name === agentName)?.adapter ?? null;
}

/** Whether this agent runs over the universal ACP adapter. */
export function isAcpAgent(agentName: string): boolean {
  const adapter = getAgentAdapter(agentName);
  if (adapter) return adapter === "acp";
  // Fallback while the catalog loads: the ACP-native agents.
  return ["Gemini CLI", "Qwen Code", "OpenCode", "GitHub Copilot CLI", "Grok Build"].includes(agentName);
}

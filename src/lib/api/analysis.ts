/** Auto-split from tauri-api.ts — domain: analysis. Import via "@/lib/tauri-api". */
import { invoke } from "@tauri-apps/api/core";

export interface CodebaseAnalysis {
  path: string; total_files: number; total_lines: number;
  languages: Record<string, number>; largest_files: Array<{ name: string; size_bytes: number }>;
}
export const codeAnalysisApi = {
  analyze: (path: string) => invoke<CodebaseAnalysis>("analyze_codebase", { path }),
};

// Config Backup
export interface BackupEntry {
  name: string; path: string; size_bytes: number; created_at: number;
}

// API Provider Preset
export const apiPresetApi = {
  apply: (presetId: string, apiKey: string) =>
    invoke<string>("apply_api_preset", { presetId, apiKey }),
};

// Architecture Graph
export type NodeType = "file" | "directory" | "module" | "function" | "class" | "interface" | "component" | "hook" | "route" | "config" | "test" | "style" | "asset" | "domain" | "flow" | "external";
export type EdgeType = "contains" | "imports" | "exports" | "calls" | "extends" | "implements" | "depends_on" | "belongs_to" | "configures" | "tests" | "styles";
export type ArchLayer = "api" | "service" | "data" | "ui" | "utility" | "config" | "test" | "infrastructure" | "unknown";


// Skill Library Features

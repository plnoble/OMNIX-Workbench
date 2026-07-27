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
export const configBackupApi = {
  backup: (filePath: string, category: string) =>
    invoke<string | null>("backup_config_file", { filePath, category }),
  list: (category: string) =>
    invoke<BackupEntry[]>("list_backups", { category }),
  restore: (backupPath: string, targetPath: string) =>
    invoke("restore_backup", { backupPath, targetPath }),
};

// API Provider Preset
export const apiPresetApi = {
  apply: (presetId: string, apiKey: string) =>
    invoke<string>("apply_api_preset", { presetId, apiKey }),
};

// Architecture Graph
export type NodeType = "file" | "directory" | "module" | "function" | "class" | "interface" | "component" | "hook" | "route" | "config" | "test" | "style" | "asset" | "domain" | "flow" | "external";
export type EdgeType = "contains" | "imports" | "exports" | "calls" | "extends" | "implements" | "depends_on" | "belongs_to" | "configures" | "tests" | "styles";
export type ArchLayer = "api" | "service" | "data" | "ui" | "utility" | "config" | "test" | "infrastructure" | "unknown";

export interface GraphNode {
  id: string; name: string; node_type: NodeType; path: string; layer: ArchLayer;
  language: string | null; summary: string | null; line_count: number;
  fingerprint: string; complexity: string | null; tags: string[];
}
export interface GraphEdge { source: string; target: string; edge_type: EdgeType; weight: number; }
export interface GraphStats {
  total_files: number; total_lines: number;
  languages: Record<string, number>; layers: Record<string, number>;
  node_count: number; edge_count: number;
}
export interface ArchitectureGraph {
  version: number; project_path: string; project_name: string; generated_at: string;
  nodes: GraphNode[]; edges: GraphEdge[];
  layers: Record<string, string[]>; stats: GraphStats;
}
export const architectureApi = {
  build: (projectPath: string) => invoke<ArchitectureGraph>("build_architecture_graph", { projectPath }),
  save: (graph: ArchitectureGraph) => invoke<string>("save_architecture_graph", { graph }),
  load: (projectName: string) => invoke<ArchitectureGraph>("load_architecture_graph", { projectName }),
  getIgnorePatterns: (projectPath: string) => invoke<string[]>("get_ignore_patterns", { projectPath }),
};

// Skill Library Features

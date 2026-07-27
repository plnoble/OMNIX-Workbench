/**
 * OMNIX Workbench - Typed Tauri IPC API Wrapper (barrel).
 *
 * The implementation was split into domain modules under ./api/ — this file
 * re-exports everything so all existing `@/lib/tauri-api` imports keep
 * working unchanged. Add new APIs in the matching domain module.
 */

export * from "./api/platform";
export * from "./api/system";
export * from "./api/skills";
export * from "./api/office";
export * from "./api/conversations";
export * from "./api/remote";
export * from "./api/content";
export * from "./api/workspace";
export * from "./api/monitor";
export * from "./api/analysis";

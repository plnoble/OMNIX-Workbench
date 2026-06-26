# Current Development State

Project: OMNIX Workbench

## Objective

Complete the full OMNIX Workbench real-gap plan: finish and verify single-Agent runtime, Skills/distillation, Team scheduling, Knowledge/Quick Assistant, fresh Windows packages, and desktop-delivered test/manual documentation.

## Status

The complete real-gap release candidate is implemented and packaged. Claude/Codex single-Agent runtime, real Skill market/fusion approval, evidence-backed distillation, dependency-aware Team scheduling, named Knowledge bases, conservative Quick Assistant capture, responsive desktop chrome, and DPI-aware release QA are connected. TypeScript, Vite, Cargo check, all non-ignored Rust library tests, WiX MSI, NSIS, release launch, full-window visual QA, minimum-window visual QA, and Desktop document delivery pass.

## Current Branch

`feat/msi-packaging-and-refactor-slice`, tracking `origin/feat/msi-packaging-and-refactor-slice`.

The worktree contains extensive pre-existing uncommitted naming, icon, packaging, and refactor changes. Preserve and build on them; do not revert unrelated files. Preserve `.omx/icon-candidates/` as design history.

## Delivery Scope

- Claude Code and Codex only for the first supported runtime milestone.
- Structured runtime events: Claude stream JSON and Codex app-server JSON, with PTY retained only as an explicitly labeled compatibility fallback.
- Session model precedence: session override, Agent binding, then Agent default.
- Direct and plan modes are supported. Goal mode moves to Labs until persistent run/retry/verification exists.
- System CLI takes priority; missing tools are installed to an isolated OMNIX-managed prefix.
- Existing SQLite data, including model providers and API keys, is migrated in place.
- First package checkpoint occurs after the single-Agent workflow passes acceptance. Skills/distillation, Team, and Knowledge/Quick Assistant follow in that order.

## Risks

- Claude and Codex expose different event, approval, model, and resume protocols; unsupported behavior must be shown honestly instead of silently falling back.
- Model compatibility is protocol-specific. Enabled models remain visible, but incompatible routes must be disabled with a reason.
- Runtime processes and database writes must not hold synchronous locks across `await`.
- Full-access permission is per-session and requires explicit confirmation; it must not become a persistent global default.
- Packaging must preserve the current product name, E1 icon, Tauri identifier, and WiX upgrade code.

## Acceptance Criteria

- Detect or install a real Claude Code/Codex CLI without creating mock executables.
- Choose Agent, workspace, model, permission, and direct/plan mode; prove those values reach the runtime.
- Persist user, assistant, tool, approval, error, and terminal/runtime records.
- Start, send, approve, stop, resume, restart the app, and restore the conversation history.
- Show real workspace files, Git branch, and changes instead of placeholder text.
- Stop seeding mock conversations and remove only known mock IDs through an idempotent migration.
- Remove Team simulation actions until real scheduling is implemented.
- Pass TypeScript, Vite, Rust checks/tests, desktop visual smoke tests, and Windows packaging.

## Baseline Verification

On 2026-06-24 before implementation:

- `npx.cmd tsc --noEmit` passed.
- `npm.cmd run build` passed with existing dynamic/static import chunk warnings.
- `cargo check` passed with 126 existing unused/dead-code warnings.
- `prompt_guard::tests::test_injection_detection_data_exfil` passed.

## Next Action

Three fixes/features are implemented in the worktree (not yet in a fresh package), all gate-verified:

1. Codex `thread/start` 5s timeout → broken pipe. Raised to a 30s budget with process-death detection and start-time readiness wait (`runtime_manager.rs`).
2. All Agents shared one chat. Switching Agent now switches to that Agent's own latest conversation; sidebar lists only the active Agent's conversations (`useConversations.ts`, `App.tsx`, `AppSidebar.tsx`).
3. Codex/Claude default model now reaches the runtime (roadmap P1). New `responses_bridge.rs` translates Responses↔Chat in the session gateway so any provider works with Codex; binding/global-default resolution + `is_default` pre-selection in the Work page. Validated end-to-end against real `codex app-server`.

Gates pass: `cargo test --lib` (76 passed, 4 ignored), `cargo check` (90 warnings), `tsc --noEmit`, `npm run build`. Reference projects cloned to gitignored `scratch/`; the full borrowing roadmap (P2 model center, P3 MCP sync, P4 Team board, P5 assistants/skills) is in the approved plan, not yet implemented.

Next: rebuild (dev or release) and have the user (a) re-run the Codex turn and Agent-switch flow, and (b) configure a real Chat-Completions provider (DeepSeek/Volcano) as the Codex default and confirm Codex uses it through the translating gateway. Gemini CLI, OpenCode, goal pursuit remain Labs/future work.

## Borrowing roadmap status (2026-06-25)

- P1 (Codex/Claude default model via translating gateway): done, user-accepted.
- P2 (unified model center: global ☆ Agent default, provider presets; capability icons + health already existed): done, user-accepted.
- Acceptance UX rounds (Codex timeout, per-Agent chat, 对话/工作 split, tab preload, waiting indicator, workspace-required Work, portaled delete dialog, two-default-model naming, ☆ pre-select fix): done, user-accepted.
- P3 (MCP one-config sync to ~/.claude.json + ~/.codex/config.toml, with backup/validate/undo): done. MCP also split into a focused page (removed from Settings).
- P4 (Team board: Worker dependency DAG + status summary over team_get_run_detail): done.
- P5a (office assistant presets), P5b (workspace → skill generation in SkillHub): done.
- P5c (remote access + unattended cron): assessed as already implemented; no filler built.
- Borrowing roadmap P1–P5 complete. **Outstanding: rebuild a fresh Windows MSI/NSIS package** — all P1–P5 work is only in the `tauri dev` build; the Desktop artifacts predate P1. The dev app runs via `npm run tauri dev` (frontend hot-reloads; closing the window stops the dev server).

## Naming Contract

- Product: `OMNIX Workbench`
- Compact UI brand: `OMNIX`
- Descriptor: `多 Agent 开发与协作工作台`
- Package/binary slug: `omnix-workbench`
- Tauri identifier: `com.omnix.workbench`
- Repository: `plnoble/OMNIX-Workbench`
- Provisional icon: E1 from `src-tauri/icons/omnix-workbench-e1.png`

# Development Worklog

## 2026-06-23

- Finalized the product naming hierarchy: `OMNIX Workbench` for product/window/installer/docs, `OMNIX` for compact UI, `多 Agent 开发与协作工作台` as the Chinese descriptor, and `omnix-workbench` for package/crate/binary artifacts.
- Updated Tauri product metadata and identifier, Node/Cargo package names, Rust library crate, web title, license, README, user-visible dock/onboarding copy, and source headers.
- Centralized frontend brand constants in `src/lib/constants.ts`.
- Removed the obsolete unreachable `WorkbenchTab`; renamed the backend `workbench` command domain and run-schema helpers to neutral `runs` naming.
- Added a permanent naming standard to `AGENTS.md` and recorded the architecture decision in `decisions.md`.
- Preserved the existing SQLite location (`~/.omnix/omnix.db`) and stable WiX upgrade code so the identity rename does not reset user data or MSI upgrade continuity.
- Verification passed: `npx.cmd tsc --noEmit`, `npm.cmd run build`, `cargo check`, formal MSI build outside the sandbox, and NSIS build.
- Produced `omnix-workbench.exe`, `OMNIX Workbench_0.1.0_x64_en-US.msi`, and `OMNIX Workbench_0.1.0_x64-setup.exe`; Windows exe metadata reports `OMNIX Workbench`.
- Confirmed current source/configuration contains no retired product/package names and parsed all 19 changed JSON files as UTF-8.
- Renamed the GitHub repository from `plnoble/OMNIX-Development-Tools` to `plnoble/OMNIX-Workbench`, updated local `origin`, and refreshed README clone instructions plus the permanent naming contract.
- Applied the user-selected provisional E1 icon: generated an isolated O+X master, removed the chroma-key background, regenerated all 53 Tauri icons, replaced favicon/header/status-dock branding, and removed Vite/React placeholder assets.
- Detected that the first rebuilt exe still contained the old icon because Cargo reused `resource.lib`; added icon/config `rerun-if-changed` rules to `build.rs`, rebuilt, and verified E1 by parsing PE icon resources directly.
- Added explicit NSIS installer/uninstaller icon configuration and verified the rebuilt NSIS embeds E1.
- MSI rebuild was not run because non-sandbox approval was rejected after the Codex usage allowance was exhausted; the existing MSI is marked stale for this icon slice.

## 2026-06-24

- Resumed the pending E1 MSI task after non-sandbox approval became available.
- `npm.cmd run tauri:build:msi` passed normal WiX ICE validation and generated `OMNIX Workbench_0.1.0_x64_en-US.msi` (11,825,152 bytes, 2026-06-24 08:47:51).
- Decompiled the MSI with WiX `dark.exe`; verified the packaged executable and MSI `ProductIcon` both match the E1 master with approximately 0.248 resize-level image difference.
- Confirmed stable WiX upgrade code `1339290b-b4f1-5e65-b4b9-f8b0141ebc54` and removed temporary verification directories.

## 2026-06-12

- Initialized the agent collaboration protocol for OMNIX Workbench (the local checkout folder still used its historical name at this point).
- Started implementation of the multi-agent workbench refactor. Agreed product direction: Workbench-led all-in-one AI client with Agent + Team + Skill as the main spine, resources as support, and unfinished features visible under Labs.
- Added Workbench backend schema and commands for `WorkspaceRun`, `TeamPlan`, `AgentRun`, and visible `LabFeature` registry.
- Added frontend Workbench and Labs tabs, made Workbench the default entry, merged chat into the Workbench screen, and reshaped navigation into Core / Resources / Labs.
- Added typed frontend Workbench/Labs API wrappers and shared types for runs, assignments, skill packages, sync targets, labs, and resource bindings.
- Added OpenCode to the primary skill-sync adapter registry.
- Fixed `prompt_guard` data-exfil detection for "show me your system prompt".
- Isolated four slow/manual Rust tests with explicit `#[ignore]` reasons so `cargo test --lib` can be a usable baseline.
- Verification passed: `npx.cmd tsc --noEmit`, `npm.cmd run build`, `cargo check`, `cargo test --lib`, and preview visual smoke test.
- Started second refactor slice after AionUi and Cherry Studio review. User feedback rejected the visible Workbench pile-up and requested a cleaner first screen, separate single-agent Work and Team flows, configurable fixed-vs-launcher navigation, a Cherry-like Models center, optional Quick Assistant, and optional multi-knowledge-base RAG for ordinary chat only.
- Completed second refactor slice:
  - Default route changed to `工作`; old `chat` and `workbench` navigation requests normalize to the new single-agent Work entry.
  - Added configurable top-bar/application launcher layout with persistent `ui.navigation.layout`, including fixed, launcher, hidden, and restore-default placements.
  - Reworked Work page into Agent selector plus large auto-growing input, model selector, permission mode, work mode, optional search, optional Knowledge binding for ordinary chat only, and a collapsible workspace panel.
  - Split Models into a focused app page and removed the visible model platform tab from Settings.
  - Added Search and Quick Assistant app pages so those features can live in the launcher instead of the main workflow.
  - Rewrote Agents into a card-based local agent page with detection, install/update actions, version/path display, start-work flow, and model binding drawer.
  - Fixed custom model saving by requiring a selected provider and using provider-bound stable IDs.
  - Updated backend model import IDs and API-key usage so encrypted active keys are decrypted before model fetch and health checks.
  - Verification passed: `npx.cmd tsc --noEmit`, `npm.cmd run build`, and `cargo check`.
  - Visual preview could not be kept alive in this host; Playwright is not installed, so visual QA remains a manual follow-up.
- Applied user UI feedback after desktop testing:
  - Narrower windows no longer show the right workspace panel at `lg`; it now waits until `xl` and uses a smaller width before `2xl`.
  - Left contextual sidebar now shrinks from `w-72` to `w-60` on narrower desktop widths.
  - Work mode changed from a flat segmented control plus duplicate label to a compact dropdown matching the permission selector style.
  - Verification passed: `npx.cmd tsc --noEmit` and `npm.cmd run build`.
- Applied second user UI feedback patch:
  - Removed `Settings` and `Diagnostics` from the configurable application registry because they are fixed title-bar controls.
  - Hid the duplicate Settings theme selector; theme switching now lives only in the title bar.
  - Clarified launcher semantics in the UI: fixed = title bar, collected = app grid, hidden = hidden recovery area.
  - Fixed navigation placement persistence to use the latest layout state, preventing stale clicks from moving another entry unexpectedly.
  - Added front/back ordering controls for title-bar fixed entries.
  - Reworked `SkillHub` height and scroll boundaries so skill content, topology, marketplace, and scan panels no longer overlap in smaller desktop windows.
  - Verification passed: `npx.cmd tsc --noEmit` and `npm.cmd run build`.

## 2026-06-13

- Applied desktop shell consistency and model-center bug patch after user testing:
  - Moved `AppHeader` to the top-level app chrome so title-bar navigation, theme, settings, diagnostics, and preview controls keep a stable position across Work, Agents, Skills, Models, and other pages.
  - Hid the contextual left sidebar outside Work/Team surfaces, so Agents and resource pages no longer show a misleading "work context" rail.
  - Reworked the shared dialog component to follow theme tokens instead of hard-coded dark/accent styling.
  - Reworked workspace selection to use a Windows folder picker command instead of asking the user to manually type a path.
  - Added `pick_directory` Tauri command and `shellApi.pickDirectory()` wrapper for the workspace modal.
  - Fixed Ark/Volces model fetching so Volcano-style API addresses do not fall back to Anthropic/Claude known-model defaults; successful fetch/import now replaces stale model rows for that provider.
  - Verification passed: `npx.cmd tsc --noEmit`, `npm.cmd run build`, and `cargo check`.
- Started third refactor slice:
  - Scope: project protocol/evolution loop, Skill Library furnace/sets/market, model and agent binding bug fixes, assistant template app, and Windows packaging.
  - Root-cause notes: `get_active_models` currently builds invalid `pm.pm.id` SQL; Work input folder button is an ambiguous panel toggle; Agent built-in model options all resolve to the same default binding value.
- Completed third refactor slice implementation:
  - Fixed `get_active_models` by replacing the malformed dynamic `pm.pm.id` SQL with an explicit joined select of enabled platform models.
  - Removed the ambiguous Work input folder button and changed Work model values to provider-qualified model routes.
  - Extended `agent_platform_bindings` with `binding_kind` and `builtin_model`, and split Agent binding into Agent default, Agent official/builtin, and OMNIX enabled model options.
  - Added project protocol Tauri commands and schema for status, init preview, confirmed init, events, archive/distillation, protocol actions, and evolution proposals.
  - Added workspace modal protocol preview/confirmation without overwriting existing project files.
  - Added persistent Skill Sets and a `sync_skill_set_to_tools` command; reorganized Skills into library, furnace, sets, market, and sync sub-sections.
  - Replaced the static-only Skill marketplace surface with real `search_skill_market` search plus preview/import-to-local draft behavior.
  - Added the Assistant resource app backed by `agentTemplateApi.getAll()`, with favorites, copy, and one-click handoff into Work input.
  - Verification passed: `npx.cmd tsc --noEmit`, `cargo check`, and `npm.cmd run build`.
  - Full Tauri bundle built the release exe but failed in WiX MSI `light.exe`; NSIS-only bundle passed and produced `src-tauri/target/release/bundle/nsis/omnix-app_0.1.0_x64-setup.exe`.

## 2026-06-14

- Investigated the MSI packaging failure with a systematic WiX/Windows Installer split:
  - `light.exe` normal validation fails at ICE01/ICE02/ICE09 with Windows Installer Service access errors.
  - `light.exe -sval` links a diagnostic MSI successfully, proving the generated WiX object and file binding are not the root problem.
  - Windows Installer service, `WindowsInstaller.Installer` COM, and VBScript/JScript registry entries are present, but WiX ICE validation still fails from this process.
- Added packaging scripts: `tauri:build`, `tauri:build:msi`, `tauri:build:nsis`, `diagnose:msi`, and `repair:msi-env`.
- Added `scripts/diagnose-msi.ps1` for repeatable MSI diagnostics and `scripts/repair-msi-environment.ps1` for elevated Windows Installer/script-engine repair.
- Added a stable WiX `upgradeCode` to `src-tauri/tauri.conf.json`; generated `main.wxs` now contains `1339290b-b4f1-5e65-b4b9-f8b0141ebc54`.
- Verification:
  - `npx.cmd tsc --noEmit` passed.
  - `cargo check` passed with existing unused/dead-code warnings.
  - `npm.cmd run build` passed.
  - `npm.cmd run diagnose:msi -- -RunSuppressValidation` reproduced normal ICE failure and confirmed `-sval` diagnostic linking succeeds.
  - `npm.cmd run tauri:build:msi` and `npm.cmd run tauri:build` still fail at WiX `light.exe` until elevated repair can run.

## 2026-06-17

- Resolved the long-standing MSI packaging blocker with elevated Windows Installer repair.
  - Ran `npm.cmd run repair:msi-env` from an elevated PowerShell (admin session). First attempt aborted because `scripts/repair-msi-environment.ps1` threw on `msiexec.exe /unregister` leaving `$LASTEXITCODE` empty (`$null -ne 0` true). Fixed `Invoke-Native` to treat an unset exit code as success; re-ran and the full repair completed.
  - `npm.cmd run diagnose:msi` now passes end-to-end: Windows Installer service Running, `WindowsInstaller.Installer` COM present, VBScript/JScript x64+x86 registered, WiX `light.exe` normal validation returns exit code 0 (only non-fatal ICE03/ICE40/ICE57/ICE61 warnings remain).
  - `npm.cmd run tauri:build:msi` produced a validated MSI: `src-tauri/target/release/bundle/msi/omnix-app_0.1.0_x64_en-US.msi` (11.8 MB, today 13:44). The MSI ICE validation access-error blocker is cleared.
  - Recorded the `Invoke-Native` empty-exit-code bug in `error-ledger.md`.

## 2026-06-24

- Started the first "real gap" milestone after a repository-wide truthfulness audit.
- Locked the first deliverable to Claude Code and Codex single-Agent work; Skills/distillation, Team, and Knowledge/Quick Assistant remain ordered follow-up milestones.
- Confirmed current baseline: TypeScript and Vite build pass, `cargo check` passes with 126 existing warnings, and the previously failing prompt-guard regression now passes.
- Confirmed concrete gaps: Work model/permission/mode selections do not consistently reach the runtime, assistant output is not persisted, Codex managed install creates a mock CLI, Team uses a simulation panel, Knowledge selection searches the global pool, and marketplace imports can create placeholder skill content.
- Selected structured adapters (Claude stream JSON, Codex app-server JSON), system-first isolated managed installation, per-session model override precedence, in-place SQLite migration, and a package checkpoint immediately after the single-Agent closure.
- Implemented the first real single-Agent runtime milestone:
  - Added a unified Claude Code/Codex runtime model, structured event normalization, permission/work-mode mapping, model precedence and compatibility, official managed-install metadata, and Windows system-CLI shim normalization.
  - Claude Code uses stream JSON; Codex uses the app-server JSON protocol. PTY remains isolated as legacy compatibility and is no longer presented as structured runtime output.
  - Added persistent `agent_sessions` and `runtime_events`, enriched message metadata/order/status, start/send/approve/stop/resume commands, and app-restart history restoration.
  - Added a session-scoped model gateway with Anthropic/OpenAI Responses routes, primary-key plus failover behavior, and per-key status/latency/error recording without logging secrets.
  - Work now loads protocol-compatible models, confirms full access per session, displays structured approvals, stops superseded processes when configuration changes, and loads a real workspace file tree, Git branch, and changes.
  - Memory distillation preview now loads persisted conversation messages instead of hard-coded mock transcripts or placeholder history.
  - Removed known mock conversation seeding, exact-cleaned the two historical mock IDs, removed the unused Team simulation tree/command, and made Team an explicit pending capability surface.
  - Removed unused `model_provider.rs` and `proxy_middleware.rs` abstractions after moving required behavior into the active runtime and gateway modules.
- Verified local real CLI availability without spending a model request: Claude Code `2.1.179`, Codex `0.139.0`, and a live Codex app-server `initialize` response over persistent stdio.
- Verification passed: `npx.cmd tsc --noEmit`, `npm.cmd run build`, `cargo check`, and `cargo test --lib` (`60 passed`, `0 failed`, `4 ignored`). Runtime-focused tests passed `19/19` before the Windows shim regression test was added. Rust warning count fell from 126 to 89 in `cargo check`.
- A fresh Windows package was not produced. The required non-sandbox Tauri build approval and both available local visual-QA paths were rejected by the host usage/security controls before execution. Existing MSI/release-exe timestamps are 2026-06-24 08:47 and the NSIS timestamp is 2026-06-23 21:46, all predating this runtime milestone.

- Completed the remaining real-gap implementation:
  - Added named knowledge bases, scoped multi-base RAG selection for ordinary chat, citation metadata, and activity-key decryption for direct RAG requests. Workspace and Team flows remain unbound by default.
  - Added conservative Quick Assistant auto-capture, application/window blacklist checks, and native file/folder selection for Knowledge imports.
  - Replaced placeholder Skill market imports with real GitHub `SKILL.md` preview/import and provenance hashes. Added model-selected fusion drafts with explicit conflict output and approve/reject application.
  - Marked Claude/Codex skill sync as verified and Gemini/OpenCode/other layouts as experimental instead of implying complete support.
  - Added a unified distillation inbox. Conversation evidence produces pending memory, skill, and protocol candidates; only approved memory/skill candidates write to their stores.
  - Implemented real Team manager planning through the Claude/Codex runtime, mandatory user approval, dependency/concurrency scheduling, Worker approvals, retries, stop, persistence, and final manager validation.
  - Rebuilt the Memory page around the evidence-backed inbox and removed the legacy auto-write command from the public Tauri surface.
  - Fixed the Status Dock click/drag conflict by limiting drag to its grip, and changed the minimum desktop size from 1024x680 to 860x600 with compact icon-only header labels at narrow widths.
- Verification passed: `npx.cmd tsc --noEmit`, `npm.cmd run build`, `cargo check`, and `cargo test --lib` (`65 passed`, `0 failed`, `4 ignored`).
- Native window inspection confirmed the 150% DPI failure mode: 1024 logical pixels can consume the full 1536 physical-pixel display width. The new minimum and compact header address the root cause.
- Closed the release-candidate integration and high-DPI edge cases:
  - Team stop now observes cancelled runtime sessions within 500 ms instead of waiting for the full turn timeout; manual retry requeues blocked downstream assignments. Added a regression test.
  - Added display-aware initial sizing, a 640x520 minimum, compact navigation/sidebar behavior, and narrow-window workspace-panel defaults.
  - Investigated a suspected WebView2 DPI mismatch. The apparent half-width crop came from a DPI-unaware PowerShell capture process receiving virtualized coordinates; in-process Tauri `inner_size` and Win32 `GetClientRect` both reported the full 2560-pixel client width. Kept a defensive native/reported-width correction for genuine mismatches, but removed the temporary frontend zoom permission and diagnostic title code.
  - Added a short-height Work welcome state that hides secondary copy/cards while preserving the title, composer, model, permission, mode, knowledge, search, and send controls.

## 2026-06-25

- Completed final verification after the short-window adjustment: `npx.cmd tsc --noEmit`, `npm.cmd run build`, `cargo check`, and `cargo test --lib` all pass; Rust tests report 70 passed, 0 failed, and 4 ignored.
- `cargo check` reports 90 known legacy warnings, down from the 126-warning baseline.
- `npm.cmd run tauri -- build` passed normal WiX validation and NSIS generation. Fresh artifacts were timestamped 2026-06-25 06:24-06:25.
- Launched the release exe and used a Per-Monitor-V2 native capture process. The full 2586x1655 physical window and the 1280x1040 physical / 640x520 logical minimum window both show complete non-overlapping controls.
- Created `docs/OMNIX-Workbench-0.1.0-发布清单.md` with sizes, SHA256 values, gate results, real capabilities, and explicit boundaries.
- Copied MSI, NSIS, release exe, complete detection plan, user manual, and release manifest to the Desktop `OMNIX Workbench 0.1.0 测试版` folder.
- Began user acceptance (round A). Verified worktree health (`tsc` clean, release exe present, Desktop delivery intact), real CLIs on PATH (Claude Code `2.1.179`, Codex `0.139.0`), and launched the release exe successfully.
- Acceptance found two real bugs; root-caused and fixed both:
  - Codex session start failed with "did not return a thread id within 5 seconds" then "管道正在被关闭 (os error 232)". Reproduced against the real `codex app-server` and inspected the authoritative protocol schema (`generate-json-schema`): method names (`thread/start`/`turn/start`) and the `result.thread.id` response shape were already correct. Real cause: Codex 0.139.0 boots the user's `~/.codex/config.toml` MCP servers synchronously during `thread/start`, so the response arrived after the hard-coded 5s budget (observed ~6s with 6 MCP servers, several failing with os error 267). Raised the budget to 30s (`CODEX_THREAD_START_TIMEOUT`), added eager child-process liveness detection so a dead Codex yields an actionable error instead of a broken-pipe write, moved the readiness wait into `launch_session` so failures surface at start and the first message is instant, and pointed the error text at the MCP config.
  - All Agents shared one chat thread because the Work-page Agent strip only flipped `activeAgent` while keeping the open conversation. Per user decision (Model A), switching Agent now switches to that Agent's own latest conversation via a new `selectAgent` action; each Agent keeps independent history; the work-context sidebar lists only the active Agent's conversations; new conversations bind to the active Agent.
  - Verification passed: `cargo check` (90 known warnings), `cargo test --lib runtime` (22 passed), `npx.cmd tsc --noEmit`, and `npm.cmd run build`.
- Studied three reference projects (cc-switch, AionUi, Cherry Studio) and wrote a phased borrowing roadmap; landed roadmap phase P1 (Codex/Claude default model truly reaches the runtime):
  - Root cause was two-layered: (1) the Work-page model selector defaulted to "Agent 官方默认" which sends `model: null`, so Codex used its own `gpt-5.5/openai`; (2) the session gateway hard-rejected non-Responses providers and did no protocol translation, while Codex 0.139 only emits `wire_api="responses"` and most user providers (DeepSeek/Volcano) only speak Chat Completions.
  - Captured a real Codex Responses request from a live `codex app-server` turn to shape the translator precisely (fixture `src-tauri/src/fixtures/codex_responses_request.json`).
  - Added `responses_bridge.rs`: Responses→Chat request translation and a streaming Chat→Responses SSE translator; wired into `proxy.rs handle_responses_for_session` (native Responses providers still forwarded verbatim; Chat providers translated to `/chat/completions`).
  - `evaluate_model_compatibility` now marks Chat-Completions providers as Gateway-selectable for Codex.
  - Resolution: `runtime_start_session` and `load_runtime_model_options` resolve Agent binding → global `default_model` setting → Agent default, and mark the default option (`is_default`); the Work page now pre-selects it.
  - Validated end-to-end against the real `codex app-server`: Codex accepted the translator's exact Responses SSE output, completed the turn, and surfaced the assistant text. Request translation, streaming text, tool calls, and non-streaming fallback are unit-tested.
  - Verification passed: `cargo test --lib` (76 passed, 4 ignored), `cargo check` (90 known warnings), `npx.cmd tsc --noEmit`, `npm.cmd run build`. Reference repos cloned to gitignored `scratch/`.
- Acceptance UX round after P1 (frontend only):
  - Codex empty model list: added an inline hint in the Work composer when the active Agent has custom models but none are selectable (e.g. only Anthropic providers enabled for Codex), explaining Codex needs an OpenAI-protocol provider. Root cause was config, not code — the user's only enabled provider is Anthropic-typed (Volcano Ark `/api/coding`), which Codex can't use; the gateway translates Codex↔OpenAI-Chat, not Codex↔Anthropic.
  - Split 对话 (Chat) and 工作 (Work) into two distinct top-level surfaces (user choice). A conversation is Chat when unbound (`workspace_path = "direct"`) and Work when bound to a workspace. New `chat` registry entry (default surface), `surface` prop on `ChatTab`, `enterSurface`/surface-aware `selectAgent` in `useConversations`, sidebar lists each surface's own conversations, and Work now requires selecting a workspace before sending (CTA + disabled send).
  - Tab-switch lag: preload lazy tab chunks on idle in `App.tsx` so switching no longer flashes the Suspense loader.
  - First-token wait: added a "正在启动 … 并等待响应" indicator (`startingConversations` in the hook) shown from send until the first runtime event, covering the Codex thread/start MCP-boot delay.
  - Verification passed: `npx.cmd tsc --noEmit`, `npm.cmd run build`.
- Fixed two more acceptance findings on the new Work surface: 工作 no longer auto-defaults a stale workspace (`newConversation` and surface entry reset to `direct`, and 工作 no longer silently reopens a previous workspace; "新工作会话" opens the workspace picker), and the delete-confirm dialog no longer overflows the sidebar (it was `fixed` inside a `backdrop-blur` aside that became its containing block — now portaled to `document.body`). `tsc` + `build` pass.
- Landed roadmap **P2 (统一模型中心)**:
  - Added a global default-model picker (the keystone tying to P1): a ☆ on each enabled model in `PlatformSubTab` sets `default_model` (`platform_id:model_name`) via `settingsApi`, with a header badge showing the current default. The runtime already falls back to it (Agent binding → global default → Agent default) and the Work page pre-selects it.
  - Added ZCF-style provider presets to `PlatformModal`: a quick-preset selector prefills protocol + endpoint (DeepSeek, 火山 OpenAI-兼容, OpenAI, Anthropic, OpenRouter, SiliconFlow, GLM, Kimi, 百炼, Ollama, LM Studio) so users don't mis-configure (the user's earlier Volcano-as-Anthropic mistake). Only the API Key is left to enter.
  - Capability icons (vision/reasoning/coding/tools/…) and health-check visualization already existed in `PlatformSubTab`; P2 completes the Cherry-style model center.
  - Verification passed: `npx.cmd tsc --noEmit`, `npm.cmd run build`. Deferred: surfacing provider presets that call `apply_api_preset` directly (form-prefill chosen instead to avoid reload plumbing).
- Fixed the P2 follow-up acceptance findings:
  - The Work-page model selector did not pre-select the Agent default after switching Agents (e.g. set a ☆ default, switch to Claude Code, still showed "Agent 官方默认"). Root cause: the selector preserved the previous Agent's `selectedModelId`, and the shared `agent_default` option id is valid for every Agent, so it masked the new Agent's `is_default`. Fixed by always selecting the Agent's default on Agent switch.
  - Clarified that the new Models ☆ default (`default_model`) and the existing Settings "默认大模型" (`target_model`) are NOT duplicates — `target_model` drives OMNIX's built-in features (划词翻译/语言检测/知识库, via the general proxy gateway), `default_model` drives the Agent runtime when no per-Agent binding exists (via the session gateway). Renamed Settings → "内置功能默认模型" with a clarifying description, and relabeled the Models ☆ controls as "Agent 默认". Kept both (deleting `target_model` would break translation/QA).
  - Verification passed: `npx.cmd tsc --noEmit`, `npm.cmd run build`.
- Landed roadmap **P4 (Team 协作看板)** v1:
  - Added `src/components/TeamGraph.tsx` — a deterministic layered (topological) DAG of a Team run's Workers, colored by live status (longest-path depth layout, bezier dependency edges, pulsing running nodes), backed entirely by `team_get_run_detail`'s existing worker data (no new backend).
  - Integrated into `TeamTab`: renamed the section to 协作看板, added a per-status count summary and the dependency graph above the worker list; clicking a graph node highlights and scrolls to that Worker card. Live updates ride the existing 1.5s detail poll.
  - Deferred honestly (no fake panel): an Agent-to-Agent mailbox panel — `team_runtime` does not currently populate the `aionui.rs` mailbox, so that is a separate follow-up rather than a placeholder.
  - Verification passed: `npx.cmd tsc --noEmit`, `npm.cmd run build`. The visual board needs a real Team run (manager planning spends a model request / user credentials).
- Landed roadmap **P3 (MCP 一次配置全同步)**:
  - Added `src-tauri/src/commands/mcp_sync.rs` (+ `toml_edit = "0.22"` dependency) — syncs OMNIX-managed MCP servers into the agents' native config: Claude `~/.claude.json` `mcpServers` (serde_json merge) and Codex `~/.codex/config.toml` `[mcp_servers.<name>]` (toml_edit, preserving comments/other tables/model_providers). Commands: `mcp_sync_to_agents`, `mcp_remove_from_agent`, `mcp_get_agent_states`.
  - Safety (these are the user's real config files — a bad MCP entry previously slowed Codex's thread/start): backs up via `crate::backup::backup_file` before writing; merge-only (upsert by name, never bulk-delete); preserves all other config; re-parses rendered output before replacing; atomic temp-file+rename write. Codex stdio-only (remote http/sse servers reported as skipped).
  - Frontend: `mcpSyncApi` wrappers + `McpSubTab` per-server controls — a 同步 button (writes both agents), per-agent ✓/— sync-state chips loaded from `mcp_get_agent_states`, and click-to-撤销.
  - Verification: `cargo test --lib mcp_sync` 3 passed (TOML merge preserves existing config + adds stdio server; Claude stdio vs remote spec; remote skipped for Codex); `npx.cmd tsc --noEmit`, `npm.cmd run build` pass. Live write to a real agent config still needs user testing.
- Made MCP a focused page (user feedback): opening MCP from the title bar previously rendered the full `SettingsTab` (System + Backup tabs visible, looked like Settings). Exported `McpSubTab`, added focused `src/components/tabs/McpTab.tsx` (mirrors ModelsTab), routed `activeTab === "mcp"` to it, and removed the `mcp` sub-tab from the Settings tab bar (Settings now only system + backup). `tsc` + `build` pass.
- Landed roadmap **P5a (助手模板库预设)**:
  - The assistant library was already rich (47 built-in templates via `agent_templates.rs::get_all_templates`, surfaced by `AssistantsTab` with category filter/search/favorites/copy/one-click handoff into Work). The genuine gap vs AionUi/Cherry was document-creation assistants.
  - Added a new 办公 (Office) category with 6 assistants — PPT 制作, Word 文档撰写, Excel 表格/数据, 学术论文写作, 会议纪要, 周报/工作汇报 — the first three reference this project's existing pptx/docx/xlsx skills.
  - Verification: `cargo check` passes (no errors). New templates surface in the Assistants page (53 total).
- Landed roadmap **P5b (Skills 生成)**:
  - The workspace-skill-generation backend already existed but had no UI (`scan_workspace_for_skills` / `generate_skill_from_files` via `skillGeneratorApi`, plus a registered-but-unwrapped `create_skill`). Added a `create` wrapper to `skillLibraryApi` and a new "生成" section to `SkillHub`: pick workspace → scan files → select + name → generate a SKILL.md draft → review → save as a local skill (or copy).
  - Verification: `npx.cmd tsc --noEmit`, `npm.cmd run build` pass.
- **P5c (远程/定时)**: assessed as already implemented — not building filler. Remote cross-device access (`get_remote_access_info` → Dashboard "远程跨设备调试") and unattended cron (`CronTab` with create/edit/toggle/delete/history + an existing "立即运行" manual trigger via `trigger_cron_task`) both already exist and work. The three-tier skill source idea (builtin/custom/extension) was not forced because skills only carry a `source_type` flag, not a meaningful three-tier taxonomy; the generation feature delivers the concrete P5b value instead.
- Borrowing roadmap P1–P5 complete (P5c was already present). Note: a fresh Windows MSI/NSIS package has NOT been rebuilt since P1; all P1–P5 work is currently only in the `tauri dev` build.

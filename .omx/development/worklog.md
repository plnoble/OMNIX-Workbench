# Development Worklog

## 2026-06-12

- Initialized the agent collaboration protocol for `OMNIX-Development Tools`.
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

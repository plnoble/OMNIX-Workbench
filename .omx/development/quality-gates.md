# Quality Gates

Use this file before risky changes, handoff, delivery, or release. Apply only relevant categories and mark the rest as N/A.

Statuses:

- Pass: verified with evidence.
- Warn: partial coverage or residual risk.
- Fail: issue must be fixed or explicitly deferred.
- N/A: not relevant to this project or change.

## Pre-Change Gate

- Objective, dependencies, risks, impact scope, and acceptance criteria are clear.
- Existing worktree state has been inspected.
- Affected files or modules are identified.
- Verification approach is known before implementation starts.
- Security, privacy, data, API, frontend, or release-sensitive areas are flagged when relevant.

## Pre-Delivery Gate

### Security and Privacy

- Inputs are validated and outputs are encoded where applicable.
- Secrets, tokens, passwords, and PII are not logged or committed.
- Authentication and authorization changes are reviewed for least privilege.
- File paths, uploads, system commands, and outbound requests are checked for injection or traversal risk.

### Data, API, and Consistency

- Schema changes use migrations where applicable.
- API shape, versioning, validation, pagination, and idempotency are reviewed when applicable.
- Money uses integer minor units or decimal types, never binary floating point.
- Time storage and transfer use UTC when the project handles time-sensitive data.

### Code Quality and Maintainability

- The change follows existing project patterns.
- Duplication, dead code, broad types, empty catches, and unhandled TODO/FIXME/HACK comments are reviewed.
- Error handling is explicit and does not silently swallow failures.
- Configuration is separated from code and required config fails fast.

### Testing and Verification

- Unit, integration, E2E, or manual verification is selected according to risk.
- Boundary cases and regression scenarios are covered where relevant.
- Verification commands and results are recorded in `current.md` or `handoff.md`.
- Known unverified areas are stated explicitly.

### Frontend, Accessibility, and UX

- Loading, empty, error, and boundary states are handled when UI is affected.
- Keyboard access, labels, alt text, focus visibility, and color contrast are checked when UI is affected.
- Mobile layout and user-visible text are checked when frontend behavior changes.

### Operations, Dependencies, and Release

- Dependency changes are reviewed for lockfiles, unused packages, vulnerabilities, and licenses.
- Logs, metrics, health checks, retries, timeouts, graceful shutdown, and rate limits are considered when relevant.
- README, API docs, ADRs, changelog, release notes, or migration notes are updated when needed.
- Rollback or recovery path is known for risky delivery.

## Gate Result Template

### YYYY-MM-DD - Gate name

| Category | Status | Evidence | Notes |
| --- | --- | --- | --- |
| Security and privacy | N/A |  |  |
| Data, API, and consistency | N/A |  |  |
| Code quality and maintainability | N/A |  |  |
| Testing and verification | N/A |  |  |
| Frontend, accessibility, and UX | N/A |  |  |
| Operations, dependencies, and release | N/A |  |  |

Open issues:

- None.

### 2026-06-12 - Pre-change gate for second workspace shell refactor

| Category | Status | Evidence | Notes |
| --- | --- | --- | --- |
| Security and privacy | Warn | Review scope includes model API keys and selection assistant settings. | Must avoid logging secrets and keep key storage semantics explicit. |
| Data, API, and consistency | Warn | Existing generic `settings` table and platform/model tables will be reused. | Navigation layout can be stored in settings; model ID/save logic needs validation. |
| Code quality and maintainability | Warn | Worktree inspected with `git status --short`; affected modules identified in `current.md`. | Existing first-slice modified files must be preserved and refactored, not reverted. |
| Testing and verification | Pass | Planned checks: `npx.cmd tsc --noEmit`, `npm.cmd run build`, `cargo check`. | `cargo test --lib` is not required for this UI-heavy slice unless Rust changes become broad. |
| Frontend, accessibility, and UX | Warn | Change targets main shell, navigation, chat input, Models, Agents, Knowledge entry. | Must verify no text overflow and no card-within-card clutter after implementation. |
| Operations, dependencies, and release | N/A | No dependency addition planned. |  |

Open issues:

- Visual verification may be limited if the desktop app cannot be launched in this environment.

### 2026-06-12 - Pre-delivery gate for second workspace shell refactor

| Category | Status | Evidence | Notes |
| --- | --- | --- | --- |
| Security and privacy | Pass | Model API keys remain stored through existing encrypted `platform_api_keys`; request paths now decrypt active keys before model fetch/check. | No new secret logging added. |
| Data, API, and consistency | Pass | Navigation layout persists through `settingsApi`; model IDs now include provider binding. | SQLite migration compatibility is not guaranteed by plan, but schema changes were not required in this slice. |
| Code quality and maintainability | Pass | New shell registry/hook split: `appRegistry.tsx`, `useNavigationLayout.ts`, focused tabs for Models/Search/Quick Assistant/Agents. | `SettingsTab` still contains legacy subpage code and should be split further in later slices. |
| Testing and verification | Pass | `npx.cmd tsc --noEmit`, `npm.cmd run build`, and `cargo check` passed. | `cargo check` retains existing unused/dead-code warnings. |
| Frontend, accessibility, and UX | Warn | Main new files were checked with UTF-8 reads for Chinese UI text. | Automated screenshot/visual QA not completed because Playwright is not installed and background preview server did not stay alive in this host. |
| Operations, dependencies, and release | Pass | No dependency additions. Attempted preview server startup did not leave a listening port. | User should launch local dev/Tauri app for visual review. |

Open issues:

- Manual visual QA still needed for light/dark themes and common desktop widths.
- MCP still reuses the Settings component as a focused route; a dedicated MCP page should be split later.

### 2026-06-13 - Pre-delivery gate for app-grid and skill-page UI feedback

| Category | Status | Evidence | Notes |
| --- | --- | --- | --- |
| Security and privacy | N/A | Frontend shell/layout-only change. | No API-key, CORS, auth, or clipboard capture behavior changed. |
| Data, API, and consistency | Pass | `useNavigationLayout` now normalizes/persists latest layout state via a ref-backed updater. | Existing saved `settings`/`dashboard` launcher IDs are dropped by registry normalization because they are no longer app-grid entries. |
| Code quality and maintainability | Warn | `AppHeader` and `appRegistry` were rewritten to remove encoded legacy text and clarify product semantics. | `SettingsTab` still has transitional legacy code; later slice should fully split/remove old Settings subpages instead of hiding pieces. |
| Testing and verification | Pass | `npx.cmd tsc --noEmit` and `npm.cmd run build` passed. | Vite emitted existing dynamic/static import chunking warnings only. |
| Frontend, accessibility, and UX | Warn | App grid now explains fixed/collected/hidden, title-bar entries can reorder, duplicate Settings/Diagnostics launcher cards are gone, and Skill page has bounded scroll regions. | Automated screenshot QA still not run; user should verify in the desktop app. |
| Operations, dependencies, and release | Pass | No dependency changes. | No release packaging performed. |

Open issues:

- Manual visual QA still needed for the Skill page at the user's minimized desktop window size.
- Settings still carries legacy hidden theme-selector code pending a deeper Settings split.

### 2026-06-13 - Pre-delivery gate for shell consistency, workspace picker, and Ark model fetch

| Category | Status | Evidence | Notes |
| --- | --- | --- | --- |
| Security and privacy | Warn | Added a Windows folder picker command using PowerShell/Windows Forms; no secrets are logged. | The command only returns a selected directory path. It still depends on local PowerShell availability. |
| Data, API, and consistency | Pass | Ark/Volces endpoint detection now bypasses Anthropic fallback and successful model import deletes stale `platform_models` rows for the provider before inserting refreshed rows. | This intentionally changes refresh semantics to "replace provider model list" after a non-empty fetch/catalog result. |
| Code quality and maintainability | Warn | Header is now app-level chrome and non-work pages no longer render the contextual sidebar. | `App.tsx` still has transitional shell code from prior slices and should be cleaned after the layout stabilizes. |
| Testing and verification | Pass | `npx.cmd tsc --noEmit`, `npm.cmd run build`, and `cargo check` passed. | `cargo check` still reports existing unused/dead-code warnings. |
| Frontend, accessibility, and UX | Warn | Dialog primitives now follow theme tokens, workspace path is selected through a folder picker, and non-work pages get more space. | Native folder picker and visual layout still need manual Tauri desktop verification. |
| Operations, dependencies, and release | Pass | No new npm/cargo dependencies. | Uses built-in Windows PowerShell and Windows Forms. |

Open issues:

- Manual desktop verification needed for the native folder picker and title-bar alignment at the user's window sizes.
- The model-center fallback catalog for Ark/Volces is conservative; a later slice should call provider-specific model-list APIs if available for the configured account.

### 2026-06-14 - Pre-delivery gate for MSI packaging follow-up

| Category | Status | Evidence | Notes |
| --- | --- | --- | --- |
| Security and privacy | Pass | New scripts do not log secrets; repair script only touches Windows Installer and script-engine registration. | Elevated repair is explicit and not hidden behind normal build commands. |
| Data, API, and consistency | Pass | `src-tauri/tauri.conf.json` now pins WiX `upgradeCode` to `1339290b-b4f1-5e65-b4b9-f8b0141ebc54`. | No SQLite or product API changes. |
| Code quality and maintainability | Pass | Added focused npm scripts and two small PowerShell scripts for diagnosis/repair. | Scripts are Windows-specific by design because the failure is Windows MSI-specific. |
| Testing and verification | Warn | `npx.cmd tsc --noEmit`, `cargo check`, `npm.cmd run build`, and `npm.cmd run diagnose:msi -- -RunSuppressValidation` completed. | `npm.cmd run tauri:build:msi` and `npm.cmd run tauri:build` still fail at WiX `light.exe` until elevated repair runs. |
| Frontend, accessibility, and UX | N/A | Packaging-only change. | No UI changed. |
| Operations, dependencies, and release | Warn | Release exe is regenerated; formal MSI remains blocked; stale MSI is still older than current build. | Do not publish `diagnostic-sval.msi`; use elevated repair then rebuild MSI. |

Open issues:

- Need elevated PowerShell to run `npm.cmd run repair:msi-env`.
- Need rerun `npm.cmd run tauri:build:msi` after repair and confirm a current-date formal MSI exists.

### 2026-06-23 - Pre-delivery gate for product naming migration

| Category | Status | Evidence | Notes |
| --- | --- | --- | --- |
| Security and privacy | Pass | Database remains at `~/.omnix/omnix.db`; no credential or network behavior changed. | Tauri identifier changed, but the database path is identifier-independent. |
| Data, API, and consistency | Pass | Canonical naming is recorded in `AGENTS.md`; package, crate, Tauri, UI, docs, and artifact metadata agree. | Stable MSI upgrade code is preserved. |
| Code quality and maintainability | Pass | Removed unreachable `WorkbenchTab`; renamed backend feature bucket to neutral `runs`; retired-name source scan is clean. | Existing Rust dead-code warnings remain baseline debt. |
| Testing and verification | Pass | TypeScript, Vite build, Cargo check, UTF-8 JSON parse, diff check, MSI, and NSIS builds passed. | Sandboxed MSI reproduced the known ICE access boundary; approved non-sandbox validation passed. |
| Frontend, accessibility, and UX | Pass | Compact UI brand remains `OMNIX`; formal product surfaces use `OMNIX Workbench`. | No layout behavior changed beyond removal of an unreachable page. |
| Operations, dependencies, and release | Pass | New MSI, NSIS, and exe artifacts exist with current timestamps and correct names. | Repository name remains historical infrastructure until explicitly renamed. |

Open issues:

- Application icon is not finalized; `.omx/icon-candidates/` is intentionally preserved for the next branding decision.

### 2026-06-23 - Pre-delivery gate for provisional E1 icon rollout

| Category | Status | Evidence | Notes |
| --- | --- | --- | --- |
| Security and privacy | N/A | Branding assets and build metadata only. | No user data, credentials, or network behavior changed. |
| Data, API, and consistency | Pass | One RGBA master generated all Tauri variants; web and in-app surfaces use `/omnix-workbench-icon.png`. | E1 is explicitly provisional. |
| Code quality and maintainability | Pass | `build.rs` tracks icon/config inputs; NSIS icon paths are explicit. | Prevents stale Windows resource reuse. |
| Testing and verification | Warn | TypeScript, Vite, NSIS, alpha checks, and direct PE extraction passed. | Validated MSI rebuild is pending non-sandbox approval. |
| Frontend, accessibility, and UX | Pass | Decorative brand images use empty alt text and fixed dimensions; status remains visible as an overlay. | Manual desktop viewing is still useful at different display scales. |
| Operations, dependencies, and release | Warn | Release exe and NSIS embed E1; 53 Tauri icon files generated. | Current MSI predates E1 and must not ship as the icon-updated package. |

Open issues:

- Rebuild and verify MSI through the approved non-sandbox WiX path when available.

### 2026-06-24 - E1 MSI closure gate

| Category | Status | Evidence | Notes |
| --- | --- | --- | --- |
| Security and privacy | N/A | Packaging verification only. | No user data or credentials changed. |
| Data, API, and consistency | Pass | MSI product name and upgrade code match the naming contract. | Existing upgrade continuity is preserved. |
| Code quality and maintainability | Pass | Icon inputs are tracked by `build.rs`; MSI uses the generated E1 icon set. | Future icon replacements trigger Windows resource rebuilds. |
| Testing and verification | Pass | Standard WiX build exit 0; `dark.exe` extraction and PE/icon comparison passed. | No suppressed-validation artifact was used. |
| Frontend, accessibility, and UX | N/A | No additional UI change in this closure step. | Prior favicon/header/status-dock verification remains applicable. |
| Operations, dependencies, and release | Pass | Current MSI is 11,825,152 bytes with timestamp 2026-06-24 08:47:51. | MSI, NSIS, and release exe now all correspond to E1. |

Open issues:

- E1 remains a provisional brand decision pending future user confirmation.

### 2026-06-24 - Pre-change gate for single-Agent real-gap milestone

| Category | Status | Evidence | Notes |
| --- | --- | --- | --- |
| Security and privacy | Warn | Scope includes API-key routing, process arguments, approvals, workspace files, and full-access mode. | Never log decrypted keys; full access is per-session and explicitly confirmed. |
| Data, API, and consistency | Warn | Existing `conversations`, `messages`, bindings, provider/key tables, and known mock IDs were inspected. | Use idempotent migrations and preserve user model/key data. |
| Code quality and maintainability | Warn | `git status --short` shows extensive pre-existing changes; runtime currently spans hooks, PTY manager, proxy, and large command modules. | Keep edits scoped and do not revert naming/icon/packaging work. |
| Testing and verification | Pass | Baseline commands passed; TDD sequence and adapter/session tests are defined in `current.md`. | Each new runtime behavior begins with a failing test. |
| Frontend, accessibility, and UX | Warn | Work UI is the primary affected surface; current workspace panel and Team simulation contain placeholder behavior. | Preserve the clean first screen and add explicit loading/error/unsupported states. |
| Operations, dependencies, and release | Warn | Real CLI install/update and Windows packages are in scope. | Prefer system CLI, isolate managed installs, preserve MSI identity and E1 resources. |

Open issues:

- Codex app-server is version-sensitive; detect capabilities and fail visibly when unavailable.
- Live model/Agent smoke tests may require user credentials or network access; deterministic fake-executable tests remain mandatory.

### 2026-06-24 - Single-Agent real-gap implementation gate

| Category | Status | Evidence | Notes |
| --- | --- | --- | --- |
| Security and privacy | Pass | API keys remain encrypted at rest, are decrypted only for requests, are never emitted in runtime events, and full access requires per-session confirmation. | Primary Key plus failover attempts record status/latency/error only. |
| Data, API, and consistency | Pass | Idempotent session/event/message/key migrations; exact mock-ID cleanup; model compatibility and precedence covered by tests. | Existing provider, key, conversation, and skill data are preserved. |
| Code quality and maintainability | Pass | Structured `runtime/` and session gateway are active; unused provider/middleware abstractions and Team simulator were removed. | `cargo check` warnings reduced from 126 to 89; remaining warnings are known legacy debt. |
| Testing and verification | Pass | TypeScript, Vite build, Cargo check, and all lib tests passed: 60 passed, 0 failed, 4 ignored. Real CLI version checks and Codex app-server initialize passed. | No paid/live model turn was sent. |
| Frontend, accessibility, and UX | Warn | Work is chat-first, incompatible models expose reasons, approvals are structured, ordinary chat hides the workspace panel, and Team is truthfully pending. | Automated light/dark and width screenshots were blocked by host execution/browser policy. |
| Operations, dependencies, and release | Warn | Product metadata, E1 icons, MSI identity, and packaging scripts remain intact. | Fresh MSI/NSIS/release exe could not be generated because the required execution approval was rejected before build start. |

Open issues:

- Run a fresh full Tauri bundle as soon as non-sandbox execution approval is available; do not distribute the pre-runtime artifacts as this milestone.
- Complete native desktop visual checks at 1024, 1280, and 1365 widths in both themes.
- User acceptance of real Claude/Codex turns is required before starting Skills/distillation.

### 2026-06-24 - Full real-gap pre-package gate

| Category | Status | Evidence | Notes |
| --- | --- | --- | --- |
| Security and privacy | Pass | Runtime keys remain encrypted; Quick Assistant blacklists both manual and auto capture; full access stays session-scoped. | No secrets appear in logs or artifacts. |
| Data, API, and consistency | Pass | Named Knowledge bindings, evidence-backed distillation inbox, real Team states, Worker retry/cancel behavior, and model compatibility are persisted and covered by focused tests. | SQLite migrations remain idempotent and existing providers/keys are preserved. |
| Code quality and maintainability | Pass | Work, Team, Skills, Knowledge, memory/distillation, runtime, and model gateway use active domain APIs rather than mock UI. | `cargo check` warnings are 90 versus the 126-warning baseline; remaining items are legacy debt. |
| Testing and verification | Pass | `npm.cmd run build`, `cargo check`, and `cargo test --lib` passed. | One parallel Vite gate failed transiently; the isolated canonical build passed and the race is recorded. |
| Frontend, accessibility, and UX | Warn | Compact header/sidebar, overlay workspace panel, monitor-fit sizing, and a guarded native/reported-width correction are implemented. | Final DPI-aware native screenshot still required at this gate. |
| Operations, dependencies, and release | Warn | A standard validated MSI/NSIS/release bundle succeeded before the last visual adjustment. | Final bundle and Desktop document copy still required at this gate. |

Open issues:

- Closed by the 2026-06-25 final package gate below.

### 2026-06-25 - Final real-gap package gate

| Category | Status | Evidence | Notes |
| --- | --- | --- | --- |
| Security and privacy | Pass | Keys stay encrypted, full access is session-scoped, Quick Assistant uses blacklist checks, and no temporary frontend zoom capability remains. | No secrets are present in release records. |
| Data, API, and consistency | Pass | Idempotent schema, persisted Agent/Team/Skill/Knowledge/distillation states, explicit compatibility, and primary-key failover tests. | Existing user providers and keys are preserved. |
| Code quality and maintainability | Pass | Active runtime/domain APIs replace mock paths; protocol records and user docs are current. | 90 legacy warnings remain, down from 126. |
| Testing and verification | Pass | TypeScript, Vite, Cargo check, and Rust library tests pass: 70 passed, 0 failed, 4 ignored. | Online turns require the user's credentials and network. |
| Frontend, accessibility, and UX | Pass | DPI-aware full-window and 640x520 logical minimum-window release captures show complete, non-overlapping controls. | Short-height welcome state preserves every actionable control. |
| Operations, dependencies, and release | Pass | Normal WiX MSI and NSIS build passed; hashes and fresh timestamps recorded; six delivery files copied to Desktop. | MSI, NSIS, release exe, test plan, manual, and manifest delivered. |

Open issues:

- User acceptance with real Claude Code/Codex credentials.
- Gemini CLI/OpenCode structured runtime, goal pursuit, and unverified sync targets remain explicit future work or Labs.

### 2026-06-25 - Pre-delivery gate for borrowing roadmap P1+P2 and acceptance UX

| Category | Status | Evidence | Notes |
| --- | --- | --- | --- |
| Security and privacy | Pass | The responses bridge forwards only translated request/response bodies; API keys stay in the gateway's existing encrypted resolution and are not logged. | No new secret surfaces; provider presets prefill only public endpoints. |
| Data, API, and consistency | Pass | New `default_model` setting reuses the generic `settings` table; resolution order Agent binding → global default → Agent default is unit-tested; `target_model` vs `default_model` separation documented in decisions. | Default stored as `platform_id:model_name`; `split_once(':')` keeps colon-bearing model names intact. |
| Code quality and maintainability | Pass | `responses_bridge.rs` is isolated with pure-function unit tests; ChatTab surface/selection logic and sidebar filtering follow existing patterns. | Provider-preset apply-path and per-agent manual selection memory deferred. |
| Testing and verification | Pass | `cargo test --lib` 76 passed / 4 ignored; `cargo check` 90 legacy warnings; `npx.cmd tsc --noEmit` and `npm.cmd run build` pass. Bridge response shape validated end-to-end against the real `codex app-server`. | Live use of a real Chat-Completions provider with Codex still needs the user's key. |
| Frontend, accessibility, and UX | Pass | Added: Codex no-model hint, 对话/工作 split with workspace-required CTA, tab preload, first-token waiting indicator, model-center ☆ default + capability/health, portaled delete dialog, clarified setting names. | Hot-reloaded and accepted by the user across several rounds. |
| Operations, dependencies, and release | Warn | Frontend-only + isolated Rust module; no dependency changes. | These changes are not yet in a fresh MSI/NSIS package; a rebuild is required before redistribution. |

Open issues:

- P3 (MCP sync), P4 (Team board), P5 (assistants/Skills/remote) are planned but not implemented.
- A fresh Windows package has not been rebuilt since P1; current Desktop artifacts predate P1/P2.

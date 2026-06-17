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

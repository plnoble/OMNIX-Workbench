# Agent Handoff

## Current Objective

MSI packaging follow-up for OMNIX after the third refactor slice. The repo now has explicit Windows packaging scripts, diagnostics, and repair tooling, but the validated MSI still requires an elevated local Windows Installer/WiX environment repair.

## What Changed

- 2026-06-14 MSI packaging follow-up:
  - Added `tauri:build`, `tauri:build:msi`, `tauri:build:nsis`, `diagnose:msi`, and `repair:msi-env` npm scripts.
  - Added `scripts/diagnose-msi.ps1` to report WiX tool paths, Windows Installer service state, COM/script-engine registration, normal `light.exe` validation, and optional `-sval` control linking.
  - Added `scripts/repair-msi-environment.ps1` for elevated `msiexec`, VBScript/JScript, and `msiserver` repair, with optional DISM/SFC health scan.
  - Added a stable WiX `upgradeCode` to `src-tauri/tauri.conf.json`.
  - Confirmed `main.wxs` receives `UpgradeCode="1339290b-b4f1-5e65-b4b9-f8b0141ebc54"`.
- `src/App.tsx` now defaults to `work`; legacy `chat` and `workbench` navigation requests normalize to Work.
- `src/lib/appRegistry.tsx` and `src/hooks/useNavigationLayout.ts` define and persist fixed/launcher/hidden app placement.
- `src/components/layout/AppHeader.tsx` now provides fixed core entries, `+` application grid, theme/settings/diagnostics buttons, and per-entry placement actions.
- `src/components/layout/AppSidebar.tsx` is context-only: new chat, search, cron, conversations, workspaces for Work/Team; resource pages do not show the old resource stack.
- `src/components/tabs/ChatTab.tsx` is the single-agent Work page with Agent selector, large auto-growing input, model selector, permission mode, work mode, optional Knowledge/search, and collapsible workspace panel.
- `src/components/tabs/ModelsTab.tsx` wraps the model center as an independent app; Settings hides the visible platform tab.
- `src/components/tabs/SearchResourceTab.tsx` and `src/components/tabs/QuickAssistantTab.tsx` provide optional launcher apps.
- `src/components/tabs/AgentHubTab.tsx` was rewritten as spacious agent cards plus a detail drawer for detection/install/update/model binding.
- `src/hooks/usePlatforms.ts` now errors clearly when adding a custom model without a provider and uses provider-bound stable IDs.
- `src-tauri/src/commands/platforms.rs` now uses provider-bound model IDs for fetched models and decrypts active API keys before model list fetch and health checks.
- 2026-06-13 UI feedback patch: Work page now hides the right workspace panel until `xl`, shrinks it before `2xl`, shrinks the left contextual sidebar on narrower desktop widths, and uses a dropdown for work mode instead of wide segmented buttons plus duplicate text.
- 2026-06-13 app-grid/Skill feedback patch: Settings and Diagnostics are no longer app-grid entries, the Settings theme card is hidden because theme switching lives in the title bar, launcher copy now explains fixed/collected/hidden states, fixed title-bar entries can move forward/backward, navigation placement persistence uses latest state, and `SkillHub` now owns height/scroll boundaries to avoid overlap.
- 2026-06-13 shell consistency/workspace/model patch: `AppHeader` is now top-level global chrome, non-Work/Team pages no longer render `AppSidebar`, shared dialogs follow theme tokens, `WorkspaceModal` opens a Windows folder picker through `pick_directory`, and Ark/Volces endpoints no longer import/display Claude fallback catalogs.

## Verified

- MSI follow-up verification on 2026-06-14:
  - `npx.cmd tsc --noEmit` passed.
  - `cargo check` passed with existing unused/dead-code warnings.
  - `npm.cmd run build` passed with existing Vite chunking warnings.
  - `npm.cmd run diagnose:msi -- -RunSuppressValidation` reproduced the ICE validation failure and proved `-sval` diagnostic linking succeeds.
  - `npm.cmd run tauri:build:msi` failed at WiX `light.exe`.
  - `npm.cmd run tauri:build` failed at the same WiX `light.exe` MSI stage.
- `npx.cmd tsc --noEmit` passed.
- `npm.cmd run build` passed.
- `cargo check` passed with existing unused/dead-code warnings.
- Latest UI feedback patch also passed `npx.cmd tsc --noEmit` and `npm.cmd run build`.
- Latest app-grid/Skill feedback patch also passed `npx.cmd tsc --noEmit` and `npm.cmd run build`.
- Latest shell consistency/workspace/model patch passed `npx.cmd tsc --noEmit`, `npm.cmd run build`, and `cargo check`.

## Not Verified

- Automated visual screenshot QA was not completed. `npm.cmd ls playwright --depth=0` showed Playwright is not installed, and the host did not keep Vite/static preview background servers listening.
- Full Tauri desktop runtime behavior for agent install/update actions was not exercised in this environment.
- The exact minimized-window Skill page screenshot was not re-captured automatically; user desktop verification is still needed.
- The native Windows folder picker was added but not manually clicked inside the Tauri desktop runtime during this session.
- The Ark/Volces model fetch fix was verified by type/build/check only; it still needs manual provider refresh against the user's configured endpoint/account.

## Known Risks or Blockers

- Validated MSI output is still blocked until `npm.cmd run repair:msi-env` is run from an elevated PowerShell or the WiX/Windows Installer validation environment is otherwise repaired.
- `diagnostic-sval.msi` is not a release artifact. It exists only to prove generated WiX inputs can link when MSI validation is suppressed.
- MCP currently routes through `SettingsTab` focused on the MCP subtab; later slices should split it into its own page.
- Knowledge is still document-level, not a full multi-named-knowledge-base model.
- Agent built-in model selection is represented in UI but deeper per-agent execution config should be expanded later.
- `pick_directory` is currently Windows-first via PowerShell/Windows Forms. Cross-platform folder picking should use a dedicated Tauri dialog path if needed.
- Successful provider model refresh now clears previous model rows for that provider. This fixes stale fallback rows, but a future data model may need a manual-only flag if users want hand-added models to survive refresh.

## Next Recommended Action

Run elevated MSI repair and rebuild:

1. Open PowerShell as Administrator in `D:\Agent\Project\OMNIX-Development Tools`.
2. Run `npm.cmd run repair:msi-env`.
3. Run `npm.cmd run diagnose:msi`.
4. Run `npm.cmd run tauri:build:msi`.
5. Run `npm.cmd run tauri:build`.

Have the user manually test the desktop app: first screen, top launcher fixed/hidden behavior, Models save/fetch/check, Agents card/detail flow, and long input auto-growth. Then use the feedback for the next reference-software borrowing round.
Also verify the app launcher after removing Settings/Diagnostics, title-bar entry ordering, and the Skill page in the user's smaller desktop window.
Also verify the new global title-bar alignment across Work/Agents/Skills/Models, light-theme workspace picker, no left Work rail on non-work pages, and Ark/Volces provider refresh behavior.

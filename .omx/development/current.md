# Current Development State

Project: OMNIX-Development Tools

## Objective

Fix the OMNIX Windows MSI packaging path after the third refactor slice. Keep MSI as a required target, add repeatable diagnostics/repair scripts, and preserve NSIS as a fallback installer.

## Status

MSI packaging is unblocked and verified. Elevated Windows Installer repair (`repair:msi-env`) cleared the WiX `light.exe` ICE validation access errors. `diagnose:msi` now passes end-to-end and `tauri:build:msi` produced a validated MSI at `src-tauri/target/release/bundle/msi/omnix-app_0.1.0_x64_en-US.msi`. A robustness bug in `scripts/repair-msi-environment.ps1` (false throw on empty `$LASTEXITCODE` from `msiexec /unregister`) was fixed and recorded in the error ledger. Remaining work: run the full `tauri:build` to confirm both NSIS+MSI bundles, clean up diagnostic MSI artifacts from the bundle dir, and decide whether to commit the script fix alongside the in-flight uncommitted UI/backend refactor.

## Current Branch

master...origin/master. Existing untracked `.claude/` and `.omx/` directories are preserved.

## Last Verification

Latest MSI packaging resolution on 2026-06-17:
- Ran `npm.cmd run repair:msi-env` from an elevated PowerShell; fixed an `Invoke-Native` empty-exit-code bug in `scripts/repair-msi-environment.ps1`, then re-ran successfully.
- `npm.cmd run diagnose:msi` passed: Windows Installer service Running, COM + VBScript/JScript x64/x86 registered, WiX `light.exe` normal validation exit code 0 (only non-fatal ICE03/ICE40/ICE57/ICE61 warnings).
- `npm.cmd run tauri:build:msi` produced validated MSI: `src-tauri/target/release/bundle/msi/omnix-app_0.1.0_x64_en-US.msi` (11.8 MB).
- Full `npm.cmd run tauri:build` (NSIS+MSI) not yet re-run this session.

Latest MSI packaging follow-up on 2026-06-14:
- `npx.cmd tsc --noEmit` passed.
- `cargo check` passed with existing unused/dead-code warnings.
- `npm.cmd run build` passed with existing Vite chunking warnings.
- `npm.cmd run diagnose:msi -- -RunSuppressValidation` reproduced normal WiX ICE validation failure and confirmed `-sval` diagnostic linking succeeds.
- `npm.cmd run tauri:build:msi` built `src-tauri/target/release/omnix-app.exe` but failed at WiX `light.exe`.
- `npm.cmd run tauri:build` failed at the same WiX `light.exe` MSI stage.
- Generated `main.wxs` contains stable `UpgradeCode="1339290b-b4f1-5e65-b4b9-f8b0141ebc54"`.

Latest third-slice verification on 2026-06-13:
- `npx.cmd tsc --noEmit` passed.
- `cargo check` passed with existing unused/dead-code warnings.
- `npm.cmd run build` passed with existing Vite chunking warnings.
- `npm.cmd run tauri -- build` built `src-tauri/target/release/omnix-app.exe` but failed in the WiX MSI `light.exe` stage.
- `npm.cmd run tauri -- build --bundles nsis` passed and produced `src-tauri/target/release/bundle/nsis/omnix-app_0.1.0_x64-setup.exe`.

Completed verification on 2026-06-12 for the second refactor slice:
- `npx.cmd tsc --noEmit` passed.
- `npm.cmd run build` passed.
- `cargo check` passed with existing unused/dead-code warnings.
- `npm.cmd ls playwright --depth=0` showed Playwright is not installed.
- Vite/static preview background launch was attempted but this host did not keep the background server listening; visual validation still needs a local desktop/dev-server pass.

Latest UI feedback patch on 2026-06-13:
- `npx.cmd tsc --noEmit` passed.
- `npm.cmd run build` passed.
- Fixed narrower-window clipping by delaying/hiding the right workspace panel and shrinking the left contextual sidebar.
- Changed work-mode control from segmented buttons plus duplicate label to a compact dropdown.

Latest app-grid and skill-page feedback patch on 2026-06-13:
- `npx.cmd tsc --noEmit` passed.
- `npm.cmd run build` passed.
- Removed Settings/Diagnostics from the configurable app grid because they are title-bar utilities.
- Hid the duplicate Settings theme selector, clarified fixed/collected/hidden launcher semantics, added title-bar entry ordering, and fixed stale navigation placement updates.
- Reworked Skill page height/scroll boundaries to prevent markdown, topology, marketplace, and scanner sections from overlapping.

Latest shell consistency/workspace/model patch on 2026-06-13:
- `npx.cmd tsc --noEmit` passed.
- `npm.cmd run build` passed.
- `cargo check` passed with existing unused/dead-code warnings.
- Header controls now live in the top-level app shell.
- `AppSidebar` only renders for Work/Team context surfaces.
- Workspace modal uses `pick_directory` instead of manual path entry.
- Shared dialog primitives use semantic theme tokens.
- Ark/Volces endpoint detection avoids Anthropic/Claude fallback and clears stale provider model rows on successful refresh.

## Blockers

MSI packaging blocker is cleared (2026-06-17). No active blockers. Optional follow-ups remain manual: re-run full `tauri:build` to confirm the NSIS+MSI double bundle, remove diagnostic-only MSI artifacts (`diagnostic-sval.msi`, `diagnostic-normal.msi`, `diagnostic-test.wixpdb`, `diagnostic-verbose.wixpdb`) from the bundle directory so only release installers ship, and review the large set of uncommitted UI/backend refactor changes for a logical commit split. Visual validation remains manual because the local environment did not keep a background preview server alive. Four long/integration tests are intentionally ignored in the lib baseline:
- `agent::tests::test_memory_injection`
- `db::tests::test_db_manager_and_active_account`
- `tests::tests::test_db_concurrency`
- `tests::tests::test_all_five_agents_interactive`

## Next Action

MSI is fixed. Suggested next steps:

1. Re-run `npm.cmd run tauri:build` (elevated) to confirm both NSIS and MSI bundles generate cleanly.
2. Clean diagnostic-only artifacts from `src-tauri/target/release/bundle/msi/` (`diagnostic-*.msi`, `diagnostic-*.wixpdb`) — they are control artifacts, not release installers.
3. Decide commit strategy for the large uncommitted working tree (UI shell refactor + new Tauri commands + the `repair-msi-environment.ps1` fix). Do not commit on the user's behalf without confirmation.
4. Optional: run `npx.cmd tsc --noEmit`, `cargo check`, and `npm.cmd run build` to re-confirm the in-flight changes still pass.

Do not ship `diagnostic-sval.msi` or any `diagnostic-*` artifact; they only prove WiX can link when MSI validation is suppressed.

## Notes for Next Agent

Read `AGENTS.md` and this file before changing code. Follow anti-failure rules: avoid unsafe CORS credentials patterns, do not hold sync mutex guards across await, and never use `git push -f`.

Key files from this slice:
- `src-tauri/src/commands/workbench.rs`
- `src/components/tabs/WorkbenchTab.tsx`
- `src/components/tabs/LabsTab.tsx`
- `src/components/layout/AppSidebar.tsx`
- `src/components/layout/AppHeader.tsx`
- `src/lib/tauri-api.ts`
- `src-tauri/src/commands/project_protocol.rs`
- `src-tauri/src/commands/skill_sets.rs`
- `src/components/tabs/AssistantsTab.tsx`
- `src/components/modals/WorkspaceModal.tsx`
- `src/components/tabs/AgentHubTab.tsx`
- `src/components/tabs/ChatTab.tsx`

Current second-slice target files:
- `src/App.tsx`
- `src/lib/appRegistry.tsx`
- `src/hooks/useNavigationLayout.ts`
- `src/components/layout/AppHeader.tsx`
- `src/components/layout/AppSidebar.tsx`
- `src/components/modals/WorkspaceModal.tsx`
- `src/components/ui/dialog.tsx`
- `src/components/tabs/ChatTab.tsx`
- `src/components/tabs/ModelsTab.tsx`
- `src/components/tabs/SearchResourceTab.tsx`
- `src/components/tabs/QuickAssistantTab.tsx`
- `src/components/tabs/AgentHubTab.tsx`
- `src/components/tabs/SettingsTab.tsx`
- `src/hooks/usePlatforms.ts`
- `src/lib/tauri-api.ts`
- `src/types/index.ts`
- `src-tauri/src/commands/platforms.rs`
- `src-tauri/src/commands/windows.rs`
- `src-tauri/src/lib.rs`

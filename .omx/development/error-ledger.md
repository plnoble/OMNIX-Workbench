# Error Ledger

Use this file to record mistakes that should not be repeated.

## Entry Template

### YYYY-MM-DD - Short title

- Symptom:
- Wrong assumption:
- Root cause:
- Detection method:
- Fix:
- Prevention rule:
- Skill candidate: yes/no

## Entries

### 2026-06-13 - Overloaded left-sidebar and homepage shell

- Symptom: The first refactor kept too much of the old product shape: Workbench, sessions, runs, resources, settings, and Labs were crowded into the visible shell. Users entered the app without a clear first action.
- Wrong assumption: A developer workbench should expose all important modules immediately, and a left global sidebar can carry most navigation.
- Root cause: I treated the refactor as rearranging existing panels instead of redesigning the product entry around the user's primary workflow.
- Detection method: User feedback after comparing AionUi and Cherry Studio: first screen should be "choose Agent + start input"; frequent modules can live horizontally in the top bar; low-frequency modules should move into an app grid.
- Fix: Default route changed to single-agent Work; Team, Agents, Skills became separate core entries; top bar plus configurable app launcher replaced the global resource stack; Models/Search/Quick Assistant became focused apps.
- Prevention rule: For user-facing shell refactors, identify the primary first action and keep the first screen dedicated to it. Do not stack global resources in the left sidebar or homepage. Use top horizontal navigation for fixed common entries and an app launcher/grid for low-frequency or experimental features.
- Skill candidate: yes

### 2026-06-13 - Workspace page clipped on narrower desktop windows

- Symptom: When the desktop window was made smaller, the right side of the Work page was cut off instead of the layout adapting. The input toolbar also showed a selected work mode twice.
- Wrong assumption: A `lg` breakpoint was wide enough to show the right workspace panel alongside a fixed left sidebar and dense input toolbar.
- Root cause: Fixed-width left and right panels consumed too much horizontal space at around the minimum desktop width, while a segmented work-mode control added unnecessary width and duplicate text.
- Detection method: User screenshot and manual desktop testing feedback.
- Fix: Left sidebar now shrinks on narrower widths; right workspace panel appears only from `xl` and grows at `2xl`; work mode is now a compact dropdown and the duplicate selected-mode text was removed.
- Prevention rule: For dense desktop app shells, optional side panels should not appear at the same breakpoint as the minimum supported window width. Use progressive disclosure: shrink primary sidebars, hide optional side panels, and replace wide segmented controls with selects when toolbar space is tight.
- Skill candidate: yes

### 2026-06-13 - App launcher placement semantics were ambiguous

- Symptom: Settings and Diagnostics appeared in the app grid even though they already had fixed title-bar buttons. "Collect" and "Hide" looked nearly equivalent, and repeated placement clicks could make another entry appear hidden or unavailable.
- Wrong assumption: A generic fixed/launcher/hidden control set would be self-explanatory without product copy or stricter state handling.
- Root cause: The app registry still treated title-bar utilities as launcher entries, and `useNavigationLayout.moveEntry` used a captured layout snapshot instead of the latest layout when persisting rapid changes.
- Detection method: User screenshots of duplicate Settings/Diagnostics launcher cards and the collect/hide state bug.
- Fix: Removed Settings/Diagnostics from the app registry, clarified the UI copy for fixed/collected/hidden, added fixed-entry ordering controls, and changed navigation persistence to use a ref-backed latest layout state.
- Prevention rule: When an item is promoted to a permanent chrome control, remove it from configurable app surfaces unless there is a clear second purpose. For persisted UI layout mutations, update from the latest state rather than a stale render snapshot.
- Skill candidate: yes

### 2026-06-13 - Skill page content overlapped marketplace cards

- Symptom: Skill markdown content visually overlapped the marketplace section, especially after the new shell reduced available vertical space.
- Wrong assumption: A child page could keep using `calc(100vh - header)` after it was moved into a nested app shell.
- Root cause: `SkillHub` calculated its own viewport height inside a parent that already manages height, then mixed a shrinkable details card with downstream marketplace/scanner cards in the same scroll flow.
- Detection method: User screenshot showing skill content, headings, and marketplace cards drawn over each other.
- Fix: The Skill page now inherits parent height, gives the detail card a stable minimum height, bounds the markdown/topology panes with internal scrolling, and makes marketplace/scanner cards flex-none sections.
- Prevention rule: In nested desktop shells, page components should use `h-full min-h-0` from their parent instead of recalculating `100vh`. Long editors and graphs need explicit internal scroll boundaries before adding lower sections.
- Skill candidate: yes

### 2026-06-13 - Global chrome drifted with page layout

- Symptom: Work page and other pages showed the top-right title controls in slightly different positions, making the app feel informal.
- Wrong assumption: Rendering the header inside the main content column was equivalent to rendering it as global window chrome.
- Root cause: The header lived inside the content layout while optional side panels and page-specific structure changed available width.
- Detection method: User visual feedback comparing Work against other pages.
- Fix: Moved `AppHeader` above the sidebar/content row and made it a full app-level chrome strip.
- Prevention rule: Window-level controls such as fixed navigation, theme, settings, diagnostics, and window utilities must be owned by the app shell, not by an individual page or content row.
- Skill candidate: yes

### 2026-06-13 - Context sidebar leaked into non-context pages

- Symptom: Agents and other resource pages still showed a left rail that felt like "work context", even though those pages do not need conversation/workspace context.
- Wrong assumption: A fallback sidebar explaining the current page was harmless.
- Root cause: The sidebar component tried to provide both Work/Team context and generic page navigation, recreating the old left-side clutter pattern.
- Detection method: User feedback that "智能体或者其他页面" should not show "工作上下文".
- Fix: `AppSidebar` now returns `null` outside Work/Team surfaces.
- Prevention rule: A left sidebar in this product is contextual, not global. If a page has no contextual object list, it should use full width or define its own local layout.
- Skill candidate: yes

### 2026-06-13 - Shared dialog hard-coded dark styling

- Symptom: In light theme, "选择工作区" opened as a black dialog with purple text, breaking the theme.
- Wrong assumption: A dark translucent dialog could serve as a universal modal style.
- Root cause: The shared `DialogContent` and `DialogTitle` classes used hard-coded dark/accent colors instead of semantic theme tokens.
- Detection method: User screenshot and light-theme testing.
- Fix: Dialog now uses `bg-popover`, `text-popover-foreground`, `border-border`, and `text-foreground`.
- Prevention rule: Shared UI primitives must use theme tokens only. Hard-coded theme-specific colors belong in one-off visuals, not reusable components.
- Skill candidate: yes

### 2026-06-13 - Ark/Volces model fetch fell back to Claude models

- Symptom: Fetching models for `https://ark.cn-beijing.volces.com/api/coding` repeatedly produced Claude model names.
- Wrong assumption: Provider `api_type` was sufficient to choose the model-list strategy and fallback catalog.
- Root cause: Volcano/Ark handling was nested under OpenAI-like behavior, so an Ark address configured with an Anthropic-like type used Anthropic known-model fallback. Existing rows were also not cleared, so stale Claude rows could remain visible.
- Detection method: User reported the wrong model list for the Ark/Volces endpoint.
- Fix: Detect Volces/Ark endpoint families before `api_type` fallback and replace the provider's stored model rows when a fetch/import succeeds.
- Prevention rule: Model-center fetch logic must consider endpoint family as well as provider type. On successful refresh, stale provider rows should not remain mixed with the new catalog.
- Skill candidate: yes

### 2026-06-14 - MSI build failure misread as app packaging failure

- Symptom: `npm.cmd run tauri -- build` built the release exe, then failed while producing `bundle/msi/omnix-app_0.1.0_x64_en-US.msi`; the visible MSI file in the bundle directory was stale from an earlier date.
- Wrong assumption: A failed full Tauri build means the application binary or WiX source is broken.
- Root cause: WiX `light.exe` reaches MSI database validation and fails ICE actions with Windows Installer Service access errors. `light.exe -sval` can link a diagnostic MSI, so the generated `wxs/wixobj` and CAB/file binding are not the failing layer.
- Detection method: Ran `light.exe` manually with verbose output and a suppressed-validation control; added `npm.cmd run diagnose:msi` to repeat the layered checks.
- Fix: Added explicit MSI/NSIS/full build scripts, `diagnose-msi.ps1`, elevated `repair-msi-environment.ps1`, and a stable WiX `upgradeCode`. Actual validated MSI still requires elevated Windows Installer/script-engine repair.
- Prevention rule: For Tauri Windows packaging, verify artifact timestamps and split failures into app compile, Tauri bundle, WiX link, and MSI ICE validation layers. Never ship `-sval` output as a release MSI.
- Skill candidate: yes

### 2026-06-17 - Elevated MSI repair script threw on empty exit code

- Symptom: `npm.cmd run repair:msi-env` in an elevated session aborted at the first step with `msiexec.exe exited with code` (code value blank).
- Wrong assumption: Any non-integer `$LASTEXITCODE` after invoking a native executable is a failure, and `$LASTEXITCODE -ne 0` is a safe success check.
- Root cause: `msiexec.exe /unregister` (and similar silent unregistration ops) can leave `$LASTEXITCODE` unset (`$null`/empty) in PowerShell. `$null -ne 0` evaluates to `$true`, so the `Invoke-Native` guard threw a false failure.
- Detection method: Read `scripts/repair-msi-environment.ps1` `Invoke-Native`; observed the blank code value in the thrown message.
- Fix: Treat an unset `$LASTEXITCODE` as success in `Invoke-Native`; only throw on an explicit non-zero integer. A real `msiexec` failure surfaces as a non-zero code or a Windows Installer error dialog, not an empty code.
- Prevention rule: In PowerShell wrappers around native exes (especially `msiexec`, `regsvr32`, silent ops), guard `$LASTEXITCODE` for null/empty before numeric comparison. Do not assume every native call sets a numeric exit code.
- Skill candidate: yes

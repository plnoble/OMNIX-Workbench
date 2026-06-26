# Skill Candidates

Use this file to collect lessons that may become future global skills, project rules, scripts, or templates.

## Candidate Template

### YYYY-MM-DD - Candidate title

- Trigger:
- Reusable lesson:
- Evidence:
- Proposed form: project rule / global skill / script / template / discard
- Promotion threshold:

## Candidates

### 2026-06-13 - Frontend shell refactor checklist

- Trigger: Building or refactoring an app/workbench/client shell with many modules, resources, Labs features, settings, and primary workflows.
- Reusable lesson: Start from the user's first action, not from the existing feature inventory. Keep the default screen focused on one primary workflow. Put frequent cross-cutting entries in a horizontal top bar. Put lower-frequency resources, Labs, diagnostics, and optional helpers into a configurable application grid. Keep the left sidebar contextual, not a dumping ground for global navigation.
- Evidence: OMNIX first refactor looked like a Workbench pile-up; after AionUi and Cherry Studio references, the shell changed to Work/Team/Agents/Skills fixed entries plus `+` launcher for Models, Knowledge, Memory, MCP/Search, Quick Assistant, Cron, Compare, Code Analysis, and Labs.
- Proposed form: project rule first; promote to global skill if this repeats in another app shell.
- Promotion threshold: Promote after one more UI-heavy refactor where top-bar/app-grid/context-sidebar decisions prevent feature pile-up.

### 2026-06-13 - Tauri visual smoke testing workflow

- Trigger: Frontend changes affect the default route, navigation shell, text density, resizable panels, or theme behavior.
- Reusable lesson: `tsc`, `vite build`, and `cargo check` prove code health but not visual coherence. Need a repeatable desktop/browser screenshot path for 1365x768 and wider desktop widths.
- Evidence: This slice passed build checks, but Playwright was not installed and the host did not keep preview servers listening, so visual QA remained manual.
- Proposed form: script/template.
- Promotion threshold: Add once a stable dev-server or Tauri screenshot runner is available.

### 2026-06-13 - Configurable app shell state checklist

- Trigger: A product shell lets users pin, collect, hide, or reorder modules across a title bar and application grid.
- Reusable lesson: Permanent title-bar utilities should not also be configurable app-grid entries unless they have a second distinct surface. UI copy must explain placement semantics in the surface itself. Layout mutations should persist from latest state, not a render snapshot, and fixed entries need explicit reorder controls if drag-and-drop is absent.
- Evidence: OMNIX showed Settings/Diagnostics both in title-bar buttons and the launcher. The collect/hide actions were ambiguous and stale state could make another item appear to change placement unexpectedly.
- Proposed form: project rule / template checklist.
- Promotion threshold: Promote to a global frontend shell skill after another configurable navigation project.

### 2026-06-13 - Nested desktop page height checklist

- Trigger: A page with sidebars, editors, graphs, marketplaces, logs, or scanner panels is embedded under an app shell that already owns viewport height.
- Reusable lesson: Nested pages should inherit `h-full min-h-0` from the shell instead of using `calc(100vh - header)`. Long editors and graph panes need their own scroll boundaries, and downstream sections should be `flex-none` so they never sit under an overflowing primary panel.
- Evidence: `SkillHub` kept old viewport math after the shell refactor, causing markdown content to overlap the marketplace cards in a smaller desktop window.
- Proposed form: project rule / visual QA checklist.
- Promotion threshold: Promote after adding a reusable screenshot smoke test for 1024/1280/1365 desktop widths.

### 2026-06-13 - Desktop app chrome checklist

- Trigger: A desktop client has title-bar controls, fixed navigation, optional side panels, and multiple page types.
- Reusable lesson: Global chrome must be outside the page content row so right-side utilities do not drift between pages. Context sidebars should render only for surfaces that truly have current context; resource/config pages should not inherit Work rails by default. Shared primitives must use semantic theme tokens.
- Evidence: OMNIX showed different top-right positions across Work and other pages, Agents inherited a misleading work-context sidebar, and a shared dialog appeared dark/purple under the light theme.
- Proposed form: project rule / frontend shell skill.
- Promotion threshold: Promote after one more desktop-shell iteration where these checks prevent a visual regression.

### 2026-06-13 - Model provider endpoint checklist

- Trigger: A model center supports custom providers, custom base URLs, multiple API styles, and model-list fetching.
- Reusable lesson: Fetch strategy should consider both the declared provider/API type and the endpoint family. Successful refresh should replace stale provider model rows rather than mixing old fallback catalog entries with new results.
- Evidence: An Ark/Volces endpoint configured in a way that hit Anthropic fallback showed Claude models. Replacing the provider rows after endpoint-family detection fixed the visible stale/wrong catalog.
- Proposed form: project rule / backend model-center checklist.
- Promotion threshold: Promote after adding real provider-specific model-list implementations for at least one more non-OpenAI provider.

### 2026-06-14 - Tauri Windows MSI packaging checklist

- Trigger: Tauri Windows builds produce `omnix-app.exe` but fail while bundling MSI, or output directories contain old MSI files after a failed build.
- Reusable lesson: Split the pipeline into frontend build, Rust release build, Tauri bundling, WiX candle/light, and MSI ICE validation. Check artifact timestamps before declaring a package current. Use `light.exe -sval` only as a diagnostic control, never as a release path.
- Evidence: OMNIX `tauri build` produced a current release exe but failed at WiX `light.exe`; the formal MSI in `bundle/msi` was stale, while `diagnostic-sval.msi` proved WiX linking works when validation is bypassed.
- Proposed form: script + release checklist.
- Promotion threshold: Promote after one more Windows MSI packaging issue or after adding timestamp enforcement to release scripts.

### 2026-06-25 - DPI-aware Tauri native visual QA

- Trigger: A Windows desktop app appears clipped, oversized, or inconsistent under 125%-200% display scaling, especially when evidence comes from PowerShell or Win32 screenshots.
- Reusable lesson: The capture process is part of the measurement chain. Call `SetProcessDpiAwarenessContext(PER_MONITOR_AWARE_V2)` before `EnumWindows`, `GetWindowRect`, `SetWindowPos`, `CopyFromScreen`, or `PrintWindow`; compare in-process Tauri `inner_size` with Win32 `GetClientRect` before changing WebView zoom. Capture both the default and minimum logical window.
- Evidence: A DPI-unaware process reported about 1293 pixels for a client that both in-process APIs reported as 2560 pixels. PMv2 capture produced the complete 2586x1655 window and prevented a false WebView regression diagnosis.
- Proposed form: checked-in PowerShell script plus desktop visual QA checklist.
- Promotion threshold: Implement immediately for the next Windows release; promote globally after reuse in one other Tauri/Electron/WebView desktop project.

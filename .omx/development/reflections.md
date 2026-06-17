# Reflections

Use this file at the end of meaningful tasks. Keep entries short and focused on reusable learning.

## Entry Template

### YYYY-MM-DD - Task or milestone

- Related task:
- What worked:
- What failed or slowed down:
- Root lessons:
- Token or time waste:
- Process improvements:
- Future optimizations:
- Potential skill candidates:

## Entries

### 2026-06-12 - Second shell refactor

- Related task: Replace the visible Workbench pile-up with a focused Work/Team/app-launcher shell.
- What worked: Splitting `appRegistry`, `useNavigationLayout`, and focused app tabs made the navigation change easier to reason about and verify.
- What failed or slowed down: Starting a background preview server was unreliable in this host; automated screenshot QA needs an installed browser testing dependency or a desktop-runner path.
- Root lessons: For user-facing refactors, first remove the confusing entry point, then deepen individual feature domains. Otherwise old product assumptions leak into the new UI.
- Token or time waste: Duplicating Settings props for the transitional MCP route is noisy and should be cleaned by a dedicated MCP page.
- Process improvements: Add a lightweight visual smoke script to the repo so future agents can capture the Work page without ad hoc server startup attempts.
- Future optimizations: Split `SettingsTab` further, add named knowledge bases, and formalize agent built-in model config.
- Potential skill candidates: Frontend shell refactor checklist; Tauri visual smoke testing workflow.

### 2026-06-13 - Narrow-window work page feedback

- Related task: Fix Work page clipping and simplify the work-mode control.
- What worked: Replacing a wide segmented control with a select immediately reduced toolbar pressure; moving optional right panel display from `lg` to `xl` protected the minimum desktop width.
- What failed or slowed down: The initial shell used visually pleasing fixed panels but did not adequately test the normal-window/minimum-width case.
- Root lessons: In dense desktop tools, optional side panels need later breakpoints than core content. Toolbar controls should degrade to selects before they create wrapping or duplicate labels.
- Token or time waste: None significant.
- Process improvements: Add a visual checklist item for minimum window width before considering a shell refactor ready.
- Future optimizations: Add an automated 1024/1280/1365 width screenshot smoke test.
- Potential skill candidates: Responsive desktop shell checklist.

### 2026-06-13 - App grid semantics and Skill page overlap feedback

- Related task: Remove duplicate title-bar utilities from the app grid, clarify fixed/collected/hidden behavior, add fixed-entry ordering, and stop Skill page text overlap.
- What worked: Treating title-bar controls as permanent chrome simplified the launcher; adding explicit copy made the placement model understandable without extra documentation.
- What failed or slowed down: The earlier layout mutation code relied on a render-time snapshot, which is fragile for quick consecutive UI actions. The Skill page also inherited old viewport math from before the shell refactor.
- Root lessons: Configurable navigation needs both clear mental-model labels and latest-state persistence. Nested pages should inherit height from the shell and define internal scroll areas for long editors, graphs, and secondary sections.
- Token or time waste: Some source files still contain legacy encoded text, which made exact patching slower; small registry/header rewrites were cleaner than patching garbled strings.
- Process improvements: Add a pre-delivery check for duplicate entries between title-bar chrome and app launcher, plus a height ownership check for every nested tab.
- Future optimizations: Fully split Settings into smaller pages and replace transitional hidden legacy blocks with deleted code after the shell settles.
- Potential skill candidates: Configurable app-shell UX checklist; nested-height layout checklist.

### 2026-06-13 - Shell consistency and model-center endpoint feedback

- Related task: Stabilize title-bar controls, remove non-work sidebars, make workspace selection native, and stop Ark/Volces model fetch from showing Claude models.
- What worked: Moving the header out of the page row made the visual hierarchy cleaner; theme-token dialog primitives fixed the light-theme issue at the source instead of only patching one modal.
- What failed or slowed down: The earlier architecture still had old Workbench assumptions: a shared sidebar tried to help every page, and model fetching trusted provider type more than the actual API endpoint.
- Root lessons: Global chrome should be structural, not page-local. Context rails should only appear where they hold real context. Model centers must model provider, endpoint family, and refresh semantics separately.
- Token or time waste: Source files with legacy encoded Chinese made exact small patches brittle; future shell cleanup should delete transitional hidden blocks once the user confirms the new layout.
- Process improvements: Add checks for title-bar alignment across pages, theme-token usage in shared primitives, and provider-refresh behavior when adding model-center features.
- Future optimizations: Replace the PowerShell folder picker with a Tauri dialog plugin if the project later accepts a dependency; add provider-specific Ark/Volces model-list integration.
- Potential skill candidates: Desktop app chrome checklist; model-provider endpoint checklist.

### 2026-06-14 - MSI packaging investigation

- Related task: Fix Tauri MSI packaging after the Windows desktop build.
- What worked: Splitting the chain into Tauri config, WiX link, and MSI ICE validation exposed the real failing layer. The `-sval` control was useful as evidence, not as a fix.
- What failed or slowed down: Elevated command approval did not complete, so the actual Windows Installer environment repair could not be executed from this session.
- Root lessons: MSI failures need artifact timestamp checks and layer-specific diagnostics. A stale MSI in the output directory is dangerous because it can look like a successful current build.
- Token or time waste: Full `tauri build` repeated the same MSI blocker after MSI-only already proved it, but it created clean evidence for the requested test plan.
- Process improvements: Keep `diagnose:msi` and `repair:msi-env` as standard release tools.
- Future optimizations: Add a CI/release checklist that refuses stale Windows bundle artifacts and prints installer timestamps.
- Potential skill candidates: Tauri Windows MSI packaging checklist.

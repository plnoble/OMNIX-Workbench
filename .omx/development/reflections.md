# Reflections

Use this file at the end of meaningful tasks. Keep entries short and focused on reusable learning.

### 2026-06-25 - Borrowing roadmap P2 and naming two similar concepts apart

- Related task: Land the unified model center (P2) and resolve the "two default models" confusion.
- What worked: Tracing each setting to its actual consumer (`target_model` → proxy/internal features; `default_model` → Agent runtime) before answering "is this a duplicate?" turned a vague worry into a precise, defensible answer. Reusing the existing P1 backend (`default_model` resolution) meant P2's keystone was mostly a UI surface plus naming.
- What failed or slowed down: A subtle pre-select bug — keeping a selection identified by a cross-Agent-shared id (`agent_default`) across Agent switches masked the new Agent's default. Easy to miss because the backend was correct.
- Root lessons: When two features share a vocabulary ("默认模型"), the fix is usually to name them by their consumer, not to merge them. And selection state keyed by a changing value must re-derive on that change rather than preserve a shared-id choice.
- Process improvements: Check existing surfaces (capability icons, health checks already existed) before building — much of a "new" phase may already be done.
- Potential skill candidates: none new.

### 2026-06-25 - Probe the real CLI protocol before trusting assumptions

- Related task: Fix Codex ignoring the user's default model; build a Responses↔Chat gateway bridge.
- What worked: Running the real `codex app-server` and `generate-json-schema` to capture the exact Responses request, and replaying my translator's exact SSE output back into Codex to confirm acceptance. This turned a high-risk streaming translator into a validated one without spending a real model request.
- What failed or slowed down: A backgrounded capture server died with its shell, and a second server's port conflict made the output land in the wrong file — cost a couple of iterations. A single self-contained driver process (server + codex driver in one) was the reliable shape.
- Root lessons: For protocol bridges, ground every field in captured ground truth, not docs alone; and validate the hard direction (response events the peer must parse) against the real peer. Two reference projects (cc-switch) confirmed the upstream constraint (Codex dropped Chat Completions) and the auth field shape.
- Process improvements: Keep a captured request as a test fixture; unit-test pure translation and live-validate the streaming shape.
- Potential skill candidates: "Capture-and-replay a CLI's wire protocol to de-risk an adapter".

### 2026-06-23 - Product identity and domain naming

- Related task: Finalize OMNIX Workbench naming across application, packages, source, and release metadata.
- What worked: Defining a hierarchy before editing separated formal product identity, compact UI branding, human-readable descriptor, and machine-safe package names.
- What failed or slowed down: Earlier iterations used `DevFlow`, `omnix-app`, and `Workbench` interchangeably as brand, package, route, and feature domain. That made the product shape harder to explain and left dead UI behind.
- Root lessons: Product name, navigation vocabulary, domain vocabulary, and artifact slug are separate contracts. A product called Workbench should not contain another catch-all Workbench page.
- Process improvements: Keep canonical names in frontend constants, Tauri metadata, and `AGENTS.md`; search retired names before every release build.
- Future optimizations: Add a lightweight naming-lint script if retired names reappear.
- Potential skill candidates: Product naming migration checklist for desktop applications.

### 2026-06-23 - Provisional E1 icon rollout

- Related task: Replace every OMNIX Workbench product icon with the selected E1 concept.
- What worked: Generating one RGBA master and using `tauri icon` kept platform variants consistent; direct PE parsing exposed stale embedded resources that source-file inspection missed.
- What failed or slowed down: Cargo resource caching preserved the old executable icon, NSIS needed separate installer icon configuration, and MSI verification required an unavailable non-sandbox approval.
- Root lessons: Icon rollout has three layers: source assets, in-app/web references, and embedded package resources. Each needs separate verification.
- Process improvements: Track icon inputs in `build.rs`, configure installer icons explicitly, and extract packaged PE resources in release QA.
- Future optimizations: Add a reusable Windows icon verification script and rebuild the MSI when non-sandbox approval returns.
- Potential skill candidates: Tauri cross-platform icon rollout and package verification checklist.

### 2026-06-24 - E1 MSI closure

- Related task: Finish the icon rollout after non-sandbox WiX approval became available.
- What worked: Decompiling the final MSI and comparing its embedded executable/ProductIcon against the master provided stronger evidence than checking the output filename.
- What failed or slowed down: Completion crossed sessions because MSI validation required an external approval path.
- Root lessons: A package is only verified when its embedded payload and metadata are inspected, not merely when a bundle command exits successfully.
- Process improvements: Keep normal WiX validation plus `dark.exe` payload inspection in Windows release QA.
- Future optimizations: Automate MSI icon comparison in a reusable verification script if Windows releases become frequent.
- Potential skill candidates: MSI payload/icon verification helper.

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

### 2026-06-24 - Structured single-Agent runtime

- Related task: Replace placeholder execution with a persistent Claude Code/Codex runtime.
- What worked: Starting with protocol fixtures and fake executables made argument mapping, JSON parsing, persistence, approval, stop, and resume behavior testable without credentials. A broadcast boundary kept the runtime independent from Tauri.
- What failed or slowed down: The first test fixture initialized the full production database; tying the core manager to `AppHandle` prevented the Windows test binary from starting; npm shim resolution differed between PowerShell and child-process callers.
- Root lessons: A real Agent integration is a protocol adapter plus lifecycle state machine, not a terminal parser. Desktop handles, database seeding, and shell-specific command resolution must stay outside that core.
- Token or time waste: `cargo fmt --check` exposed broad pre-existing formatting drift and was too noisy for a scoped change; future formatting work should target touched files or be a dedicated repository-wide slice.
- Process improvements: Keep fake executable fixtures, model compatibility tests, and real no-cost CLI initialize probes in the release gate. Persist before emitting events, and check artifact timestamps after packaging.
- Future optimizations: Add an automated native Tauri screenshot runner and split remaining legacy PTY code from `useConversations` once Team adopts the same runtime layer.
- Potential skill candidates: Structured CLI Agent adapter checklist; Windows npm CLI resolution checklist.

### 2026-06-25 - Full real-gap release and DPI-aware desktop QA

- Related task: Complete the real single-Agent, Skills/distillation, Team, Knowledge/Quick Assistant plan and package a Windows test release.
- What worked: Protocol-level fake executables, persisted state-machine tests, sequential release gates, normal WiX validation, and Per-Monitor-V2 native screenshots gave evidence at the right layers.
- What failed or slowed down: A DPI-unaware capture process produced convincing but false half-window clipping; treating the screenshot as physical evidence led to an unnecessary frontend zoom experiment before in-process measurements contradicted it.
- Root lessons: Desktop visual QA is part of the tested system. Its DPI context, window handle, coordinate space, and artifact timestamp must be explicit. Keep primary workflows honest and put unsupported runtimes behind visible status rather than simulated success.
- Token or time waste: Repeated packaging and screenshots before the DPI virtualization discrepancy was isolated.
- Process improvements: Run release gates sequentially, calculate hashes only after the final package, capture at full and minimum logical sizes with PMv2 awareness, and copy docs/manifest beside the installer.
- Future optimizations: Turn the native capture script and artifact manifest generation into checked-in release helpers; reduce the remaining 90 Rust warnings in domain-sized slices.
- Potential skill candidates: DPI-aware Tauri visual QA; structured Agent runtime acceptance; Windows release manifest generator.

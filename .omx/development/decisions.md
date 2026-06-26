# Decision Log

## 2026-06-25 - Keep two distinct default-model settings, not one

Decision: Keep both `target_model` (Settings → "内置功能默认模型") and `default_model` (Models → ☆ "Agent 默认模型") as separate settings with clarified names, rather than merging or deleting one.

Alternatives considered:

- Delete `target_model` as a duplicate of the new `default_model`.
- Unify both into a single global default model.

Why chosen:

They serve different consumers: `target_model` drives OMNIX's own built-in AI features (划词翻译/语言检测/知识库问答) through the general proxy gateway (stored as a bare model name), while `default_model` drives the external Agent runtime (Codex/Claude) when no per-Agent binding exists, through the session gateway (stored as `platform_id:model_name`). Cherry Studio similarly separates default-conversation / translation / topic-naming models. Deleting `target_model` would break translation/QA.

Consequences:

- Renamed/relabeled both with explicit descriptions and cross-references so users understand the split.
- The two keep different storage formats and resolution paths by design.

## 2026-06-25 - Codex model routing via a translating gateway, not config.toml rewrites

Decision: Let Codex reach any user-configured provider by having the OMNIX session gateway translate between the Responses API (Codex) and Chat Completions (most providers), rather than writing provider routes into `~/.codex/config.toml` like cc-switch.

Alternatives considered:

- cc-switch approach: write `[model_providers.<id>]` (base_url/wire_api/experimental_bearer_token) directly into `~/.codex/config.toml`.
- Restrict Codex to only Responses-native providers (status quo).

Why chosen:

Codex 0.139 only emits `wire_api = "responses"`, but DeepSeek/Volcano/most third-party providers only speak Chat Completions. cc-switch's direct-config approach therefore fails for them (their issues #2553/#3668). A translating gateway makes any provider usable with Codex and keeps a single source of truth (DB), avoiding mutation of the user's Codex config and ChatGPT login cache.

Consequences:

- New `responses_bridge.rs` translates request + streaming response; `evaluate_model_compatibility` now marks Chat providers as Gateway-selectable for Codex.
- The bridge must track Codex's Responses event schema; validated end-to-end against the real `codex app-server`.
- "Agent 官方默认" now resolves through Agent binding → global default model before falling back, so a configured default actually reaches Codex.

## 2026-06-12 - Adopt shared agent protocol

Decision: Use `AGENTS.md` and `.omx/development/` as the shared source of truth for agent collaboration.

Alternatives considered:

- Startup prompt only.
- Tool-specific skill only.

Why chosen:

Shared project files survive handoff across tools and sessions.

Consequences:

- Agents must keep records current.
- Tool-specific skills can improve behavior but should not replace project state files.

## 2026-06-12 - Replace visible Workbench dashboard with configurable app shell

Decision: Use a hybrid top bar plus app launcher as the primary shell. Fixed default entries are Work, Team, Agents, and Skills; resource and experimental tools default into the launcher and can be pinned, moved, hidden, or restored.

Alternatives considered:

- Keep the first-slice Workbench dashboard and refine its panels.
- Put every feature in a left global sidebar.
- Use a pure browser-style multi-tab shell for everything.

Why chosen:

The user wants the first screen to be immediately actionable and not overloaded. A hybrid shell keeps the core path visible while allowing frequent tools to be pinned over time.

Consequences:

- `Workbench` becomes an internal run/session model, not the visible default page.
- Left sidebar becomes contextual to the active surface instead of global navigation.
- Navigation preferences need persistent user layout state.

## 2026-06-12 - Scope Knowledge and Quick Assistant as optional apps

Decision: Knowledge and Quick Assistant stay in the product, but default to optional app-launcher entries. Knowledge is only manually attached to ordinary single-agent chat; workspaces and Team do not use it by default.

Alternatives considered:

- Remove both features.
- Make Knowledge global for all chats and development runs.
- Put Quick Assistant settings under system settings only.

Why chosen:

Both features are useful but should not compete with the development workflow. Keeping them optional avoids clutter and prevents RAG from polluting coding tasks.

Consequences:

- Knowledge needs a multi-knowledge-base model rather than only a document pool.
- Chat input needs a manual knowledge selector.
- Quick Assistant should have a productized app entry and conservative trigger defaults.

## 2026-06-12 - Split focused apps before deep feature completion

Decision: For this slice, split Models, Agents, Search, and Quick Assistant into focused app surfaces first, while leaving deeper backend work such as named knowledge bases and dedicated MCP management for later slices.

Alternatives considered:

- Fully complete every backend domain before changing the shell.
- Keep Models/Search/Quick Assistant inside Settings until their backend is mature.

Why chosen:

The most painful user issue is orientation and visual overload. A focused shell makes future borrowing/refactor discussions easier because each feature has a clear home.

Consequences:

- Some focused pages still reuse existing backend primitives.
- MCP remains a transitional route using the Settings subpage.
- Visual QA and user feedback should drive the next depth pass.

## 2026-06-13 - Treat title controls as global chrome

Decision: `AppHeader` belongs above the sidebar/content row as application chrome. Work/Team can have a contextual sidebar, but Agents, Models, Skills, Knowledge, and other resource pages should not inherit a generic work-context rail.

Alternatives considered:

- Keep the header inside `main` and tune widths per page.
- Keep a generic left rail for every page.

Why chosen:

The user noticed that right-side controls shifted across pages and that "工作上下文" on Agents/resource pages felt conceptually wrong. App-level controls need a stable home, and context sidebars should only appear where they carry real context.

Consequences:

- Title-bar utilities stay aligned across pages.
- Non-work pages get more horizontal space.
- Future resource pages should define their own local layout instead of relying on `AppSidebar`.

## 2026-06-13 - Use native folder selection for workspaces

Decision: Workspace selection should open a system folder picker instead of requiring manual path entry.

Alternatives considered:

- Keep text entry with better placeholder/help text.
- Add drag-and-drop folder support first.

Why chosen:

Manual path entry is error-prone and does not match the user's expectation for a desktop app.

Consequences:

- Added a Windows-first `pick_directory` command.
- The workspace modal now shows a read-only path and a "选择" button.
- macOS/Linux picker support should be revisited if this app becomes cross-platform.

## 2026-06-13 - Model refresh replaces stale provider rows

Decision: When a provider model fetch/import returns a non-empty model list, clear that provider's existing `platform_models` rows before inserting refreshed rows. Detect Ark/Volces endpoint families before API-type fallbacks.

Alternatives considered:

- Keep old rows and append/update new rows.
- Only change the displayed fallback list without clearing stale rows.

Why chosen:

The user saw Claude rows for an Ark/Volces endpoint. Mixing stale fallback rows with new provider rows makes the model center untrustworthy.

Consequences:

- Provider refresh behaves more like a source-of-truth sync.
- Custom/manual model additions under the same provider may be replaced by a later successful fetch; if needed, manual-only models should get an explicit flag in a later data model.

## 2026-06-13 - Keep project protocol writes local and approval-oriented

Decision: Project protocol initialization may create project-local files only after explicit user confirmation, and archive/distillation creates database records and drafts instead of silently editing global skills or rules.

Alternatives considered:

- Auto-edit global Codex/Claude skill files during distillation.
- Only record plain logs and defer all distillation.

Why chosen:

The user wants an experience evolution loop, but protocol and skill changes can affect future behavior broadly. Drafts and local records preserve learning while keeping user control over rule changes.

Consequences:

- The first version is safe and inspectable.
- Applying a protocol or skill evolution proposal still needs product UI refinement.
- Future iterations can add richer diff application once review flows are mature.

## 2026-06-13 - Treat Agent model binding as a three-source choice

Decision: Agent model binding is represented as `default`, `builtin`, or `omnix`; builtin models are no longer encoded as the same value as the default option.

Alternatives considered:

- Keep only OMNIX platform models.
- Keep the prior default/builtin ambiguity and rely on labels.

Why chosen:

The user needs to preserve official Agent behavior while optionally routing through any enabled OMNIX model. These are different product choices and need distinct data.

Consequences:

- Existing saved bindings default to `omnix`.
- Builtin bindings are ignored by the OMNIX proxy router and left to the Agent's own runtime.
- Future UI can add per-agent explanations for what each builtin model means.

## 2026-06-13 - Ship NSIS when WiX MSI is blocked

Decision: Treat the NSIS installer plus release exe as the current Windows deliverable when the full Tauri bundle fails in WiX MSI `light.exe`.

Alternatives considered:

- Block delivery until MSI succeeds.
- Ship only the raw exe.

Why chosen:

The app compiled successfully and NSIS packaging succeeded. The MSI failure is isolated to the WiX bundling stage and should not block the user from installing/testing the current desktop build.

Consequences:

- Current installer path is `src-tauri/target/release/bundle/nsis/omnix-app_0.1.0_x64-setup.exe`.
- MSI needs a separate follow-up investigation, ideally outside sandbox with full WiX output.

## 2026-06-14 - Keep MSI required but do not ship suppressed-validation MSI

Decision: Keep MSI as a required Windows packaging target, add project scripts for MSI diagnostics/repair, and reject `light.exe -sval` output as a formal release artifact.

Alternatives considered:

- Change default packaging to NSIS only.
- Suppress MSI validation and ship the resulting MSI.
- Keep only informal manual WiX commands.

Why chosen:

The user explicitly wants MSI fixed. The `-sval` control proves the WiX object can link, but it bypasses the validation that makes MSI trustworthy. A repeatable diagnostic and elevated repair path is safer than silently producing a weakened MSI.

Consequences:

- `npm.cmd run tauri:build:msi` remains the acceptance command.
- `npm.cmd run diagnose:msi` captures WiX/Windows Installer state.
- `npm.cmd run repair:msi-env` must be run from elevated PowerShell when ICE validation fails.
- `diagnostic-sval.msi` must never be treated as the release MSI.

## 2026-06-23 - Fix the OMNIX product naming hierarchy

Decision: Use `OMNIX Workbench` as the formal product name, `OMNIX` as the compact UI label, `多 Agent 开发与协作工作台` as the Chinese descriptor, and `omnix-workbench` as the package/crate/binary slug. `Workbench` describes the entire product and must not become a navigation item or catch-all page.

Alternatives considered:

- Keep `OMNIX DevFlow` as the formal name.
- Use `OMNIX` alone everywhere.
- Use separate names for the desktop application and installer.

Why chosen:

The hierarchy keeps the brand concise inside dense UI while giving the installed product a clear, professional identity. It also prevents the previous architectural mistake where “Workbench” could become another overloaded page inside the workbench itself.

Consequences:

- Tauri product/window/installer metadata uses `OMNIX Workbench` and identifier `com.omnix.workbench`.
- Node and Cargo packages use `omnix-workbench`; the Rust library crate is `omnix_workbench_lib`.
- Existing `OMNIX DevFlow`, `omnix-app`, and `omnix_app` names are retired.
- The repository was subsequently renamed to `plnoble/OMNIX-Workbench` on 2026-06-23, and new clones must use that slug.
- The stable MSI `upgradeCode` remains unchanged so Windows Installer upgrade identity is preserved.

## 2026-06-23 - Adopt E1 as the provisional OMNIX Workbench icon

Decision: Use the E1 mark provisionally across product surfaces: a teal O for continuous development and accumulated experience, a coral X for multi-Agent convergence and capability multiplication, and a yellow center for experience distillation.

Alternatives considered:

- Keep the earlier generic circular icon.
- Use one of the E2-E4 refinements.
- Delay integration until a final trademark decision.

Why chosen:

E1 preserves the clearest O/X reading at taskbar size while carrying the develop, distill, and develop-again product idea. Applying it provisionally lets the product feel coherent without pretending the identity can no longer evolve.

Consequences:

- Master RGBA source is `src-tauri/icons/omnix-workbench-e1.png`.
- Tauri-generated PNG/ICO/ICNS/mobile variants, web favicon, title bar, status dock, release exe, and NSIS use E1.
- `build.rs` tracks `icons/icon.ico` and `tauri.conf.json` so icon changes rebuild Windows resources.
- E1 remains explicitly provisional and can be replaced after later visual testing.

## 2026-06-24 - Make runtime truthfulness the next product boundary

Decision: Deliver Claude Code and Codex as the first real single-Agent runtime before expanding Skills, Team, Knowledge, or additional Agent support.

Alternatives considered:

- Implement all visible modules in parallel.
- Build Team orchestration before fixing the single-Agent runtime.
- Keep the existing PTY parser and add more UI around it.

Why chosen:

Every later workflow depends on trustworthy model routing, permissions, structured events, persistence, process lifecycle, and resume semantics. Parallel feature work would compound the current placeholder behavior.

Consequences:

- Claude uses stream-JSON events and Codex uses app-server JSON; PTY is a labeled compatibility fallback only.
- Session model selection overrides Agent binding, which overrides the Agent default.
- Direct and plan modes are supported first; goal mode remains visible only as a Labs capability until it has a persistent verification loop.
- System CLIs are preferred and never overwritten; managed installs live in an OMNIX-owned prefix.
- Enabled models remain visible with explicit compatibility status instead of being silently accepted or hidden.
- Existing SQLite data is migrated in place; only exact known mock seed records are removed.

## 2026-06-24 - Separate runtime core events from the Tauri application handle

Decision: The runtime manager owns a Tokio broadcast channel and has no `tauri::AppHandle` dependency. Tauri setup forwards those events to `agent-session-event` for the UI.

Alternatives considered:

- Store `AppHandle` directly inside the runtime manager.
- Emit only from command handlers and let process readers return raw output.

Why chosen:

The process lifecycle must be testable without a desktop activation context, and background readers need one durable event path. Decoupling also keeps persistence-before-broadcast ordering explicit.

Consequences:

- Fake Claude/Codex executables can exercise the full runtime manager in `cargo test --lib`.
- Tauri is an output adapter rather than a dependency of the core runtime.
- Every structured event is persisted before it is broadcast to the frontend.

## 2026-06-24 - Treat model compatibility as an Agent protocol contract

Decision: Enabled OMNIX models remain visible, but each Agent adapter decides whether a model is native, gateway-compatible, unsupported, or unhealthy. Codex only enables OMNIX providers explicitly configured for the Responses wire protocol in the first milestone.

Alternatives considered:

- Let every enabled model appear selectable for every Agent.
- Hide incompatible models entirely.

Why chosen:

An enabled provider is not proof that its API protocol matches an Agent. Showing disabled options with reasons preserves discoverability without claiming false support.

Consequences:

- Selection precedence is session override, Agent binding, then Agent default.
- Claude supports verified Anthropic and translated gateway routes.
- Codex rejects OpenAI-compatible chat-only routes until a real Responses adapter exists.

## 2026-06-24 - Reuse the runtime adapter for Team instead of creating a second executor

Decision: Team managers and Workers run through the same Claude/Codex `RuntimeManager` used by single-Agent work.

Why chosen: Model routing, approval, persistence, structured events, process lifecycle, and resume semantics must not diverge between Work and Team.

Consequences:

- The manager produces a validated JSON dependency graph and cannot start Workers before explicit user approval.
- Workers persist real session IDs, dependency results, retry counts, approval state, and validation results.
- Unsupported Gemini/OpenCode Team execution remains unavailable rather than falling back to fake or parsed terminal output.

## 2026-06-24 - Distillation is an inbox, not an automatic mutation

Decision: Model-generated memory, Skill, and protocol improvements are stored as evidence-backed pending candidates.

Why chosen: Conversation inference is useful but not authoritative enough to edit project rules or global capabilities without review.

Consequences:

- Memory and Skill candidates write only after approval.
- Protocol candidates remain inspectable proposals; no global skill or source file is silently modified.
- The legacy direct auto-save distillation command is no longer exposed through Tauri IPC.

## 2026-06-24 - Desktop minimum size must account for Windows DPI scaling

Decision: Set the minimum window to 640x520 logical pixels, collapse pinned navigation labels below the desktop breakpoint, and use a short-height welcome state that removes secondary content before core controls are compressed.

Why chosen: At 150% scaling, 1024 logical pixels require approximately 1536 physical pixels and can exceed the usable width of a 1536-pixel display.

Consequences:

- Default 1280-wide windows retain text labels.
- Narrow windows retain icon access and tooltips while protecting fixed diagnostics/theme/settings controls.
- Visual acceptance must include a high-DPI minimum-size check, not only raw screenshot pixel dimensions.

## 2026-06-25 - Guard native/Tauri width mismatch and make visual QA DPI-aware

Decision: Compare Tauri's in-process `inner_size` with Win32 `GetClientRect`. Apply WebView zoom only when the native/reported width ratio is below 0.8. Native screenshot tools must opt into Per-Monitor-V2 DPI awareness before enumerating, resizing, or capturing windows.

Why chosen: The original half-width capture was produced by a DPI-unaware external PowerShell process whose `GetWindowRect` coordinates were virtualized. Inside the application, Tauri and native Win32 both reported the same full client width, and a DPI-aware capture showed the complete UI. The runtime guard still protects a genuine host mismatch without changing a normal mapping.

Consequences:

- Normal DPI and correctly mapped high-DPI systems keep zoom 1.
- Only a genuine native/reported mismatch receives a proportional correction; the tested host keeps zoom 1.
- No frontend `set-webview-zoom` permission or temporary diagnostic title is required.
- Minimum dimensions are 640x520, and the initial logical size is fitted to the active monitor before centering.
- Release screenshots are invalid evidence unless the capture process declares Per-Monitor-V2 awareness first.

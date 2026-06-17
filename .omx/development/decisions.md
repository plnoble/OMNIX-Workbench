# Decision Log

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

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

### 2026-06-25 - Agent default model not pre-selected after switching Agents

- Symptom: After setting a global default model (☆) and switching the Work page to another Agent (e.g. Claude Code), the model selector still showed "Agent 官方默认" instead of the configured default.
- Wrong assumption: Preserving the current `selectedModelId` across model-option reloads is always safe ("keep the user's choice if it is still selectable").
- Root cause: The `agent_default` option id is shared across all Agents and is always selectable, so when the Agent changed, the "keep current if still selectable" branch kept the previous Agent's `agent_default` selection and masked the new Agent's `is_default` option.
- Detection method: User acceptance — the backend correctly marked `is_default`, but the dropdown stayed on "Agent 官方默认".
- Fix: On Agent switch (`runtimeAgentId` change) always select the Agent's default (`is_default` then first selectable) instead of carrying over the previous selection. `src/components/tabs/ChatTab.tsx`.
- Prevention rule: When state is keyed by a value that changes (active Agent), do not preserve a selection identified by an id that is shared across those values; re-derive the default on the keyed change.
- Skill candidate: no.

### 2026-06-25 - Codex session start timed out at 5s and then broke the pipe

- Symptom: Starting a Codex turn failed with "Codex app-server did not return a thread id within 5 seconds"; a retry failed with "管道正在被关闭。(os error 232)".
- Wrong assumption: `thread/start` resolves almost immediately, so a fixed 5-second poll budget in `wait_for_external_session` is enough.
- Root cause: Codex 0.139.0 starts the MCP servers declared in `~/.codex/config.toml` synchronously while handling `thread/start`, so the response can arrive after 5s (the test user had 6 MCP servers, several failing slowly with "目录名称无效 os error 267"). When the 5s wait expired, the session was abandoned and the next stdin write hit the closing pipe (os error 232).
- Detection method: Replayed the exact handshake against the real `codex app-server --listen stdio://`; identical params returned a valid `result.thread.id` at ~6s but not at ~4s, and `RUST_LOG` showed MCP startup interleaved with the `thread/start` request.
- Fix: Raised the Codex thread-start budget to 30s (`CODEX_THREAD_START_TIMEOUT`), added eager child-process liveness detection in `wait_for_external_session` so a dead Codex yields an actionable error instead of a broken-pipe write, and moved the readiness wait into `launch_session` so failures surface at start and the first message is instant. Error text now points at the MCP config.
- Prevention rule: Never assume an external Agent's protocol responses are instantaneous; budget for MCP/tool boot, and detect process death before writing to its stdin.
- Skill candidate: yes — "probe the real CLI protocol/schema before trusting hard-coded timeouts and message shapes".

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

### 2026-06-23 - Updated icon files did not update the Windows executable

- Symptom: Tauri-generated PNG/ICO files showed E1, but direct PE extraction from the rebuilt release executable still returned the old icon.
- Wrong assumption: Replacing `src-tauri/icons/icon.ico` and running `tauri build` would necessarily regenerate the Windows resource library.
- Root cause: Cargo did not have an explicit change dependency for `icons/icon.ico`, so the existing `resource.lib` was reused even after package cleaning/relinking.
- Detection method: Parsed RT_GROUP_ICON and RT_ICON resources directly from the PE instead of trusting source files or Windows Explorer cache.
- Fix: Added `cargo:rerun-if-changed=icons/icon.ico` and `cargo:rerun-if-changed=tauri.conf.json` in `src-tauri/build.rs`, then rebuilt and rechecked PE resources.
- Prevention rule: Treat application icons as build inputs. After icon changes, verify the packaged PE resource, not only `src-tauri/icons`.
- Skill candidate: yes

### 2026-06-23 - Chroma-key intermediate path was not writable

- Symptom: Copying the generated icon source to `C:\tmp` failed with access denied.
- Wrong assumption: The nominal temporary root would be writable from the current managed desktop session.
- Root cause: This session's effective filesystem permissions did not allow that destination despite the broader workspace policy.
- Detection method: PowerShell `Copy-Item` returned `UnauthorizedAccessException` before chroma removal started.
- Fix: Used the workspace-local `tmp/imagegen` directory for intermediates.
- Prevention rule: Prefer workspace-local temporary directories for image-generation post-processing in managed desktop sessions.
- Skill candidate: no

### 2026-06-24 - Multiple Cargo test names were passed as separate filters

- Symptom: `cargo test test_a test_b --lib` failed before running tests with an unexpected-argument error.
- Root cause: Cargo accepts one positional test-name filter, not multiple independent filters.
- Fix: Run each exact filter separately or use one shared substring; use `cargo test --lib` for the final gate.
- Prevention rule: Do not treat Cargo test filters like a list of test targets.
- Skill candidate: no

### 2026-06-24 - File picker function was inserted inside a PowerShell raw string

- Symptom: Rust reported many misleading unknown-prefix errors on ordinary strings such as `powershell.exe` and an unterminated quote.
- Root cause: The patch matched the first closing brace inside `pick_directory`'s raw PowerShell string, so the new Rust function became string content.
- Fix: Restore the raw-string terminator and place `pick_file` after the complete `pick_directory` function.
- Prevention rule: When patching functions containing embedded scripts, anchor edits on the Rust function boundary, not a brace inside the script body; run `cargo check` immediately.
- Skill candidate: yes

### 2026-06-24 - Status Dock drag handler swallowed its click action

- Symptom: Clicking the floating status card did not bring the main window forward even though a click handler existed.
- Root cause: The whole card started native window dragging on every left-button `mousedown`, preventing the normal click sequence.
- Fix: Make the card clickable and restrict `startDragging()` to the six-dot grip.
- Prevention rule: A control cannot safely use the same full surface for immediate native dragging and click activation; give dragging a dedicated handle or movement threshold.
- Skill candidate: yes

### 2026-06-24 - 1024 logical pixels overflowed a 1536-pixel Windows display

- Symptom: At 150% display scaling, the minimum-sized window still lost its right-side content.
- Root cause: Tauri window dimensions are logical pixels; 1024 logical pixels map to roughly 1536 physical pixels before borders and available-work-area constraints.
- Fix: Lower the minimum to 860x600 logical pixels and collapse pinned labels at narrow widths.
- Prevention rule: Validate desktop minimum dimensions under common DPI scales (100%, 125%, 150%), not only one physical screenshot resolution.
- Skill candidate: yes

### 2026-06-23 - One-off Node/libuv failure after a successful frontend build

- Symptom: One NSIS attempt finished Vite output, then Node 24.14.0 aborted in `src\win\async.c` while Tauri was closing `beforeBuildCommand`.
- Wrong assumption: A successful Vite output guarantees the parent Tauri process will observe a clean child exit.
- Root cause: The exact command passed on immediate reproduction without code changes, indicating an intermittent Node/libuv Windows process-shutdown failure rather than deterministic application or icon configuration failure.
- Detection method: Re-ran the identical `npm.cmd run tauri:build:nsis` command and compared the failure boundary and exit code.
- Fix: No code workaround was added; the successful reproduction run generated the package normally.
- Prevention rule: For this specific assertion, rerun once after confirming standalone `npm.cmd run build` passes; only change code if the failure reproduces consistently.
- Skill candidate: no

### 2026-06-24 - Runtime tests inherited slow production database seeding

- Symptom: A narrow session-persistence test hung even though it only needed conversations, messages, sessions, and events.
- Wrong assumption: Reusing the full `DbManager::new_with_path` initialization was harmless in unit tests.
- Root cause: Production initialization performs broad schema setup and seed work unrelated to the runtime test, including paths already known to be slow on Windows.
- Detection method: Replaced the fixture incrementally and observed the test complete immediately with only runtime tables.
- Fix: Added a minimal `new_runtime_test` fixture for deterministic runtime persistence tests; broad seeded-database tests remain explicit manual integration tests.
- Prevention rule: Unit-test fixtures should initialize only their owned schema. Do not make a narrow domain test pay for production seeding.
- Skill candidate: yes

### 2026-06-24 - Tauri AppHandle made the runtime test binary fail before tests ran

- Symptom: Runtime-manager tests failed with `STATUS_ENTRYPOINT_NOT_FOUND` before executing assertions.
- Wrong assumption: Holding a Tauri `AppHandle` in the core process manager would remain inert during Rust unit tests.
- Root cause: Linking the desktop handle path pulled in a Windows Common Controls activation-context dependency that the lib-test process did not initialize.
- Detection method: Import inspection identified `comctl32.dll!TaskDialogIndirect`; removing the AppHandle dependency allowed the same test binary to start.
- Fix: Runtime core now broadcasts internal events; Tauri setup forwards them to the window event bus.
- Prevention rule: Keep desktop framework handles out of testable runtime/process cores. Use an event adapter at the application boundary.
- Skill candidate: yes

### 2026-06-24 - Windows npm detection could choose a non-executable shim

- Symptom: PowerShell resolved `claude`/`codex` to `.ps1` shims that were blocked by execution policy, while `.cmd` worked.
- Wrong assumption: Any result from PATH discovery is equally safe to pass to a Windows child process.
- Root cause: npm installs extensionless, PowerShell, and command shims side by side; resolution differs by caller and policy.
- Detection method: Compared `Get-Command -All`, direct version calls, and the paths returned for installed CLI names.
- Fix: Windows Agent detection prefers an adjacent `.cmd` for extensionless or `.ps1` results; a regression test creates all three shim forms.
- Prevention rule: Normalize Windows npm CLI paths before persisting or spawning them, and test the exact shim type selected.
- Skill candidate: yes

### 2026-06-24 - Host approval limit blocked packaging and automated visual QA

- Symptom: The approved non-sandbox Tauri bundle command and background Vite server were rejected before execution; the in-app browser also blocked local `file://` navigation.
- Wrong assumption: Passing all code gates implied the current session could always complete desktop packaging and browser screenshots.
- Root cause: Host execution usage/approval limits and browser URL policy are external to the repository.
- Detection method: The approval system rejected the commands before process output or artifact timestamps changed.
- Fix: No bypass was attempted. Existing artifact timestamps were inspected and explicitly marked stale for this milestone.
- Prevention rule: Verify timestamps after every release command. When execution is blocked externally, report packaging and visual QA as pending instead of reusing old artifacts.
- Skill candidate: no

### 2026-06-24 - rustfmt recursively reformatted the Rust module tree

- Symptom: Running `rustfmt src-tauri/src/lib.rs` touched child modules and produced a large unrelated diff before failing on pre-existing trailing whitespace in `proxy.rs`.
- Wrong assumption: Direct rustfmt on the crate root would format only the named file.
- Root cause: rustfmt follows `mod` declarations unless child traversal is disabled.
- Detection method: `git diff --stat` showed newly modified Rust files that were clean immediately before the command.
- Fix: Restored only the known-clean files changed by that command and retained the intentional files. Did not revert pre-existing or semantic work.
- Prevention rule: For one Rust file in a dirty tree, use `rustfmt --config skip_children=true` from the correct working directory, or let `cargo fmt` run only when whole-repository formatting is explicitly intended.
- Skill candidate: yes

### 2026-06-24 - Parallel release gates caused a transient Vite output-name error

- Symptom: A parallel `tsc`, Vite build, and Cargo check run failed during `generateBundle` because Rollup received an absolute `index.html` output name; an isolated `npm.cmd run build` passed immediately afterward.
- Wrong assumption: All independent-looking compile gates are safe to run concurrently against shared generated output.
- Root cause: The gate processes share build/output metadata and can race on Windows even when only one command visibly invokes Vite.
- Detection method: Confirmed no dev server or build process remained, then reran the same build alone successfully.
- Fix: Final release gates run sequentially for commands that read or write `dist` or Tauri generated context.
- Prevention rule: Parallelize read-only inspection, not release builds sharing `dist`, `target`, or generated Tauri context.
- Skill candidate: yes

### 2026-06-25 - DPI-unaware screenshot process created false half-window clipping

- Symptom: External PowerShell `GetWindowRect` and screen captures showed an approximately half-width window and appeared to prove that the right side of the WebView was clipped.
- Wrong assumption: Coordinates returned to the external capture process were physical pixels and could be compared directly with in-app CSS/native measurements.
- Root cause: Windows DPI virtualization scaled coordinates for the DPI-unaware PowerShell process. The application itself reported a 2560-pixel client width through both Tauri `inner_size` and in-process Win32 `GetClientRect`, while the external process reported about 1293 pixels.
- Detection method: Repeated window enumeration and capture after calling `SetProcessDpiAwarenessContext(PER_MONITOR_AWARE_V2)`; the resulting 2586x1655 window showed the complete UI.
- Fix: Make native QA scripts DPI-aware before enumeration/capture; retain only a guarded in-process native/reported-width correction for real host mismatches. Remove temporary frontend zoom permission and diagnostic title code.
- Prevention rule: High-DPI visual evidence must state the capture process DPI-awareness context. Never infer WebView clipping from a DPI-virtualized screenshot alone.
- Skill candidate: yes

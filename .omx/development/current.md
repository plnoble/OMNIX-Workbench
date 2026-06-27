# Current Development State

Project: OMNIX Workbench

## Objective

Complete the full OMNIX Workbench real-gap plan: finish and verify single-Agent runtime, Skills/distillation, Team scheduling, Knowledge/Quick Assistant, fresh Windows packages, and desktop-delivered test/manual documentation.

## Status

The complete real-gap release candidate is implemented and packaged. Claude/Codex single-Agent runtime, real Skill market/fusion approval, evidence-backed distillation, dependency-aware Team scheduling, named Knowledge bases, conservative Quick Assistant capture, responsive desktop chrome, and DPI-aware release QA are connected. TypeScript, Vite, Cargo check, all non-ignored Rust library tests, WiX MSI, NSIS, release launch, full-window visual QA, minimum-window visual QA, and Desktop document delivery pass.

## Current Branch

`feat/msi-packaging-and-refactor-slice`, tracking `origin/feat/msi-packaging-and-refactor-slice`.

The worktree contains extensive pre-existing uncommitted naming, icon, packaging, and refactor changes. Preserve and build on them; do not revert unrelated files. Preserve `.omx/icon-candidates/` as design history.

## Delivery Scope

- Claude Code and Codex only for the first supported runtime milestone.
- Structured runtime events: Claude stream JSON and Codex app-server JSON, with PTY retained only as an explicitly labeled compatibility fallback.
- Session model precedence: session override, Agent binding, then Agent default.
- Direct and plan modes are supported. Goal mode moves to Labs until persistent run/retry/verification exists.
- System CLI takes priority; missing tools are installed to an isolated OMNIX-managed prefix.
- Existing SQLite data, including model providers and API keys, is migrated in place.
- First package checkpoint occurs after the single-Agent workflow passes acceptance. Skills/distillation, Team, and Knowledge/Quick Assistant follow in that order.

## Risks

- Claude and Codex expose different event, approval, model, and resume protocols; unsupported behavior must be shown honestly instead of silently falling back.
- Model compatibility is protocol-specific. Enabled models remain visible, but incompatible routes must be disabled with a reason.
- Runtime processes and database writes must not hold synchronous locks across `await`.
- Full-access permission is per-session and requires explicit confirmation; it must not become a persistent global default.
- Packaging must preserve the current product name, E1 icon, Tauri identifier, and WiX upgrade code.

## Acceptance Criteria

- Detect or install a real Claude Code/Codex CLI without creating mock executables.
- Choose Agent, workspace, model, permission, and direct/plan mode; prove those values reach the runtime.
- Persist user, assistant, tool, approval, error, and terminal/runtime records.
- Start, send, approve, stop, resume, restart the app, and restore the conversation history.
- Show real workspace files, Git branch, and changes instead of placeholder text.
- Stop seeding mock conversations and remove only known mock IDs through an idempotent migration.
- Remove Team simulation actions until real scheduling is implemented.
- Pass TypeScript, Vite, Rust checks/tests, desktop visual smoke tests, and Windows packaging.

## Baseline Verification

On 2026-06-24 before implementation:

- `npx.cmd tsc --noEmit` passed.
- `npm.cmd run build` passed with existing dynamic/static import chunk warnings.
- `cargo check` passed with 126 existing unused/dead-code warnings.
- `prompt_guard::tests::test_injection_detection_data_exfil` passed.

## Next Action

Three fixes/features are implemented in the worktree (not yet in a fresh package), all gate-verified:

1. Codex `thread/start` 5s timeout → broken pipe. Raised to a 30s budget with process-death detection and start-time readiness wait (`runtime_manager.rs`).
2. All Agents shared one chat. Switching Agent now switches to that Agent's own latest conversation; sidebar lists only the active Agent's conversations (`useConversations.ts`, `App.tsx`, `AppSidebar.tsx`).
3. Codex/Claude default model now reaches the runtime (roadmap P1). New `responses_bridge.rs` translates Responses↔Chat in the session gateway so any provider works with Codex; binding/global-default resolution + `is_default` pre-selection in the Work page. Validated end-to-end against real `codex app-server`.

Gates pass: `cargo test --lib` (76 passed, 4 ignored), `cargo check` (90 warnings), `tsc --noEmit`, `npm run build`. Reference projects cloned to gitignored `scratch/`; the full borrowing roadmap (P2 model center, P3 MCP sync, P4 Team board, P5 assistants/skills) is in the approved plan, not yet implemented.

Next: rebuild (dev or release) and have the user (a) re-run the Codex turn and Agent-switch flow, and (b) configure a real Chat-Completions provider (DeepSeek/Volcano) as the Codex default and confirm Codex uses it through the translating gateway. Gemini CLI, OpenCode, goal pursuit remain Labs/future work.

## Borrowing roadmap status (2026-06-25)

- P1 (Codex/Claude default model via translating gateway): done, user-accepted.
- P2 (unified model center: global ☆ Agent default, provider presets; capability icons + health already existed): done, user-accepted.
- Acceptance UX rounds (Codex timeout, per-Agent chat, 对话/工作 split, tab preload, waiting indicator, workspace-required Work, portaled delete dialog, two-default-model naming, ☆ pre-select fix): done, user-accepted.
- P3 (MCP one-config sync to ~/.claude.json + ~/.codex/config.toml, with backup/validate/undo): done. MCP also split into a focused page (removed from Settings).
- P4 (Team board: Worker dependency DAG + status summary over team_get_run_detail): done.
- P5a (office assistant presets), P5b (workspace → skill generation in SkillHub): done.
- P5c (remote access + unattended cron): assessed as already implemented; no filler built.
- Borrowing roadmap P1–P5 complete and **released as 0.2.0**: fresh standalone exe + NSIS + MSI built and copied to Desktop `OMNIX Workbench 0.2.0`; version bumped; CHANGELOG written; all work committed and pushed to `origin/master`; the `feat/msi-packaging-and-refactor-slice` branch deleted (master is the sole branch + GitHub default).
- Next action: user acceptance of the 0.2.0 standalone build (especially the live paths that need real credentials — Codex with a Chat-Completions provider via the translating gateway, MCP sync write to real agent config, and a real Team run). The dev app still runs via `npm run tauri dev` for iteration.

## Desktop-app roadmap (R-series, started 2026-06-26)

Approved plan: `~/.claude/plans/piped-stargazing-eagle.md` — R1 checkpoints+diff, R2 panel workspace+file preview, R3 worktree parallel+background, R4 context-compaction+token UI+hooks, R5 划词助手 deepen + notes + Cherry suite, R6 Cowork/computer-use.
- **R1 (检查点/回滚 + 逐文件 diff) — done in the worktree, not yet committed/released.** Shadow-ref Git checkpoints (`commands/checkpoints.rs`), auto-checkpoint before Direct-mode workspace turns, `WorkspaceCheckpoints` panel in ChatTab. `cargo test --lib` 80 passed. Live test: run a Direct-mode turn in a real Git workspace, confirm a checkpoint appears, the diff shows per-file changes, single-file 还原 works, 回退 restores, and `git log` stays clean.
- **R2 (文件预览面板) — done in the worktree.** `read_workspace_file` (`commands/workspace.rs`) + `FilePreviewPanel.tsx` (image/PDF/markdown/code/text inline, binary/Office → system app). Click a workspace file to preview.
- **R3 (worktree 并行会话) — done in the worktree.** `commands/worktrees.rs` (create/list/merge/remove isolated worktrees, dirty/ahead badges, conflict-safe merge) + `WorktreePanel.tsx` in ChatTab. `cargo test --lib worktrees` passed.
- **R4 (Token 活动消耗 + Hooks + 远程增强) — done in the worktree.** Token/cost panel (`TokenActivityPanel.tsx`, cost + daily chart over `request_logs`); user-state hooks engine (`commands/hooks.rs`, event→action, fired from the runtime event loop) with its own focused page; remote `POST /api/remote/send` (drive a session from the phone). `cargo test --lib hooks` passed. Architecture finding recorded: the agent runtime delegates to external CLIs which own their context window/compaction — so OMNIX does **not** build an in-runtime auto-compaction gauge (see memory `runtime-delegates-context`).
- **R5 (划词助手深挖 + 笔记 + 多模型同对话 + 知识库/助手库/翻译 polish) — DONE in the worktree.** Custom Quick Actions (`commands/quick_actions.rs` + `QuickActionsEditor.tsx`); brand-new Notes (`commands/notes.rs` + `NotesTab.tsx`, with QA 存为笔记); always-on selection auto-popup fix (`selection.rs`); **多模型同对话** (`CompareHub` multi-turn per-model threads); plus the final polish — 知识库 citation copy (`KnowledgeHub.tsx` 引用 button; multi-source attribution already existed), 翻译历史面板 (`TranslationHistoryPanel.tsx`, surfacing `get_translation_history`), 助手库 custom assistants + import/export share (`commands/custom_assistants.rs` + `AssistantsTab.tsx`). **R5 complete.**
- **R6 (Cowork / computer-use) — REMOVED per user.** The user wants neither the manual computer-use testbed nor autonomous agent control; only remote phone access. `cowork.rs` / `CoworkTab.tsx` deleted, `xcap`/`png` deps dropped.
- **远程手机访问（AionUi 风格）— done in the worktree (incl. QR + 手机审批).** Phone web app to view + continue OMNIX agent conversations (NOT remote desktop control). `proxy.rs` has `runtime_manager` + token-gated endpoints (conversations/messages/chat/agents/workspaces/new + pending/respond for approval) + `remote_access_enabled` LAN binding; `set_remote_access` restarts the proxy; `remote_dashboard.html` is a mobile chat UI with an approve/deny bar; `DashboardTab` enable toggle + URL/copy + **QR code** (`qrcode` lib). Cross-network reach is the user's own tunnel. Honest limit: structured approval回传 is Codex-only. The remote feature is the last roadmap item before packaging 0.3.0.
- All R-series changes are on `master` (worktree) and will need committing + a future package bump (0.3.0) once a batch is accepted. (Per user: package after R6 + remote are done.)

### R-series backlog (deferred follow-ups, intentionally not faked)

- **R2 follow-up — arrangeable multi-pane workspace + embedded terminal pane.** The workspace panel currently stacks checkpoints/diff + worktrees + file tree + a single preview drawer. A fuller Claude-Code-desktop "Code tab" would let the user drag/rearrange multiple panes (diff | preview | terminal | plan) and embed the existing PTY (`terminal.ts`) as a live pane. **Office (docx/xlsx/pptx) in-app rendering is intentionally deferred** (opens with the system app) to avoid a heavy viewer dependency.
- **R3 follow-up — in-session background-task / sub-agent panel: DONE** (`commands/subagents.rs` + `SubAgentPanel.tsx`). A sub-agent is an independent child session in its own worktree, concurrent with the parent (real session-level overlap). **Still pending: process overlap (pipelining)** — concurrent turns reconciled into ONE session — which needs turn-engine changes in `runtime_manager.rs` (std-mutex-across-await constraint, event ordering) and real correctness risk; intentionally not built.
- **R4 (a) context-window meter — DONE** (accurate, scoped to the OMNIX-stored transcript). `ContextMeter.tsx` in the chat composer over `get_context_budget` (CJK-aware) + `compact_conversation_context`. Option (b) — a labeled estimate for agent-CLI live context — was not built (OMNIX can't see the CLI's true context).
- **R2 follow-up still pending** — arrangeable multi-pane workspace + embedded terminal pane (Office in-app rendering stays deferred).

## Naming Contract

- Product: `OMNIX Workbench`
- Compact UI brand: `OMNIX`
- Descriptor: `多 Agent 开发与协作工作台`
- Package/binary slug: `omnix-workbench`
- Tauri identifier: `com.omnix.workbench`
- Repository: `plnoble/OMNIX-Workbench`
- Provisional icon: E1 from `src-tauri/icons/omnix-workbench-e1.png`

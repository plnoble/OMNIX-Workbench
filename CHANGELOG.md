# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.4.0] — 2026-07-03

The "self-evolution loop + multi-agent runtime" release: OMNIX now learns from every project and can drive four more coding agents.

### Added
- **Multi-agent runtime via a universal ACP adapter** — Gemini CLI, Qwen Code, OpenCode, and GitHub Copilot CLI are now first-class runnable agents (previously only Claude Code + Codex). All four speak the Agent Client Protocol (JSON-RPC 2.0 over stdio); a single adapter (`runtime_acp.rs`) drives them, so adding a future ACP agent is one `agent_definition` entry. The transport is bidirectional: OMNIX serves the agent's `fs/read|write_text_file` requests (workspace-constrained) and its `session/request_permission` requests (auto-approved under full access, auto-rejected in plan mode, otherwise surfaced for your decision).
- **In-app model selection for ACP agents** — the composer shows the agent's own model list (from `session/new` config options) and switches it live via `session/set_config_option`; the choice is remembered per-agent for the next session.
- **Self-evolution loop** — OMNIX records runtime errors/approvals as a project protocol, distills them (three sources: conversation + OMNIX-recorded signals + the agent's own protocol notes) into reusable "lessons", and injects the most relevant ones into every new workspace's agent-native context file (CLAUDE.md / AGENTS.md / GEMINI.md / …). Lessons are deduped by embedding similarity and their effectiveness is tracked. A new **进化中枢 (Evolution Hub)** panel reviews/applies proposals and shows protocol status & events.
- **Agent process-crash detection** — if an agent process exits unexpectedly, the session is marked failed with an actionable message instead of hanging on "running" forever.
- **Resizable Quick Assistant window** — the 划词 popup can be dragged and resized (East/South/SouthEast grips), with size persisted.

### Changed
- **Quick Assistant rewritten for Cherry Studio parity** — no flicker, no cursor-following, never steals focus mid-selection (fixes broken copy); reads the selection once on mouse-up. Click-away dismissal now tracks the live (draggable/resizable) window bounds.
- **Embedding model is now a single fixed setting** shared by memory vectors, workspace profiles, and the knowledge base.
- **Frontend agent registry is backend-driven** — the UI loads the runnable-agent list from `runtime_get_agent_catalog` instead of hardcoded per-component maps; runtime dispatch uses a typed `AdapterKind` enum so a new adapter fails to compile until every dispatch site handles it.
- ACP reasoning ("thinking") now renders as a collapsible block instead of blending into the reply; the reply is consolidated and persisted so it survives a conversation reload.
- Sidebar gained an explicit **历史与归档 (History & Archive)** entry (the icon-only entry point was undiscoverable).

### Fixed
- OpenCode produced empty turns because its default ACP model was unusable; OMNIX now fixes the default and lets you pick a working model in-app.
- ACP sessions no longer fail with "Session not found" after an app restart (a dead in-memory session is replaced with a fresh one; the OMNIX transcript is preserved).
- StatusDock now applies the light/dark theme (it was stuck on dark).
- Removed a large amount of dead code and hardcoded colors (now design tokens).

### Docs
- Added `BORROWINGS.md` — registers each borrowed feature (Cherry Studio 划词, AingDesk search injection, ACP, Codex app-server) with its behavior contract, so re-borrowing after an upstream update is a contract-checked regression rather than a guess.

## [0.3.0] — 2026-06-27

The "desktop-app roadmap" release (R1–R5) plus a phone remote-access feature and a large round of user-acceptance fixes.

### Added
- **Workspace checkpoints + per-file diff review (R1)** — before a Direct-mode workspace turn, OMNIX auto-snapshots the working tree onto a Git shadow ref (`refs/omnix/checkpoints/…`, no commit pollution); a timeline lets you rewind, and a per-file diff lets you accept/reject single files (`commands/checkpoints.rs`, `WorkspaceCheckpoints.tsx`).
- **In-app file preview (R2)** — click a workspace file to preview images/PDF/Markdown/code inline; Office/binary open with the system app (`read_workspace_file`, `FilePreviewPanel.tsx`).
- **Parallel sessions via Git worktrees (R3)** — spin up an isolated `omnix/<branch>` worktree per session, with dirty/ahead badges and conflict-safe merge; plus an in-session **background sub-agent** panel (each runs in its own worktree, concurrent with the parent) (`commands/worktrees.rs`, `commands/subagents.rs`).
- **Token activity & cost panel (R4)** — surfaces the collected `request_logs` usage with estimated cost and a daily chart (`TokenActivityPanel.tsx`).
- **User-state hooks (R4)** — event→action rules (notify / shell command / log) fired from the runtime event loop, with their own page (`commands/hooks.rs`, `HooksTab.tsx`).
- **Context-window meter (R4)** — an accurate token gauge over the OMNIX-stored conversation transcript, with one-click compaction.
- **Custom Quick Assistant actions + always-on selection popup (R5)** — define your own prompt-based 划词 actions; the popup now appears next to the cursor on selection (no copy, no hotkey) (`commands/quick_actions.rs`).
- **Notes (R5)** — local Markdown notes (mirrored to `~/.omnix/notes/*.md`), with "save from Quick Assistant", "save agent message to notes", and an optional notes-MCP for agents.
- **Dedicated translation page (R5)** — Google/有道-style two-pane multilingual AI translation with history.
- **Multi-model same-conversation (R5)** — CompareHub upgraded from one-shot to a multi-turn side-by-side conversation per model.
- **Custom assistants + import/export (R5)** — create your own assistants and share them as JSON.
- **Knowledge-base export/import** — move a knowledge base (documents + chunks + embeddings) between machines as a portable `.omnixkb.json`.
- **Phone remote access (AionUi-style)** — enable LAN binding to view + continue your agent conversations from a phone (chat thread, send, start new session, approve/deny), with a QR code on the dashboard. Cross-network reach is the user's own tunnel.

### Changed
- Navigation simplified to two tiers: 固定(标题栏) / 收纳(宫格); the 隐藏 tier was removed.
- Translation now uses the unified "内置功能默认模型" — no separate model picker.

### Fixed
- **Translation never displayed** despite succeeding — Rust↔TS field-name mismatches on the translate result and history; now use dedicated structs (`TranslateResult`/`TranslateHistoryEntry`).
- Select dropdowns and toasts rendered dark-on-dark in light theme (now use theme `popover` tokens).
- Quick Assistant popup follows the app theme, is movable/closeable, and auto-dismisses; the "auto-capture" toggle now persists correctly.
- Remote access info served `local_ip`/`connection_url` but the UI read `ip`/`url` (blank link/QR); aligned via serde rename.
- Folder-open permission fixed; the notes folder opens via a native command.

## [0.2.0] — 2026-06-26

A large feature release driven by reference-project borrowing (cc-switch, AionUi, Cherry Studio) and user acceptance testing.

### Added
- **Codex/Claude default model via a translating gateway** — the OMNIX session gateway now translates between Codex's Responses API and providers that only speak Chat Completions (DeepSeek, Volcano, most OpenAI-compatible relays), so any configured provider works with Codex (`responses_bridge.rs`). Validated end-to-end against a real `codex app-server`.
- **Unified model center (P2)** — a global ☆ "Agent default" model (used when an Agent has no binding), plus quick provider presets in the add-provider form (DeepSeek, 火山 OpenAI-compatible, OpenAI, Anthropic, SiliconFlow, GLM, Kimi, 百炼, Ollama, LM Studio). Capability icons and health checks surfaced in the model list.
- **MCP one-config sync (P3)** — sync OMNIX MCP servers into the agents' native config: Claude `~/.claude.json` and Codex `~/.codex/config.toml` (syntax-preserving via `toml_edit`). Backs up before writing, merges only, validates output, supports per-agent undo. MCP is now its own focused page.
- **Team collaboration board (P4)** — a layered Worker dependency DAG colored by live status, plus a per-status count summary, in the Team tab.
- **对话 / 工作 split** — distinct Chat (no workspace) and Work (workspace-required) surfaces; each Agent keeps independent conversation history; switching Agent loads that Agent's own conversation.
- **Office assistant presets (P5a)** — PPT/Word/Excel/学术论文/会议纪要/周报 assistants (leveraging the bundled pptx/docx/xlsx skills); 53 built-in assistants total.
- **Skill generation from a workspace (P5b)** — scan a project, select files, and have a model generate a SKILL.md draft to save as a local skill.
- First-token waiting indicator and idle preloading of tab chunks for smoother navigation.

### Changed
- Global default model resolution order: session override → Agent binding → global default → Agent default.
- Settings "默认大模型" renamed to "内置功能默认模型" and clarified as distinct from the Agent default model.
- MCP and the model center are focused pages; Settings now only holds system + data backup.

### Fixed
- Codex session start no longer times out at 5s and breaks the stdin pipe — the `thread/start` budget is 30s with process-death detection, accommodating Codex booting MCP servers.
- The Agent model selector now re-derives the default on Agent switch (was masking the new Agent's default with the shared "Agent default" option).
- The Work surface no longer auto-defaults a stale workspace; a workspace is required before sending.
- The conversation delete-confirm dialog no longer overflows the sidebar (portaled to the document body).

## [0.1.0] — 2026-06-12

### Security
- **P1**: Directory traversal defense — `validate_file_path()` rejects absolute paths, `..` components, and symlink escapes to system directories
- **P2**: Content Security Policy enabled — `default-src 'self'`, restricted `connect-src`, no inline scripts
- **P3**: API keys encrypted at rest — AES-256-GCM via `getrandom` CSPRNG, Windows DACL key file permissions via icacls
- **P4**: `.gitignore` completeness — added `target/`, `*.exe`, `*.msi`, `.omnix/`, `cherry-studio-ref/`
- **P5**: SQLite indexes — 9 indexes on high-traffic columns (messages, platform_models, request_logs, etc.)
- **P6**: reqwest timeout — all 7 HTTP clients now have 30s timeout (no more hanging connections)
- **M1**: Encryption key generation — removed weak time+PID fallback, `getrandom::getrandom()` panics clearly on failure
- **M2**: Input validation — `validate_id()`, `validate_name()`, `validate_workspace_path()` applied to 20+ commands
- **M3**: Removed Google Fonts CDN — fonts use system-ui / Cascadia Code fallbacks (no external requests)

### Added
- Conversation archive/unarchive with fullscreen history view
- Team collaboration textarea auto-resize (Shift+Enter newline, Enter send)
- ChatTab orphan model detection with auto-heal
- Volcano/Ark model fetch — hardcoded doubao model list (avoids tenant-wide API)
- Search providers: Google, Bing, Tavily, Exa, Zhipu, Bocha, Jina (Rust implementations)
- Skill YAML frontmatter (Multica-inspired)
- Skill DAG typed dependency graph
- Prompt injection detection (Odysseus)
- Model capability auto-detection (Cherry Studio pattern)
- Per-agent API provider binding (CC Switch inspired)
- Circuit breaker + session usage tracking + model pricing
- Async mailbox, task dependencies, persistent cron, YOLO mode
- `input_validation` module — shared ID/name/path validation for all Tauri commands

### Changed
- Default theme: "跟随系统" (auto) instead of "dark"
- Account seeding: `seed_completed` guard prevents re-seeding after user deletion
- Cron task seeding: same guard pattern
- Font size normalization: eliminated all `text-[9px]`/`text-[10px]`/`text-[11px]` → `text-xs`/`text-sm`
- Theme-aware colors: `text-white` → `text-foreground`, `bg-black/N` → `bg-muted/N` throughout
- QuickAssistant: auto-capture + action bar + model dropdown
- SettingsTab: top tab bar + platform list always visible
- AppSidebar: narrow (w-56), archive/delete buttons per conversation
- CompareHub: theme-aware, larger system prompt textarea, button-style quick templates
- MemoryHub: card-based layout with shadows, theme-aware modals
- SkillHub: 2-row header with flex-wrap toolbar, theme-aware backgrounds

### Fixed
- Conversation deletion: Tauri camelCase↔snake_case parameter mapping (`{ id }` → `{ conversationId: id }`)
- Volcano model fetch: `/api/v3/models` returns all tenant models — now uses hardcoded list
- Model mapping showing only mimo-v2-pro: removed stale fallback, added orphan detection
- Global shortcut crash: Alt+Space already registered by system
- Selection assistant auto-capture: UIA polling + clipboard fallback
- Translation/detection: connected to platform models instead of separate config
- Textarea auto-expand in ChatTab and TeamTab
- White text invisible in light theme across CompareHub, MemoryHub, SkillHub
- SkillHub right-side text overlapping
- Fire mountain (Volcano) model fetch — final fix with hardcoded doubao list
- `crypto.rs`: removed unused `OsRng` import

### Removed
- Google Fonts CDN links from `index.html` (privacy + CSP compliance)
- `text-[9px]`, `text-[10px]`, `text-[11px]` arbitrary pixel sizes (125+ occurrences)

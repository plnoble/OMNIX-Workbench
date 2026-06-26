# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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

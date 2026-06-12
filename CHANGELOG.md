# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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

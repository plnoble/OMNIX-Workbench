# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

> **0.5.0 起的条目由发版提交生成。** 每条是那次发版提交的标题，后面括号里是
> 提交号——完整说明（改了什么、为什么、升级影响）在提交正文里，`git show <hash>`
> 就能看到，GitHub Releases 页面也有。
>
> 这份文件此前停在 0.4.0，而应用已经发到 0.28.0；更新器的说明里却写着「详见
> CHANGELOG」，指向一份对不上的文件。补齐用的是真实提交记录，没有事后追写。

## [0.29.2] — 2026-08-14

安全与数据完整性修复（Grok 审核后逐条复现）：

- **备份不再静默丢掉知识库**。三份手抄的表清单漂开了，白名单里三张表根本不存在，
  而导出方要的 `kb_documents` / `kb_chunks` / `kb_embeddings` 全被拒后只
  `log::warn` 跳过——用户看到「导出成功」，知识库不在备份里。
- **改个账号名不再毁掉 API Key**。列表早就做了脱敏，但编辑表单会把掩码原样提交
  回来，真 Key 被覆盖成 `abcd...wxyz`。
- 账号 / 搜索供应商的 Key 改为加密入库，列表不再返回完整 Key，存量启动时就地加密。
- 一键预设不再把 Key 写成明文（且平台已有 Key 时预设「存了不生效」的问题一并修掉）。
- 技能名改用路径段校验，`..\..\.ssh` 这类名字不再能跳出技能目录。
- 通用设置读写接口加凭据类黑名单——它此前能读走远程网关令牌。
- 远程面板：转义补上引号，会话 id 改 data 属性 + 事件委托，不再进 `onclick`。
- 删掉 WSL 模式：它的开关根本不落盘，而背后是一个「谁修好开关谁就打开局域网
  无鉴权入口」的潜伏洞。
- 删掉新安装时种下的、指向开发者本机路径的定时任务。

## [0.29.1] — 2026-08-13

修 0.29.0 的回归：Auto 路由按已被清空的 `model_platforms.api_key` 判断「有没有
Key」，而 Key 早已迁进 `platform_api_keys`——升级用户开机后 Auto 可能一个模型都
选不出来，模型中心里 Key 却是齐的。两侧候选查询合成一处并同时看新表。

## [0.29.0] — 2026-08-13

中文知识库能搜了（BM25 二元切分）+ 通知/失误检测/记忆回注三个「一直没接上」的
功能接回来 + 清掉约 900 行死代码；App.tsx 1007 → 698 行。

## [0.28.0] — 2026-08-12

手机配对不再交永久令牌 + 密钥绑账号 + 建表收一处（挖出两个死功能）（`9c6f4ab`）

## [0.27.0] — 2026-08-12

Agent 能读写 Office 了 + 幻灯自带放映器 + 四道安全闸收口（`f5bbdef`）

## [0.26.0] — 2026-08-08

Agent 真的能上网了 + 搜索合二为一 + 本地目录对着 registry 重写（`7430517`）

## [0.25.0] — 2026-08-06

工具翻译补全 + 请求级测试 + 记忆证据等级 + 技能跨 harness 共享（`3230f2c`）

## [0.24.0] — 2026-08-04

网关工具透传修复 + OMNIX 对外供能（`6498576`）

## [0.23.0] — 2026-08-03

体检 / 演讲者视图 / 可往返导出（`58d5da8`）

## [0.22.3] — 2026-08-03

大纲角色推导生效、换方案不再给重复候选（`b0bbf69`）

## [0.22.2] — 2026-08-03

修媒体槽重复出图与空条目重排（`ac8ad66`）

## [0.22.1] — 2026-08-02

修好 5 个「拖了没反应」的版式控件（`b65c990`）

## [0.22.0] — 2026-08-02

PPT 参数化版式（P0→P3 全量）（`2f495e0`）

## [0.21.0] — 2026-08-02

技能融合改为取代式（`88a433d`）

## [0.20.1] — 2026-07-31

个人记忆不再进入会被提交的上下文文件（`240d287`）

## [0.20.0] — 2026-07-30

记忆库精炼 + 相关性注入（`37b4acb`）

## [0.19.1] — 2026-07-30

Antigravity 健壮性 + 模型清单（`c986438`）

## [0.19.0] — 2026-07-30

接入 Google Antigravity（print 单轮适配器）（`de5ebff`）

## [0.18.2] — 2026-07-29

远程/技能源安全加固（Grok 审计 4×P1）（`1fe2751`）

## [0.18.1] — 2026-07-27

工程收尾批5（日志异步化/bundle预算/前端测试）（`c213dc4`）

## [0.18.0] — 2026-07-27

拆大模块批4（tauri-api/SettingsTab/proxy/ChatTab）（`a8f732a`）

## [0.17.1] — 2026-07-27

远程访问安全增强（批3）（`18df6be`）

## [0.17.0] — 2026-07-27

GUI 收口批2（设置分组/输入区瘦身/宫格两态/空态）（`61f6446`）

## [0.16.0] — 2026-07-26

GUI 收口第一波 + 性能（ABCD 首批）（`5e6f661`）

## [0.15.1] — 2026-07-25

安全加固（修复 Codex 审查 5×P1 + 2×P2）（`afb1fd9`）

## [0.15.0] — 2026-07-24

诊断并入设置（顶栏双图标收一）（`0ac61a1`）

## [0.14.0] — 2026-07-24

编排预设（交接/顾问）+ 悬浮窗可关（`2902277`）

## [0.13.7] — 2026-07-23

日志内存上限 + 记忆自动召回（`7882c95`）

## [0.13.6] — 2026-07-19

修 Grok 写文件被拒/说一句就停（`4175810`）

## [0.13.5] — 2026-07-19

对话自动滚底 + 修 Grok 消息重复（`af25f72`）

## [0.13.4] — 2026-07-18

苹果玻璃做浓，一眼可见（`1e2d43f`）

## [0.13.3] — 2026-07-18

修复 Grok 会话启动 exit code 2（`11e8533`）

## [0.13.2] — 2026-07-18

苹果风真正生效 + 工作区不再卡死（`0688f32`）

## [0.13.1] — 2026-07-18

Liquid Glass 全量换肤 + 四项反馈修复（`7d84893`）

## [0.13.0] — 2026-07-18

苹果风 GUI + 智能体重构 + 额度面板 + Grok 模型 + 技能安全门（`64f6e80`）

## [0.12.1] — 2026-07-16

Office 收敛为单一工作台（概览+演示+文档+表格）（`59ccb94`）

## [0.12.0] — 2026-07-16

Office 收官（Word/表格）+ 监督台 + 精选技能（`1ee6af4`）

## [0.11.0] — 2026-07-16

Office P0：OfficeCLI 底座 + PPT 质检/导入 + 技能自动更新（`966eb30`）

## [0.10.2] — 2026-07-16

认证中心支持 Grok 账号登录（`d69d9af`）

## [0.10.1] — 2026-07-16

修复 Grok Build 安装失败与运行时未接线（`012a7cc`）

## [0.10.0] — 2026-07-16

Grok 全支持 + PPT 大升级 + 远程开发 Labs（`3c15514`）

## [0.9.1] — 2026-07-12

全面重构：清债/技能页拆解/巨型文件拆分（`1edd077`）

## [0.9.0] — 2026-07-12

存储位置中心 + 技能中心重构 + Agent 安装管理；去品牌化（`dc62e3b`）

## [0.8.0] — 2026-07-11

终端洪泛修复 + PPT 演示面板 + 技能池治理 + 方案抉择框（`b5dfb8f`）

## [0.7.0] — 2026-07-10

in-app updater + CI auto-release + borrowing round F/G（`1cc6dbf`）

## [0.6.0] — 2026-07-09

gateway centralization (resilience/cost/MCP/model + OAuth center + CLI takeover) + session/SDD/autopilot/write round（`10faef7`）

## [0.5.0] — 2026-07-04

multimodal media pipeline + borrowing round (update/profile/handoff) + workspace management（`85859b1`）

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

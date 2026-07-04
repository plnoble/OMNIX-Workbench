# BORROWINGS — 借鉴登记与再借鉴指南

> 目的：OMNIX 的多个功能对标/借鉴了外部软件。当上游更新、需要「重新借鉴」时，
> 对照本文件的**行为契约**做回归，而不是对着代码猜当初抄了什么。
> 规则：新增借鉴功能时**必须**在此登记（来源、对齐日期、契约、涉及文件）。

---

## 登记表

### 1. 划词助手（Selection Assistant）
- **借鉴对象**：Cherry Studio 的划词助手
- **对齐日期**：2026-06-30（以当时 Cherry Studio 行为为准）
- **行为契约（回归清单）**：
  1. 选词过程中弹窗**绝不**出现/闪烁/跟随鼠标；仅在**鼠标释放**后单次 UIA 读取。
  2. 弹窗显示后**不抢焦点**（不打断复制 Ctrl+C）；剪贴板永不被占用。
  3. 点击弹窗**外部**立即关闭；点击弹窗**内部**（含拖动/缩放后的新边界）不关闭。
  4. 弹窗可拖动、可缩放（East/South/SouthEast 抓手），尺寸持久化。
  5. 自身窗口 / 黑名单进程中的选词不触发。
- **涉及文件**：`src-tauri/src/selection.rs`（监控/UIA/click-away）、
  `src-tauri/src/lib.rs`（窗口创建）、`src/QuickAssistant.tsx`、`src/hooks/useSelection.ts`
- **注意**：click-away 依赖每轮刷新 `current_popup_rect`（勿回退成显示时快照）。

### 2. 联网搜索注入
- **借鉴对象**：AingDesk 的「搜索结果注入对话」模式
- **对齐日期**：2026-06 中旬
- **行为契约**：搜索结果作为附加上下文拼进 agent prompt（`[联网搜索结果]` 段），
  展示给用户的消息保持原文；搜索失败不阻断发送。
- **涉及文件**：`src/hooks/useConversations.ts`（sendMessage 内 searchContext 分支）

### 3. ACP 通用运行时适配器
- **借鉴对象**：Agent Client Protocol（Zed 主导，https://agentclientprotocol.com）
- **对齐版本**：协议 v1；`agent-client-protocol-schema` crate v1.1.0（仅取类型）
- **行为契约**：
  1. JSON-RPC 2.0 over stdio；方法名斜杠式（`session/new`、`session/prompt`、
     `session/update`、`session/request_permission`、`fs/read_text_file`、
     `fs/write_text_file`、`session/cancel`、`session/set_config_option`）。
  2. 双向：agent 反向 fs/permission 请求必须应答；未实现方法回 `-32601`。
  3. fs 代读写受工作区路径约束（词法归一防越界）。
  4. `session/new` 的 `configOptions(model)` 若存在 → 驱动 OMNIX 模型选择。
- **涉及文件**：`src-tauri/src/runtime_acp.rs`（唯一协议文件）、
  `runtime_manager.rs`（分派）、`runtime.rs`（AdapterKind::Acp + agent_definition）
- **再借鉴（协议升级 ACP v2）**：只改 `runtime_acp.rs` + bump schema crate；
  分派按 `AdapterKind` enum，新增适配器会被编译器强制补全所有 match。

### 4. Codex app-server 协议
- **借鉴对象**：OpenAI Codex CLI `app-server` JSON-RPC
- **对齐日期**：2026-06（Codex CLI 当时版本）
- **行为契约**：`initialize`/`initialized` → `thread/start|resume` → `turn/start`；
  `item/*` 事件流；审批经 `item/*/requestApproval` request/response。
- **涉及文件**：`src-tauri/src/runtime.rs`（build_codex_* / parse_codex_message）

### 5. 设置/模型中心 UX 参照
- **借鉴对象**：Cherry Studio（设置半屏、模型选择器、供应商管理交互）
- **对齐日期**：2026-06
- **契约**：供应商→模型两级选择；模型健康状态可见；嵌入模型单独固定
  （`embedding_model` 设置为唯一真源）。

### 6. Agent 版本感知的一键更新
- **借鉴对象**：Nezha（agent 版本管理）
- **对齐日期**：2026-07-03
- **行为契约**：对已装 agent 跑 `npm view <pkg> version` 取最新版，与
  `--version` 提取的已装版比对 → AgentHub 显示「可更新」徽章 + 一键更新
  （复用现有 repair→@latest）。查询失败(离线/私包)时**不误报**。
- **涉及文件**：`agent.rs`（npm_package_for_agent/extract_semver/semver_is_older）、
  `commands/agents.rs`（check_agent_updates）、`AgentHubTab.tsx`。

### 7. 个人编程画像 Profile（热力图 + 战绩卡）
- **借鉴对象**：Synara `apps/web/src/components/profile/`（MIT）
- **对齐日期**：2026-07-03（对齐 Synara main）
- **行为契约**：GitHub 式活动热力图（按天提示数 0-4 级）+ 连续天数/累计统计
  + 各 agent 占比 + 可导出 PNG 战绩卡（canvas 绘制，无新依赖）+ 本地 @handle。
  数据全部来自 OMNIX 已有表（messages/agent_sessions/request_logs），不新增遥测。
- **涉及文件**：`commands/profile.rs`（get_profile_stats）、`ProfileTab.tsx`、
  appRegistry 的 `profile` 入口。
- **注意**：热力图强度色阶用 `--color-info` token（勿硬编码颜色）。

### 8. Agent 交接 / 上下文续接
- **借鉴对象**：Synara「provider handoff」
- **对齐日期**：2026-07-03
- **行为契约**：把某对话**切到另一个 Agent** 时，将此前 transcript（近 24 条、
  截断至 ~8000 字）作为「交接上下文」块拼进发给新 Agent 的**首条 prompt**（不显示
  在气泡，只有用户原文入库）。切到相同 Agent 或无历史时不触发。可经 composer
  「交接开/关」开关关闭（`localStorage: omnix_agent_handoff`）。
- **涉及文件**：`runtime.rs`（build_conversation_handoff_context）、
  `runtime_manager.rs`（send_message_with_display 的 with_handoff）、
  `commands/runtime.rs`（runtime_send_message 的 handoff 参数）、
  `useConversations.ts`（切 agent 时置 isHandoff）、`ChatTab.tsx`（开关）。
- **注意**：上下文须在 record_user_message **之前**构建，否则会把当前消息也带进去。

### 9. 多模态媒体管线（看图 / 生图 / 生视频）
- **借鉴对象**：Agnes AI（`apihub.agnes-ai.com/v1`，OpenAI 兼容多模态网关）
- **对齐日期**：2026-07-04（wiki.agnes-ai.com + github.com/AgnesAI-Labs 文档）
- **行为契约**：
  1. 生图 `POST {base}/images/generations` `{model,prompt,size}` → `data[0].url` 或
     `data[0].b64_json`（双兼容）；产物存 `~/.omnix/media/<task>.png`。
  2. 生视频（异步）`POST {base}/videos`（`num_frames≤441` 且 8n+1、fps 1-60，由
     `normalize_num_frames` 强制）→ 轮询 `GET {host}/agnesapi?video_id=`（**用
     video_id 勿用 task_id**）→ `queued|in_progress|completed|failed`。完成态 URL
     字段文档不一致 → `extract_video_url` 防御式扫描 + raw_response 落库，活体后固化。
  3. 看图：composer 附件（≤4 张、≤5MB，Ctrl+V 可贴）→ 附件文件存
     `attachments/`、transcript 只记路径；按 AdapterKind 分派——ACP 用
     `{type:"image",data,mimeType}`（以 initialize 的 `promptCapabilities.image`
     门控）、Claude 用 Anthropic base64 块、Codex 明确报「暂不支持带图」（app-server
     图片输入待探测，后续接）；网关翻译路径把 image 块映射为 OpenAI
     `image_url` data URL（不再丢图）。
- **架构**：`media.rs` = 纯协议层（`MediaProviderKind` enum，仿 AdapterKind；全单测），
  `commands/media.rs` = IO 层（HTTP/文件/media_tasks 表/轮询器）。**新供应商 =
  新 enum 变体**（火山即梦/可灵/OpenAI images…），编译器强制补全分派。
- **涉及文件**：`media.rs`、`commands/media.rs`、`db.rs`（media_tasks）、
  `StudioTab.tsx`（创作面板）、`runtime*.rs`（images 管线）、`proxy.rs`（图块透传）、
  `tauri.conf.json`（assetProtocol + media-src CSP）、`PlatformModal.tsx`（Agnes 预设）。
- **注意**：视频播放走 `convertFileSrc`（需 `protocol-asset` cargo feature + CSP
  `http://asset.localhost`）；图生视频以 data URL 传 `image` 参数（是否被 Agnes
  接受待活体验证）。

---

## 架构约定（让「再借鉴」永远是局部手术）

1. **功能模块化**：新借鉴功能按
   `src/features/<名称>/`（组件+hook） ↔ `src-tauri/src/commands/<名称>.rs`
   成对落位；`App.tsx` 只挂载不承载逻辑。重新借鉴 = 换模块内部，接口不变。
2. **Agent 注册单一真源**：后端 `runtime.rs::agent_definition` 是唯一注册表；
   前端经 `runtime_get_agent_catalog` 拉取（`src/lib/agentRegistry.ts`），
   **禁止**在组件里新增 agent 名称/ID 硬编码。
3. **协议归协议文件**：每种外部协议一个文件（`runtime_acp.rs` 模式）；
   分派用 `AdapterKind` enum，不用字符串。
4. **主题只用 tokens**：颜色一律 CSS 变量（`src/styles/globals.css`）；
   借鉴 UI 时把对方配色翻译成现有 token，不引入硬编码色值。
5. **升级流程**：改动前先读本文件对应条目 → 升级 → 按「行为契约」逐条回归 →
   更新对齐日期与契约差异。

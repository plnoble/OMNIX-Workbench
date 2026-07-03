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

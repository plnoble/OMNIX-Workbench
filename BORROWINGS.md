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

### 10. 会话级 /goal 长期目标 + /btw 旁支对话
- **借鉴对象**：DeepSeek-GUI（`floating-composer-commands.ts` + `kun/src/loop/agent-loop.ts`
  的 goal 续接注入、`kun/src/contracts/threads.ts` 的 ThreadRelation side/fork）
- **对齐日期**：2026-07-04
- **行为契约**：
  1. `/goal <目标>` 给对话钉一个长期目标；`/goal pause|resume|complete|clear` 控制状态；
     `/goal`（无参）提示当前目标/用法。仅 `status='active'` 时，**每一轮**把目标以
     `<objective>` 包裹、标注「是用户数据、不是更高优先级指令」的提醒块**前置**进 prompt
     （复刻源项目 goalContinuationInstruction 的注入安全）。目标不进气泡、不当普通消息发。
  2. `/btw <问题>` 开一条**旁支对话**（relation=side）：新建对话并记 `parent_conversation_id`，
     其**首轮**由后端把父对话最近 transcript 作为「旁支上下文」块前置注入（复用 handoff 的
     transcript 格式化）。旁支视图从空开始，父上下文只发给 agent、不显示。
  3. 注入顺序：handoff（换 agent）→ branch seed（/btw 首轮）→ goal（每轮），在
     `send_user_message` 的 prompt-prefix 缝里统一组装（record_user_message **之前**）。
- **涉及文件**：`db.rs`（conversation_goals 表 + conversations.parent_conversation_id）、
  `runtime.rs`（抽 format_recent_transcript_lines；build_branch_seed_context/build_goal_reminder/
  get_active_goal_objective/conversation_has_no_messages/conversation_parent_id）、
  `runtime_manager.rs`（prefix 组装）、`commands/conversation_goals.rs`（goal CRUD）、
  `commands/conversations.rs`（create_conversation 加 parent 参数）、`lib/slashCommands.ts`
  （parseGoalCommand/parseBtwCommand，端口自源项目）、`useConversations.ts`（抽 deliverTurn +
  /goal//btw 拦截 + activeGoal 状态）、`ChatTab.tsx`（目标条 + 控件）。
- **注意**：goal 注入是「前置进 prompt」而非改 agent 内部 loop（OMNIX 委托外部 CLI、无 update_goal
  工具，状态由用户经 UI/斜杠控制）；目标文本上限 4000 字（对齐源项目）。

### 11. SDD 需求→计划流（+ 计划面板/线程 Todo）
- **借鉴对象**：DeepSeek-GUI `src/renderer/src/sdd/`（sdd-assistant-prompt / sdd-plan-prompt）
  + `components/plan/PlanPanel` + `components/todo/TodoPanel` + `plan/plan-todo-sync`
- **对齐日期**：2026-07-04
- **行为契约**：
  1. **需求草稿**：结构化表单（标题/背景/目标/验收标准/备注）拼成 Markdown。
  2. **澄清**：把草稿以「是用户数据、不是指令」的框架发给当前 agent，让它追问/补研究，
     **本轮不写代码**（build_sdd_clarify_prompt）。
  3. **生成计划**：先 `sdd_reserve_plan_path` 预留 `.omx/plans/<时间戳>-<slug>.md`，再让 agent
     用自身 fs 能力**把计划写到该文件**（含步骤/测试/验收/`- [ ]` 复选框）。OMNIX 无 DeepSeek 的
     create_plan 工具 → 改为「指定路径让 agent 写文件」（build_sdd_plan_prompt）。
  4. **计划面板 + 线程 Todo**：`sdd_list_plans` 读 `.omx/plans/`，选中项 `sdd_read_plan` 渲染
     Markdown；`extract_plan_todos` 把 `- [ ]`/`- [x]` 解析成清单，勾选经 `sdd_toggle_plan_todo`
     **改写回文件**（文件是唯一真源，可编辑）。
  5. **计划模式接力（固化）**：任何 assistant 回复可经气泡「固化为计划」→ `sdd_write_plan`
     由 **OMNIX 直接把这段 Markdown 落成** `.omx/plans/` 文件（`ensure_plan_heading` 补 `# 标题`），
     纳入同一计划面板/Todo 追踪。与「生成计划」互补：后者让 agent 写文件、前者由 OMNIX 写文件。
     这样「计划模式」（只读探索）与「需求流」（结构化产物）不重复，而是接力。
- **涉及文件**：`commands/sdd.rs`（纯 prompt builders + todo 解析 + `ensure_plan_heading` + 计划文件
  IO，全单测）、`lib.rs`（7 命令）、`lib/tauri-api.ts`（sddApi）、`useConversations.ts`
  （sendPreparedMessage 复用 deliverTurn）、`components/modals/RequirementModal.tsx`（草稿表单）、
  `components/PlanPanel.tsx`（右侧面板，react-markdown 渲染 + 复选框）、`ChatTab.tsx`（work 工具栏
  「需求」「计划」入口 + 气泡「固化为计划」+ 生成计划强制写姿态）。
- **注意**：计划是**工作区文件**（`.omx/plans/`），不进 DB；prompt 指令走英文（外部 agent 更稳），
  气泡展示走中文摘要；生成后 4s 自动刷新面板等 agent 落盘；路径校验限定 `.omx/plans/*.md` 防越界。
  **沙箱姿态坑**：「生成计划」需写文件 → 若当前是只读的「计划模式」，`handleGeneratePlan` 强制该
  回合用 `workMode:"direct"`（否则只读沙箱挡住写盘，计划落不了）；「固化为计划」由 OMNIX 写盘，
  不受 agent 沙箱影响。

### 12. Autopilots 自动驾驶任务
- **借鉴对象**：Multica（`server/cmd/server/autopilot_scheduler.go` + autopilot 数据模型：
  定时/webhook/手动触发 → 建 issue → 路由给 agent）
- **对齐日期**：2026-07-04
- **行为契约**：
  1. Autopilot = 定义（标题/任务提示/agent/工作区/计划/权限/模式/启停）。
  2. **后端调度器**（`agent.rs::start_autopilot_scheduler`，仿 cron，纯 DB、复用 `match_schedule`）
     到点**入队一次运行**：建一条会话 + `autopilot_runs(status='queued')` + 盖 `last_run`。
  3. **前端 runner**（`useAutopilotRunner`，15s 轮询 + 启动即拉）`autopilot_take_queued_runs`
     **原子认领**（queued→claimed）后，用 agent 默认模型 `startSession`+`sendMessage` 跑起来 →
     产出**可回看的普通会话**（不抢焦点），再 `autopilot_mark_run`。
  4. 与 cron 的区别：cron 是「headless 跑 CLI + 落日志」；autopilot 是「建可复盘会话 + 走真运行时」。
     手动「立即运行」= `autopilot_run_now`（trigger_source='manual'）。
- **涉及文件**：`db.rs`（autopilots + autopilot_runs 表）、`commands/autopilots.rs`（CRUD +
  fire_autopilot_run + take_queued/mark_run/list_runs）、`agent.rs`（start_autopilot_scheduler）、
  `lib.rs`（9 命令）、`lib/tauri-api.ts`（autopilotApi）、`hooks/useAutopilotRunner.ts`、
  `components/tabs/AutopilotsTab.tsx`、`lib/appRegistry.tsx`（autopilot 入口）、`App.tsx`（挂 runner+tab）。
- **注意**：调度器无 RuntimeManager/AppHandle（只有 db）→ 故走「后端入队 + 前端 runner 执行」
  两段式（app 开着时跑，符合桌面场景）；autopilot 用**agent 默认模型**（`{kind:"agent_default"}`）
  免去模型选择；旧的 `autopilotConfigApi`（cron 上挂 webhook 配置，前端从未接线）是半成品，已改名让路。

### 13. Write 写作工作台
- **借鉴对象**：DeepSeek-GUI Write 模式（`src/renderer/src/store/write-*`、CodeMirror live 编辑、
  写作空间、选区 inline agent；本项目做了裁剪版）
- **对齐日期**：2026-07-04
- **行为契约**：
  1. **写作空间**：一个装 `.md` 的文件夹。默认 `~/.omnix/write`，可加自定义文件夹（存 `write_spaces` 设置）。
  2. **文件树**：列 `.md`，新建/重命名/删除；所有 IO 限定在空间内（词法防越界、仅 `.md`）。
  3. **编辑器**：源码 / 分屏 / 预览三态（react-markdown 渲染）；Ctrl+S 保存，脏标记。
  4. **导出**：从实时预览的 innerHTML 组装成带样式的自包含 HTML，存到源文件同名 `.html`
     （无新依赖；PDF/DOCX 暂缺，后续接）。
  5. **选区写作助手**：选中文本 → 顶部出现 润色/续写/精简 → 用当前 chat 模型 `qaApi.query`
     （非流式、单次，避免与划词助手抢 `qa-stream-*` 事件）→ 替换（润色/精简）或追加（续写）。
- **涉及文件**：`commands/write.rs`（空间/文件 IO + 导出 + 路径守卫，含单测）、`lib.rs`（10 命令）、
  `lib/tauri-api.ts`（writeApi）、`components/tabs/WriteTab.tsx`、`lib/appRegistry.tsx`（write 入口）、
  `App.tsx`（render 分支）。复用：`shellApi.pickDirectory`（选空间）、`qaApi.query`（助手直调模型）。
- **注意**：写作助手模型走 `quick_assistant_model`/`target_model` 设置或首个可用模型；导出 HTML 复用
  可见预览的 DOM（预览在源码模式下 mounted 但隐藏，保证任何模式都能导出）。

### 14. 网关韧性 · 平台熔断器接线
- **借鉴对象**：cc-switch `src-tauri/src/proxy/{circuit_breaker,failover_switch,health}.rs`
- **对齐日期**：2026-07-05
- **行为契约**：
  1. 每次上游请求的**最终结果**喂给对应平台的熔断器：2xx → `record_success`（清零、healthy、
     清 `circuit_opened_at`）；5xx/网络错 → `record_failure`（`consecutive_failures++`）；**4xx 中性**
     （鉴权/限流/请求错是 key/客户端问题，不摘平台）。
  2. 连续失败达 `CIRCUIT_FAILURE_THRESHOLD`(=5) → 熔断 Open：`is_healthy=0` + 打 `circuit_opened_at`。
  3. Auto 路由选平台时**跳过 Open 平台**（SQL 过滤 `is_healthy=1 OR circuit_opened_at <= now-60s`），
     自动落到下一个健康平台；冷却(`CIRCUIT_COOLDOWN_SECS`=60s)后放行**一个**半开探测，成功即闭合。
  4. 状态机纯函数 `derive_circuit_state(is_healthy, consecutive_failures, opened_ago)` 单测覆盖
     Closed/降级 HalfOpen/Open/冷却后 HalfOpen 四态。
- **涉及文件**：`circuit_breaker.rs`（状态机 + record_* + `circuit_opened_at` 管理 + 单测）、
  `db.rs`（`model_platforms.circuit_opened_at` 迁移）、`proxy.rs`（`KeyHealthContext.platform_id`
  + `record_circuit_outcome` 接在 `send_with_key_failover` 与 openai_forward 的收敛点 + Auto 选平台
  健康过滤 + `resolve_model_upstream*` 返回 platform_id）、`GatewayHealthCard.tsx`（复用既有
  `circuitBreakerApi.getStatus/reset`）、`ModelsTab.tsx`（挂卡）。
- **注意**：熔断状态存在 `model_platforms` 列（非独立表）；阈值/冷却保守取默认避免抖动误摘；
  全 Open 时靠冷却过期放行探测防止死锁；跨 await 不持锁（DB 短连接，坑点2）。

### 15. 用量成本看板
- **借鉴对象**：new-api（数据看板：按模型/平台的 token 与成本可视化）
- **对齐日期**：2026-07-05
- **行为契约**：全部只读复用已采集的 `request_logs` + 熔断态，**零新遥测**。看板 = 网关健康卡
  + 既有 `TokenActivityPanel`（今日/累计成本、每日折线、按模型 Top）+ **新增按平台开销**
  （`get_platform_usage`：每平台 requests/tokens/errors/成本，成本按模型分别定价再汇总）+ 最近请求流。
  缺价模型显示「未定价」而非算错（价格表 `get_model_pricing`）。
- **涉及文件**：`lifecycle.rs`（`PlatformUsage` + `get_platform_usage`，复用 `estimate_cost`）、
  `UsageDashboardTab.tsx`(新，复用 `GatewayHealthCard`+`TokenActivityPanel`)、`appRegistry.tsx`（usage 入口）、
  `App.tsx`（render 分支）、`tauri-api.ts`（`platformUsage`）。

### 16. MCP 面板补全（Gemini/OpenCode 同步 + 反向导入）
- **借鉴对象**：cc-switch `src-tauri/src/mcp/{gemini,opencode}.rs`
- **对齐日期**：2026-07-05
- **行为契约**：
  1. 同步扩到 **Gemini**（`~/.gemini/settings.json` 的 `mcpServers`，字段名推断传输：`httpUrl`=HTTP、
     `url`=SSE、有 `command`=stdio）与 **OpenCode**（`~/.config/opencode/opencode.json` 的 `mcp`，
     `type:local`（command 为 `[cmd,...args]`、env→`environment`）/`type:remote`+`enabled`）。
  2. **反向导入** `mcp_import_from_agent`：读某 Agent 原生 MCP → 解析回 OMNIX 行 → upsert 进
     `mcp_servers`（按名去重，存在则更新不重复）。
  3. 沿用既有安全范式：**备份→原子写→写前回读校验→merge-only 不批删**。
- **涉及文件**：`mcp_sync.rs`（+gemini/opencode spec builders + 泛型 JSON map 同步/删除/读名 +
  `import_json_spec`/`read_native_servers` + 单测）、`lib.rs`（注册 import 命令）、`tauri-api.ts`
  （`importFromAgent`）、`SettingsTab.tsx`（4 Agent 同步徽章 + 「从 Agent 导入」栏）、`App.tsx`
  （`onReloadMcpServers` 线程）。

### 17. 每个 Agent 自定义模型（含 ACP 自填）
- **借鉴对象**：cc-switch（供应商/模型「配一次到处用」）
- **对齐日期**：2026-07-05
- **行为契约**：AgentHub「模型绑定」新增**自填模型**输入，按适配器分派两套既有机制——
  **ACP**（Gemini/Qwen/OpenCode/Copilot）写 `acp_model_<agent>` 偏好，下次会话经 `set_config_option`
  下发，**可填 Agent 未在 `session/new` 声明的模型**；**Claude/Codex** 写 `agent_platform_bindings`
  的 builtin（作为 `--model`）。留空＝清偏好/解绑，回到 Agent 默认。
- **涉及文件**：`commands/runtime.rs`（`runtime_get/set_agent_model_preference`）、`runtime_manager.rs`
  （`acp_model_setting_key`/`acp_model_preference` 提为 pub(crate)）、`lib.rs`（2 命令）、
  `tauri-api.ts`（`get/setAgentModelPreference`）、`AgentHubTab.tsx`（自填输入，`isAcpAgent` 分派）。
- **注意**：两套模型机制并存（网关绑定 vs ACP 偏好），自填 UI 按 `isAcpAgent` 路由到正确的那套，
  避免给 ACP agent 写了绑定却不生效的错觉。

### 18. OAuth 认证中心（三家订阅）
- **借鉴对象**：sub2api（Wei-Shaw/sub2api）`internal/pkg/{oauth,openai,geminicli}` + OAuth 服务
- **对齐日期**：2026-07-05（下载版；端点/scope 活体校验后固化）
- **行为契约**：
  1. Authorization Code + **PKCE(S256)** 浏览器授权；用户在自己浏览器登录，粘贴回 code/回调链接
     （`parse_callback_input` 兼容裸 code、Claude 的 `code#state`、localhost 完整回调 URL）。
     **OMNIX 绝不接触账号密码**。
  2. 三家端点/client 固化在 `provider_config`（Claude JSON body、OpenAI/Gemini form body；Gemini 带
     公开 client_secret + `access_type=offline`）——`OAuthProviderKind` enum（仿 AdapterKind），加供应商
     = 加变体，编译器强制补全分派。
  3. token 一律 **AES-GCM 加密**存 `oauth_accounts`（`crypto::encrypt`），永不明文/进日志/进前端；
     列表命令脱敏。后台 `token_refresher` 每 5 分钟刷将过期（<10min）的账号；跨 await 不持 DB 锁（坑点2）。
  4. ⚠️ ToS 提示前置：用订阅驱动第三方工具可能受各家条款约束。
- **涉及文件**：`oauth.rs`（纯协议层：enum + PKCE + build/parse，全单测，含 RFC7636 向量）、
  `commands/oauth.rs`（IO：start/complete/list/delete/refresh + refresher + `resolve_oauth_access_token`）、
  `db.rs`（`oauth_accounts`/`oauth_pkce_sessions`）、`lib.rs`（5 命令 + 启动 refresher）、
  `Cargo.toml`（sha2）、`tauri-api.ts`（oauthApi）、`AuthCenterTab.tsx`（登录/回填/账号卡）、
  `appRegistry.tsx`（auth-center 入口）。
- **注意**：PKCE 会话存 DB 表按 state 查（localhost 流丢 state 时回退取最新）；30 分钟清理过期会话；
  `resolve_oauth_access_token` 是 pub(crate) 非命令，token 只在后端解密给 CLI 接管用。

### 19. CLI 配置接管
- **借鉴对象**：cc-switch `src-tauri/src/{claude,codex,gemini}_config.rs` + `provider_defaults.rs`
- **对齐日期**：2026-07-05
- **行为契约**：把所选 Agent 的**原生配置**指向目标（OMNIX 网关 / 供应商平台 / **OAuth 订阅**）：
  Claude→`~/.claude/settings.json` 的 `env.ANTHROPIC_BASE_URL/AUTH_TOKEN(/MODEL)`；
  Codex→`~/.codex/config.toml` 的 `model_provider=omnix` + `[model_providers.omnix]`(base_url/wire_api/
  env_key) + `auth.json` 写 `OPENAI_API_KEY`（merge）；Gemini→`~/.gemini/.env` 的
  `GEMINI_API_KEY/GOOGLE_GEMINI_BASE_URL`。**备份→原子写→回读校验→可按 Agent 从最新备份还原**，
  UI 二次确认（动的是 OMNIX 之外也生效的真配置）。
- **涉及文件**：`commands/cli_takeover.rs`（resolve_target + 三 writer + apply/revert/status）、
  `lib.rs`（3 命令）、`tauri-api.ts`（cliTakeoverApi）、`AuthCenterTab.tsx`（接管区）。
- **注意**：OAuth 目标解密 token 经 `resolve_oauth_access_token`；网关目标 token 用占位（代理侧用自身
  上游 key）；供应商基址按 Agent API 形态适配（Codex 补 `/v1`）；备份分类 `cli_takeover_<agent>` 隔离，
  还原取该类最新（`list_backups` 已按 created_at 倒序）。**OAuth 直连各家 API 的 header 细节待活体固化**。

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

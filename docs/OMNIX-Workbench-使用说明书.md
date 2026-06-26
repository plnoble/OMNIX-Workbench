# OMNIX Workbench 使用说明书

版本：0.1.0  
产品：OMNIX Workbench  
说明：多 Agent 开发与协作工作台

## 1. 打开方式

### 安装版

- MSI：打开 `src-tauri/target/release/bundle/msi/OMNIX Workbench_0.1.0_x64_en-US.msi`，安装后从开始菜单启动 `OMNIX Workbench`。
- NSIS：打开 `src-tauri/target/release/bundle/nsis/OMNIX Workbench_0.1.0_x64-setup.exe`。

### 独立运行

双击 `src-tauri/target/release/omnix-workbench.exe`。

### 开发模式

在项目根目录运行：

```powershell
npm.cmd install
npm.cmd run tauri -- dev
```

只运行 `npm.cmd run dev` 只能查看前端壳层，不能使用 Tauri 数据库、文件选择、Agent 进程等桌面能力。

## 2. 第一次使用

1. 打开标题栏右侧“设置”，检查主题和系统配置。
2. 在应用宫格打开 `Models`，添加模型供应商、API 地址和至少一个 API Key。
3. 获取模型列表，启用需要的模型并做健康检查。
4. 打开“智能体”，检测 Claude Code 或 Codex；缺失时可使用托管安装。
5. 回到“工作”，选择 Agent 后输入任务。

## 3. 顶栏与应用宫格

默认固定入口：

- 工作：单 Agent 对话和开发工作。
- 团队：多个 Agent 的计划、执行和验收。
- 智能体：检测、安装、更新和模型绑定。
- 技能：技能库、熔炉、组合、市场和同步。

点击顶栏 `+`/宫格按钮可以打开其他应用。每个应用可以：

- 固定到顶栏。
- 收纳到应用宫格。
- 隐藏到恢复区。
- 恢复默认布局。
- 对固定入口执行前移/后移。

标题栏右侧固定保留诊断、主题和设置，避免主页面重复堆放。

## 4. 工作：单 Agent

### 普通对话

不选工作区即可开始对话。普通对话可以手动绑定一个或多个知识库。

### 工作区开发

点击“选择工作区”，从电脑中选择文件夹。右侧工作区面板显示：

- 文件树。
- Git 分支。
- 当前变更。
- 打开工作区入口。

### 运行选项

- Agent：首期真实结构化支持 Claude Code、Codex。
- 模型：会话选择优先于 Agent 绑定，Agent 绑定优先于官方默认。
- 直接执行：允许 Agent 按权限执行任务。
- 计划模式：先规划，不直接修改。
- 请求审批：每次敏感操作请求确认。
- 风险时审批：安全操作自动执行，风险操作请求确认。
- 完全访问：每个会话必须单独确认，不保存为全局默认。

消息、工具调用、审批、错误和原始日志分别保存。应用重启后可以恢复历史。

## 5. 团队：Team

1. 填写目标、边界和验收标准。
2. 选择工作区和队长（Claude Code 或 Codex）。
3. 点击“生成队长计划”。
4. 检查任务、负责 Agent、依赖和验收标准。
5. 点击“确认计划”。确认前不会启动 Worker。
6. 选择并发数并启动。

运行中可以查看每个 Worker 的状态、重试次数、结果和验收状态。遇到审批会在 Team 页面显示批准/拒绝按钮。所有 Worker 完成后，队长会检查工作区并给出 PASS 或 FAIL。

## 6. 智能体

每个 Agent 卡片显示检测状态、版本、来源和当前模型。详情中可以：

- 重新检测。
- 托管安装或更新。
- 卸载 OMNIX 托管版本。
- 使用 Agent 默认模型。
- 选择 Agent 官方/自带模型。
- 选择 Models 中已启用且协议兼容的模型。

OMNIX 优先使用系统现有 CLI，不覆盖系统安装。Gemini CLI、OpenCode 当前显示待适配状态，不代表完整结构化运行支持。

## 7. Models 模型中心

模型中心只管理模型资源，不包含系统设置。

主要功能：

- 添加自定义供应商。
- 选择 OpenAI、Responses、Anthropic、Gemini、Ollama 等 API 类型。
- 保存多个加密 API Key。
- 设置主 Key 和故障切换 Key。
- 从 API 地址获取模型列表。
- 批量健康检查。
- 自动识别视觉、音频、推理、编码、长上下文、工具、Embedding、速度等能力。

模型显示“启用”不等于适合所有 Agent。不可兼容模型会保留在列表中并说明原因。

## 8. 技能

### 技能库

查看、编辑、启用、收藏和分类本地技能包。

### 熔炉

选择至少两个技能和一个启用模型，生成融合草案。草案显示冲突和取舍，批准后才会创建新技能。

### 组合

将多个技能保存为 Skill Set，并同步到目标工具。

### 市场

搜索真实 Git 技能来源，预览 `SKILL.md`、仓库、revision 和内容哈希后导入。不会在下载失败时生成占位技能。

### 同步

- Claude Code：已验证。
- Codex：已验证。
- Gemini CLI、OpenCode、Cursor、Copilot：实验性目录适配，使用前检查目标工具版本。

支持冲突策略、漂移扫描和 Git 更新检查。

## 9. 经验与记忆

“Memory”应用包含长期记忆和蒸馏收件箱。

1. 选择一个真实会话。
2. 选择 Models 中启用的模型。
3. 生成候选。
4. 查看候选内容和证据消息 ID。
5. 批准或拒绝。

批准的记忆进入防错记忆库；批准的技能进入技能库；协议建议保留为可审查记录。模型不会自动修改全局规则。

## 10. Knowledge 知识库

可以创建多个命名知识库，并分别导入文件或目录。导入使用 Windows 系统选择器。

处理流程：

1. 创建知识库。
2. 导入文档。
3. 选择 Embedding 模型并生成向量。
4. 在 Knowledge 中搜索或进行 RAG 问答。
5. 在普通对话输入区手动选择一个或多个知识库。

回答会显示知识库和文档引用。知识库默认关闭；工作区开发和 Team 默认不接入。

## 11. 快捷助手

快捷助手支持翻译、解释、总结、润色、搜索和复制。

- 自动划词捕获默认关闭。
- 可以在应用中设置触发方式、模型和黑名单。
- 密码管理器、Windows 安全窗口等默认在黑名单内。
- 全局快捷键冲突只会记录警告，不会让主应用退出。

状态浮条：

- 单击卡片：唤起主窗口。
- 拖动右侧六点手柄：移动浮条。
- 右键：打开浮条菜单。

## 12. 助手模板

“助手”应用提供 Bug Fixer、Code Reviewer、Frontend Builder、Architecture Advisor、Git Expert、RCA Writer、PRD Drafter、Summarizer、Translator、SQL Expert、Security Auditor 等模板。

可以查看、收藏、复制提示词，或一键带入工作页。

## 13. 其他资源与 Labs

- MCP：管理 MCP Server 和工具能力。
- Search：配置联网搜索供应商和查看搜索历史。
- Cron：定时任务，实验功能。
- Compare：多模型结果比较，实验功能。
- Code Analysis：代码结构分析，Incomplete/Labs。
- Labs：查看实验功能完成度和风险。

实验功能可见不代表已经稳定，界面中的 Experimental/Incomplete 标记应作为使用边界。

## 14. 数据与安全

- 主数据库：`%USERPROFILE%\.omnix\omnix.db`。
- 中央技能目录：`%USERPROFILE%\.omnix\skills\`。
- API Key 加密保存，界面默认只显示掩码。
- 不要把 `%USERPROFILE%\.omnix\`、Agent 登录信息或 API Key 提交到 Git。
- 完全访问权限只对当前会话有效。

## 15. 常见问题

### 工作页没有模型

打开 Models，确认供应商和模型都已启用，并先运行健康检查。协议不兼容的模型会显示但不可选择。

### Agent 未检测到

在智能体页重新检测。Windows npm CLI 会优先使用 `.cmd` shim；也可使用 OMNIX 托管安装。

### Team 不能启动

确认已经选择真实工作区、安装 Claude Code/Codex，并点击了“确认计划”。循环依赖或不支持的 Agent 会阻止启动。

### MSI 构建在 WiX 阶段失败

运行：

```powershell
npm.cmd run diagnose:msi
```

必要时以管理员 PowerShell 运行 `npm.cmd run repair:msi-env`。`-sval` 生成的 MSI 仅用于诊断，不作为正式交付。

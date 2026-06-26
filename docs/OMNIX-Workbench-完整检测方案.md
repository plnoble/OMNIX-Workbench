# OMNIX Workbench 完整检测方案

版本：0.1.0  
适用平台：Windows 10/11  
产品定位：多 Agent 开发与协作工作台

## 1. 检测目标

确认 OMNIX Workbench 的界面、数据迁移、模型路由、Claude Code/Codex 运行、Team 调度、Skills、知识库、快捷助手和 Windows 安装包均与界面声明一致，不使用 mock 成功状态。

## 2. 自动化基础门禁

在项目根目录依次执行：

```powershell
npx.cmd tsc --noEmit
npm.cmd run build
Set-Location src-tauri
cargo check
cargo test --lib
```

验收标准：

- TypeScript 无错误。
- Vite 生产构建成功。
- `cargo check` 成功；unused/dead-code 警告作为已知技术债记录。
- Rust 库测试无失败；需要真实 Agent、完整种子数据库或压力环境的测试可以保持 `ignored`，但必须列明原因。

## 3. 安装与启动

### 3.1 MSI

1. 运行 `src-tauri/target/release/bundle/msi/OMNIX Workbench_0.1.0_x64_en-US.msi`。
2. 确认安装器名称为 `OMNIX Workbench`，图标为 E1 O/X 标志。
3. 完成安装并从开始菜单启动。
4. 再次运行 MSI，验证修复/升级身份稳定，不出现另一个同名产品。

### 3.2 NSIS

1. 运行 `src-tauri/target/release/bundle/nsis/OMNIX Workbench_0.1.0_x64-setup.exe`。
2. 完成安装、启动、卸载检查。
3. NSIS 是 MSI 的备用正式安装方式，不使用旧日期产物代替新构建。

### 3.3 独立程序

直接运行 `src-tauri/target/release/omnix-workbench.exe`。确认数据库仍使用 `%USERPROFILE%\.omnix\omnix.db`，不会因启动方式变化而创建第二份数据。

## 4. 桌面与视觉检查

分别检查浅色、深色和跟随系统主题：

- 默认窗口 `1280×800`。
- 最小窗口 `640×520`；启动尺寸会按当前显示器可用逻辑尺寸收敛。
- Windows 缩放比例 100%、125%、150%、200%。
- 在 WebView DPR 与原生窗口像素映射不一致时，应用应自动校正缩放；正常 DPI 映射不得被二次缩小。
- 顶栏固定入口在窄窗口切换为图标模式；诊断、主题、设置始终可见。
- 页面文字、按钮、选择器和卡片无重叠；横向滚动只出现在明确的横向列表。
- 非 Work/Team 页面不显示“工作上下文”侧栏。
- 状态浮条卡片单击可唤起主窗口，右侧六点手柄可拖动，右键菜单可用。

## 5. Models 检测

1. 新建自定义供应商，填写 API 类型和地址。
2. 添加两个 API Key；确认只显示掩码。
3. 指定一个活动 Key，拉取模型列表。
4. 启用模型并执行批量健康检查。
5. 检查状态、延迟、错误原因、最近检测时间和能力图标。
6. 工作页必须显示所有“供应商启用 + 模型启用”的模型。
7. Claude 只允许 Anthropic 或已验证网关兼容模型；Codex 只允许 Responses 兼容模型。其他模型可见但必须说明不可选原因。
8. 让主 Key 返回认证错误，确认请求转向启用的故障切换 Key，并记录失败原因但不记录明文密钥。

## 6. Agents 检测

对 Claude Code 和 Codex：

- 检测系统安装路径、来源和版本。
- 系统未安装时，托管安装进入 OMNIX 独立目录。
- 更新、卸载只作用于托管安装，不覆盖系统 CLI。
- 模型绑定可选择 Agent 默认、官方/自带模型、OMNIX 已启用且兼容的模型。

Gemini CLI、OpenCode 当前运行适配必须显示“待适配”；不得以终端文本解析冒充结构化支持。

## 7. 单 Agent 工作闭环

1. 新建普通对话，不选工作区；选择 Claude Code 或 Codex。
2. 新建工作区会话，通过系统文件夹选择器选择真实目录。
3. 分别选择 Agent 默认模型和一个兼容 OMNIX 模型。
4. 检查直接执行、计划模式，以及请求审批、风险时审批、完全访问。
5. 完全访问必须每个会话再次确认，不能保存为全局默认。
6. 发送包含长文本的任务，输入框应自动增高且不遮挡消息。
7. 验证用户消息、助手消息、工具调用、审批、错误、结束事件分别持久化。
8. 停止会话，重启应用，确认历史、运行事件和外部 session ID 可恢复。
9. 工作区面板显示真实文件树、Git 分支和变更；点击工作区名称可打开目录。

## 8. Team 检测

1. 填写团队目标、选择工作区和队长。
2. 点击“生成队长计划”，确认调用真实 Claude/Codex 计划会话。
3. 未确认计划前，启动 Worker 按钮不可用。
4. 检查任务 ID、Agent、依赖、验收标准和重试次数。
5. 循环依赖、未知依赖、重复 ID 和不支持的 Agent 必须在启动前拒绝。
6. 确认计划后设置并发数，启动 Worker。
7. 检查依赖任务不会提前启动；独立任务可并发。
8. 触发 Codex 审批，确认 Team 页面显示等待审批并能批准/拒绝。
9. 触发失败，确认按预算重试；预算耗尽后标记失败并阻塞依赖任务。
10. 全部 Worker 完成后，队长必须检查工作区并给出 PASS/FAIL 验收结论。
11. 停止 Team run 后，活动 Worker 进程和状态均变为取消。

## 9. Skills 检测

- 技能库：创建、编辑、启用、收藏、分类、导入和导出。
- 技能市场：搜索真实 Git 来源、预览真实 `SKILL.md`、显示仓库/revision/hash 后导入；网络或内容缺失时不得生成占位技能。
- 熔炉：选择至少两个技能和一个 Models 中启用模型；生成融合内容、冲突说明和草案；批准前不得写入技能库。
- 组合：保存 Skill Set，检查依赖冲突，并同步到选定工具。
- 同步：Claude/Codex 显示“已验证同步”；Gemini/OpenCode 等显示“实验性适配”。
- 冲突与漂移：验证 skip/overwrite/rename、内容哈希和漂移扫描。

## 10. 经验与记忆检测

1. 选择有真实消息的会话和一个启用模型。
2. 生成蒸馏候选。
3. 检查每个候选包含类型、内容和证据消息 ID。
4. 拒绝候选后不得写入记忆、技能或协议。
5. 批准记忆候选后写入长期记忆；批准技能候选后写入技能库。
6. 协议候选保留为审批记录，不自动修改全局 Skill 或 OMNIX 源码。

## 11. Knowledge 检测

1. 新建两个命名知识库。
2. 使用系统文件/文件夹选择器分别导入不同文档。
3. 生成 Embedding，执行 BM25 + 向量 + RRF 检索。
4. 普通对话选择一个或多个知识库，确认结果只来自绑定库并显示知识库、文档引用。
5. 默认不选择知识库时不运行 RAG。
6. 工作区开发会话和 Team 不自动使用知识库。

## 12. 快捷助手检测

- 默认关闭自动划词捕获。
- 开启后测试翻译、解释、总结、润色、搜索、复制。
- KeePass、1Password、Bitwarden、Windows Security 等黑名单窗口不得记录选中文本。
- 手动捕获和自动捕获都必须执行黑名单检查。
- 快捷键冲突不得导致主应用崩溃。

## 13. 应用壳与其他功能

- 顶栏固定、应用宫格收纳、隐藏、恢复默认和前后排序持久化。
- 设置和诊断只作为固定标题栏控制，不在应用宫格重复出现。
- 助手模板支持收藏、复制和带入工作页。
- MCP、Search、Cron、Compare、Code Analysis、Labs 能打开；实验或不完整能力必须显示状态，不得伪装稳定支持。

## 14. 打包门禁

```powershell
npm.cmd run tauri:build:msi
npm.cmd run tauri:build:nsis
npm.cmd run tauri:build
```

验收标准：

- `src-tauri/target/release/omnix-workbench.exe` 为本轮时间。
- MSI 与 NSIS 都是本轮时间，产品名和 E1 图标正确。
- MSI 使用正常 WiX ICE 校验；`-sval` 产物只能诊断，不能交付。
- 启动 release exe 后重复检查首页、模型选择、Team、Skills、知识库和记忆页。

## 15. 已知边界

- 首期真实结构化运行只承诺 Claude Code、Codex。
- Gemini CLI、OpenCode 的运行和技能同步仍需分别验证；界面应标记状态。
- macOS/Linux 划词体验属于 Labs。
- Rust 仍有既有 unused/dead-code 警告，后续按领域删除，不以全局 `allow` 掩盖。

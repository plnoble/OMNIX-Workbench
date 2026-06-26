# OMNIX Workbench 0.1.0 发布清单

发布日期：2026-06-25  
平台：Windows 10/11 x64  
产品定位：多 Agent 开发与协作工作台

## 交付产物

| 产物 | 大小 | SHA256 |
| --- | ---: | --- |
| `OMNIX Workbench_0.1.0_x64_en-US.msi` | 12,296,192 bytes | `2E92C98E591DF8D225B9B7654B78CCF1D08D680A3028B9A7AE856FED6C64CC52` |
| `OMNIX Workbench_0.1.0_x64-setup.exe` | 8,307,865 bytes | `6949F3A797BE84472ACDD851552E4F4C7878AD062FC29B1004C4B591AD8EF8B2` |
| `omnix-workbench.exe` | 28,801,024 bytes | `1DBBA2848082A1A0E339CD98A06D24670C481161B54E38EDE9F717441DE961DE` |

建议优先使用 MSI；NSIS 是功能等价的备用安装器；独立 EXE 适合免安装测试。

## 自动化验证

- `npx.cmd tsc --noEmit`：通过。
- `npm.cmd run build`：通过；保留两条已知 Vite 分块提示，不影响产物。
- `cargo check`：通过；90 条 unused/dead-code 警告，较本轮基线 126 条下降。
- `cargo test --lib`：70 通过、0 失败、4 按设计忽略。
- `npm.cmd run tauri -- build`：通过，正常 WiX 校验 MSI 与 NSIS 均生成。

## 视觉验证

- 在 Windows 200% 缩放环境下以 Per-Monitor-V2 截图进程检查 release 程序。
- 全尺寸主窗口的顶栏、Agent 横向选择、上下文侧栏、欢迎区、输入区和右侧操作完整可见。
- 最小逻辑尺寸 `640x520` 下启用短窗口欢迎形态；模型、审批、工作模式、知识库、搜索和发送控件无重叠或横向截断。
- 工作区面板在窄窗口改为覆盖层，不挤压主要对话区。

## 本版真实能力

- Claude Code `stream-json` 与 Codex `app-server` 结构化单 Agent 运行。
- 会话、消息、工具、审批、错误、原始事件、停止与恢复持久化。
- 真实工作区文件树、Git 分支和变更。
- 模型兼容判断、会话/绑定/默认优先级与主 Key 加故障切换。
- 用户确认后运行的 Team 计划、依赖调度、Worker 重试/停止和最终验证。
- 真实 Skill Git 导入、融合草案审批、Skill Set 与同步状态。
- 证据驱动的记忆、技能和协议蒸馏收件箱。
- 命名知识库、多库 RAG、引用来源与保守型快捷助手。

## 明确边界

- 首期真实运行适配承诺 Claude Code 与 Codex；Gemini CLI、OpenCode 等在完成结构化适配前显示为待适配。
- `追求目标`、未验证的 Skill 同步目标和部分跨平台快捷助手能力继续放在 Labs。
- 真实在线模型请求依赖用户自己的 CLI 登录、供应商 API Key 和网络环境；离线自动化使用协议假程序验证，不消耗用户模型额度。
- Rust 剩余 90 条警告是后续技术债，不是本次构建失败。

## 配套文档

- `OMNIX-Workbench-完整检测方案.md`
- `OMNIX-Workbench-使用说明书.md`


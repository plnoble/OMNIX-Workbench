# OMNIX Workbench

> **多 Agent AI 开发工具的统一编排枢纽** — 一个桌面应用管理所有 AI 编码 Agent 的技能、同步、执行和协作。

[![Tauri v2](https://img.shields.io/badge/Tauri-v2-blue)](https://tauri.app)
[![React 19](https://img.shields.io/badge/React-19-61dafb)](https://react.dev)
[![Rust](https://img.shields.io/badge/Rust-2021-orange)](https://www.rust-lang.org)
[![TypeScript](https://img.shields.io/badge/TypeScript-strict-blue)](https://www.typescriptlang.org)
[![License: MIT](https://img.shields.io/badge/License-MIT-green)](LICENSE)

---

## 🎯 它是什么？

OMNIX Workbench 是一个 **多 Agent 开发与协作工作台**，让你在一个桌面应用中统一管理 AI 编码 Agent、团队、技能与模型资源（Claude Code、Gemini CLI、Codex、OpenCode 等）。

**核心能力：**
- 🔄 **技能同步引擎** — 将 Skill 文件一键同步到所有 Agent 的技能目录
- 🧠 **55 个预设角色模板** — Bug 修复、代码审查、PRD 撰写等开箱即用
- 🌐 **Git 技能源** — 从 Git 仓库发现、导入、追踪更新
- 📦 **技能包导入导出** — `.skill` 格式打包分享
- 🤖 **7 个 Agent CLI 支持** — Claude Code / Gemini CLI / Codex / Copilot / Qwen Code / Antigravity / OpenCode
- 🔌 **协议翻译代理** — Anthropic ↔ OpenAI 格式互转，任何 Agent 用任何 LLM

---

## 📸 架构总览

```
┌──────────────────────────────────────────────────────┐
│                    Tauri Desktop App                  │
├────────────────────────┬─────────────────────────────┤
│   Frontend (React)     │    Backend (Rust)           │
│                        │                             │
│  ┌─ SkillTab          │  ┌─ tool_adapters.rs        │
│  ├─ AgentHubTab       │  ├─ sync_engine.rs           │
│  ├─ ChatTab           │  ├─ agent_templates.rs       │
│  ├─ KnowledgeTab      │  ├─ skill_frontmatter.rs     │
│  ├─ MemoryTab         │  ├─ knowledge.rs (RAG)       │
│  ├─ SupervisionTab    │  ├─ selection.rs (Win32 UIA) │
│  ├─ SettingsTab       │  ├─ proxy*.rs (网关 + 鉴权)  │
│  └─ DashboardTab      │  └─ runtime*.rs (会话运行时) │
│    …共 31 个 Tab      │                             │
│                        │                             │
│  21 个自定义 Hooks      │  SQLite 74 表 + FTS5        │
└────────────────────────┴─────────────────────────────┘
```

---

## 🚀 快速开始

### 环境要求

- **Node.js** ≥ 18
- **Rust** 1.94.0（CI 钉死这个版本，见 `.github/workflows/ci.yml`——`clippy -D warnings`
  是阻塞门，浮动工具链会让门在没碰过相关代码的提交上变红）
- **Git**

### 安装与运行

```bash
# 克隆仓库
git clone https://github.com/plnoble/OMNIX-Workbench.git
cd OMNIX-Workbench

# 安装前端依赖
npm install

# 启动开发模式（前端 + 后端热重载）
npx tauri dev

# 构建生产版本
npx tauri build
```

---

## 🧩 核心功能

### 1. 技能同步引擎（Skill Sync）

将 `SKILL.md` 文件同步到所有已安装 AI Agent 的技能目录：

| 功能 | 说明 |
|------|------|
| **单向同步** | 选择技能 → 选择目标工具 → 一键同步 |
| **批量同步** | 多个技能 → 所有已安装工具 |
| **冲突检测** | 目标已存在时提供 skip/overwrite/rename 策略 |
| **漂移检测** | 自动检测源文件变更，标记需要更新的技能 |
| **全磁盘扫描** | 发现未管理的技能文件，一键导入 |

**支持的工具适配器：**

| 工具 | 技能路径 | 检测方式 |
|------|----------|----------|
| Claude Code | `~/.claude/skills/` | `which claude` |
| Cursor | `~/.cursor/skills/` | 安装目录检测 |
| GitHub Copilot | `~/.github/copilot/skills/` | VS Code 扩展扫描 |
| Gemini CLI | `~/.gemini/skills/` | `which gemini` |
| Codex | `~/.codex/skills/` | `which codex` |
| OpenCode | `~/.opencode/skills/` | `which opencode` |

### 2. Agent 模板库

55 个预设角色模板，每个包含专业的系统提示和关联技能：

| 分类 | 数量 |
|------|------|
| **Engineering** | 9 |
| **Product** | 7 |
| **办公** / **Writing** | 6 / 6 |
| **Meta** | 5 |
| **DevOps** / **Data** | 4 / 4 |
| **Workflow** / **Life** / **Education** / **Design** | 3 / 3 / 3 / 3 |
| **Security** | 2 |

### 3. Git 技能源

从 Git 仓库发现和导入技能：

```bash
# 流程：输入 Git URL → 克隆 → 扫描 skills/ 目录 → 选择导入
# 自动追踪 source_revision，检测更新
```

- 浅克隆（`--depth 1`）到 `~/.omnix/skill_cache/`
- 自动扫描 `skills/<name>/SKILL.md`
- 追踪 `source_type=git` + `source_ref=URL` + `source_revision=hash`
- 30 天自动清理过期缓存

### 4. Skill Frontmatter 标准化

SKILL.md 使用 YAML frontmatter 实现自描述：

```yaml
---
name: web-design-guidelines
description: Review UI code for compliance
category: Design
version: "1.0.0"
author: vercel
argument-hint: <file-or-pattern>
skills:
  - code-reviewer
  - frontend-builder
---

# Web Design Guidelines
Actual skill content here...
```

### 5. 协议翻译代理

内置 Axum HTTP 代理服务器（端口 1421），实现：

- **Anthropic → OpenAI** 格式翻译
- **OpenAI → Anthropic** 格式翻译
- **Stream 事件双向转换**
- **动态能力路由** — 模型设为 "Auto" 时，根据请求内容（vision/reasoning/coding/speedy）自动选择最佳模型

### 6. RAG 知识库

- 文档分块（Markdown / 代码 / 纯文本，带重叠）
- BM25 全文搜索（SQLite FTS5）+ 向量相似度搜索
- Reciprocal Rank Fusion (RRF) 混合排序
- 支持 Ollama + OpenAI-compatible 嵌入模型

### 7. Windows 原生选择助手

- **Tier 1**: Windows UI Automation (UIA) 被动捕获，不依赖剪贴板
- **Tier 2**: SendInput Ctrl+C + 剪贴板读取（兜底方案）
- 全局快捷键触发（默认 `Ctrl+Alt+C`）

---

## 🏗️ 技术栈

| 层 | 技术 | 版本 |
|---|---|---|
| **桌面框架** | Tauri | v2 |
| **前端** | React + TypeScript (strict) | 19.x |
| **构建** | Vite | 7.x |
| **UI 组件** | shadcn/ui (Radix) + Tailwind CSS | 4.x |
| **后端** | Rust + Tokio (async) | 2021 edition |
| **HTTP 代理** | Axum | 0.7 |
| **数据库** | SQLite (rusqlite, bundled) | 0.33 |
| **HTTP 客户端** | reqwest | 0.12 |
| **可视化** | D3.js (拓扑图) | 7.x |

---

## 📁 项目结构

```
OMNIX-Workbench/
├── src/                          # 前端源码
│   ├── App.tsx                   # 主编排器
│   ├── components/
│   │   ├── tabs/                 # 31 个功能 Tab
│   │   ├── modals/               # 6 个 Modal
│   │   ├── layout/               # Header/Sidebar/Preview
│   │   └── ui/                   # shadcn/ui 组件
│   ├── hooks/                    # 21 个自定义 Hook
│   ├── lib/                      # tauri-api.ts / api/ 包装 / utils
│   └── types/                    # TypeScript 类型
├── src-tauri/                    # 后端源码
│   ├── src/
│   │   ├── lib.rs                # 应用初始化 + 402 个命令注册
│   │   ├── commands/             # Tauri 命令（56 个文件，按领域分）
│   │   ├── db.rs / db_schema.rs  # SQLite（74 张表）
│   │   ├── proxy*.rs             # 网关：Anthropic ↔ OpenAI 翻译、鉴权、远程面板
│   │   ├── runtime*.rs           # Agent 会话运行时（Claude / Codex / ACP / print）
│   │   ├── agent.rs              # Agent 检测、安装、定时任务调度
│   │   ├── tool_adapters.rs      # 工具适配器 (6 个)
│   │   ├── sync_engine.rs        # 同步引擎 + 扫描器 + Git 源
│   │   ├── agent_templates.rs    # 55 个 Agent 模板
│   │   ├── skill_frontmatter.rs  # YAML frontmatter 解析
│   │   ├── knowledge.rs          # RAG 知识库引擎
│   │   └── selection.rs          # Win32 选择助手
│   └── Cargo.toml
├── logs/                         # 开发记忆日志
│   ├── decisions/                # 架构决策记录 (DEC-xxx)
│   ├── tasks/                    # 任务追踪 (TASK-xxx)
│   ├── timeline/                 # 时间线事件 (EVENT-xxx)
│   ├── reflections/              # 事后回顾 (REF-xxx)
│   ├── bugs/                     # Bug 记录
│   └── reviews/                  # Code Review 记录
└── memory/                       # Agent 记忆库
    ├── working_memory/
    ├── episodic_memory/
    ├── semantic_memory/
    └── skill_memory/
```

---

## 📊 代码规模

| 类别 | 规模 |
|------|------|
| Rust 后端 | ~67,200 行 |
| TypeScript 前端 | ~32,800 行 |
| **合计** | **~100,000 行** |
| Tauri 命令 | 402 个 |
| SQLite 表 | 74 张 |
| Rust 测试 | 568 个 |
| 前端测试 | 50 个 |

> 数字用脚本数出来的，不是估的。上一版这里写着 ~18,000 行——那是很久以前的
> 数字，一直没更新。
>
> **命令数是 461 → 402，唯一一个降下来的数字。** 不是删功能，是删「注册了但前端
> 根本够不着」的死命令——那些命令编译得过、测试也绿，只是没有任何一行前端代码会
> 调用它们。详见下面的接线守卫。

---

## 🔧 开发命令

```bash
# 前端开发服务器
npm run dev

# Tauri 开发模式（前端 + 后端）
npx tauri dev

# 构建生产版本
npx tauri build
```

**提交前把 CI 的门在本地跑一遍**（顺序与 CI 一致，任意一步红就别提交）：

```bash
npx tsc --noEmit && npx vite build && npx vitest run && npm run lint && cd src-tauri && cargo test --lib && cargo clippy --lib --tests -- -D warnings && cd .. && bash .github/scripts/pitfall-guard.sh
```

本地 Rust 工具链要和 CI 一致（1.94.0），否则 `clippy -D warnings` 可能本地绿、CI 红：

```bash
rustup toolchain install 1.94.0 --component clippy
```

---

## 🛡️ 工程原则

本项目严格遵循以下开发规范：

- **SOLID / DRY / KISS / YAGNI** — 不过度设计，不重复代码
- **TypeScript strict** — 禁止 `any`，语义类型区分
- **结构化日志** — JSON 格式，Trace ID 贯穿
- **安全编码** — 参数化 SQL，输入验证，密钥不硬编码
- **AI Development Memory** — 每个任务/决策/错误都有结构化记录

### CI 阻塞门

失败即拦，没有 `continue-on-error`：

| 门 | 命令 |
|---|---|
| 类型检查 + 构建（含产物体积预算） | `tsc --noEmit && vite build` |
| 前端单测 | `vitest run` |
| 前端 lint | `biome lint src --diagnostic-level=error` |
| Rust 单测 | `cargo test --lib` |
| Rust lint | `cargo clippy --lib --tests -- -D warnings` |
| 坑点守卫 | `.github/scripts/pitfall-guard.sh` |

`rustfmt` 等仍是非阻塞信号——但**如实报红**，不再用 `continue-on-error` 伪装成绿勾。

**坑点守卫**扫的是 `CLAUDE.md` 里记着的三条历史事故（`credentials: 'include'` 配
通配符 CORS、跨 `await` 持有 `std::sync::MutexGuard`、`git push -f`）。写下来跟拦得住
是两回事，这一步负责拦。

### 接线守卫

这个项目最高产的一类 bug 不是逻辑写错，而是**两端各自都对、中间没接上**——后端命令
注册了、编译过了、测试全绿，但没有任何一行前端代码会调它。Rust 编译器和 TypeScript
都不会吭声，因为跨端只靠字符串对上号。

四道守卫各守一条接缝，都是**只允许变短的棘轮**：

| 守卫 | 抓什么 |
|---|---|
| `eventWiring.test.ts` | 后端 `emit` 的事件，前端没有 `listen` |
| `commandWiring.test.ts` | 注册的命令没有前端调用方（死代码） |
| 同上 | 前端 `invoke` 了根本不存在的命令（**调到就是运行时报错**） |
| 同上 | API 包装没有任何组件在用（整条链是死的） |

外加一道数据库层的「写了但没人读」检查：某张表只有写入没有读取，说明要么功能没接完，
要么这张表该删。

---

## 📄 License

MIT License

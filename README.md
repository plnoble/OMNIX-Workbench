# OMNIX DevFlow

> **多 Agent AI 开发工具的统一编排枢纽** — 一个桌面应用管理所有 AI 编码 Agent 的技能、同步、执行和协作。

[![Tauri v2](https://img.shields.io/badge/Tauri-v2-blue)](https://tauri.app)
[![React 19](https://img.shields.io/badge/React-19-61dafb)](https://react.dev)
[![Rust](https://img.shields.io/badge/Rust-2024-orange)](https://www.rust-lang.org)
[![TypeScript](https://img.shields.io/badge/TypeScript-strict-blue)](https://www.typescriptlang.org)
[![License: MIT](https://img.shields.io/badge/License-MIT-green)](LICENSE)

---

## 🎯 它是什么？

OMNIX DevFlow 是一个 **Tauri v2 桌面应用**，让你在一个界面中统一管理多个 AI 编码 Agent（Claude Code、Gemini CLI、Codex、GitHub Copilot CLI 等）。

**核心能力：**
- 🔄 **技能同步引擎** — 将 Skill 文件一键同步到所有 Agent 的技能目录
- 🧠 **25 个预设角色模板** — Bug 修复、代码审查、PRD 撰写等开箱即用
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
│  ┌─ SkillHub          │  ┌─ tool_adapters.rs        │
│  ├─ AgentHub          │  ├─ sync_engine.rs           │
│  ├─ ChatTab           │  ├─ agent_templates.rs       │
│  ├─ KnowledgeHub      │  ├─ skill_frontmatter.rs     │
│  ├─ MemoryHub         │  ├─ knowledge.rs (RAG)       │
│  ├─ CompareHub        │  ├─ selection.rs (Win32 UIA) │
│  ├─ SettingsTab       │  ├─ proxy.rs (Anthropic↔OAI) │
│  └─ DashboardTab      │  └─ agent.rs (PTY Manager)   │
│                        │                             │
│  17 个自定义 Hooks      │  SQLite 20 表 + FTS5        │
└────────────────────────┴─────────────────────────────┘
```

---

## 🚀 快速开始

### 环境要求

- **Node.js** ≥ 18
- **Rust** ≥ 1.70
- **Git**

### 安装与运行

```bash
# 克隆仓库
git clone https://github.com/plnoble/OMNIX-Development-Tools.git
cd OMNIX-Development-Tools

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

### 2. Agent 模板库

25 个预设角色模板，每个包含专业的系统提示和关联技能：

| 分类 | 模板 |
|------|------|
| **Engineering** | Bug Fixer, Code Reviewer, Frontend Builder, Commit Message, PR Description, ADR Writer, RCA Writer... |
| **Product** | PRD Drafter, PRD Critic, OKR Drafter, One-Pager, User Story Writer, Brainstormer |
| **Writing** | Summarizer, 中英互译, Email Reply, Writing Critic, JD Writer |
| **Design** | UX Copywriter, HTML Slides, Tutor |

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
| **后端** | Rust + Tokio (async) | 2024 edition |
| **HTTP 代理** | Axum | 0.7 |
| **数据库** | SQLite (rusqlite, bundled) | 0.31 |
| **终端** | portable-pty | 0.8 |
| **HTTP 客户端** | reqwest | 0.12 |
| **可视化** | D3.js (拓扑图) | 7.x |

---

## 📁 项目结构

```
OMNIX-Development-Tools/
├── src/                          # 前端源码
│   ├── App.tsx                   # 主编排器
│   ├── components/
│   │   ├── tabs/                 # 10 个功能 Tab
│   │   ├── modals/               # 6 个 Modal
│   │   ├── layout/               # Header/Sidebar/Preview
│   │   └── ui/                   # shadcn/ui 组件
│   ├── hooks/                    # 17 个自定义 Hook
│   ├── lib/                      # tauri-api.ts / utils
│   └── types/                    # TypeScript 类型
├── src-tauri/                    # 后端源码
│   ├── src/
│   │   ├── lib.rs                # 应用初始化 + 60+ 命令注册
│   │   ├── commands.rs           # Tauri 命令处理器 (5000+ 行)
│   │   ├── db.rs                 # SQLite 数据库 (20 表)
│   │   ├── proxy.rs              # Anthropic ↔ OpenAI 翻译代理
│   │   ├── agent.rs              # Agent 子进程管理 (PTY)
│   │   ├── tool_adapters.rs      # 工具适配器 (5 个)
│   │   ├── sync_engine.rs        # 同步引擎 + 扫描器 + Git 源
│   │   ├── agent_templates.rs    # 25 个 Agent 模板
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

| 类别 | 行数 |
|------|------|
| Rust 后端 | ~12,000+ |
| TypeScript 前端 | ~6,000+ |
| **合计** | **~18,000+** |

---

## 🔧 开发命令

```bash
# 前端开发服务器
npm run dev

# Tauri 开发模式（前端 + 后端）
npx tauri dev

# TypeScript 类型检查
npx tsc --noEmit

# Rust 编译检查
cd src-tauri && cargo check

# Rust 单元测试
cd src-tauri && cargo test --lib

# 构建生产版本
npx tauri build
```

---

## 🛡️ 工程原则

本项目严格遵循以下开发规范：

- **SOLID / DRY / KISS / YAGNI** — 不过度设计，不重复代码
- **TypeScript strict** — 禁止 `any`，语义类型区分
- **Rust `#![deny(unused)]`** — 零编译警告
- **结构化日志** — JSON 格式，Trace ID 贯穿
- **安全编码** — 参数化 SQL，输入验证，密钥不硬编码
- **AI Development Memory** — 每个任务/决策/错误都有结构化记录

---

## 📄 License

MIT License

---

## 🙏 致谢

借鉴了以下项目的优秀设计：
- [Multica](https://github.com/multica-ai/multica) — Agent 模板系统、Skill Frontmatter、Skills Lock File、Autopilot 模式
- [SkillsLM](https://github.com/) — 工具适配器架构、Skill 同步引擎
- [shadcn/ui](https://ui.shadcn.com) — UI 组件库
- [Tauri](https://tauri.app) — 桌面应用框架

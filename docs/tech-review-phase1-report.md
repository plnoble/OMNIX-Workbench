# OMNIX Workbench 技术审查报告（第一阶段 · 只审不改）

审查范围：全仓库（Rust ~67k 行 + TS ~32k 行，461 个 Tauri 命令，77 张表）
审查方式：静态审查 + 实际运行验证（build/test/audit/复现实验）
审查日期：2026-08-15

---

## 一、软件理解

- **定位**：多 Agent 开发与协作工作台（Tauri v2 桌面应用，Windows 优先）
- **技术栈**：Rust 2021 + Tokio + Axum + rusqlite(FTS5) + portable-pty；React 19 + TS strict + Vite 7 + Tailwind 4 + Radix
- **架构**：前端 10+ Tab → invoke 461 命令 → 后端模块（agent/runtime/proxy(1421)/knowledge/selection/crypto）；SQLite 单库；AES-256-GCM+DPAPI 密钥保护；Anthropic↔OpenAI 协议翻译网关
- **数据流**：UI → 命令层（含 input_validation）→ DbManager(r2d2 池) / AgentManager(PTY) / ProxyServer
- **关键路径**：对话流式响应、CLI Agent 会话管理、checkpoint/worktree 版本控制、RAG 检索

## 二、运行验证（全部实际执行）

| 检查 | 结果 |
|---|---|
| tsc --noEmit（strict 全开） | ✅ 0 错误 |
| vitest run | ✅ 48/48 通过（1.91s） |
| npm run build + bundle budget | ✅ 成功，主包 404KB < 480KB 预算 |
| cargo check --lib / --tests | ✅ 通过 |
| cargo test --lib | ⚠️ 552 通过 / **2 失败** / 7 ignored |
| cargo clippy --lib | ⚠️ 97 个风格类警告（无正确性问题） |
| npm audit (prod) | ⚠️ nanoid High + esbuild (dev-only) |

## 三、问题清单

### 高危

**P1 · Antigravity 安装器管道死锁**
- 位置：`src-tauri/src/agent.rs:518-524`
- 问题：`stdout(Stdio::piped()).stderr(Stdio::piped())` 后只 `wait().await`，从不读取管道
- 触发：安装脚本输出 > 管道缓冲区（Windows 4-64KB）→ 子进程写阻塞 + 父进程等退出 = 双向死锁
- 影响：用户点击"安装 Antigravity"后 UI 永久卡死，无超时无取消
- 证据：代码中 grep 确认无 `child.stdout.take()` 或 `wait_with_output`；同文件 1289 行的其他代码路径有正确读取
- 修复：改用 `child.wait_with_output().await`
- 建议立即修复：是

**P2 · checkpoint 对 git update-ref 静默失败零防御（本次 2 个测试失败的根因链）**
- 位置：`src-tauri/src/commands/checkpoints.rs:151-152`、`worktrees.rs:168`
- 问题：`update-ref`/`worktree add -b` 返回 0 即认为成功，无 rev-parse 回读校验
- 实测证据（本机复现，官方 Git 2.55.0.2 与 PortableGit 2.55.0.3 双复现，node 子进程亦复现，reftable 后端正常）：
  - `git update-ref refs/x/y/z HEAD` → exit=0 但 ref 文件未落盘、reflog 无记录
  - `git worktree add -b omnix/s1 <path> HEAD` → `fatal: invalid reference`（斜杠分支名均失败，普通名成功）
  - 归因：git 2.55 files-ref 后端在本机环境静默丢写（疑 EDR/文件系统拦截，非 OMNIX 代码缺陷）；但 OMNIX 代码信任 exit code 使故障**不可见**
- 影响：checkpoint 显示"创建成功"但 ref 不存在 → 恢复时才发现数据从未快照（数据安全功能静默失效）；worktree 功能完全不可用
- 修复：① update-ref 后 `rev-parse --verify` 回读，失败立即报错并落库标记；② 测试与产品对 git 版本行为做兼容探测；③ 向 git 社区确认 2.55 是否为已知回归
- 建议立即修复：是（防御层是产品责任）

### 中危

**P3 · nanoid 3.3.16 GHSA-2v37-7h3g-55p8（High 级依赖漏洞）**
- 位置：package-lock.json（postcss 间接依赖）
- 影响：自定义 generator size=0 时无限循环（DoS 面，桌面应用实际触达低）
- 修复：`npm audit fix`（升至 3.3.18+）
- 建议立即修复：是（一行命令）

**P4 · toggle_selection_auto_capture 的 try_state+manage 竞态**
- 位置：`src-tauri/src/commands/selection.rs:430-435`
- 问题：并发首次调用均见 `try_state()==None`，双重 `manage()` 导致第二次 panic
- 触发：极小窗口并发调用（单 UI 开关场景概率低）
- 修复：启动时统一 manage，或用 OnceLock/全局静态
- 建议立即修复：否（低概率）

**P5 · esbuild 0.27.7 GHSA-g7r4-m6w7-qqqr**
- 位置：devDependencies（vite 传递）
- 影响：仅 Windows 开发服务器任意文件读；生产构建产物不受影响
- 修复：随依赖升级；非紧急

### 低危 / 优化建议

**P6 · clippy 97 个风格警告**（if let → filter_map、io::Error::other 等）— 建议一次性清零并加 CI 门禁
**P7 · 仓库根临时残留**：agy_out.txt（0 字节）、t.txt（0 字节）、debug.log（WebView2 GPU 报错日志）、scratch/（20 个调试脚本）— 均未被 git 跟踪，建议删除或加入 .gitignore
**P8 · Antigravity `irm | iex` 安装模式**：URL 拉取即执行，供应链信任面；系厂商官方安装方式的行业惯例，标记为已接受风险
**P9 · 密钥迁移失败不拦启动**（migrate_plaintext_secrets_in_place 失败仅日志）：建议失败时在 UI 显式警示"仍有明文密钥"

### 安全审查通过项（正面结论，均有代码证据）

- ✅ SQL 动态拼接：全部编译期常量（CONVERSATION_OWNED_TABLES、迁移表列表、BILLED_INPUT），逐一验证
- ✅ 命令注入：无 shell:true；hooks.rs 三重防护（禁分隔符→目录约束→canonicalize 防符号链接逃逸）；remote_dev 4 处 ssh 调用全为常量/白名单查表
- ✅ XSS：react-markdown 5 处使用点均无 rehype-raw/dangerouslySetInnerHTML
- ✅ 并发：std::sync::Mutex 无跨 await 持锁（Send 约束结构性兜底）；lifecycle.rs 锁作用域显式收拢
- ✅ 密钥：AES-256-GCM + DPAPI，历史明文双写漏洞已修复，启动自动迁移
- ✅ 网络：代理默认 127.0.0.1，远程访问显式开启 + CORS 白名单 + ct_eq 常量时间令牌比较
- ✅ 生产代码零 TODO/FIXME/临时代码（唯一命中是检测工具的字符串常量）

## 四、软件健康报告

### 综合评分：**87 / 100**

| 维度 | 评价 |
|---|---|
| 功能正确性 | 82（P1 死锁 + P2 防御缺失） |
| 安全性 | 93（核心面全绿，仅依赖漏洞） |
| 架构 | 90（模块边界清晰、历史坑注释文化极佳） |
| 测试 | 90（552 Rust + 48 前端，覆盖真实往返场景） |
| 工程卫生 | 88（clippy 警告、临时文件残留） |

### 分级统计
- Critical：0
- High：3（P1、P2、P3）
- Medium：2（P4、P5）
- Low：4（P6-P9）

### 是否适合正式使用
**基本适合，修复 P1/P2 后可放心发布。** 核心对话、Agent 管理、密钥保护、网关功能可靠；checkpoint/worktree 在受影响 git 环境下不可用（P2），Antigravity 安装有卡死风险（P1）。

### Top 10 优先问题（修复顺序）
1. P1 Antigravity 管道死锁（wait_with_output）
2. P2 checkpoint 回读校验 + git 兼容探测
3. P3 npm audit fix（nanoid）
4. P2 附属：向 git 社区报障确认 2.55 回归
5. P4 selection manage 竞态
6. P9 密钥迁移失败 UI 警示
7. P6 clippy 清零 + CI 门禁
8. P7 临时文件清理
9. P5 esbuild 随依赖升级
10. P8 供应链风险记录入档

### 架构最薄弱三处
1. **外部 git 无验证信任**：checkpoint/worktree 全押 git ref 行为，无回读、无降级、无版本探测
2. **子进程输出管理不一致**：Antigravity 死锁模式 vs 其他路径正确读取，缺统一封装
3. **运行时惰性 manage 状态**：AutoCaptureHandle 模式易复制出竞态

### 最易出线上事故三处
1. 用户装 Antigravity → UI 永久卡死（P1）
2. 用户依赖 checkpoint 回滚 → 发现快照从未存在（P2 假成功）
3. 明文密钥迁移静默失败 → 用户误以为已加密（P9）

### 下一步建议
- 第二阶段按 Top 10 顺序修复（P1/P3 均为单行级改动）
- CI 增加：cargo clippy -D warnings、cargo test、npm audit、tsc --noEmit 四门禁
- 建立 git 版本兼容矩阵测试（files vs reftable、2.4x vs 2.55）

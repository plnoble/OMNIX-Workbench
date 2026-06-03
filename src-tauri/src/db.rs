use rusqlite::{params, Connection, Result};
use std::fs;
use std::path::PathBuf;

pub struct DbManager {
    db_path: PathBuf,
}

impl DbManager {
    pub fn new() -> Self {
        // Resolve home directory path on Windows / Linux
        let home_dir = dirs::home_dir().unwrap_or_else(|| PathBuf::from("C:\\Users\\87953"));
        let mut omnix_dir = home_dir.clone();
        omnix_dir.push(".omnix");
        
        // Ensure directory exists
        if !omnix_dir.exists() {
            fs::create_dir_all(&omnix_dir).expect("Failed to create .omnix data directory");
        }
        
        let mut db_path = omnix_dir;
        db_path.push("omnix.db");
        
        let db = Self { db_path };
        db.init_schema().expect("Failed to initialize database schema");
        db
    }

    pub fn new_with_path(db_path: PathBuf) -> Self {
        let db = Self { db_path };
        db.init_schema().expect("Failed to initialize database schema");
        db
    }

    pub fn get_connection(&self) -> Result<Connection> {
        Connection::open(&self.db_path)
    }

    pub fn init_schema(&self) -> Result<()> {
        let conn = self.get_connection()?;
        
        // 1. Settings Table (atomic key-value config)
        conn.execute(
            "CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL,
                updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        )?;

        // 2. Agents Table (discovered & installed CLI tools)
        conn.execute(
            "CREATE TABLE IF NOT EXISTS agents (
                name TEXT PRIMARY KEY,
                path TEXT NOT NULL,
                version TEXT NOT NULL,
                status TEXT NOT NULL,
                updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        )?;

        // 3. Conversations Table (chat/agent sessions)
        conn.execute(
            "CREATE TABLE IF NOT EXISTS conversations (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                workspace_path TEXT NOT NULL,
                active_agent TEXT NOT NULL,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        )?;

        // 4. Messages Table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS messages (
                id TEXT PRIMARY KEY,
                conversation_id TEXT NOT NULL,
                role TEXT NOT NULL,
                content TEXT NOT NULL,
                timestamp DATETIME DEFAULT CURRENT_TIMESTAMP,
                FOREIGN KEY(conversation_id) REFERENCES conversations(id) ON DELETE CASCADE
            )",
            [],
        )?;

        // 5. Skills Table (custom and third party skills)
        conn.execute(
            "CREATE TABLE IF NOT EXISTS skills (
                name TEXT PRIMARY KEY,
                description TEXT NOT NULL,
                file_path TEXT NOT NULL,
                profile TEXT NOT NULL DEFAULT 'Core',
                is_active INTEGER NOT NULL DEFAULT 1,
                dependencies TEXT NOT NULL DEFAULT '[]', -- JSON array of dependent skills
                updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        )?;

        // 6. Memory Table (anti-failure incident dict)
        conn.execute(
            "CREATE TABLE IF NOT EXISTS memories (
                id TEXT PRIMARY KEY,
                incident_desc TEXT NOT NULL,
                code_pattern TEXT NOT NULL,
                remediation TEXT NOT NULL,
                keywords TEXT NOT NULL, -- comma-separated tags
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        )?;

        // 6b. Agent Accounts Table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS agent_accounts (
                id TEXT PRIMARY KEY,
                account_name TEXT NOT NULL,
                api_key TEXT NOT NULL,
                api_host TEXT NOT NULL,
                target_model TEXT NOT NULL,
                is_active INTEGER NOT NULL DEFAULT 0,
                updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        )?;

        // 7. Tasks Table (pipeline/todo plans)
        conn.execute(
            "CREATE TABLE IF NOT EXISTS tasks (
                id TEXT PRIMARY KEY,
                conversation_id TEXT NOT NULL,
                title TEXT NOT NULL,
                status TEXT NOT NULL, -- 'todo', 'in_progress', 'done'
                order_num INTEGER NOT NULL,
                dependencies TEXT NOT NULL DEFAULT '[]',
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        )?;

        // Seed default settings if empty
        self.seed_default_settings(&conn)?;

        // Seed default skills if empty
        self.seed_default_skills(&conn)?;

        // Seed default accounts if empty
        self.seed_default_accounts(&conn)?;

        // Seed default memories if empty
        self.seed_default_memories(&conn)?;

        // Seed default conversations if empty
        self.seed_default_conversations(&conn)?;

        Ok(())
    }

    fn seed_default_conversations(&self, conn: &Connection) -> Result<()> {
        let mut stmt = conn.prepare("SELECT COUNT(*) FROM conversations")?;
        let count: i64 = stmt.query_row([], |r| r.get(0))?;
        if count > 0 {
            return Ok(());
        }

        // Insert a mock conversation
        conn.execute(
            "INSERT INTO conversations (id, title, workspace_path, active_agent)
             VALUES (?1, ?2, ?3, ?4)",
            params!["mock_sess_cors", "会话 #1: Web 前端 CORS 调试", "d:/Agent/Project/MyWebDemo", "Claude Code"],
        )?;

        // Insert messages showing a CORS issue being discussed and fixed
        let messages = vec![
            ("msg_1", "user", "我遇到了预检请求(Preflight)拦截错误，CORS 报错，说 Origin 不能是通配符 *，因为 credentials 设为了 include。"),
            ("msg_2", "assistant", "对的，当在 fetch 中设置 `credentials: 'include'` 时，浏览器的安全策略要求后端响应的 CORS 头 `Access-Control-Allow-Origin` 必须指定明确的域名（比如 `http://localhost:3000`），而不能是通配符 `*`。此外，`Access-Control-Allow-Credentials` 必须设为 `true`。"),
            ("msg_3", "user", "明白了，修改了后端的 Access-Control-Allow-Origin 为具体的请求源后成功了。"),
            ("msg_4", "assistant", "太棒了！这是一个非常经典的 Web 踩坑点。在处理跨域凭证传递时，切记不要使用 *。"),
        ];

        for (msg_id, role, content) in messages {
            conn.execute(
                "INSERT INTO messages (id, conversation_id, role, content)
                 VALUES (?1, ?2, ?3, ?4)",
                params![msg_id, "mock_sess_cors", role, content],
            )?;
        }

        // Insert another mock conversation for a Tokio lock deadlock
        conn.execute(
            "INSERT INTO conversations (id, title, workspace_path, active_agent)
             VALUES (?1, ?2, ?3, ?4)",
            params!["mock_sess_lock", "会话 #2: Rust Tokio 异步死锁排查", "d:/Agent/Project/MyRustService", "Google Antigravity"],
        )?;

        let messages_lock = vec![
            ("msg_l1", "user", "我的 Rust 异步服务卡住了，日志停在一个 await 处。我用了 std::sync::Mutex。"),
            ("msg_l2", "assistant", "在异步任务中跨越 `.await` 点持有 `std::sync::MutexGuard` 会导致线程被阻塞或者出现 Send 校验失败、死锁。你应该使用 `tokio::sync::Mutex`，或者用一个花括号作用域，在 `.await` 之前显式 drop 掉 `MutexGuard`。"),
            ("msg_l3", "user", "我用 tokio::sync::Mutex 替换了 std::sync::Mutex，并把持有锁的代码段包在了作用域内。重新测试，程序不再卡死了。"),
            ("msg_l4", "assistant", "完美！在异步上下文中，一定要防范同步锁跨 await 点的情况，否则很容易造成死锁崩溃。"),
        ];

        for (msg_id, role, content) in messages_lock {
            conn.execute(
                "INSERT INTO messages (id, conversation_id, role, content)
                 VALUES (?1, ?2, ?3, ?4)",
                params![msg_id, "mock_sess_lock", role, content],
            )?;
        }

        Ok(())
    }

    fn seed_default_accounts(&self, conn: &Connection) -> Result<()> {
        let mut stmt = conn.prepare("SELECT COUNT(*) FROM agent_accounts")?;
        let count: i64 = stmt.query_row([], |r| r.get(0))?;
        if count > 0 {
            return Ok(());
        }

        // Fetch current setting configurations to establish default profile
        let api_key = self.get_setting("api_key")?.unwrap_or_default();
        let api_host = self.get_setting("api_host")?.unwrap_or_else(|| "https://api.openai.com/v1".to_string());
        
        conn.execute(
            "INSERT INTO agent_accounts (id, account_name, api_key, api_host, target_model, is_active)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params!["default_profile", "默认账户 (Default Profile)", api_key, api_host, "deepseek-chat", 1],
        )?;

        // Seed a corporate/alternative mock account for switcher demonstration
        conn.execute(
            "INSERT INTO agent_accounts (id, account_name, api_key, api_host, target_model, is_active)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params!["company_pro", "企业专线 (High-Capacity Pro)", "sk-corp-unlimited-token-key-profile", "https://api.openai.com/v1", "gpt-4o", 0],
        )?;

        Ok(())
    }

    fn seed_default_memories(&self, conn: &Connection) -> Result<()> {
        let mut stmt = conn.prepare("SELECT COUNT(*) FROM memories")?;
        let count: i64 = stmt.query_row([], |r| r.get(0))?;
        if count > 0 {
            return Ok(());
        }

        let defaults = vec![
            (
                "mem_001",
                "跨域请求中 credentials 与 Origin 冲突导致预检拦截。",
                "fetch(url, { credentials: 'include', mode: 'cors' })",
                "当请求设置 credentials 为 include 时，后端 CORS 响应头 Access-Control-Allow-Origin 不能设为通配符 *，必须指定明确的域名 Origin。",
                "cors,fetch,credentials,web"
            ),
            (
                "mem_002",
                "Tokio 线程手动锁死：在 async fn 内阻塞等待 sync 互斥锁发生 panic 死锁。",
                "std::sync::MutexGuard across await point",
                "在异步 Task 跨 await 时不能持有 std::sync::MutexGuard，否则会导致 Send 校验失败或死锁。必须使用 tokio::sync::Mutex 或者在 await 前显式释放锁作用域。",
                "tokio,lock,deadlock,async"
            ),
            (
                "mem_003",
                "Git 强制覆写推送导致公共代码库提交日志被覆盖损坏。",
                "git push -f",
                "在多人协作仓库中绝不能执行 git push -f。强制更新必须通过分支审批 PR，或使用 --force-with-lease 安全锁推送。",
                "git,push,deploy,safety"
            )
        ];

        for (id, desc, pattern, rem, kw) in defaults {
            conn.execute(
                "INSERT INTO memories (id, incident_desc, code_pattern, remediation, keywords)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![id, desc, pattern, rem, kw],
            )?;
        }

        Ok(())
    }

    fn seed_default_skills(&self, conn: &Connection) -> Result<()> {
        // Check if skills table is empty
        let mut stmt = conn.prepare("SELECT COUNT(*) FROM skills")?;
        let count: i64 = stmt.query_row([], |r| r.get(0))?;
        if count > 0 {
            return Ok(());
        }

        // Get or create the local ~/.omnix/skills directory
        let home_dir = dirs::home_dir().unwrap_or_else(|| PathBuf::from("C:\\Users\\87953"));
        let mut skills_dir = home_dir.clone();
        skills_dir.push(".omnix");
        skills_dir.push("skills");
        if !skills_dir.exists() {
            let _ = fs::create_dir_all(&skills_dir);
        }

        let default_skills = vec![
            (
                "file_reader",
                "读取本地文件内容，支持分块读取、按行读取及大文件流式读取。",
                "[]",
                // Core
                "### Role & Identity\n你是一个专业的高效文件读取智能体。你的任务是读取本地文件内容，为上游分析器提供原始代码和文本数据。\n\n### Core Knowledge\n- 掌握编码检测：UTF-8, UTF-16, GBK 等常见字符编码。\n- 掌握流式读取：针对大文件，支持逐行或者块读取以防内存泄漏。\n\n### Step-by-Step Workflow\n1. 校验输入文件路径，确保其属于当前工作区边界。\n2. 读取文件 metadata 校验其大小。\n3. 调用底层流读取文件，以 UTF-8 编码解码输出。\n\n### Quality Checklist\n- [ ] 是否确认文件路径合法性？\n- [ ] 是否在超大文件时启用了分页读取？\n\n### Anti-Patterns\n- 🚫 严禁越权读取系统根目录外的敏感系统文件。",
                // Minimal
                "### Workflow\n1. 检查文件路径。\n2. 逐行读取文件内容并输出。",
                // Comprehensive
                "### Role & Identity\n你是一个超强性能的文件检索与读取专家，能处理数百兆的大型日志或海量代码库文件...\n\n### Core Knowledge\n- 支持多线程并发预读取和哈希校验缓存。\n- 内置基于 Trie 树结构的代码符号快速索引机制。\n\n### Quality Checklist\n- [ ] 分块大小是否优化？\n- [ ] 编码解析是否鲁棒？"
            ),
            (
                "file_writer",
                "写入和修改本地文件，支持原子性覆盖写入及备份。",
                "[\"file_reader\"]",
                "### Role & Identity\n你是一个安全的代码写入与文件重写智能体。你负责将重构或生成的代码保存到本地。\n\n### Core Knowledge\n- 原子写入规范：必须使用临时文件 .tmp 进行安全写入校验后再 rename 替换。\n- 目录自动构建：写入前如果父目录不存在需自动递归创建。\n\n### Step-by-Step Workflow\n1. 校验目标文件写入路径。\n2. 创建同名临时文件 .tmp 并写入最新内容。\n3. 执行完整性校验（校验长度及行数）。\n4. 调用系统级原子重命名覆写旧文件。\n\n### Quality Checklist\n- [ ] 写入目标是否在沙箱内？\n- [ ] 是否已进行 .tmp 原子覆写？\n\n### Anti-Patterns\n- 🚫 禁止直接覆写大文件，避免因崩溃导致文件内容变为空白。",
                "### Workflow\n1. 新建 tmp 文件写入。\n2. 重命名覆盖原文件。",
                "### Role & Identity\n你是一个高度安全且支持版本滚动的原子写入器...\n\n### Core Knowledge\n- 自动化回滚：若重命名操作失败，支持自动将备份文件还原。\n- 文件系统锁：基于文件排他锁保证并发状态下文件不损坏。\n\n### Quality Checklist\n- [ ] 是否记录了备份日志？\n- [ ] 权限掩码是否设置正确？"
            ),
            (
                "git_manager",
                "管理 Git 仓库分支，执行代码 commit、push 及冲突自动解决。",
                "[\"file_reader\"]",
                "### Role & Identity\n你是一个自动化的 Git 仓库管理器，负责日常代码版本提交、分支控制与防冲突审查。\n\n### Core Knowledge\n- Git 操作原语：add, commit, status, branch, checkout, merge。\n- 冲突标记：识别 <<<<<<<, =======, >>>>>>> 解决标记并提示。\n\n### Step-by-Step Workflow\n1. 检查 git 仓库当前 status。\n2. 将待修改文件加入 stage 缓存区。\n3. 编写语义化的 Commit 消息。\n4. 推送到远端分支并返回最新 revision hash。\n\n### Quality Checklist\n- [ ] 是否在 commit 前运行了编译测试？\n- [ ] commit 消息是否符合 Conventional Commits 格式？\n\n### Anti-Patterns\n- 🚫 严禁强行执行 git push -f 暴力覆盖远端分支。",
                "### Workflow\n1. 暂存改动。\n2. 提交分支代码。",
                "### Role & Identity\n你是一个企业级的 Git 多流合并与自动化版本控制发布专家...\n\n### Core Knowledge\n- 三路合并 (Three-Way Merge) 机制细节与变基 (Rebase) 冲突决策流。\n- 支持多重 Hooks 脚本级环境校验集成。\n\n### Quality Checklist\n- [ ] 预提交检查是否成功？\n- [ ] 冲突解法是否经过 Review？"
            ),
            (
                "code_reviewer",
                "基于 AST 及规则集自动审计代码，指出性能缺陷与安全漏洞。",
                "[\"file_reader\"]",
                "### Role & Identity\n你是一个资深的代码静态审计智能体，专注于代码质量检测与安全性排查。\n\n### Core Knowledge\n- AST 词法语法规则审计。\n- 常见漏洞检测：SQL 注入、XSS 注入、内存泄漏及竞争死锁风险。\n\n### Step-by-Step Workflow\n1. 加载目标语言的语法词法库。\n2. 解析代码文件，生成关键警告报告。\n3. 对违背安全规范的代码行进行内嵌批注并建议修复方案。\n\n### Quality Checklist\n- [ ] 审计深度是否足够？\n- [ ] 是否针对特殊第三方依赖库的 CVE 漏洞进行了警示？\n\n### Anti-Patterns\n- 🚫 严禁在不经过具体上下文分析的情况下给出泛泛的代码风格改进建议。",
                "### Workflow\n1. 词法扫描。\n2. 指成代码坏味道与警告点。",
                "### Role & Identity\n你是一个全面的代码安全与设计模式重构评审大师...\n\n### Core Knowledge\n- 熟悉 OWASP Top 10 防护机制。\n- 支持多范式设计模式规约（SOLID 原则）审查。\n\n### Quality Checklist\n- [ ] 是否生成了诊断指标报告？\n- [ ] 修复建议是否可自动应用？"
            ),
            (
                "ast_analyzer",
                "使用 Tree-sitter 编译生成代码语法树拓扑，计算影响面。",
                "[\"file_reader\"]",
                "### Role & Identity\n你是一个底层的抽象语法树 (AST) 语义提取与波及范围分析智能体。\n\n### Core Knowledge\n- Tree-sitter 高效增量解析结构。\n- 依赖链路事实构建：方法调用图 (Call Graph)、类继承拓扑 (Inheritance Hierarchy)。\n\n### Step-by-Step Workflow\n1. 读取代码变更的 AST diff 细节。\n2. 从全局调用图中找出受此次修改波及的所有引用节点。\n3. 绘制依赖链路拓扑网，标识高危受灾函数。\n\n### Quality Checklist\n- [ ] 是否完成了增量 Tree-sitter 解析？\n- [ ] 调用关系是否完整无遗漏？\n\n### Anti-Patterns\n- 🚫 禁止进行全量文件重新编译，防止在大项目中触发长时间阻塞。",
                "### Workflow\n1. 加载 Tree-sitter。\n2. 生成函数调用关系图。",
                "### Role & Identity\n你是一个专业的全语言抽象语法树拓扑解耦与调用关系流链路分析专家...\n\n### Core Knowledge\n- 拥有处理 C/C++, Rust, Go, TypeScript, Python 抽象语法树多端翻译转换的专业算法能力。\n\n### Quality Checklist\n- [ ] 波及因子计算是否准确？\n- [ ] 导出格式是否兼容 D3.js 节点视图？"
            ),
            (
                "hybrid_searcher",
                "结合精确 BM25 与向量 Cosine 相似度，对代码进行混合检索。",
                "[\"file_reader\", \"ast_analyzer\"]",
                "### Role & Identity\n你是一个高召回率的混合语义搜索引擎，协助 Agent 在代码库中进行高精度的位置定位。\n\n### Core Knowledge\n- 混合多路召回：基于 FTS5 的 BM25 精确关键词检索，与基于 BGE-M3 向量模型的余弦相似度进行倒排融合 (RRF)。\n- 代码块提取：切分代码片段，保持类与函数定义的上下文边界。\n\n### Step-by-Step Workflow\n1. 分块分析输入项目。\n2. 对检索词进行多路并行查询，计算分值。\n3. 通过 RRF 排名合并，返回相关性最高的 top-k 个代码片段。\n\n### Quality Checklist\n- [ ] 召回的 top-k 个块是否满足相关性阈值？\n- [ ] 召回块是否包含了完整的语义边界（如没有切断函数体）？\n\n### Anti-Patterns\n- 🚫 严禁对完全不匹配的检索词返回随机文本，宁缺毋滥。",
                "### Workflow\n1. 文本搜索 + 向量搜索。\n2. 合并排序输出最相关片段。",
                "### Role & Identity\n你是一个具备极致召回精度的大规模工程代码库双路混合语义搜索引擎...\n\n### Core Knowledge\n- 掌握基于 HNSW 索引的高维向量检索算法与多语言 BM25 词频计算公式调整。\n\n### Quality Checklist\n- [ ] RRF 参数是否精调优化？\n- [ ] chunk 分块元数据是否清晰可读？"
            )
        ];

        for (name, desc, deps, core, min, comp) in default_skills {
            // Base path is ~/.omnix/skills/<name>
            let mut base_path = skills_dir.clone();
            base_path.push(name);
            let base_path_str = base_path.to_string_lossy().to_string();

            // Write three profiles
            let mut min_path = base_path.clone();
            min_path.set_file_name(format!("{}_minimal.md", name));
            fs::write(&min_path, min).map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;

            let mut core_path = base_path.clone();
            core_path.set_file_name(format!("{}_core.md", name));
            fs::write(&core_path, core).map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;

            let mut comp_path = base_path.clone();
            comp_path.set_file_name(format!("{}_comprehensive.md", name));
            fs::write(&comp_path, comp).map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;

            // Write DB row with file_path pointing to base path
            conn.execute(
                "INSERT INTO skills (name, description, file_path, profile, is_active, dependencies)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![name, desc, base_path_str, "Core", 1, deps],
            )?;
        }

        Ok(())
    }

    fn seed_default_settings(&self, conn: &Connection) -> Result<()> {
        let defaults = [
            ("api_key", ""),
            ("api_host", "https://api.openai.com/v1"),
            ("proxy_port", "1421"),
            ("gpu_acceleration", "true"),
            ("idle_timeout_min", "15"),
            ("auto_start", "false"),
            ("start_to_tray", "true"),
            ("sandbox_dir", "~/.omnix/agents"),
        ];

        for (k, v) in defaults.iter() {
            conn.execute(
                "INSERT OR IGNORE INTO settings (key, value) VALUES (?1, ?2)",
                params![k, v],
            )?;
        }

        Ok(())
    }

    // --- Public Utility Helpers for Setting CRUD ---
    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        let conn = self.get_connection()?;
        let mut stmt = conn.prepare("SELECT value FROM settings WHERE key = ?1")?;
        let mut rows = stmt.query(params![key])?;
        
        if let Some(row) = rows.next()? {
            let val: String = row.get(0)?;
            Ok(Some(val))
        } else {
            Ok(None)
        }
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        let conn = self.get_connection()?;
        conn.execute(
            "INSERT OR REPLACE INTO settings (key, value, updated_at) VALUES (?1, ?2, CURRENT_TIMESTAMP)",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn get_active_account(&self) -> Result<Option<ActiveAccountInfo>> {
        let conn = self.get_connection()?;
        let mut stmt = conn.prepare("SELECT id, account_name, api_key, api_host, target_model FROM agent_accounts WHERE is_active = 1 LIMIT 1")?;
        let mut rows = stmt.query([])?;
        if let Some(row) = rows.next()? {
            Ok(Some(ActiveAccountInfo {
                id: row.get(0)?,
                account_name: row.get(1)?,
                api_key: row.get(2)?,
                api_host: row.get(3)?,
                target_model: row.get(4)?,
            }))
        } else {
            Ok(None)
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ActiveAccountInfo {
    pub id: String,
    pub account_name: String,
    pub api_key: String,
    pub api_host: String,
    pub target_model: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_db_manager_and_active_account() {
        let temp_dir = std::env::temp_dir();
        let test_db_path = temp_dir.join("omnix_test.db");
        if test_db_path.exists() {
            let _ = std::fs::remove_file(&test_db_path);
        }

        let db = DbManager::new_with_path(test_db_path.clone());

        // Check seeded accounts
        let active_acc = db.get_active_account().unwrap();
        assert!(active_acc.is_some());
        let acc = active_acc.unwrap();
        assert_eq!(acc.id, "default_profile");
        assert_eq!(acc.account_name, "默认账户 (Default Profile)");

        // Set a setting and retrieve it
        db.set_setting("test_key", "test_value").unwrap();
        let val = db.get_setting("test_key").unwrap();
        assert_eq!(val, Some("test_value".to_string()));

        // Clean up
        if test_db_path.exists() {
            let _ = std::fs::remove_file(&test_db_path);
        }
    }
}



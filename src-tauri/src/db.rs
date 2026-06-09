use rusqlite::{params, Connection, Result};
use std::fs;
use std::path::PathBuf;

pub struct DbManager {
    db_path: PathBuf,
}

impl DbManager {
    pub fn new() -> Self {
        // Resolve home directory path on Windows / Linux
        let home_dir = dirs::home_dir().expect("Failed to determine home directory. Cannot initialize database.");
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

    #[allow(dead_code)]
    pub fn new_with_path(db_path: PathBuf) -> Self {
        let db = Self { db_path };
        db.init_schema().expect("Failed to initialize database schema");
        db
    }

    pub fn get_connection(&self) -> Result<Connection> {
        let conn = Connection::open(&self.db_path)?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        Ok(conn)
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
                updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                source_type TEXT NOT NULL DEFAULT 'local', -- 'local' | 'git' | 'builtin'
                source_ref TEXT NULL,                      -- Git URL or local import path
                source_revision TEXT NULL,                 -- Git commit hash
                central_path TEXT NOT NULL DEFAULT '',     -- Central storage path (~/.omnix/skills/<name>)
                content_hash TEXT NULL,                    -- SHA256 of SKILL.md content
                starred INTEGER NOT NULL DEFAULT 0,        -- Favorite flag
                category TEXT NULL                         -- Skill category tag
            )",
            [],
        )?;

        // 5b. Skill Targets Table (sync tracking per tool)
        conn.execute(
            "CREATE TABLE IF NOT EXISTS skill_targets (
                id TEXT PRIMARY KEY,
                skill_id TEXT NOT NULL,
                tool TEXT NOT NULL,                    -- 'claude_code' | 'cursor' | 'copilot' | etc.
                target_path TEXT NOT NULL,             -- Actual synced path on disk
                mode TEXT NOT NULL DEFAULT 'copy',     -- 'copy' | 'symlink'
                status TEXT NOT NULL DEFAULT 'pending',-- 'synced' | 'error' | 'pending'
                last_error TEXT NULL,
                synced_at INTEGER NULL,
                FOREIGN KEY(skill_id) REFERENCES skills(name) ON DELETE CASCADE,
                UNIQUE(skill_id, tool)
            )",
            [],
        )?;

        // Migration: add new columns to existing skills table if they don't exist
        let migrations = [
            "ALTER TABLE skills ADD COLUMN source_type TEXT NOT NULL DEFAULT 'local'",
            "ALTER TABLE skills ADD COLUMN source_ref TEXT NULL",
            "ALTER TABLE skills ADD COLUMN source_revision TEXT NULL",
            "ALTER TABLE skills ADD COLUMN central_path TEXT NOT NULL DEFAULT ''",
            "ALTER TABLE skills ADD COLUMN content_hash TEXT NULL",
            "ALTER TABLE skills ADD COLUMN starred INTEGER NOT NULL DEFAULT 0",
            "ALTER TABLE skills ADD COLUMN category TEXT NULL",
            // model_platforms weighted routing fields (New API/Sub2API inspired)
            "ALTER TABLE model_platforms ADD COLUMN weight INTEGER NOT NULL DEFAULT 1",
            "ALTER TABLE model_platforms ADD COLUMN priority INTEGER NOT NULL DEFAULT 0",
            "ALTER TABLE model_platforms ADD COLUMN max_retries INTEGER NOT NULL DEFAULT 2",
            "ALTER TABLE model_platforms ADD COLUMN is_healthy INTEGER NOT NULL DEFAULT 1",
            "ALTER TABLE model_platforms ADD COLUMN consecutive_failures INTEGER NOT NULL DEFAULT 0",
            "ALTER TABLE model_platforms ADD COLUMN last_error TEXT NULL",
            "ALTER TABLE model_platforms ADD COLUMN last_used_at DATETIME NULL",
        ];
        for sql in &migrations {
            // ALTER TABLE ADD COLUMN silently fails if column already exists in SQLite,
            // but we catch and ignore the error
            let _ = conn.execute(sql, []);
        }

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

        // 6a. Request Logs (API proxy usage tracking — New API/Sub2API inspired)
        conn.execute(
            "CREATE TABLE IF NOT EXISTS request_logs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp DATETIME DEFAULT CURRENT_TIMESTAMP,
                model TEXT NOT NULL,
                platform TEXT NULL,
                prompt_tokens INTEGER NOT NULL DEFAULT 0,
                completion_tokens INTEGER NOT NULL DEFAULT 0,
                total_tokens INTEGER NOT NULL DEFAULT 0,
                latency_ms INTEGER NOT NULL DEFAULT 0,
                status_code INTEGER NOT NULL DEFAULT 200,
                is_stream INTEGER NOT NULL DEFAULT 0,
                is_error INTEGER NOT NULL DEFAULT 0,
                error_message TEXT NULL,
                request_id TEXT NULL,
                source TEXT NOT NULL DEFAULT 'proxy'
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
                agent_name TEXT DEFAULT '',
                updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        )?;

        // Check if agent_name column exists in agent_accounts, if not add it
        let has_agent_name = {
            let mut stmt = conn.prepare("PRAGMA table_info(agent_accounts)")?;
            let mut rows = stmt.query([])?;
            let mut found = false;
            while let Some(row) = rows.next()? {
                let name: String = row.get(1)?;
                if name == "agent_name" {
                    found = true;
                    break;
                }
            }
            found
        };
        if !has_agent_name {
            let _ = conn.execute("ALTER TABLE agent_accounts ADD COLUMN agent_name TEXT DEFAULT ''", []);
        }

        // 6c. Custom Models Table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS custom_models (
                name TEXT PRIMARY KEY,
                source TEXT NOT NULL DEFAULT 'API',
                has_vision INTEGER NOT NULL DEFAULT 0,
                has_audio INTEGER NOT NULL DEFAULT 0,
                has_reasoning INTEGER NOT NULL DEFAULT 0,
                has_coding INTEGER NOT NULL DEFAULT 0,
                has_long_context INTEGER NOT NULL DEFAULT 0,
                has_tool_use INTEGER NOT NULL DEFAULT 0,
                has_embedding INTEGER NOT NULL DEFAULT 0,
                has_speedy INTEGER NOT NULL DEFAULT 0,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        )?;

        // 6d. Model Platforms Table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS model_platforms (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                api_type TEXT NOT NULL, -- 'openai', 'anthropic', 'ollama'
                api_key TEXT NOT NULL,
                api_address TEXT NOT NULL,
                is_enabled INTEGER NOT NULL DEFAULT 1,
                weight INTEGER NOT NULL DEFAULT 1,       -- 加权路由权重 (1-100)
                priority INTEGER NOT NULL DEFAULT 0,     -- 优先级 (越高越优先)
                max_retries INTEGER NOT NULL DEFAULT 2,  -- 最大重试次数
                is_healthy INTEGER NOT NULL DEFAULT 1,   -- 健康状态 (1=healthy, 0=unhealthy)
                consecutive_failures INTEGER NOT NULL DEFAULT 0,
                last_error TEXT NULL,
                last_used_at DATETIME NULL,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        )?;

        // 6e. Platform Models Table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS platform_models (
                id TEXT PRIMARY KEY, -- platform_id + \":\" + model_name
                platform_id TEXT NOT NULL,
                model_name TEXT NOT NULL,
                has_vision INTEGER NOT NULL DEFAULT 0,
                has_audio INTEGER NOT NULL DEFAULT 0,
                has_reasoning INTEGER NOT NULL DEFAULT 0,
                has_coding INTEGER NOT NULL DEFAULT 1,
                is_enabled INTEGER NOT NULL DEFAULT 1,
                status TEXT NOT NULL DEFAULT 'unknown',
                FOREIGN KEY(platform_id) REFERENCES model_platforms(id) ON DELETE CASCADE
            )",
            [],
        )?;

        // 6e-migration. Add extended capability columns to platform_models if missing
        {
            let cols: Vec<String> = conn
                .prepare("PRAGMA table_info(platform_models)")?
                .query_map([], |row| row.get::<_, String>(1))?
                .filter_map(|r| r.ok())
                .collect();

            if !cols.iter().any(|c| c == "has_long_context") {
                let _ = conn.execute("ALTER TABLE platform_models ADD COLUMN has_long_context INTEGER NOT NULL DEFAULT 0", []);
            }
            if !cols.iter().any(|c| c == "has_tool_use") {
                let _ = conn.execute("ALTER TABLE platform_models ADD COLUMN has_tool_use INTEGER NOT NULL DEFAULT 1", []);
            }
            if !cols.iter().any(|c| c == "has_embedding") {
                let _ = conn.execute("ALTER TABLE platform_models ADD COLUMN has_embedding INTEGER NOT NULL DEFAULT 0", []);
            }
            if !cols.iter().any(|c| c == "has_speedy") {
                let _ = conn.execute("ALTER TABLE platform_models ADD COLUMN has_speedy INTEGER NOT NULL DEFAULT 0", []);
            }
        }

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

        // 8. Cron Tasks Table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS cron_tasks (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                schedule TEXT NOT NULL,
                agent_name TEXT NOT NULL,
                args TEXT NOT NULL,
                workspace_dir TEXT NOT NULL,
                is_active INTEGER NOT NULL DEFAULT 1,
                last_run DATETIME,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        )?;

        // 9. Cron Runs Table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS cron_runs (
                id TEXT PRIMARY KEY,
                task_id TEXT NOT NULL,
                status TEXT NOT NULL,
                log_path TEXT NOT NULL,
                started_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                finished_at DATETIME
            )",
            [],
        )?;

        // 10. Knowledge Base Documents Table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS kb_documents (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                source_path TEXT NOT NULL,
                file_type TEXT NOT NULL DEFAULT 'text',
                file_hash TEXT NOT NULL DEFAULT '',
                chunk_count INTEGER NOT NULL DEFAULT 0,
                total_chars INTEGER NOT NULL DEFAULT 0,
                embedding_model TEXT NOT NULL DEFAULT '',
                embedding_status TEXT NOT NULL DEFAULT 'pending',
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        )?;

        // 11. Knowledge Base Chunks Table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS kb_chunks (
                id TEXT PRIMARY KEY,
                document_id TEXT NOT NULL,
                chunk_index INTEGER NOT NULL,
                content TEXT NOT NULL,
                char_start INTEGER NOT NULL DEFAULT 0,
                char_end INTEGER NOT NULL DEFAULT 0,
                metadata TEXT NOT NULL DEFAULT '{}',
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                FOREIGN KEY(document_id) REFERENCES kb_documents(id) ON DELETE CASCADE
            )",
            [],
        )?;

        // 12. Knowledge Base Embeddings Table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS kb_embeddings (
                chunk_id TEXT PRIMARY KEY,
                embedding BLOB NOT NULL,
                model TEXT NOT NULL,
                dimensions INTEGER NOT NULL,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                FOREIGN KEY(chunk_id) REFERENCES kb_chunks(id) ON DELETE CASCADE
            )",
            [],
        )?;

        // 14. Selection History Table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS selection_history (
                id TEXT PRIMARY KEY,
                captured_text TEXT NOT NULL,
                source TEXT NOT NULL DEFAULT '',
                window_title TEXT NOT NULL DEFAULT '',
                process_name TEXT NOT NULL DEFAULT '',
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        )?;

        // 15. Translation History Table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS translation_history (
                id TEXT PRIMARY KEY,
                source_text TEXT NOT NULL,
                target_text TEXT NOT NULL,
                source_lang TEXT NOT NULL DEFAULT '',
                target_lang TEXT NOT NULL DEFAULT '',
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        )?;

        // 16. MCP Servers Table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS mcp_servers (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                command TEXT NOT NULL DEFAULT '',
                args TEXT NOT NULL DEFAULT '[]',
                env TEXT NOT NULL DEFAULT '{}',
                url TEXT NOT NULL DEFAULT '',
                server_type TEXT NOT NULL DEFAULT 'stdio',
                is_enabled INTEGER NOT NULL DEFAULT 1,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        )?;

        // 17. Prompt Library Table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS prompt_library (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                content TEXT NOT NULL,
                category TEXT NOT NULL DEFAULT 'general',
                order_key INTEGER NOT NULL DEFAULT 0,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        )?;

        // 18. Search Providers Table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS search_providers (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                api_type TEXT NOT NULL,
                api_key TEXT NOT NULL DEFAULT '',
                api_address TEXT NOT NULL DEFAULT '',
                is_enabled INTEGER NOT NULL DEFAULT 1,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        )?;

        // 19. Search History Table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS search_history (
                id TEXT PRIMARY KEY,
                query TEXT NOT NULL,
                provider_id TEXT,
                result_count INTEGER NOT NULL DEFAULT 0,
                results_json TEXT NOT NULL DEFAULT '[]',
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        )?;

        // 20. Activity Log Table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS activity_log (
                id TEXT PRIMARY KEY,
                action TEXT NOT NULL,
                target TEXT NOT NULL DEFAULT '',
                details TEXT NOT NULL DEFAULT '',
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        )?;

        // 13. FTS5 Full-Text Index on chunks (external content mode)
        conn.execute(
            "CREATE VIRTUAL TABLE IF NOT EXISTS kb_chunks_fts USING fts5(
                chunk_id,
                content,
                content='kb_chunks',
                content_rowid=rowid,
                tokenize='porter unicode61'
            )",
            [],
        )?;

        // FTS5 sync triggers
        conn.execute(
            "CREATE TRIGGER IF NOT EXISTS kb_chunks_ai AFTER INSERT ON kb_chunks BEGIN
                INSERT INTO kb_chunks_fts(rowid, chunk_id, content) VALUES (new.rowid, new.id, new.content);
            END",
            [],
        )?;
        conn.execute(
            "CREATE TRIGGER IF NOT EXISTS kb_chunks_ad AFTER DELETE ON kb_chunks BEGIN
                INSERT INTO kb_chunks_fts(kb_chunks_fts, rowid, chunk_id, content) VALUES('delete', old.rowid, old.id, old.content);
            END",
            [],
        )?;
        conn.execute(
            "CREATE TRIGGER IF NOT EXISTS kb_chunks_au AFTER UPDATE ON kb_chunks BEGIN
                INSERT INTO kb_chunks_fts(kb_chunks_fts, rowid, chunk_id, content) VALUES('delete', old.rowid, old.id, old.content);
                INSERT INTO kb_chunks_fts(rowid, chunk_id, content) VALUES (new.rowid, new.id, new.content);
            END",
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

        // Seed default cron tasks if empty
        self.seed_default_cron_tasks(&conn)?;

        // Seed default platforms if empty
        self.seed_default_platforms(&conn)?;
        self.seed_default_models(&conn)?;

        // Seed default search providers if empty
        self.seed_default_search_providers(&conn)?;

        Ok(())
    }

    fn seed_default_conversations(&self, conn: &Connection) -> Result<()> {
        let mut stmt = conn.prepare("SELECT COUNT(*) FROM conversations")?;
        let count: i64 = stmt.query_row([], |r| r.get(0))?;
        if count > 0 {
            return Ok(());
        }

        conn.execute("BEGIN TRANSACTION", [])?;

        let res = (|| -> Result<()> {
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
        })();

        match res {
            Ok(_) => {
                conn.execute("COMMIT", [])?;
                Ok(())
            }
            Err(e) => {
                let _ = conn.execute("ROLLBACK", []);
                Err(e)
            }
        }
    }

    fn seed_default_accounts(&self, conn: &Connection) -> Result<()> {
        // Fetch current setting configurations to establish default profile
        let api_key = self.get_setting("api_key")?.unwrap_or_default();
        let api_host = self.get_setting("api_host")?.unwrap_or_else(|| "https://api.openai.com/v1".to_string());
        
        let agents_to_seed = vec![
            ("claude_code_default", "Claude Code 默认账户", "", "https://api.anthropic.com/v1", "claude-3-5-sonnet", "Claude Code"),
            ("gemini_cli_default", "Gemini CLI 默认账户", "", "https://generativelanguage.googleapis.com", "gemini-2.0-flash", "Gemini CLI"),
            ("codex_default", "Codex 默认账户", &api_key, &api_host, "gpt-4o", "Codex"),
            ("qwen_code_default", "Qwen Code 默认账户", "", "https://dashscope.aliyuncs.com/compatible-mode/v1", "qwen-plus", "Qwen Code"),
            ("github_copilot_cli_default", "GitHub Copilot CLI 默认账户", "", "https://api.github.com", "gpt-4o", "GitHub Copilot CLI"),
            ("google_antigravity_default", "Google Antigravity 默认账户", "", "https://api.openai.com/v1", "gpt-4o", "Google Antigravity"),
            ("opencode_default", "OpenCode 默认账户", "", "https://api.openai.com/v1", "gpt-4o", "OpenCode"),
        ];

        for (id, name, key, host, model, agent_name) in agents_to_seed {
            let exists: i64 = conn.query_row(
                "SELECT COUNT(*) FROM agent_accounts WHERE agent_name = ?1",
                params![agent_name],
                |r| r.get(0)
            )?;
            if exists == 0 {
                conn.execute(
                    "INSERT INTO agent_accounts (id, account_name, api_key, api_host, target_model, agent_name, is_active)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1)",
                    params![id, name, key, host, model, agent_name],
                )?;
            }
        }

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

        // Get or create the local ~/.omnix/skills directory
        let home_dir = match dirs::home_dir() {
            Some(h) => h,
            None => return Ok(()), // Skip seeding if no home dir
        };
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
            let mut check_stmt = conn.prepare("SELECT COUNT(*) FROM skills WHERE name = ?1")?;
            let exists_count: i64 = check_stmt.query_row(params![name], |r| r.get(0))?;
            if exists_count > 0 {
                continue;
            }
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
            ("quick_assistant_shortcut", "Alt+Space"),
            ("quick_assistant_model", "deepseek-chat"),
            ("quick_assistant_enabled", "true"),
            ("quick_assistant_use_kb", "true"),
            ("selection_assistant_shortcut", "Ctrl+Alt+C"),
            ("selection_assistant_capture_mode", "hybrid"),
            ("selection_assistant_show_on_capture", "true"),
            ("selection_assistant_preserve_clipboard", "false"),
            ("translate_preferred_lang", "zh-cn"),
            ("translate_alter_lang", "en-us"),
            ("translate_model", ""),
            ("translate_auto_detect", "true"),
            ("translate_prompt", ""),
            ("web_search_provider", "tavily"),
            ("web_search_api_key", ""),
            ("web_search_enabled", "false"),
            ("theme_mode", "dark"),
        ];

        for (k, v) in defaults.iter() {
            conn.execute(
                "INSERT OR IGNORE INTO settings (key, value) VALUES (?1, ?2)",
                params![k, v],
            )?;
        }

        // Generate a cryptographically secure random remote_token if not set
        let mut stmt = conn.prepare("SELECT value FROM settings WHERE key = 'remote_token'")?;
        let exists = stmt.exists([]).unwrap_or(false);
        if !exists {
            let token = {
                use std::io::Read;
                let mut rng_bytes = [0u8; 32];
                // Use OS CSPRNG via /dev/urandom or CryptGenRandom
                if let Ok(mut f) = std::fs::File::open("/dev/urandom").or_else(|_| std::fs::File::open("CON")) {
                    let _ = f.read_exact(&mut rng_bytes);
                } else {
                    // Fallback: mix time + thread + process id (still better than DefaultHasher alone)
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default();
                    let seed = now.as_nanos() as u128;
                    for (i, b) in rng_bytes.iter_mut().enumerate() {
                        *b = ((seed >> (i % 16 * 8)) & 0xFF) as u8;
                    }
                }
                format!("tok_{:064x}", u128::from_be_bytes(rng_bytes[0..16].try_into().unwrap_or([0u8; 16])))
            };
            let _ = conn.execute(
                "INSERT INTO settings (key, value) VALUES ('remote_token', ?1)",
                params![token],
            );
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

    pub fn get_active_account_for_agent(&self, agent_name: &str) -> Result<Option<ActiveAccountInfo>> {
        let conn = self.get_connection()?;
        let mut stmt = conn.prepare("SELECT id, account_name, api_key, api_host, target_model FROM agent_accounts WHERE agent_name = ?1 AND is_active = 1 LIMIT 1")?;
        let mut rows = stmt.query(params![agent_name])?;
        if let Some(row) = rows.next()? {
            Ok(Some(ActiveAccountInfo {
                id: row.get(0)?,
                account_name: row.get(1)?,
                api_key: row.get(2)?,
                api_host: row.get(3)?,
                target_model: row.get(4)?,
            }))
        } else {
            // Fallback to any active account
            self.get_active_account()
        }
    }

    pub fn get_account_by_id(&self, id: &str) -> Result<Option<ActiveAccountInfo>> {
        let conn = self.get_connection()?;
        let mut stmt = conn.prepare("SELECT id, account_name, api_key, api_host, target_model FROM agent_accounts WHERE id = ?1 LIMIT 1")?;
        let mut rows = stmt.query(params![id])?;
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

    fn seed_default_cron_tasks(&self, conn: &Connection) -> Result<()> {
        let mut stmt = conn.prepare("SELECT COUNT(*) FROM cron_tasks")?;
        let count: i64 = stmt.query_row([], |r| r.get(0))?;
        if count > 0 {
            return Ok(());
        }

        conn.execute(
            "INSERT INTO cron_tasks (id, title, schedule, agent_name, args, workspace_dir, is_active)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1)",
            params![
                "task_backup_git",
                "代码库每 15 分钟自动 Git 增量备份",
                "*/15 * * * *",
                "git_manager",
                "[\"status\"]",
                "d:/Agent/Project/OMNIX-Development Tools"
            ],
        )?;

        conn.execute(
            "INSERT INTO cron_tasks (id, title, schedule, agent_name, args, workspace_dir, is_active)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1)",
            params![
                "task_security_scan",
                "每日凌晨安全代码审计扫描",
                "daily at 02:00",
                "code_reviewer",
                "[]",
                "d:/Agent/Project/OMNIX-Development Tools"
            ],
        )?;

        Ok(())
    }

    fn seed_default_platforms(&self, conn: &Connection) -> Result<()> {
        let mut stmt = conn.prepare("SELECT COUNT(*) FROM model_platforms")?;
        let count: i64 = stmt.query_row([], |r| r.get(0))?;
        if count > 0 {
            return Ok(());
        }

        // Seed default platforms
        let default_platforms = vec![
            ("ollama", "Ollama", "ollama", "", "http://localhost:11434"),
            ("deepseek", "DeepSeek", "openai", "", "https://api.deepseek.com/v1"),
            ("siliconflow", "硅基流动 (SiliconFlow)", "openai", "", "https://api.siliconflow.cn/v1"),
            ("openai", "OpenAI", "openai", "", "https://api.openai.com/v1"),
            ("anthropic", "Anthropic", "anthropic", "", "https://api.anthropic.com"),
        ];

        for (id, name, api_type, api_key, api_address) in default_platforms {
            conn.execute(
                "INSERT INTO model_platforms (id, name, api_type, api_key, api_address, is_enabled)
                 VALUES (?1, ?2, ?3, ?4, ?5, 1)",
                params![id, name, api_type, api_key, api_address],
            )?;
        }

        // Migrate old settings if present
        let old_key: Option<String> = conn.query_row(
            "SELECT value FROM settings WHERE key = 'api_key'",
            [],
            |r| r.get(0)
        ).ok();
        let old_host: Option<String> = conn.query_row(
            "SELECT value FROM settings WHERE key = 'api_host'",
            [],
            |r| r.get(0)
        ).ok();

        if let (Some(k), Some(h)) = (old_key, old_host) {
            if !k.trim().is_empty() && !h.trim().is_empty() {
                let api_type = if h.contains("anthropic") { "anthropic" } else { "openai" };
                let _ = conn.execute(
                    "INSERT INTO model_platforms (id, name, api_type, api_key, api_address, is_enabled)
                     VALUES (?1, ?2, ?3, ?4, ?5, 1)",
                    params!["imported-default", "中转网关默认配置", api_type, k, h],
                );
            }
        }

        Ok(())
    }

    fn seed_default_models(&self, conn: &Connection) -> Result<()> {
        let mut stmt = conn.prepare("SELECT COUNT(*) FROM platform_models")?;
        let count: i64 = stmt.query_row([], |r| r.get(0))?;
        if count > 0 {
            return Ok(());
        }

        // Seed default models for each platform.
        // These provide "out of the box" model entries so that:
        // - QA / Translation / Chat can find a chat model
        // - Knowledge Base can find an embedding model
        // All are seeded as disabled; users enable them after adding API keys.
        // Ollama models are auto-enabled (local, no API key needed).
        let default_models: Vec<(&str, &str, &str, i32, i32, i32, i32, i32, i32, i32, i32)> = vec![
            // DeepSeek models
            ("deepseek:deepseek-chat",        "deepseek", "deepseek-chat",       1, 0, 1, 1, 0, 1, 0, 1),
            ("deepseek:deepseek-reasoner",     "deepseek", "deepseek-reasoner",   0, 0, 1, 0, 0, 1, 0, 0),
            // OpenAI models
            ("openai:gpt-4o",                  "openai",   "gpt-4o",              1, 1, 1, 1, 1, 1, 0, 0),
            ("openai:gpt-4o-mini",             "openai",   "gpt-4o-mini",         1, 0, 0, 1, 0, 1, 0, 1),
            ("openai:text-embedding-3-small",  "openai",   "text-embedding-3-small", 0, 0, 0, 0, 0, 0, 1, 1),
            ("openai:text-embedding-3-large",  "openai",   "text-embedding-3-large", 0, 0, 0, 0, 0, 0, 1, 0),
            // Anthropic models
            ("anthropic:claude-sonnet-4-6",    "anthropic", "claude-sonnet-4-6",  1, 0, 1, 1, 1, 1, 0, 0),
            ("anthropic:claude-haiku-4-5",     "anthropic", "claude-haiku-4-5-20251001", 0, 0, 0, 1, 0, 1, 0, 1),
            // SiliconFlow models (popular Chinese provider with free tier)
            ("siliconflow:deepseek-ai/DeepSeek-V3", "siliconflow", "deepseek-ai/DeepSeek-V3", 1, 0, 1, 1, 0, 1, 0, 1),
            ("siliconflow:BAAI/bge-m3",        "siliconflow", "BAAI/bge-m3",       0, 0, 0, 0, 0, 0, 1, 1),
            ("siliconflow:Qwen/Qwen3-Embedding-0.6B", "siliconflow", "Qwen/Qwen3-Embedding-0.6B", 0, 0, 0, 0, 0, 0, 1, 1),
            // Ollama models (local, free, no API key needed)
            ("ollama:qwen2.5:7b",              "ollama",   "qwen2.5:7b",          0, 0, 0, 1, 0, 1, 0, 1),
            ("ollama:nomic-embed-text",        "ollama",   "nomic-embed-text",    0, 0, 0, 0, 0, 0, 1, 1),
            ("ollama:bge-m3",                  "ollama",   "bge-m3",              0, 0, 0, 0, 0, 0, 1, 0),
        ];

        // columns: id, platform_id, model_name, has_vision, has_audio, has_reasoning,
        //          has_coding, has_long_context, has_tool_use, has_embedding, has_speedy
        for (id, platform_id, model_name, vision, audio, reasoning, coding, long_ctx, tool_use, embedding, speedy) in default_models.iter() {
            let _ = conn.execute(
                "INSERT OR IGNORE INTO platform_models
                 (id, platform_id, model_name, has_vision, has_audio, has_reasoning,
                  has_coding, has_long_context, has_tool_use, is_enabled, status, has_embedding, has_speedy)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 0, 'unknown', ?10, ?11)",
                params![id, platform_id, model_name, vision, audio, reasoning, coding, long_ctx, tool_use, embedding, speedy],
            );
        }

        // Auto-enable Ollama models (local, no API key needed)
        let _ = conn.execute(
            "UPDATE platform_models SET is_enabled = 1 WHERE platform_id = 'ollama'",
            [],
        );

        Ok(())
    }

    // ── Selection History Methods ───────────────────────

    pub fn add_selection_history(
        &self,
        text: &str,
        source: &str,
        window_title: &str,
        process_name: &str,
    ) -> Result<String> {
        let conn = self.get_connection()?;
        let id = format!("sel_{}", chrono::Utc::now().timestamp_millis());

        // Truncate text to 100KB to prevent DB bloat
        let truncated = if text.len() > 100_000 {
            format!("{}…", &text[..100_000])
        } else {
            text.to_string()
        };

        conn.execute(
            "INSERT INTO selection_history (id, captured_text, source, window_title, process_name)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, truncated, source, window_title, process_name],
        )?;
        Ok(id)
    }

    pub fn get_selection_history(&self, limit: u32) -> Result<Vec<crate::selection::SelectionHistoryEntry>> {
        let conn = self.get_connection()?;
        let mut stmt = conn.prepare(
            "SELECT id, captured_text, source, window_title, process_name, created_at
             FROM selection_history ORDER BY created_at DESC LIMIT ?1"
        )?;
        let entries = stmt.query_map(params![limit], |row| {
            Ok(crate::selection::SelectionHistoryEntry {
                id: row.get(0)?,
                captured_text: row.get(1)?,
                source: row.get(2)?,
                window_title: row.get(3)?,
                process_name: row.get(4)?,
                created_at: row.get(5)?,
            })
        })?.filter_map(|e| e.ok()).collect();
        Ok(entries)
    }

    pub fn delete_selection_history_item(&self, id: &str) -> Result<()> {
        let conn = self.get_connection()?;
        conn.execute("DELETE FROM selection_history WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn clear_selection_history(&self) -> Result<()> {
        let conn = self.get_connection()?;
        conn.execute("DELETE FROM selection_history", [])?;
        Ok(())
    }

    // ── Translation History Methods ──────────────────────

    pub fn add_translation_history(
        &self,
        source_text: &str,
        target_text: &str,
        source_lang: &str,
        target_lang: &str,
    ) -> Result<String> {
        let conn = self.get_connection()?;
        let id = format!("tr_{}", chrono::Utc::now().timestamp_millis());

        let truncated_source = if source_text.len() > 100_000 {
            format!("{}…", &source_text[..100_000])
        } else {
            source_text.to_string()
        };
        let truncated_target = if target_text.len() > 100_000 {
            format!("{}…", &target_text[..100_000])
        } else {
            target_text.to_string()
        };

        conn.execute(
            "INSERT INTO translation_history (id, source_text, target_text, source_lang, target_lang)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, truncated_source, truncated_target, source_lang, target_lang],
        )?;
        Ok(id)
    }

    pub fn get_translation_history(&self, limit: u32) -> Result<Vec<crate::selection::SelectionHistoryEntry>> {
        let conn = self.get_connection()?;
        let mut stmt = conn.prepare(
            "SELECT id, source_text, target_text, source_lang, target_lang, created_at
             FROM translation_history ORDER BY created_at DESC LIMIT ?1"
        )?;
        let entries = stmt.query_map(params![limit], |row| {
            Ok(crate::selection::SelectionHistoryEntry {
                id: row.get(0)?,
                captured_text: row.get(1)?,
                source: row.get(3)?,   // source_lang
                window_title: row.get(2)?,  // target_text (repurposed field)
                process_name: row.get(4)?,   // target_lang
                created_at: row.get(5)?,
            })
        })?.filter_map(|e| e.ok()).collect();
        Ok(entries)
    }

    pub fn delete_translation_history_item(&self, id: &str) -> Result<()> {
        let conn = self.get_connection()?;
        conn.execute("DELETE FROM translation_history WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn clear_translation_history(&self) -> Result<()> {
        let conn = self.get_connection()?;
        conn.execute("DELETE FROM translation_history", [])?;
        Ok(())
    }

    // ── Search Providers CRUD ────────────────────────────

    fn seed_default_search_providers(&self, conn: &Connection) -> Result<()> {
        let mut stmt = conn.prepare("SELECT COUNT(*) FROM search_providers")?;
        let count: i64 = stmt.query_row([], |r| r.get(0))?;
        if count > 0 {
            return Ok(());
        }
        let defaults = [
            ("sp_searxng", "SearXNG (自建)", "searxng", "", "http://localhost:8080"),
            ("sp_brave", "Brave Search", "brave", "", ""),
            ("sp_duckduckgo", "DuckDuckGo", "duckduckgo", "", ""),
        ];
        for (id, name, api_type, api_key, api_address) in &defaults {
            conn.execute(
                "INSERT INTO search_providers (id, name, api_type, api_key, api_address, is_enabled) VALUES (?1, ?2, ?3, ?4, ?5, 1)",
                params![id, name, api_type, api_key, api_address],
            )?;
        }
        Ok(())
    }

    pub fn get_search_providers(&self) -> Result<Vec<(String, String, String, String, String, bool)>> {
        let conn = self.get_connection()?;
        let mut stmt = conn.prepare(
            "SELECT id, name, api_type, api_key, api_address, is_enabled FROM search_providers ORDER BY created_at"
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, i32>(5)? != 0,
            ))
        })?;
        let mut result = Vec::new();
        for r in rows {
            if let Ok(item) = r { result.push(item); }
        }
        Ok(result)
    }

    pub fn save_search_provider(&self, id: &str, name: &str, api_type: &str, api_key: &str, api_address: &str, is_enabled: bool) -> Result<()> {
        let conn = self.get_connection()?;
        conn.execute(
            "INSERT INTO search_providers (id, name, api_type, api_key, api_address, is_enabled) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(id) DO UPDATE SET name=?2, api_type=?3, api_key=?4, api_address=?5, is_enabled=?6",
            params![id, name, api_type, api_key, api_address, is_enabled as i32],
        )?;
        Ok(())
    }

    pub fn delete_search_provider(&self, id: &str) -> Result<()> {
        let conn = self.get_connection()?;
        conn.execute("DELETE FROM search_providers WHERE id = ?1", params![id])?;
        Ok(())
    }

    // ── Search History CRUD ──────────────────────────────

    pub fn save_search_history(&self, id: &str, query: &str, provider_id: &str, result_count: i32, results_json: &str) -> Result<()> {
        let conn = self.get_connection()?;
        conn.execute(
            "INSERT INTO search_history (id, query, provider_id, result_count, results_json) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, query, provider_id, result_count, results_json],
        )?;
        Ok(())
    }

    pub fn get_search_history(&self, limit: i32) -> Result<Vec<(String, String, String, i32, String)>> {
        let conn = self.get_connection()?;
        let mut stmt = conn.prepare(
            "SELECT id, query, provider_id, result_count, created_at FROM search_history ORDER BY created_at DESC LIMIT ?1"
        )?;
        let rows = stmt.query_map(params![limit], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i32>(3)?,
                row.get::<_, String>(4)?,
            ))
        })?;
        let mut result = Vec::new();
        for r in rows {
            if let Ok(item) = r { result.push(item); }
        }
        Ok(result)
    }

    pub fn delete_search_history_item(&self, id: &str) -> Result<()> {
        let conn = self.get_connection()?;
        conn.execute("DELETE FROM search_history WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn clear_search_history(&self) -> Result<()> {
        let conn = self.get_connection()?;
        conn.execute("DELETE FROM search_history", [])?;
        Ok(())
    }

    // ── MCP Servers CRUD ─────────────────────────────────

    pub fn get_mcp_servers(&self) -> Result<Vec<(String, String, String, String, String, String, String, bool)>> {
        let conn = self.get_connection()?;
        let mut stmt = conn.prepare(
            "SELECT id, name, command, args, env, url, server_type, is_enabled FROM mcp_servers ORDER BY created_at"
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, i32>(7)? != 0,
            ))
        })?;
        let mut result = Vec::new();
        for r in rows {
            if let Ok(item) = r { result.push(item); }
        }
        Ok(result)
    }

    pub fn save_mcp_server(&self, id: &str, name: &str, command: &str, args: &str, env: &str, url: &str, server_type: &str, is_enabled: bool) -> Result<()> {
        let conn = self.get_connection()?;
        conn.execute(
            "INSERT INTO mcp_servers (id, name, command, args, env, url, server_type, is_enabled) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(id) DO UPDATE SET name=?2, command=?3, args=?4, env=?5, url=?6, server_type=?7, is_enabled=?8",
            params![id, name, command, args, env, url, server_type, is_enabled as i32],
        )?;
        Ok(())
    }

    pub fn delete_mcp_server(&self, id: &str) -> Result<()> {
        let conn = self.get_connection()?;
        conn.execute("DELETE FROM mcp_servers WHERE id = ?1", params![id])?;
        Ok(())
    }

    // ── Backup Helpers ───────────────────────────────────

    pub fn get_table_row_counts(&self) -> Result<Vec<(String, i64)>> {
        let conn = self.get_connection()?;
        let tables = [
            "settings", "agents", "conversations", "messages", "skills", "memories",
            "agent_accounts", "custom_models", "model_platforms", "platform_models",
            "tasks", "cron_tasks", "cron_runs", "kb_documents", "kb_chunks",
            "kb_embeddings", "selection_history", "translation_history",
            "mcp_servers", "prompt_library", "search_providers", "search_history",
            "activity_log",
        ];
        let mut result = Vec::new();
        for table in &tables {
            let sql = format!("SELECT COUNT(*) FROM {}", table);
            if let Ok(mut stmt) = conn.prepare(&sql) {
                if let Ok(count) = stmt.query_row([], |r| r.get::<_, i64>(0)) {
                    result.push((table.to_string(), count));
                }
            }
        }
        Ok(result)
    }

    pub fn export_table_as_json(&self, table_name: &str) -> Result<String> {
        // SQL injection prevention: whitelist valid table names
        const VALID_TABLES: &[&str] = &[
            "settings", "agents", "conversations", "messages", "skills", "memories",
            "agent_accounts", "custom_models", "model_platforms", "platform_models",
            "cron_tasks", "cron_runs", "knowledge_documents", "knowledge_chunks",
            "selection_history", "translation_history", "mcp_servers", "search_providers",
            "prompt_library", "activity_log", "skill_targets", "agent_configs",
            "autopilot_configs", "request_logs",
        ];
        if !VALID_TABLES.contains(&table_name) {
            return Err(rusqlite::Error::InvalidParameterName(format!("Invalid table name: {}", table_name)));
        }
        let conn = self.get_connection()?;
        let sql = format!("SELECT * FROM \"{}\"", table_name);
        let mut stmt = conn.prepare(&sql)?;
        let col_count = stmt.column_count();
        let col_names: Vec<String> = (0..col_count)
            .map(|i| stmt.column_name(i).unwrap_or("").to_string())
            .collect();

        let mut rows_json = Vec::new();
        let rows = stmt.query_map([], |row| {
            let mut map = serde_json::Map::new();
            for (i, col) in col_names.iter().enumerate() {
                // Try reading as string first, then as i64, then fall back to null
                let val = if let Ok(s) = row.get::<_, String>(i) {
                    // Try to parse as number or bool, otherwise keep as string
                    if let Ok(n) = s.parse::<i64>() {
                        serde_json::Value::Number(n.into())
                    } else if let Ok(f) = s.parse::<f64>() {
                        serde_json::Number::from_f64(f)
                            .map(serde_json::Value::Number)
                            .unwrap_or(serde_json::Value::String(s))
                    } else if s == "true" || s == "false" {
                        serde_json::Value::Bool(s == "true")
                    } else {
                        serde_json::Value::String(s)
                    }
                } else if let Ok(n) = row.get::<_, i64>(i) {
                    serde_json::Value::Number(n.into())
                } else if let Ok(f) = row.get::<_, f64>(i) {
                    serde_json::Number::from_f64(f)
                        .map(serde_json::Value::Number)
                        .unwrap_or(serde_json::Value::Null)
                } else {
                    serde_json::Value::Null
                };
                map.insert(col.clone(), val);
            }
            Ok(serde_json::Value::Object(map))
        })?;
        for r in rows {
            if let Ok(v) = r { rows_json.push(v); }
        }
        serde_json::to_string(&rows_json).map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))
    }

    pub fn import_table_from_json(&self, table_name: &str, rows_json: &str) -> Result<usize> {
        // SQL injection prevention: whitelist valid table names
        const VALID_TABLES: &[&str] = &[
            "settings", "agents", "conversations", "messages", "skills", "memories",
            "agent_accounts", "custom_models", "model_platforms", "platform_models",
            "cron_tasks", "cron_runs", "knowledge_documents", "knowledge_chunks",
            "selection_history", "translation_history", "mcp_servers", "search_providers",
            "prompt_library", "activity_log", "skill_targets", "agent_configs",
            "autopilot_configs", "request_logs",
        ];
        if !VALID_TABLES.contains(&table_name) {
            return Err(rusqlite::Error::InvalidParameterName(format!("Invalid table name: {}", table_name)));
        }

        let conn = self.get_connection()?;
        let rows: Vec<serde_json::Map<String, serde_json::Value>> = serde_json::from_str(rows_json)
            .map_err(|e| rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e)))?;
        if rows.is_empty() { return Ok(0); }

        // Get column names from first row
        let cols: Vec<&str> = rows[0].keys().map(|s| s.as_str()).collect();
        let col_placeholders: Vec<&str> = cols.iter().map(|_| "?").collect();
        let sql = format!(
            "INSERT OR REPLACE INTO \"{}\" ({}) VALUES ({})",
            table_name,
            cols.join(", "),
            col_placeholders.join(", ")
        );

        let mut count = 0usize;
        for row_map in &rows {
            let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
            for col in &cols {
                if let Some(val) = row_map.get(*col) {
                    match val {
                        serde_json::Value::String(s) => params.push(Box::new(s.clone())),
                        serde_json::Value::Number(n) => {
                            if let Some(i) = n.as_i64() { params.push(Box::new(i)); }
                            else if let Some(f) = n.as_f64() { params.push(Box::new(f)); }
                            else { params.push(Box::new(n.to_string())); }
                        },
                        serde_json::Value::Bool(b) => params.push(Box::new(if *b { 1i32 } else { 0i32 })),
                        serde_json::Value::Null => params.push(Box::new(rusqlite::types::Null)),
                        other => params.push(Box::new(other.to_string())),
                    }
                } else {
                    params.push(Box::new(rusqlite::types::Null));
                }
            }
            let param_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();
            if conn.execute(&sql, param_refs.as_slice()).is_ok() {
                count += 1;
            }
        }
        Ok(count)
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
        assert_eq!(acc.id, "claude_code_default");
        assert_eq!(acc.account_name, "Claude Code 默认账户");

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



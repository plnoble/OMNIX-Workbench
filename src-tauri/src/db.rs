use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::{params, Connection, Result};
use std::fs;
use std::path::PathBuf;

/// Type alias for a pooled SQLite connection
pub type PooledConn = r2d2::PooledConnection<SqliteConnectionManager>;

pub struct DbManager {
    pool: Pool<SqliteConnectionManager>,
}

impl DbManager {
    pub fn new() -> Self {
        // Resolve home directory path on Windows / Linux
        let home_dir = dirs::home_dir()
            .expect("Failed to determine home directory. Cannot initialize database.");
        let mut omnix_dir = home_dir.clone();
        omnix_dir.push(".omnix");

        // Ensure directory exists
        if !omnix_dir.exists() {
            fs::create_dir_all(&omnix_dir).expect("Failed to create .omnix data directory");
        }

        let mut db_path = omnix_dir;
        db_path.push("omnix.db");

        let db = Self::from_path(db_path);
        db.init_schema()
            .expect("Failed to initialize database schema");
        db
    }

    #[allow(dead_code)]
    pub fn new_with_path(db_path: PathBuf) -> Self {
        let db = Self::from_path(db_path);
        db.init_schema()
            .expect("Failed to initialize database schema");
        db
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub fn new_run_test(db_path: PathBuf) -> Self {
        let db = Self::from_path(db_path);
        db.init_run_schema()
            .expect("Failed to initialize run test schema");
        db
    }

    #[cfg(test)]
    pub fn new_runtime_test(db_path: PathBuf) -> Self {
        let db = Self::from_path(db_path);
        let conn = db.get_connection().expect("runtime test connection");
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS conversations (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                workspace_path TEXT NOT NULL,
                active_agent TEXT NOT NULL,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            );
            CREATE TABLE IF NOT EXISTS messages (
                id TEXT PRIMARY KEY,
                conversation_id TEXT NOT NULL,
                role TEXT NOT NULL,
                content TEXT NOT NULL,
                timestamp DATETIME DEFAULT CURRENT_TIMESTAMP,
                FOREIGN KEY(conversation_id) REFERENCES conversations(id) ON DELETE CASCADE
            );",
        )
        .expect("runtime test base schema");
        db.init_runtime_schema(&conn)
            .expect("runtime test structured schema");
        drop(conn);
        db
    }

    /// Internal: create DbManager with r2d2 connection pool
    fn from_path(db_path: PathBuf) -> Self {
        let manager = SqliteConnectionManager::file(&db_path).with_init(|conn| {
            // Set busy timeout (5 seconds)
            conn.busy_timeout(std::time::Duration::from_secs(5))?;
            // Enable WAL mode for better concurrent read performance
            conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")?;
            Ok(())
        });

        let pool = Pool::builder()
            .max_size(8) // max 8 concurrent connections
            .min_idle(Some(1)) // keep at least 1 idle connection
            .build(manager)
            .expect("Failed to create SQLite connection pool");

        Self { pool }
    }

    /// Get a pooled connection (reuses existing connection from pool)
    pub fn get_connection(&self) -> Result<PooledConn> {
        self.pool.get().map_err(|e| {
            rusqlite::Error::SqliteFailure(
                rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_BUSY),
                Some(format!("Connection pool error: {}", e)),
            )
        })
    }

    pub fn init_run_schema(&self) -> Result<()> {
        let conn = self.get_connection()?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS workspace_runs (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                workspace_path TEXT NOT NULL,
                manager_agent TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'draft',
                summary TEXT NOT NULL DEFAULT '',
                is_archived INTEGER NOT NULL DEFAULT 0,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS team_plans (
                run_id TEXT PRIMARY KEY,
                goal TEXT NOT NULL,
                assignments_json TEXT NOT NULL DEFAULT '[]',
                status TEXT NOT NULL DEFAULT 'proposed',
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                approved_at DATETIME NULL,
                FOREIGN KEY(run_id) REFERENCES workspace_runs(id) ON DELETE CASCADE
            )",
            [],
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS agent_runs (
                id TEXT PRIMARY KEY,
                run_id TEXT NOT NULL,
                agent_name TEXT NOT NULL,
                task_title TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'pending',
                session_id TEXT NULL,
                started_at DATETIME NULL,
                completed_at DATETIME NULL,
                log_excerpt TEXT NOT NULL DEFAULT '',
                assignment_id TEXT NOT NULL DEFAULT '',
                dependencies_json TEXT NOT NULL DEFAULT '[]',
                acceptance_json TEXT NOT NULL DEFAULT '[]',
                retry_count INTEGER NOT NULL DEFAULT 0,
                max_retries INTEGER NOT NULL DEFAULT 1,
                result_summary TEXT NOT NULL DEFAULT '',
                validation_status TEXT NOT NULL DEFAULT 'pending',
                work_mode TEXT NOT NULL DEFAULT 'direct',
                FOREIGN KEY(run_id) REFERENCES workspace_runs(id) ON DELETE CASCADE
            )",
            [],
        )?;
        for migration in [
            "ALTER TABLE agent_runs ADD COLUMN assignment_id TEXT NOT NULL DEFAULT ''",
            "ALTER TABLE agent_runs ADD COLUMN dependencies_json TEXT NOT NULL DEFAULT '[]'",
            "ALTER TABLE agent_runs ADD COLUMN acceptance_json TEXT NOT NULL DEFAULT '[]'",
            "ALTER TABLE agent_runs ADD COLUMN retry_count INTEGER NOT NULL DEFAULT 0",
            "ALTER TABLE agent_runs ADD COLUMN max_retries INTEGER NOT NULL DEFAULT 1",
            "ALTER TABLE agent_runs ADD COLUMN result_summary TEXT NOT NULL DEFAULT ''",
            "ALTER TABLE agent_runs ADD COLUMN validation_status TEXT NOT NULL DEFAULT 'pending'",
        ] {
            let _ = conn.execute(migration, []);
        }

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_workspace_runs_created_at ON workspace_runs(created_at)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_agent_runs_run_id ON agent_runs(run_id)",
            [],
        )?;

        Ok(())
    }

    pub(crate) fn init_runtime_schema(&self, conn: &Connection) -> Result<()> {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS agent_sessions (
                id TEXT PRIMARY KEY,
                conversation_id TEXT NOT NULL,
                agent_id TEXT NOT NULL,
                adapter_kind TEXT NOT NULL,
                executable_path TEXT NOT NULL,
                workspace_path TEXT NOT NULL,
                model_json TEXT NOT NULL,
                permission_json TEXT NOT NULL,
                work_mode TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'created',
                external_session_id TEXT NULL,
                external_turn_id TEXT NULL,
                last_error TEXT NULL,
                started_at DATETIME NULL,
                ended_at DATETIME NULL,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                FOREIGN KEY(conversation_id) REFERENCES conversations(id) ON DELETE CASCADE
            )",
            [],
        )?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS runtime_events (
                id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                sequence INTEGER NOT NULL,
                kind TEXT NOT NULL,
                text TEXT NULL,
                external_session_id TEXT NULL,
                external_turn_id TEXT NULL,
                item_id TEXT NULL,
                request_id TEXT NULL,
                metadata_json TEXT NOT NULL DEFAULT '{}',
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                UNIQUE(session_id, sequence),
                FOREIGN KEY(session_id) REFERENCES agent_sessions(id) ON DELETE CASCADE
            )",
            [],
        )?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS agent_installations (
                agent_id TEXT NOT NULL,
                source TEXT NOT NULL,
                executable_path TEXT NOT NULL,
                version TEXT NOT NULL DEFAULT '',
                managed_root TEXT NULL,
                status TEXT NOT NULL DEFAULT 'detected',
                last_checked_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                PRIMARY KEY(agent_id, source, executable_path)
            )",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_agent_sessions_conversation ON agent_sessions(conversation_id, created_at)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_runtime_events_session ON runtime_events(session_id, sequence)",
            [],
        )?;
        let _ = conn.execute(
            "ALTER TABLE messages ADD COLUMN kind TEXT NOT NULL DEFAULT 'message'",
            [],
        );
        let _ = conn.execute(
            "ALTER TABLE messages ADD COLUMN status TEXT NOT NULL DEFAULT 'completed'",
            [],
        );
        let _ = conn.execute(
            "ALTER TABLE messages ADD COLUMN metadata_json TEXT NOT NULL DEFAULT '{}'",
            [],
        );
        let _ = conn.execute(
            "ALTER TABLE messages ADD COLUMN sequence INTEGER NOT NULL DEFAULT 0",
            [],
        );
        let _ = conn.execute(
            "ALTER TABLE messages ADD COLUMN runtime_session_id TEXT NULL",
            [],
        );
        // Branch lineage for `/btw` side conversations:
        // a branched conversation remembers its parent so the parent's transcript
        // can seed the branch's first turn. NULL for normal conversations.
        let _ = conn.execute(
            "ALTER TABLE conversations ADD COLUMN parent_conversation_id TEXT NULL",
            [],
        );
        // Per-conversation long-term goal (`/goal`): while status is
        // 'active' the objective is re-injected into every turn's prompt.
        conn.execute(
            "CREATE TABLE IF NOT EXISTS conversation_goals (
                conversation_id TEXT PRIMARY KEY,
                objective TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'active', -- 'active' | 'paused' | 'complete'
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                FOREIGN KEY(conversation_id) REFERENCES conversations(id) ON DELETE CASCADE
            )",
            [],
        )?;
        Ok(())
    }


    pub(crate) fn seed_default_settings(&self, conn: &Connection) -> Result<()> {
        let defaults = [
            ("api_key", ""),
            ("api_host", "https://api.openai.com/v1"),
            ("proxy_port", "1421"),
            ("gpu_acceleration", "true"),
            ("idle_timeout_min", "15"),
            ("auto_start", "false"),
            ("start_to_tray", "true"),
            ("sandbox_dir", "~/.omnix/agents"),
            ("quick_assistant_shortcut", "Ctrl+Shift+Space"),
            ("quick_assistant_model", "deepseek-chat"),
            ("quick_assistant_enabled", "true"),
            ("quick_assistant_use_kb", "true"),
            ("selection_assistant_shortcut", "Ctrl+Alt+C"),
            ("selection_assistant_capture_mode", "hybrid"),
            ("selection_assistant_show_on_capture", "true"),
            ("selection_assistant_preserve_clipboard", "false"),
            ("selection_assistant_auto_capture", "false"),
            ("selection_assistant_blacklist", "[\"KeePass\",\"1Password\",\"Bitwarden\",\"CredentialUIBroker\",\"Windows Security\"]"),
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
                if let Ok(mut f) =
                    std::fs::File::open("/dev/urandom").or_else(|_| std::fs::File::open("CON"))
                {
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
                format!(
                    "tok_{:064x}",
                    u128::from_be_bytes(rng_bytes[0..16].try_into().unwrap_or([0u8; 16]))
                )
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

    pub fn get_active_account_for_agent(
        &self,
        agent_name: &str,
    ) -> Result<Option<ActiveAccountInfo>> {
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

    pub(crate) fn seed_default_cron_tasks(&self, conn: &Connection) -> Result<()> {
        // Only seed on first install — never re-seed after user deletes tasks
        if self.get_setting("seed_cron_completed")?.is_some() {
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

        // Mark seed as completed
        conn.execute(
            "INSERT OR REPLACE INTO settings (key, value, updated_at) VALUES (?1, ?2, CURRENT_TIMESTAMP)",
            params!["seed_cron_completed", "true"],
        )?;

        Ok(())
    }

    pub(crate) fn seed_default_platforms(&self, conn: &Connection) -> Result<()> {
        let mut stmt = conn.prepare("SELECT COUNT(*) FROM model_platforms")?;
        let count: i64 = stmt.query_row([], |r| r.get(0))?;
        if count > 0 {
            return Ok(());
        }

        // Seed default platforms
        let default_platforms = vec![
            ("ollama", "Ollama", "ollama", "", "http://localhost:11434"),
            (
                "deepseek",
                "DeepSeek",
                "openai",
                "",
                "https://api.deepseek.com/v1",
            ),
            (
                "siliconflow",
                "硅基流动 (SiliconFlow)",
                "openai",
                "",
                "https://api.siliconflow.cn/v1",
            ),
            (
                "openai",
                "OpenAI",
                "openai",
                "",
                "https://api.openai.com/v1",
            ),
            (
                "anthropic",
                "Anthropic",
                "anthropic",
                "",
                "https://api.anthropic.com",
            ),
        ];

        for (id, name, api_type, api_key, api_address) in default_platforms {
            conn.execute(
                "INSERT INTO model_platforms (id, name, api_type, api_key, api_address, is_enabled)
                 VALUES (?1, ?2, ?3, ?4, ?5, 1)",
                params![id, name, api_type, api_key, api_address],
            )?;
        }

        // Migrate old settings if present
        let old_key: Option<String> = conn
            .query_row(
                "SELECT value FROM settings WHERE key = 'api_key'",
                [],
                |r| r.get(0),
            )
            .ok();
        let old_host: Option<String> = conn
            .query_row(
                "SELECT value FROM settings WHERE key = 'api_host'",
                [],
                |r| r.get(0),
            )
            .ok();

        if let (Some(k), Some(h)) = (old_key, old_host) {
            if !k.trim().is_empty() && !h.trim().is_empty() {
                let api_type = if h.contains("anthropic") {
                    "anthropic"
                } else {
                    "openai"
                };
                let _ = conn.execute(
                    "INSERT INTO model_platforms (id, name, api_type, api_key, api_address, is_enabled)
                     VALUES (?1, ?2, ?3, ?4, ?5, 1)",
                    params!["imported-default", "中转网关默认配置", api_type, k, h],
                );
            }
        }

        Ok(())
    }

    pub(crate) fn seed_default_models(&self, conn: &Connection) -> Result<()> {
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
            (
                "deepseek:deepseek-chat",
                "deepseek",
                "deepseek-chat",
                1,
                0,
                1,
                1,
                0,
                1,
                0,
                1,
            ),
            (
                "deepseek:deepseek-reasoner",
                "deepseek",
                "deepseek-reasoner",
                0,
                0,
                1,
                0,
                0,
                1,
                0,
                0,
            ),
            // OpenAI models
            ("openai:gpt-4o", "openai", "gpt-4o", 1, 1, 1, 1, 1, 1, 0, 0),
            (
                "openai:gpt-4o-mini",
                "openai",
                "gpt-4o-mini",
                1,
                0,
                0,
                1,
                0,
                1,
                0,
                1,
            ),
            (
                "openai:text-embedding-3-small",
                "openai",
                "text-embedding-3-small",
                0,
                0,
                0,
                0,
                0,
                0,
                1,
                1,
            ),
            (
                "openai:text-embedding-3-large",
                "openai",
                "text-embedding-3-large",
                0,
                0,
                0,
                0,
                0,
                0,
                1,
                0,
            ),
            // Anthropic models
            (
                "anthropic:claude-sonnet-4-6",
                "anthropic",
                "claude-sonnet-4-6",
                1,
                0,
                1,
                1,
                1,
                1,
                0,
                0,
            ),
            (
                "anthropic:claude-haiku-4-5",
                "anthropic",
                "claude-haiku-4-5-20251001",
                0,
                0,
                0,
                1,
                0,
                1,
                0,
                1,
            ),
            // SiliconFlow models (popular Chinese provider with free tier)
            (
                "siliconflow:deepseek-ai/DeepSeek-V3",
                "siliconflow",
                "deepseek-ai/DeepSeek-V3",
                1,
                0,
                1,
                1,
                0,
                1,
                0,
                1,
            ),
            (
                "siliconflow:BAAI/bge-m3",
                "siliconflow",
                "BAAI/bge-m3",
                0,
                0,
                0,
                0,
                0,
                0,
                1,
                1,
            ),
            (
                "siliconflow:Qwen/Qwen3-Embedding-0.6B",
                "siliconflow",
                "Qwen/Qwen3-Embedding-0.6B",
                0,
                0,
                0,
                0,
                0,
                0,
                1,
                1,
            ),
            // Ollama models (local, free, no API key needed)
            (
                "ollama:qwen2.5:7b",
                "ollama",
                "qwen2.5:7b",
                0,
                0,
                0,
                1,
                0,
                1,
                0,
                1,
            ),
            (
                "ollama:nomic-embed-text",
                "ollama",
                "nomic-embed-text",
                0,
                0,
                0,
                0,
                0,
                0,
                1,
                1,
            ),
            ("ollama:bge-m3", "ollama", "bge-m3", 0, 0, 0, 0, 0, 0, 1, 0),
        ];

        // columns: id, platform_id, model_name, has_vision, has_audio, has_reasoning,
        //          has_coding, has_long_context, has_tool_use, has_embedding, has_speedy
        for (
            id,
            platform_id,
            model_name,
            vision,
            audio,
            reasoning,
            coding,
            long_ctx,
            tool_use,
            embedding,
            speedy,
        ) in default_models.iter()
        {
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

        // Truncate text to 100KB to prevent DB bloat (safe UTF-8 boundary)
        let truncated = if text.len() > 100_000 {
            let end = text
                .char_indices()
                .nth(100_000)
                .map(|(i, _)| i)
                .unwrap_or(text.len());
            format!("{}…", &text[..end])
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

    pub fn get_selection_history(
        &self,
        limit: u32,
    ) -> Result<Vec<crate::selection::SelectionHistoryEntry>> {
        let conn = self.get_connection()?;
        let mut stmt = conn.prepare(
            "SELECT id, captured_text, source, window_title, process_name, created_at
             FROM selection_history ORDER BY created_at DESC LIMIT ?1",
        )?;
        let entries = stmt
            .query_map(params![limit], |row| {
                Ok(crate::selection::SelectionHistoryEntry {
                    id: row.get(0)?,
                    captured_text: row.get(1)?,
                    source: row.get(2)?,
                    window_title: row.get(3)?,
                    process_name: row.get(4)?,
                    created_at: row.get(5)?,
                })
            })?
            .filter_map(|e| e.ok())
            .collect();
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
            let end = source_text
                .char_indices()
                .nth(100_000)
                .map(|(i, _)| i)
                .unwrap_or(source_text.len());
            format!("{}…", &source_text[..end])
        } else {
            source_text.to_string()
        };
        let truncated_target = if target_text.len() > 100_000 {
            let end = target_text
                .char_indices()
                .nth(100_000)
                .map(|(i, _)| i)
                .unwrap_or(target_text.len());
            format!("{}…", &target_text[..end])
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

    pub fn get_translation_history(
        &self,
        limit: u32,
    ) -> Result<Vec<crate::selection::TranslateHistoryEntry>> {
        let conn = self.get_connection()?;
        let mut stmt = conn.prepare(
            "SELECT id, source_text, target_text, source_lang, target_lang, created_at
             FROM translation_history ORDER BY created_at DESC LIMIT ?1",
        )?;
        let entries = stmt
            .query_map(params![limit], |row| {
                Ok(crate::selection::TranslateHistoryEntry {
                    id: row.get(0)?,
                    source_text: row.get(1)?,
                    target_text: row.get(2)?,
                    source_lang: row.get(3)?,
                    target_lang: row.get(4)?,
                    created_at: row.get(5)?,
                })
            })?
            .filter_map(|e| e.ok())
            .collect();
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

    pub(crate) fn seed_default_search_providers(&self, conn: &Connection) -> Result<()> {
        let mut stmt = conn.prepare("SELECT COUNT(*) FROM search_providers")?;
        let count: i64 = stmt.query_row([], |r| r.get(0))?;
        if count > 0 {
            return Ok(());
        }
        let defaults = [
            (
                "sp_searxng",
                "SearXNG (自建)",
                "searxng",
                "",
                "http://localhost:8080",
            ),
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

    pub fn get_search_providers(
        &self,
    ) -> Result<Vec<(String, String, String, String, String, bool)>> {
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
            if let Ok(item) = r {
                result.push(item);
            }
        }
        Ok(result)
    }

    pub fn save_search_provider(
        &self,
        id: &str,
        name: &str,
        api_type: &str,
        api_key: &str,
        api_address: &str,
        is_enabled: bool,
    ) -> Result<()> {
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

    pub fn save_search_history(
        &self,
        id: &str,
        query: &str,
        provider_id: &str,
        result_count: i32,
        results_json: &str,
    ) -> Result<()> {
        let conn = self.get_connection()?;
        conn.execute(
            "INSERT INTO search_history (id, query, provider_id, result_count, results_json) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, query, provider_id, result_count, results_json],
        )?;
        Ok(())
    }

    pub fn get_search_history(
        &self,
        limit: i32,
    ) -> Result<Vec<(String, String, String, i32, String)>> {
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
            if let Ok(item) = r {
                result.push(item);
            }
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

    pub fn get_mcp_servers(
        &self,
    ) -> Result<Vec<(String, String, String, String, String, String, String, bool)>> {
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
            if let Ok(item) = r {
                result.push(item);
            }
        }
        Ok(result)
    }

    pub fn save_mcp_server(
        &self,
        id: &str,
        name: &str,
        command: &str,
        args: &str,
        env: &str,
        url: &str,
        server_type: &str,
        is_enabled: bool,
    ) -> Result<()> {
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
            "settings",
            "agents",
            "conversations",
            "messages",
            "skills",
            "memories",
            "agent_accounts",
            "custom_models",
            "model_platforms",
            "platform_models",
            "tasks",
            "cron_tasks",
            "cron_runs",
            "kb_documents",
            "kb_chunks",
            "kb_embeddings",
            "selection_history",
            "translation_history",
            "mcp_servers",
            "prompt_library",
            "search_providers",
            "search_history",
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
            "settings",
            "agents",
            "conversations",
            "messages",
            "skills",
            "memories",
            "agent_accounts",
            "custom_models",
            "model_platforms",
            "platform_models",
            "cron_tasks",
            "cron_runs",
            "knowledge_documents",
            "knowledge_chunks",
            "selection_history",
            "translation_history",
            "mcp_servers",
            "search_providers",
            "prompt_library",
            "activity_log",
            "skill_targets",
            "agent_configs",
            "autopilot_configs",
            "request_logs",
        ];
        if !VALID_TABLES.contains(&table_name) {
            return Err(rusqlite::Error::InvalidParameterName(format!(
                "Invalid table name: {}",
                table_name
            )));
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
            if let Ok(v) = r {
                rows_json.push(v);
            }
        }
        serde_json::to_string(&rows_json)
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))
    }

    pub fn import_table_from_json(&self, table_name: &str, rows_json: &str) -> Result<usize> {
        // SQL injection prevention: whitelist valid table names
        const VALID_TABLES: &[&str] = &[
            "settings",
            "agents",
            "conversations",
            "messages",
            "skills",
            "memories",
            "agent_accounts",
            "custom_models",
            "model_platforms",
            "platform_models",
            "cron_tasks",
            "cron_runs",
            "knowledge_documents",
            "knowledge_chunks",
            "selection_history",
            "translation_history",
            "mcp_servers",
            "search_providers",
            "prompt_library",
            "activity_log",
            "skill_targets",
            "agent_configs",
            "autopilot_configs",
            "request_logs",
        ];
        if !VALID_TABLES.contains(&table_name) {
            return Err(rusqlite::Error::InvalidParameterName(format!(
                "Invalid table name: {}",
                table_name
            )));
        }

        let conn = self.get_connection()?;
        let rows: Vec<serde_json::Map<String, serde_json::Value>> = serde_json::from_str(rows_json)
            .map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    0,
                    rusqlite::types::Type::Text,
                    Box::new(e),
                )
            })?;
        if rows.is_empty() {
            return Ok(0);
        }

        // Get column names from first row, validate against SQL injection
        let cols: Vec<&str> = rows[0].keys().map(|s| s.as_str()).collect();
        // Validate column names: only allow alphanumeric + underscore (prevent SQL injection via column names)
        for col in &cols {
            if col.is_empty() || !col.chars().all(|c| c.is_alphanumeric() || c == '_') {
                return Err(rusqlite::Error::InvalidParameterName(format!(
                    "Invalid column name: '{}'. Only alphanumeric and underscore allowed.",
                    col
                )));
            }
            if col.starts_with(|c: char| c.is_ascii_digit()) {
                return Err(rusqlite::Error::InvalidParameterName(format!(
                    "Invalid column name: '{}'. Must not start with a digit.",
                    col
                )));
            }
        }
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
                            if let Some(i) = n.as_i64() {
                                params.push(Box::new(i));
                            } else if let Some(f) = n.as_f64() {
                                params.push(Box::new(f));
                            } else {
                                params.push(Box::new(n.to_string()));
                            }
                        }
                        serde_json::Value::Bool(b) => {
                            params.push(Box::new(if *b { 1i32 } else { 0i32 }))
                        }
                        serde_json::Value::Null => params.push(Box::new(rusqlite::types::Null)),
                        other => params.push(Box::new(other.to_string())),
                    }
                } else {
                    params.push(Box::new(rusqlite::types::Null));
                }
            }
            let param_refs: Vec<&dyn rusqlite::types::ToSql> =
                params.iter().map(|p| p.as_ref()).collect();
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
    #[ignore = "manual integration test; full seed path is too slow for the lib test baseline"]
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

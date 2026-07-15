//! Database schema: table creation, migrations, and first-run seeding.
//! Split out of db.rs — this is a second `impl DbManager` block, so callers
//! and method signatures are unchanged.

use rusqlite::{params, Connection, Result};
use std::fs;

use crate::db::DbManager;

impl DbManager {
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
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                task_status TEXT NOT NULL DEFAULT 'pending',    -- 'pending' | 'running' | 'completed' | 'failed'
                task_started_at DATETIME NULL,
                task_completed_at DATETIME NULL,
                task_duration_ms INTEGER NULL,
                task_summary TEXT NULL,                          -- auto-generated completion summary
                task_files_changed INTEGER NOT NULL DEFAULT 0,
                task_exit_code INTEGER NULL,
                is_archived INTEGER NOT NULL DEFAULT 0
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
                category TEXT NULL,                        -- Skill category tag
                usage_count INTEGER NOT NULL DEFAULT 0,    -- Times used by agents (compound interest)
                last_used_at DATETIME NULL,                -- Last usage timestamp
                success_count INTEGER NOT NULL DEFAULT 0,  -- Successful usages
                priority_score REAL NOT NULL DEFAULT 1.0   -- Dynamic priority (increases with usage)
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

        // 6c. Agent-Platform Bindings
        // Maps each agent to a specific API platform for per-agent routing
        conn.execute(
            "CREATE TABLE IF NOT EXISTS agent_platform_bindings (
                agent_name TEXT PRIMARY KEY,
                platform_id TEXT NOT NULL,
                model_name TEXT NULL,          -- Optional: specific model override
                binding_kind TEXT NOT NULL DEFAULT 'omnix',
                builtin_model TEXT NULL,
                enabled INTEGER NOT NULL DEFAULT 1,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        )?;

        self.init_runtime_schema(&conn)?;

        // 6d. Project Protocol / Evolution Loop
        conn.execute(
            "CREATE TABLE IF NOT EXISTS project_protocol_runs (
                id TEXT PRIMARY KEY,
                workspace_path TEXT NOT NULL UNIQUE,
                project_name TEXT NOT NULL DEFAULT '',
                enabled INTEGER NOT NULL DEFAULT 1,
                initialized INTEGER NOT NULL DEFAULT 0,
                status TEXT NOT NULL DEFAULT 'active',
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                archived_at DATETIME NULL
            )",
            [],
        )?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS project_protocol_events (
                id TEXT PRIMARY KEY,
                workspace_path TEXT NOT NULL,
                event_type TEXT NOT NULL,
                summary TEXT NOT NULL DEFAULT '',
                details_json TEXT NOT NULL DEFAULT '{}',
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        )?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS distillation_runs (
                id TEXT PRIMARY KEY,
                workspace_path TEXT NOT NULL,
                source_summary TEXT NOT NULL DEFAULT '',
                memory_count INTEGER NOT NULL DEFAULT 0,
                proposal_count INTEGER NOT NULL DEFAULT 0,
                status TEXT NOT NULL DEFAULT 'completed',
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        )?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS evolution_proposals (
                id TEXT PRIMARY KEY,
                workspace_path TEXT NOT NULL,
                proposal_type TEXT NOT NULL,
                title TEXT NOT NULL,
                rationale TEXT NOT NULL DEFAULT '',
                diff_json TEXT NOT NULL DEFAULT '{}',
                status TEXT NOT NULL DEFAULT 'pending',
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                applied_at DATETIME NULL
            )",
            [],
        )?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS protocol_actions (
                id TEXT PRIMARY KEY,
                workspace_path TEXT NOT NULL,
                action_type TEXT NOT NULL,
                title TEXT NOT NULL,
                content TEXT NOT NULL DEFAULT '',
                diff_json TEXT NOT NULL DEFAULT '{}',
                status TEXT NOT NULL DEFAULT 'pending',
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                applied_at DATETIME NULL
            )",
            [],
        )?;

        // 6e. Skill Sets
        conn.execute(
            "CREATE TABLE IF NOT EXISTS skill_sets (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                description TEXT NOT NULL DEFAULT '',
                sync_targets TEXT NOT NULL DEFAULT '[]',
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        )?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS skill_set_items (
                id TEXT PRIMARY KEY,
                skill_set_id TEXT NOT NULL,
                skill_id TEXT NOT NULL,
                order_num INTEGER NOT NULL DEFAULT 0,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                FOREIGN KEY(skill_set_id) REFERENCES skill_sets(id) ON DELETE CASCADE,
                UNIQUE(skill_set_id, skill_id)
            )",
            [],
        )?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS assistant_template_favorites (
                slug TEXT PRIMARY KEY,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        )?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS skill_fusion_drafts (
                id TEXT PRIMARY KEY,
                source_skills_json TEXT NOT NULL,
                model_id TEXT NOT NULL,
                proposed_name TEXT NOT NULL,
                description TEXT NOT NULL DEFAULT '',
                fused_content TEXT NOT NULL,
                explanation TEXT NOT NULL DEFAULT '',
                conflicts_json TEXT NOT NULL DEFAULT '[]',
                status TEXT NOT NULL DEFAULT 'pending',
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                applied_at DATETIME NULL
            )",
            [],
        )?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS distillation_inbox (
                id TEXT PRIMARY KEY,
                conversation_id TEXT NOT NULL,
                workspace_path TEXT NOT NULL DEFAULT '',
                candidate_type TEXT NOT NULL,
                title TEXT NOT NULL,
                summary TEXT NOT NULL DEFAULT '',
                payload_json TEXT NOT NULL DEFAULT '{}',
                evidence_json TEXT NOT NULL DEFAULT '[]',
                model_id TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'pending',
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                reviewed_at DATETIME NULL
            )",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_distillation_inbox_status ON distillation_inbox(status, created_at)",
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
            // Skill compound interest fields
            "ALTER TABLE skills ADD COLUMN usage_count INTEGER NOT NULL DEFAULT 0",
            "ALTER TABLE skills ADD COLUMN last_used_at DATETIME NULL",
            "ALTER TABLE skills ADD COLUMN success_count INTEGER NOT NULL DEFAULT 0",
            "ALTER TABLE skills ADD COLUMN priority_score REAL NOT NULL DEFAULT 1.0",
            // Agent task lifecycle fields
            "ALTER TABLE conversations ADD COLUMN task_status TEXT NOT NULL DEFAULT 'pending'",
            "ALTER TABLE conversations ADD COLUMN task_started_at DATETIME NULL",
            "ALTER TABLE conversations ADD COLUMN task_completed_at DATETIME NULL",
            "ALTER TABLE conversations ADD COLUMN task_duration_ms INTEGER NULL",
            "ALTER TABLE conversations ADD COLUMN task_summary TEXT NULL",
            "ALTER TABLE conversations ADD COLUMN task_files_changed INTEGER NOT NULL DEFAULT 0",
            "ALTER TABLE conversations ADD COLUMN task_exit_code INTEGER NULL",
            "ALTER TABLE conversations ADD COLUMN is_archived INTEGER NOT NULL DEFAULT 0",
            // model_platforms weighted routing fields
            "ALTER TABLE model_platforms ADD COLUMN weight INTEGER NOT NULL DEFAULT 1",
            "ALTER TABLE model_platforms ADD COLUMN priority INTEGER NOT NULL DEFAULT 0",
            "ALTER TABLE model_platforms ADD COLUMN max_retries INTEGER NOT NULL DEFAULT 2",
            "ALTER TABLE model_platforms ADD COLUMN is_healthy INTEGER NOT NULL DEFAULT 1",
            "ALTER TABLE model_platforms ADD COLUMN consecutive_failures INTEGER NOT NULL DEFAULT 0",
            "ALTER TABLE model_platforms ADD COLUMN last_error TEXT NULL",
            "ALTER TABLE model_platforms ADD COLUMN last_used_at DATETIME NULL",
            // Agent model binding fields.
            "ALTER TABLE agent_platform_bindings ADD COLUMN binding_kind TEXT NOT NULL DEFAULT 'omnix'",
            "ALTER TABLE agent_platform_bindings ADD COLUMN builtin_model TEXT NULL",
            // Skill pool governance (#3 技能池重构): every skill lives in a pool —
            // 'pending' (待定池, default: collected/forged skills are NOT used until
            // approved) or 'official' (正式池, injected via the gateway for all
            // agents). Promotion to official REQUIRES a completed review.
            "ALTER TABLE skills ADD COLUMN pool TEXT NOT NULL DEFAULT 'pending'",
            "ALTER TABLE skills ADD COLUMN review_score INTEGER NULL",
            "ALTER TABLE skills ADD COLUMN review_verdict TEXT NULL",
            "ALTER TABLE skills ADD COLUMN review_summary TEXT NOT NULL DEFAULT ''",
            "ALTER TABLE skills ADD COLUMN reviewed_at DATETIME NULL",
            // R2 技能中心：中文摘要（看得懂）+ 完整审核意见（改得动）
            "ALTER TABLE skills ADD COLUMN summary_zh TEXT NOT NULL DEFAULT ''",
            "ALTER TABLE skills ADD COLUMN review_problems TEXT NOT NULL DEFAULT '[]'",
            "ALTER TABLE skills ADD COLUMN review_improve TEXT NOT NULL DEFAULT ''",
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
                type TEXT NOT NULL DEFAULT 'experience', -- 'preference' | 'experience'
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        )?;

        // 6a. Request Logs
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
            let _ = conn.execute(
                "ALTER TABLE agent_accounts ADD COLUMN agent_name TEXT DEFAULT ''",
                [],
            );
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
        // Circuit breaker: timestamp the platform's circuit tripped open, so the
        // proxy can allow a half-open probe once the cooldown elapses.
        // Idempotent — ignored if already present.
        let _ = conn.execute(
            "ALTER TABLE model_platforms ADD COLUMN circuit_opened_at DATETIME NULL",
            [],
        );

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
                let _ = conn.execute(
                    "ALTER TABLE platform_models ADD COLUMN has_speedy INTEGER NOT NULL DEFAULT 0",
                    [],
                );
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

        self.init_run_schema()?;

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

        // 9b. Autopilots: scheduled definitions that create a
        // reviewable agent conversation on fire (not a headless CLI run like cron).
        conn.execute(
            "CREATE TABLE IF NOT EXISTS autopilots (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                prompt TEXT NOT NULL,
                agent_name TEXT NOT NULL,
                workspace_path TEXT NOT NULL,
                schedule TEXT NOT NULL,          -- reuses cron match_schedule format
                permission TEXT NOT NULL DEFAULT 'ask_on_risk',
                work_mode TEXT NOT NULL DEFAULT 'direct',
                enabled INTEGER NOT NULL DEFAULT 1,
                last_run DATETIME,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        )?;
        // A fired autopilot enqueues a run; the frontend claims queued runs and
        // executes them through the real runtime (reviewable conversation).
        conn.execute(
            "CREATE TABLE IF NOT EXISTS autopilot_runs (
                id TEXT PRIMARY KEY,
                autopilot_id TEXT NOT NULL,
                conversation_id TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'queued',   -- queued | claimed | done | failed
                trigger_source TEXT NOT NULL DEFAULT 'schedule', -- schedule | manual
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        )?;

        // 10. Named Knowledge Bases
        conn.execute(
            "CREATE TABLE IF NOT EXISTS knowledge_bases (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL UNIQUE,
                description TEXT NOT NULL DEFAULT '',
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        )?;
        conn.execute(
            "INSERT OR IGNORE INTO knowledge_bases (id, name, description)
             VALUES ('default', '默认知识库', '由旧版文档池迁移而来')",
            [],
        )?;

        // 10b. Knowledge Base Documents Table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS kb_documents (
                id TEXT PRIMARY KEY,
                knowledge_base_id TEXT NOT NULL DEFAULT 'default',
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

        // 21. Development Checklist Table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS dev_checklist (
                id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                title TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'pending',
                priority INTEGER NOT NULL DEFAULT 0,
                source TEXT NOT NULL DEFAULT 'manual',
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                completed_at DATETIME NULL
            )",
            [],
        )?;

        // 22. Agent Mailbox Table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS agent_mailbox (
                id TEXT PRIMARY KEY,
                from_agent TEXT NOT NULL,
                to_agent TEXT NOT NULL,
                subject TEXT NOT NULL DEFAULT '',
                body TEXT NOT NULL DEFAULT '',
                is_read INTEGER NOT NULL DEFAULT 0,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        )?;

        // 23. Task Dependencies Table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS task_dependencies (
                task_id TEXT NOT NULL,
                blocks_id TEXT NOT NULL,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                PRIMARY KEY (task_id, blocks_id)
            )",
            [],
        )?;

        // 24. Event Triggers Table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS event_triggers (
                id TEXT PRIMARY KEY,
                event_type TEXT NOT NULL,
                threshold INTEGER NOT NULL DEFAULT 1,
                task_id TEXT NOT NULL,
                current_count INTEGER NOT NULL DEFAULT 0,
                enabled INTEGER NOT NULL DEFAULT 1,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        )?;

        // 25. Tool Confirmation Queue
        conn.execute(
            "CREATE TABLE IF NOT EXISTS tool_confirmations (
                id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                tool_name TEXT NOT NULL,
                tool_input TEXT NOT NULL DEFAULT '',
                status TEXT NOT NULL DEFAULT 'pending',
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                resolved_at DATETIME NULL
            )",
            [],
        )?;

        // Platform API Keys (multi-key per platform, encrypted storage)
        let _ = conn.execute(
            "CREATE TABLE IF NOT EXISTS platform_api_keys (
                id TEXT PRIMARY KEY,
                platform_id TEXT NOT NULL,
                encrypted_key TEXT NOT NULL,
                label TEXT DEFAULT '',
                is_active INTEGER DEFAULT 0,
                created_at TEXT DEFAULT (datetime('now'))
            )",
            [],
        );

        // ── Performance indexes ──────────────────────────────────────
        // These are added AFTER all table creation to avoid FK ordering issues.
        // Each uses IF NOT EXISTS so they're safe to run on every startup.
        let indexes = [
            // Most critical: messages by conversation (chat loads messages per-conversation)
            "CREATE INDEX IF NOT EXISTS idx_messages_conversation_id ON messages(conversation_id)",
            // Platform models by platform (settings page loads models per-platform)
            "CREATE INDEX IF NOT EXISTS idx_platform_models_platform_id ON platform_models(platform_id)",
            // Request logs by timestamp (dashboard analytics sort by time)
            "CREATE INDEX IF NOT EXISTS idx_request_logs_timestamp ON request_logs(timestamp)",
            // Cron runs by task (history view per-task)
            "CREATE INDEX IF NOT EXISTS idx_cron_runs_task_id ON cron_runs(task_id)",
            // Tasks by conversation (PlanTree loads tasks per-conversation)
            "CREATE INDEX IF NOT EXISTS idx_tasks_conversation_id ON tasks(conversation_id)",
            // Workspace runs by recency and agent runs by parent run
            "CREATE INDEX IF NOT EXISTS idx_workspace_runs_created_at ON workspace_runs(created_at)",
            "CREATE INDEX IF NOT EXISTS idx_agent_runs_run_id ON agent_runs(run_id)",
            // Agent accounts by agent (sidebar shows accounts per-agent)
            "CREATE INDEX IF NOT EXISTS idx_agent_accounts_agent_name ON agent_accounts(agent_name)",
            // Search history by timestamp (recent searches sort)
            "CREATE INDEX IF NOT EXISTS idx_search_history_timestamp ON search_history(timestamp)",
            // Platform API keys by platform
            "CREATE INDEX IF NOT EXISTS idx_platform_api_keys_platform_id ON platform_api_keys(platform_id)",
            // Activity log by timestamp (recent activity)
            "CREATE INDEX IF NOT EXISTS idx_activity_log_created_at ON activity_log(created_at)",
            // Project protocol by workspace and recency
            "CREATE INDEX IF NOT EXISTS idx_project_protocol_events_workspace ON project_protocol_events(workspace_path, created_at)",
            "CREATE INDEX IF NOT EXISTS idx_evolution_proposals_workspace ON evolution_proposals(workspace_path, status)",
            "CREATE INDEX IF NOT EXISTS idx_protocol_actions_workspace ON protocol_actions(workspace_path, status)",
            // Skill set items by set
            "CREATE INDEX IF NOT EXISTS idx_skill_set_items_set ON skill_set_items(skill_set_id, order_num)",
        ];
        for idx_sql in &indexes {
            let _ = conn.execute(idx_sql, []);
        }

        // Seed default settings if empty
        self.seed_default_settings(&conn)?;

        // Seed default skills if empty
        self.seed_default_skills(&conn)?;

        // Seed default accounts if empty
        self.seed_default_accounts(&conn)?;

        // Seed default memories if empty
        self.seed_default_memories(&conn)?;

        // Remove only the two historical demo conversations. Real user data is untouched.
        self.remove_known_mock_conversations(&conn)?;

        // Seed default cron tasks if empty
        self.seed_default_cron_tasks(&conn)?;

        // Seed default platforms if empty
        self.seed_default_platforms(&conn)?;
        self.seed_default_models(&conn)?;

        // Seed default search providers if empty
        self.seed_default_search_providers(&conn)?;

        // Migration: add type column to memories table (idempotent)
        let _ = conn.execute(
            "ALTER TABLE memories ADD COLUMN type TEXT NOT NULL DEFAULT 'experience'",
            [],
        );
        for migration in [
            "ALTER TABLE platform_api_keys ADD COLUMN priority INTEGER NOT NULL DEFAULT 0",
            "ALTER TABLE platform_api_keys ADD COLUMN is_enabled INTEGER NOT NULL DEFAULT 1",
            "ALTER TABLE platform_api_keys ADD COLUMN last_status TEXT NOT NULL DEFAULT 'unknown'",
            "ALTER TABLE platform_api_keys ADD COLUMN last_error TEXT NULL",
            "ALTER TABLE platform_api_keys ADD COLUMN latency_ms INTEGER NULL",
            "ALTER TABLE platform_api_keys ADD COLUMN last_checked_at TEXT NULL",
        ] {
            let _ = conn.execute(migration, []);
        }
        let _ = conn.execute(
            "ALTER TABLE memories ADD COLUMN source TEXT NOT NULL DEFAULT 'manual'",
            [],
        );
        let _ = conn.execute(
            "ALTER TABLE memories ADD COLUMN workspace_path TEXT NULL",
            [],
        );
        let _ = conn.execute(
            "ALTER TABLE memories ADD COLUMN evidence_json TEXT NOT NULL DEFAULT '{}'",
            [],
        );
        let _ = conn.execute(
            "ALTER TABLE memories ADD COLUMN status TEXT NOT NULL DEFAULT 'active'",
            [],
        );
        // Evolution: relevance-based injection + dedup + effectiveness tracking.
        for migration in [
            "ALTER TABLE memories ADD COLUMN embedding BLOB NULL",
            "ALTER TABLE memories ADD COLUMN dimensions INTEGER NOT NULL DEFAULT 0",
            "ALTER TABLE memories ADD COLUMN stack_tags TEXT NOT NULL DEFAULT ''",
            "ALTER TABLE memories ADD COLUMN confidence REAL NOT NULL DEFAULT 1",
            "ALTER TABLE memories ADD COLUMN seen_count INTEGER NOT NULL DEFAULT 0",
            "ALTER TABLE memories ADD COLUMN repeated_count INTEGER NOT NULL DEFAULT 0",
            "ALTER TABLE memories ADD COLUMN last_matched_at TEXT NULL",
        ] {
            let _ = conn.execute(migration, []);
        }
        // Cached per-workspace embedding/signals so the synchronous inject path
        // can rank memories by relevance without making a network call.
        let _ = conn.execute(
            "CREATE TABLE IF NOT EXISTS workspace_profiles (
                workspace_path TEXT PRIMARY KEY,
                embedding BLOB NULL,
                dimensions INTEGER NOT NULL DEFAULT 0,
                signals TEXT NOT NULL DEFAULT '',
                updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        );
        // Media generation tasks (image + async video). Files live under
        // ~/.omnix/media/; only paths and provider metadata are stored here.
        let _ = conn.execute(
            "CREATE TABLE IF NOT EXISTS media_tasks (
                id TEXT PRIMARY KEY,
                platform_id TEXT NOT NULL,
                kind TEXT NOT NULL,
                model TEXT NOT NULL,
                prompt TEXT NOT NULL,
                params_json TEXT NOT NULL DEFAULT '{}',
                status TEXT NOT NULL DEFAULT 'pending',
                progress INTEGER NOT NULL DEFAULT 0,
                external_id TEXT NULL,
                result_path TEXT NULL,
                raw_response TEXT NULL,
                error TEXT NULL,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        );

        // OAuth auth center: subscription accounts (tokens AES-GCM encrypted,
        // never stored plaintext) + short-lived PKCE sessions for in-flight logins.
        let _ = conn.execute(
            "CREATE TABLE IF NOT EXISTS oauth_accounts (
                id TEXT PRIMARY KEY,
                provider TEXT NOT NULL,
                label TEXT NOT NULL DEFAULT '',
                access_enc TEXT NOT NULL,
                refresh_enc TEXT NULL,
                expires_at DATETIME NULL,
                scope TEXT NULL,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        );
        let _ = conn.execute(
            "CREATE TABLE IF NOT EXISTS oauth_pkce_sessions (
                state TEXT PRIMARY KEY,
                provider TEXT NOT NULL,
                code_verifier TEXT NOT NULL,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        );

        Ok(())
    }

    fn remove_known_mock_conversations(&self, conn: &Connection) -> Result<()> {
        conn.execute(
            "DELETE FROM messages WHERE conversation_id IN ('mock_sess_cors', 'mock_sess_lock')",
            [],
        )?;
        let _ = conn.execute(
            "ALTER TABLE kb_documents ADD COLUMN knowledge_base_id TEXT NOT NULL DEFAULT 'default'",
            [],
        );
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_kb_documents_base ON kb_documents(knowledge_base_id, updated_at)",
            [],
        )?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS chat_knowledge_bindings (
                conversation_id TEXT NOT NULL,
                knowledge_base_id TEXT NOT NULL,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                PRIMARY KEY(conversation_id, knowledge_base_id),
                FOREIGN KEY(conversation_id) REFERENCES conversations(id) ON DELETE CASCADE,
                FOREIGN KEY(knowledge_base_id) REFERENCES knowledge_bases(id) ON DELETE CASCADE
            )",
            [],
        )?;
        // Presentation decks (PPT panel). The whole structured Deck JSON lives
        // in model_json (single source of truth); title/theme are duplicated as
        // columns for cheap listing.
        conn.execute(
            "CREATE TABLE IF NOT EXISTS decks (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL DEFAULT '未命名演示',
                theme TEXT NOT NULL DEFAULT 'midnight',
                model_json TEXT NOT NULL DEFAULT '{}',
                slide_count INTEGER NOT NULL DEFAULT 0,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        )?;
        // Deck version snapshots — every AI mutation stores the pre-change model
        // so a bad AI edit is always one click away from being undone.
        conn.execute(
            "CREATE TABLE IF NOT EXISTS deck_versions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                deck_id TEXT NOT NULL,
                model_json TEXT NOT NULL,
                label TEXT NOT NULL DEFAULT '',
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_deck_versions ON deck_versions(deck_id, id DESC)",
            [],
        )?;

        // Reusable presentation brand masters (D).
        conn.execute(
            "CREATE TABLE IF NOT EXISTS deck_brands (
                name TEXT PRIMARY KEY,
                brand_json TEXT NOT NULL DEFAULT '{}',
                updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        )?;
        conn.execute(
            "DELETE FROM conversations WHERE id IN ('mock_sess_cors', 'mock_sess_lock')",
            [],
        )?;
        Ok(())
    }

    fn seed_default_accounts(&self, conn: &Connection) -> Result<()> {
        // Only seed on first install — never re-seed after user deletes accounts
        if self.get_setting("seed_accounts_completed")?.is_some() {
            return Ok(());
        }

        // Fetch current setting configurations to establish default profile
        let api_key = self.get_setting("api_key")?.unwrap_or_default();
        let api_host = self
            .get_setting("api_host")?
            .unwrap_or_else(|| "https://api.openai.com/v1".to_string());

        let agents_to_seed = vec![
            (
                "claude_code_default",
                "Claude Code 默认账户",
                "",
                "https://api.anthropic.com/v1",
                "claude-3-5-sonnet",
                "Claude Code",
            ),
            (
                "gemini_cli_default",
                "Gemini CLI 默认账户",
                "",
                "https://generativelanguage.googleapis.com",
                "gemini-2.0-flash",
                "Gemini CLI",
            ),
            (
                "codex_default",
                "Codex 默认账户",
                &api_key,
                &api_host,
                "gpt-4o",
                "Codex",
            ),
            (
                "qwen_code_default",
                "Qwen Code 默认账户",
                "",
                "https://dashscope.aliyuncs.com/compatible-mode/v1",
                "qwen-plus",
                "Qwen Code",
            ),
            (
                "github_copilot_cli_default",
                "GitHub Copilot CLI 默认账户",
                "",
                "https://api.github.com",
                "gpt-4o",
                "GitHub Copilot CLI",
            ),
            (
                "google_antigravity_default",
                "Google Antigravity 默认账户",
                "",
                "https://api.openai.com/v1",
                "gpt-4o",
                "Google Antigravity",
            ),
            (
                "opencode_default",
                "OpenCode 默认账户",
                "",
                "https://api.openai.com/v1",
                "gpt-4o",
                "OpenCode",
            ),
        ];

        for (id, name, key, host, model, agent_name) in agents_to_seed {
            let exists: i64 = conn.query_row(
                "SELECT COUNT(*) FROM agent_accounts WHERE agent_name = ?1",
                params![agent_name],
                |r| r.get(0),
            )?;
            if exists == 0 {
                conn.execute(
                    "INSERT INTO agent_accounts (id, account_name, api_key, api_host, target_model, agent_name, is_active)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1)",
                    params![id, name, key, host, model, agent_name],
                )?;
            }
        }

        // Mark seed as completed — future starts will skip re-seeding
        conn.execute(
            "INSERT OR REPLACE INTO settings (key, value, updated_at) VALUES (?1, ?2, CURRENT_TIMESTAMP)",
            params!["seed_accounts_completed", "true"],
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
            fs::write(&min_path, min)
                .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;

            let mut core_path = base_path.clone();
            core_path.set_file_name(format!("{}_core.md", name));
            fs::write(&core_path, core)
                .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;

            let mut comp_path = base_path.clone();
            comp_path.set_file_name(format!("{}_comprehensive.md", name));
            fs::write(&comp_path, comp)
                .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;

            // Write DB row with file_path pointing to base path
            conn.execute(
                "INSERT INTO skills (name, description, file_path, profile, is_active, dependencies)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![name, desc, base_path_str, "Core", 1, deps],
            )?;
        }

        Ok(())
    }
}

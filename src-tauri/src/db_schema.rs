//! Database schema: table creation, migrations, and first-run seeding.
//! Split out of db.rs — this is a second `impl DbManager` block, so callers
//! and method signatures are unchanged.

use rusqlite::{params, Connection, OptionalExtension, Result};
use std::fs;

use crate::db::DbManager;

/// 全部列迁移**就这一份**，而且跑在所有 `CREATE TABLE` 之后。
///
/// 以前它散在六处，夹在建表语句中间，于是踩了两个坑：
///
/// 1. **顺序坑。** `ALTER TABLE cron_runs ADD COLUMN action_summary` 排在
///    `CREATE TABLE cron_runs` 前面，新库上报 `no such table` 然后被 `let _ =`
///    吞掉——**全新安装的库里没有这一列**，升级上来的库有。定时任务的动作摘要
///    因此对所有新用户是写进虚空的，而且不会报任何错。
/// 2. **吞异常坑。** `let _ =` 把「这列已经有了」和「列名写错 / 默认值不合法 /
///    表还不存在」压成同一个结果。真失败和已完成长得一模一样。
///
/// 现在：一份清单、一个应用点、只放过「列已存在」这一种错，其余的喊出来。
/// `every_migrated_column_exists_in_a_fresh_database` 守着顺序不会再错。
pub(crate) const COLUMN_MIGRATIONS: &[&str] = &[

            "ALTER TABLE skills ADD COLUMN source_type TEXT NOT NULL DEFAULT 'local'",
            "ALTER TABLE skills ADD COLUMN source_ref TEXT NULL",
            "ALTER TABLE skills ADD COLUMN source_revision TEXT NULL",
            "ALTER TABLE skills ADD COLUMN central_path TEXT NOT NULL DEFAULT ''",
            "ALTER TABLE skills ADD COLUMN content_hash TEXT NULL",
            "ALTER TABLE skills ADD COLUMN starred INTEGER NOT NULL DEFAULT 0",
            "ALTER TABLE skills ADD COLUMN category TEXT NULL",
            // Skill compound interest fields
            // Skill auto-update: when central content last changed via the update
            // engine (drives the 「更新待复审」 badge: content_updated_at > reviewed_at)
            "ALTER TABLE skills ADD COLUMN content_updated_at DATETIME NULL",
            "ALTER TABLE skills ADD COLUMN usage_count INTEGER NOT NULL DEFAULT 0",
            "ALTER TABLE skills ADD COLUMN last_used_at DATETIME NULL",
            "ALTER TABLE skills ADD COLUMN success_count INTEGER NOT NULL DEFAULT 0",
            "ALTER TABLE skills ADD COLUMN priority_score REAL NOT NULL DEFAULT 1.0",
            // Agent task lifecycle fields
            // 编排预设（借鉴 paseo）：worker 可按预设跑只读/计划模式（顾问、委员会）
            "ALTER TABLE agent_runs ADD COLUMN work_mode TEXT NOT NULL DEFAULT 'direct'",
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
            // P2 技能锁：晋升正式池那一刻的内容指纹（SHA-256）与时间。
            // 审核认可的是**当时那份内容**，不是这个文件名——之后文件被改了要查得出来。
            "ALTER TABLE cron_runs ADD COLUMN action_summary TEXT NOT NULL DEFAULT ''",
            "ALTER TABLE skills ADD COLUMN approved_hash TEXT NULL",
            "ALTER TABLE skills ADD COLUMN approved_at TEXT NULL",
            // R2 用量计量：缓存 token 的明细。
            // `prompt_tokens` 记的是**计费口径的输入总量**（含这两列），这样
            // total_tokens / estimate_cost 等既有读取端不用改就是对的；这两列
            // 是明细，用来回答「这次到底命中了多少缓存」。
            "ALTER TABLE request_logs ADD COLUMN cache_read_tokens INTEGER NOT NULL DEFAULT 0",
            "ALTER TABLE request_logs ADD COLUMN cache_creation_tokens INTEGER NOT NULL DEFAULT 0",
            // T1 证据等级（借鉴 GenericAgent「无行动，不记忆」）：
            // acted = 结论能对上网关记录的真实工具调用；claimed = 材料里只有陈述。
            // 默认 claimed——存量数据是在没有这道校验时写的，不能追认为已验证。
            "ALTER TABLE distillation_inbox ADD COLUMN verified TEXT NOT NULL DEFAULT 'claimed'",

            // 以下几条原先散在别处，各自带一段 PRAGMA 预检或单独的 `let _ =`：
            "ALTER TABLE agent_accounts ADD COLUMN agent_name TEXT DEFAULT ''",
            // 熔断器跳闸的时刻，网关据此在冷却结束后放一次半开探测。
            "ALTER TABLE model_platforms ADD COLUMN circuit_opened_at DATETIME NULL",
            "ALTER TABLE platform_models ADD COLUMN has_long_context INTEGER NOT NULL DEFAULT 0",
            "ALTER TABLE platform_models ADD COLUMN has_tool_use INTEGER NOT NULL DEFAULT 1",
            "ALTER TABLE platform_models ADD COLUMN has_embedding INTEGER NOT NULL DEFAULT 0",
            "ALTER TABLE platform_models ADD COLUMN has_speedy INTEGER NOT NULL DEFAULT 0",
            "ALTER TABLE memories ADD COLUMN type TEXT NOT NULL DEFAULT 'experience'",
            "ALTER TABLE platform_api_keys ADD COLUMN priority INTEGER NOT NULL DEFAULT 0",
            "ALTER TABLE platform_api_keys ADD COLUMN is_enabled INTEGER NOT NULL DEFAULT 1",
            "ALTER TABLE platform_api_keys ADD COLUMN last_status TEXT NOT NULL DEFAULT 'unknown'",
            "ALTER TABLE platform_api_keys ADD COLUMN last_error TEXT NULL",
            "ALTER TABLE platform_api_keys ADD COLUMN latency_ms INTEGER NULL",
            "ALTER TABLE platform_api_keys ADD COLUMN last_checked_at TEXT NULL",
            "ALTER TABLE memories ADD COLUMN source TEXT NOT NULL DEFAULT 'manual'",
            "ALTER TABLE memories ADD COLUMN workspace_path TEXT NULL",
            "ALTER TABLE memories ADD COLUMN evidence_json TEXT NOT NULL DEFAULT '{}'",
            "ALTER TABLE memories ADD COLUMN status TEXT NOT NULL DEFAULT 'active'",
            // Evolution: relevance-based injection + dedup + effectiveness tracking.
            "ALTER TABLE memories ADD COLUMN embedding BLOB NULL",
            "ALTER TABLE memories ADD COLUMN dimensions INTEGER NOT NULL DEFAULT 0",
            "ALTER TABLE memories ADD COLUMN stack_tags TEXT NOT NULL DEFAULT ''",
            "ALTER TABLE memories ADD COLUMN confidence REAL NOT NULL DEFAULT 1",
            "ALTER TABLE memories ADD COLUMN verified TEXT NOT NULL DEFAULT 'claimed'",
            "ALTER TABLE memories ADD COLUMN seen_count INTEGER NOT NULL DEFAULT 0",
            "ALTER TABLE memories ADD COLUMN repeated_count INTEGER NOT NULL DEFAULT 0",
            "ALTER TABLE memories ADD COLUMN last_matched_at TEXT NULL",
            "ALTER TABLE kb_documents ADD COLUMN knowledge_base_id TEXT NOT NULL DEFAULT 'default'",
];

/// 迁移全部跑完后写进 `PRAGMA user_version` 的值。
///
/// 用条数本身当版本号：加一条 ALTER 就自动变一个新版本，老库据此重跑，
/// **没有需要人手同步的第二个数字**。`schema_version_matches_the_migration_count`
/// 钉住这个等式。
pub(crate) fn schema_version() -> i32 {
    COLUMN_MIGRATIONS.len() as i32
}

/// 这个错误是不是「这列已经有了」。
///
/// 只有这一种算正常——升级路径上每条 ALTER 第二次跑都会撞上它。别的都不是：
/// `no such table` 是顺序错了，`near "..."` 是语句写错了，都得看得见。
fn is_duplicate_column(error: &rusqlite::Error) -> bool {
    error.to_string().contains("duplicate column name")
}

impl DbManager {
    /// 应用列迁移，返回**不正常**的失败（已存在的不算）。
    ///
    /// 不让 `init_schema` 因此报错退出：一条坏迁移不该把整个应用锁在启动不了的
    /// 状态。所以这里记日志、把清单交出去，由测试来当那道严格的闸。
    fn apply_column_migrations(&self, conn: &Connection) -> Vec<String> {
        let mut failures = Vec::new();
        for sql in COLUMN_MIGRATIONS {
            match conn.execute(sql, []) {
                Ok(_) => {}
                Err(error) if is_duplicate_column(&error) => {}
                Err(error) => {
                    log::error!("列迁移失败：{sql} —— {error}");
                    failures.push(format!("{sql} —— {error}"));
                }
            }
        }
        failures
    }
}

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
                usage_count INTEGER NOT NULL DEFAULT 0,    -- 网关把它注入 system prompt 的次数（不是「用上了」）
                last_used_at DATETIME NULL,                -- 最后一次注入时间
                -- 下面两列已废弃，恒为默认值。原本的「技能复利」要靠它们排序，但
                -- OMNIX 是透传网关，拿不到任何一次调用的成败，写不出真实的分数。
                -- 只留不删：DROP COLUMN 塞不进 COLUMN_MIGRATIONS 的判重规则
                -- （重跑会报 no such column，被当成真失败，user_version 从此推不动）。
                success_count INTEGER NOT NULL DEFAULT 0,  -- 废弃：无人读写
                priority_score REAL NOT NULL DEFAULT 1.0   -- 废弃：无人读写
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

        // 6d-2. 深度研究：一次研究 + 它的带证据笔记
        //
        // **两张表的读取端是 `research::report`，和写入端同一轮做完。** 这个仓库
        // 最高产的 bug 模式就是「写了但没人读」——建表时不想清楚谁读，它就会变成
        // 下一个只写不读的表（`tests.rs::write_only_tables_are_declared` 会拦）。
        conn.execute(
            "CREATE TABLE IF NOT EXISTS research_runs (
                id TEXT PRIMARY KEY,
                question TEXT NOT NULL,
                workspace_path TEXT NOT NULL DEFAULT '',
                status TEXT NOT NULL DEFAULT 'running',   -- running | done | abandoned
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                finished_at DATETIME NULL
            )",
            [],
        )?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS research_notes (
                id TEXT PRIMARY KEY,
                research_id TEXT NOT NULL,
                claim TEXT NOT NULL,
                source_url TEXT NOT NULL,
                snippet TEXT NOT NULL,
                -- 来源等级：官方文档 > 一手仓库 > 技术博客 > 论坛 > 未知。
                -- 复用记忆库那套「分级可信度」的思路，不另造一套。
                tier TEXT NOT NULL DEFAULT 'unknown',
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                FOREIGN KEY(research_id) REFERENCES research_runs(id) ON DELETE CASCADE
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

        // 6. Memory Table (anti-failure incident dict)
        conn.execute(
            "CREATE TABLE IF NOT EXISTS memories (
                id TEXT PRIMARY KEY,
                incident_desc TEXT NOT NULL,
                code_pattern TEXT NOT NULL,
                remediation TEXT NOT NULL,
                keywords TEXT NOT NULL, -- comma-separated tags
                type TEXT NOT NULL DEFAULT 'experience', -- 'preference' | 'experience'
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                -- T1 证据等级。注意：这张表的 CREATE 排在上面那批 migrations
                -- **之后**，所以新库走不到那条 ALTER——列必须写在这里，
                -- 否则全新安装的库里压根没有 verified，召回查询直接失败。
                verified TEXT NOT NULL DEFAULT 'claimed'
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
                cache_read_tokens INTEGER NOT NULL DEFAULT 0,
                cache_creation_tokens INTEGER NOT NULL DEFAULT 0,
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

        // 6a-2. Router Decisions —— 每一次 Auto 选型留一行。
        //
        // **隐私契约：这张表没有任何自由文本列。** prompt 永远不进来；`needs`
        // 是一组枚举 token（vision/reasoning/coding/speedy/tools），`route_key`
        // 是会话 id 或 agent 名，不是用户写的东西。`router_decisions_carry_no_prose`
        // 守着这条线。
        //
        // 存在的理由是回答两个原来答不上来的问题：上周那轮为什么选了这个模型，
        // 以及 Auto 到底有没有在省钱。读取端是「用量成本看板」。
        conn.execute(
            "CREATE TABLE IF NOT EXISTS router_decisions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                route_key TEXT NULL,
                needs TEXT NOT NULL DEFAULT '',
                candidate_count INTEGER NOT NULL DEFAULT 0,
                chosen_model TEXT NOT NULL,
                chosen_price REAL NOT NULL DEFAULT 0,
                baseline_model TEXT NOT NULL,
                baseline_price REAL NOT NULL DEFAULT 0,
                anti_downgrade INTEGER NOT NULL DEFAULT 0
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

        // 13. FTS5 全文索引（独立表，不是外部内容表）
        //
        // 两处和以前不一样，都是被真实 bug 逼出来的：
        //
        // 1. **不再用 `content='kb_chunks'`。** 外部内容表取列值要回源表按 rowid
        //    查同名列，而 fts 的列叫 `chunk_id`、`kb_chunks` 的主键叫 `id`，于是
        //    `SELECT chunk_id FROM kb_chunks_fts` 每一行都报
        //    `no such column: T.chunk_id`——**任何语言都查不出结果**，而调用方一个
        //    `unwrap_or_default()` 把它吞成了「没有关键词命中」。现在 chunk_id 作为
        //    UNINDEXED 列直接存在 fts 表里，取值不再回源。
        //
        // 2. **索引的是二元切分后的文本**（`knowledge::segment_for_index`）。
        //    `unicode61` 把一整段中文当一个 token，「量子计算」匹配不到
        //    「量子计算的进展很快」。所以写入前把 CJK 连写段切成二元组，查询按
        //    同样规则切。英文原样，仍走 porter 词干还原。
        //
        // 因为要写切分后的文本，同步不能再靠 SQL 触发器（触发器调不到 Rust）。
        // 维护集中在 `knowledge::index_chunk` / `unindex_document` 两个函数里，
        // 三个写入点各调一次——比把切分逻辑散进触发器可控。
        conn.execute(
            "CREATE VIRTUAL TABLE IF NOT EXISTS kb_chunks_fts USING fts5(
                chunk_id UNINDEXED,
                content,
                tokenize='porter unicode61'
            )",
            [],
        )?;

        // 老库迁移：外部内容表 + 触发器那一套整个换掉。索引是派生数据，
        // 直接重建，不会丢任何用户内容。
        let legacy_fts: Option<String> = conn
            .query_row(
                "SELECT sql FROM sqlite_master WHERE type='table' AND name='kb_chunks_fts'",
                [],
                |row| row.get(0),
            )
            .optional()?;
        if legacy_fts
            .as_deref()
            .is_some_and(|sql| sql.contains("content='kb_chunks'") || sql.contains("content=kb_chunks"))
        {
            for stmt in [
                "DROP TRIGGER IF EXISTS kb_chunks_ai",
                "DROP TRIGGER IF EXISTS kb_chunks_ad",
                "DROP TRIGGER IF EXISTS kb_chunks_au",
                "DROP TABLE IF EXISTS kb_chunks_fts",
            ] {
                conn.execute(stmt, [])?;
            }
            conn.execute(
                "CREATE VIRTUAL TABLE kb_chunks_fts USING fts5(
                    chunk_id UNINDEXED,
                    content,
                    tokenize='porter unicode61'
                )",
                [],
            )?;
            // 重新灌一遍。之前那套索引从来没查出过结果，所以这里等于第一次真的建起来。
            let mut stmt = conn.prepare("SELECT id, content FROM kb_chunks")?;
            let rows: Vec<(String, String)> = stmt
                .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
                .filter_map(|r| r.ok())
                .collect();
            drop(stmt);
            for (id, content) in rows {
                conn.execute(
                    "INSERT INTO kb_chunks_fts (chunk_id, content) VALUES (?1, ?2)",
                    params![id, crate::knowledge::segment_for_index(&content)],
                )?;
            }
        }

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

        // 25b. Q2′ 事后审计：agent 实际调用过的工具。主键是 tool_use 的 id，
        // 对话历史每轮重发也只记一次。
        conn.execute(
            "CREATE TABLE IF NOT EXISTS agent_actions (
                id TEXT PRIMARY KEY,
                agent TEXT NOT NULL,
                tool_name TEXT NOT NULL,
                risk_tier TEXT NOT NULL,
                detail TEXT NOT NULL DEFAULT '',
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_agent_actions_agent_time
             ON agent_actions(agent, created_at)",
            [],
        )?;

        // 25.（已移除）工具审批队列 tool_confirmations —— 四个命令和前端包装都
        // 存在过，但两边都没有任何地方调用，表永远是空的。见 commands/automation.rs
        // 的说明。已存在的空表不删：删表不可逆，留着无害。

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
            // 最要紧的一条：按会话取消息。
            //
            // **必须是 `(conversation_id, timestamp)` 复合索引**，不能只有
            // `conversation_id`。查询是 `WHERE conversation_id = ? ORDER BY timestamp`，
            // 单列索引只能定位到会话，排序还得走 `USE TEMP B-TREE FOR ORDER BY`
            // ——也就是把该会话的**全部**消息排一遍。将来加 LIMIT 分页时，省下的
            // 只是传输量，扫描和排序一点没少。
            //
            // 复合索引把 `conversation_id` 放在前面，所以它同时覆盖了原来那条
            // 单列索引的全部用途；旧索引已 DROP（见下面的 `stale_indexes`）。
            "CREATE INDEX IF NOT EXISTS idx_messages_conversation_timestamp
             ON messages(conversation_id, timestamp)",
            // 研究笔记按次序取（出报告时要按记录顺序还原）。
            "CREATE INDEX IF NOT EXISTS idx_research_notes_run
             ON research_notes(research_id, created_at)",
            // 会话列表：`WHERE is_archived = ? ORDER BY created_at DESC`。
            // 原来是全表 `SCAN` + 临时 B 树排序。
            "CREATE INDEX IF NOT EXISTS idx_conversations_archived_created
             ON conversations(is_archived, created_at)",
            // Platform models by platform (settings page loads models per-platform)
            "CREATE INDEX IF NOT EXISTS idx_platform_models_platform_id ON platform_models(platform_id)",
            // Request logs by timestamp (dashboard analytics sort by time)
            "CREATE INDEX IF NOT EXISTS idx_request_logs_timestamp ON request_logs(timestamp)",
            "CREATE INDEX IF NOT EXISTS idx_router_decisions_created_at ON router_decisions(created_at)",
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

        // 被上面的复合索引完全取代的旧索引。留着不会出错，但每次写入都要多维护
        // 一棵 B 树，而它能服务的查询新索引都能服务（前导列相同）。
        //
        // 只删**确认被覆盖**的，不做无差别清理：判据是「旧索引的列是新索引列的前缀」。
        let stale_indexes = [
            // 被 idx_messages_conversation_timestamp 覆盖
            "DROP INDEX IF EXISTS idx_messages_conversation_id",
        ];
        for drop_sql in &stale_indexes {
            let _ = conn.execute(drop_sql, []);
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

        self.init_late_tables(&conn)?;

        // 列迁移放在**最后**：此时所有表都建好了，不会再出现「ALTER 排在 CREATE
        // 前面」那种静默丢列。版本号相同就整段跳过——这样「跑一遍全是重复列错误」
        // 不再是常态路径，真失败才有机会被看见。
        let stored: i32 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap_or(0);
        if stored != schema_version() {
            let failures = self.apply_column_migrations(&conn);
            if failures.is_empty() {
                conn.execute_batch(&format!("PRAGMA user_version = {}", schema_version()))?;
            } else {
                // 有没修好的就不推版本号，下次启动还会再试一遍。
                log::error!("{} 条列迁移没成功，schema 版本号不推进", failures.len());
            }
        }
        // 依赖迁移列的索引必须排在迁移之后。
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_kb_documents_base ON kb_documents(knowledge_base_id, updated_at)",
            [],
        )?;
        Ok(())
    }

    /// 原先散在各命令文件里、靠 `ensure_table(&db)` 懒建的那批表。
    ///
    /// 那个写法有两个真问题，不是洁癖：
    ///
    /// 1. **同一张表有好几份定义。** `agent_mailbox` 三份、`task_dependencies` 三份、
    ///    `steering_queue` / `cron_tasks_persistent` 各两份，同一个文件里就有一份
    ///    展开的、一份压缩的。而 `CREATE TABLE IF NOT EXISTS` 撞上已存在的表是
    ///    静默 no-op——谁先建谁说了算，后面那几份**永远不会生效**，却看起来像在生效。
    /// 2. 于是列名对不上时没有任何报错：`init_schema` 建的 `agent_mailbox` 有
    ///    `is_read`，命令文件按 `read` 查；`task_dependencies` 建的是 `blocks_id`，
    ///    命令文件按 `depends_on` 写。两处都被 `let _ =` 吞掉，功能整段是死的，
    ///    测试全绿、界面无异常。
    ///
    /// 收进来之后：一张表一处定义，命令只管查。`no_command_file_defines_a_table`
    /// 守着不许再散回去。
    fn init_late_tables(&self, conn: &Connection) -> Result<()> {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS checkpoints (
                id TEXT PRIMARY KEY,
                workspace_path TEXT NOT NULL,
                session_id TEXT NOT NULL DEFAULT '',
                label TEXT NOT NULL DEFAULT '',
                vcs TEXT NOT NULL DEFAULT 'git',
                ref_name TEXT NOT NULL DEFAULT '',
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            )",
            [],
        )?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS custom_assistants (
                slug TEXT PRIMARY KEY,
                name TEXT NOT NULL DEFAULT '',
                description TEXT NOT NULL DEFAULT '',
                category TEXT NOT NULL DEFAULT '自定义',
                instructions TEXT NOT NULL DEFAULT '',
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            )",
            [],
        )?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS hooks (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL DEFAULT '',
                event TEXT NOT NULL DEFAULT '*',
                matcher TEXT NOT NULL DEFAULT '',
                action_type TEXT NOT NULL DEFAULT 'notify',
                action_payload TEXT NOT NULL DEFAULT '',
                enabled INTEGER NOT NULL DEFAULT 1,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                fire_count INTEGER NOT NULL DEFAULT 0,
                last_fired_at TEXT
            )",
            [],
        )?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS hook_runs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                hook_id TEXT NOT NULL,
                hook_name TEXT NOT NULL DEFAULT '',
                session_id TEXT NOT NULL DEFAULT '',
                event TEXT NOT NULL DEFAULT '',
                fired_at TEXT NOT NULL DEFAULT (datetime('now')),
                ok INTEGER NOT NULL DEFAULT 1,
                detail TEXT NOT NULL DEFAULT ''
            )",
            [],
        )?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS notes (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL DEFAULT '',
                content TEXT NOT NULL DEFAULT '',
                tags TEXT NOT NULL DEFAULT '',
                source TEXT NOT NULL DEFAULT '',
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            )",
            [],
        )?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS quick_actions (
                id TEXT PRIMARY KEY,
                label TEXT NOT NULL DEFAULT '',
                emoji TEXT NOT NULL DEFAULT '✨',
                prompt_template TEXT NOT NULL DEFAULT '',
                enabled INTEGER NOT NULL DEFAULT 1,
                order_num INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            )",
            [],
        )?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS ssh_hosts (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                host TEXT NOT NULL,
                port INTEGER NOT NULL DEFAULT 22,
                user TEXT NOT NULL DEFAULT '',
                key_path TEXT NOT NULL DEFAULT '',
                default_workdir TEXT NOT NULL DEFAULT '',
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        )?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS subagents (
                id TEXT PRIMARY KEY,
                parent_conversation_id TEXT NOT NULL,
                title TEXT NOT NULL DEFAULT '',
                prompt TEXT NOT NULL DEFAULT '',
                agent TEXT NOT NULL DEFAULT '',
                child_conversation_id TEXT NOT NULL DEFAULT '',
                child_session_id TEXT NOT NULL DEFAULT '',
                worktree_id TEXT NOT NULL DEFAULT '',
                worktree_path TEXT NOT NULL DEFAULT '',
                status TEXT NOT NULL DEFAULT 'running',
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            )",
            [],
        )?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS worktrees (
                id TEXT PRIMARY KEY,
                repo_path TEXT NOT NULL,
                worktree_path TEXT NOT NULL,
                branch TEXT NOT NULL DEFAULT '',
                session_id TEXT NOT NULL DEFAULT '',
                label TEXT NOT NULL DEFAULT '',
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            )",
            [],
        )?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS steering_queue (
                id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                content TEXT NOT NULL,
                consumed INTEGER NOT NULL DEFAULT 0,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        )?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS cron_tasks_persistent (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                schedule TEXT NOT NULL,
                timezone TEXT NOT NULL DEFAULT 'UTC',
                agent_name TEXT NULL,
                prompt_template TEXT NULL,
                mode TEXT NOT NULL DEFAULT 'new_conversation',
                keep_awake INTEGER NOT NULL DEFAULT 0,
                enabled INTEGER NOT NULL DEFAULT 1,
                last_run_at DATETIME NULL,
                next_run_at DATETIME NULL,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        )?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS skill_audit_log (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                skill_name TEXT,
                score INTEGER,
                issues TEXT,
                audited_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        )?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS agent_configs (
                agent_name TEXT NOT NULL,
                config_key TEXT NOT NULL,
                config_value TEXT NOT NULL,
                updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                PRIMARY KEY (agent_name, config_key)
            )",
            [],
        )?;
        Ok(())
    }

    fn remove_known_mock_conversations(&self, conn: &Connection) -> Result<()> {
        conn.execute(
            "DELETE FROM messages WHERE conversation_id IN ('mock_sess_cors', 'mock_sess_lock')",
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
                "deep_research",
                "多跳检索：搜 → 读 → 提炼还没答上的子问题 → 再搜，每条结论都带出处。单次搜索答不上的问题用它。",
                "[]",
                "### 什么时候用
**多跳问题**——单次搜索答不上，需要顺着线索再查的那种：
「A 的实现依赖 B，B 在什么条件下会失效」「这个行为在哪个版本改的，改成了什么」。
一次搜索就能答的**别用**，直接 web_search。

### 循环
1. `research_start` 拿到 research_id。
2. `web_search` / `fetch_url` 找材料。
3. **每读到一条能支撑结论的东西，立刻 `research_note`**——带上 source_url 和原文片段。
   不要读完一大堆再回头补出处，那时你已经记不清哪句话来自哪里了。
4. 停下来问自己：**还有哪个子问题没答上？** 有就回第 2 步。
5. `research_report` 出报告。

### 什么时候停
**新一轮问不出新子问题就停。** 不要按固定轮数跑——简单问题跑三轮是浪费，复杂问题三轮不够。
找不到答案也要停，然后在报告里说清楚哪一环没查到，别用推测填上。

### 硬规矩
- **没有出处的结论不许记。** 没有引文的多轮搜索只是贵几倍的普通搜索。
- **原文片段必须能支撑那条结论。** URL 会改会死，片段是留在本地的证据。
- **报告里不能有没记录过的内容。** `research_report` 是从库里拼的；想写进报告就先 `research_note`。
- 标好 tier：official（官方文档）> repo（一手仓库）> blog > forum。报告按这个排序。

### 反模式
- 🚫 先写结论再去找支持它的链接。
- 🚫 把整页网页塞进 claim——claim 是一句话结论，长文放 snippet。
- 🚫 查不到就用「一般来说」「通常」糊过去。查不到就说查不到。",
                "### 循环
1. research_start
2. 搜 / 抓 → research_note（必须带出处）
3. 还有子问题没答上就回第 2 步
4. 问不出新子问题 → research_report",
                "### 进阶
- **交叉验证**：关键结论找两个独立来源。只有一个来源时在 claim 里写明「仅一处佐证」。
- **冲突处理**：两个来源打架时两条都记下，各自标 tier，让报告把等级高的排前面——不要自己选一个然后把另一个丢掉。
- **时效**：注意材料的日期。三年前的博客讲的行为可能早就改了，去一手仓库确认。
- **子问题要具体**：「再查查 B」不是子问题，「B 在 Windows 上走不走系统代理」才是。"
            ),
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

#[cfg(test)]
mod single_source_of_truth {
    /// 建表这件事只许发生在这一个模块里。
    ///
    /// 收拢之前，同一张表能有三份定义散在命令文件里，而 `CREATE TABLE IF NOT EXISTS`
    /// 撞上已存在的表是**静默 no-op**——谁先建谁说了算，剩下几份永远不生效，却看着
    /// 像在生效。`agent_mailbox` 和 `task_dependencies` 就是这么把两个功能整段做死的：
    /// 库里的列叫 `is_read` / `blocks_id`，命令按 `read` / `depends_on` 查，
    /// 错误全被 `let _ =` 吞掉，测试全绿、界面无异常。
    ///
    /// 所以这条守的不是风格，是那类 bug 的入口。
    #[test]
    fn no_command_file_defines_a_table() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/commands");
        let mut offenders = Vec::new();
        for entry in std::fs::read_dir(&dir).expect("读 src/commands").flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            let source = std::fs::read_to_string(&path).expect("读命令文件");
            // 测试模块里自己造表是允许的——那是被测对象的替身，不是产品 schema。
            let production = source
                .split_once("#[cfg(test)]")
                .map(|(before, _)| before)
                .unwrap_or(&source);
            // 注释里**说到**建表不算建表。原来是整段扫，于是一条解释「为什么不能
            // 依赖 CREATE TABLE IF NOT EXISTS」的注释就能把这条守卫点红——守卫
            // 看的应该是代码，不是散文。
            let code: String = production
                .lines()
                .filter(|line| !line.trim_start().starts_with("//"))
                .collect::<Vec<_>>()
                .join("\n");
            if code.contains("CREATE TABLE") {
                offenders.push(path.file_name().unwrap().to_string_lossy().to_string());
            }
        }
        assert!(
            offenders.is_empty(),
            "这些命令文件在自己建表：{offenders:?}\n\
             把定义搬到 db_schema.rs 的 init_late_tables，命令只管查。\n\
             散着建的代价见本模块注释——它不会报错，只会让功能悄悄失效。"
        );
    }

    /// 收拢之后，一个全新的库必须一次就带齐所有表。
    ///
    /// 懒建时代这条根本无从谈起：表要等对应命令第一次被调用才出现。
    #[test]
    fn a_fresh_database_has_every_table() {
        let path = std::env::temp_dir().join(format!(
            "omnix_schema_{}_{}.sqlite",
            std::process::id(),
            chrono::Utc::now().timestamp_micros()
        ));
        let db = crate::db::DbManager::new_with_path(path.clone());
        db.init_schema().expect("init_schema");
        let conn = db.get_connection().expect("连接");
        let mut stmt = conn
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table'")
            .expect("查表");
        let tables: std::collections::BTreeSet<String> = stmt
            .query_map([], |r| r.get::<_, String>(0))
            .expect("枚举")
            .flatten()
            .collect();
        drop(stmt);
        drop(conn);

        // 原先靠 ensure_table 懒建的那批，一个都不能少。
        for expected in [
            "checkpoints", "custom_assistants", "hooks", "hook_runs", "notes",
            "quick_actions", "ssh_hosts", "subagents", "worktrees",
            "steering_queue", "cron_tasks_persistent", "skill_audit_log", "agent_configs",
        ] {
            assert!(tables.contains(expected), "新库缺表：{expected}");
        }
        let _ = std::fs::remove_file(&path);
    }

    /// 命令查的列，得真的存在。
    ///
    /// 这条是冲着已经发生过的那次事故写的：`agent_mailbox` 库里是 `is_read`、
    /// 命令按 `read` 查了不知道多久。列名对不上时 SQLite 只在**执行**时报错，
    /// 而那两处都写成 `let _ =`，于是一声不吭。
    ///
    /// 信箱和 steering 那两组命令后来整组删除了（没有任何生产方），断言也随之
    /// 移除——守卫要守的是**还活着的命令**，对着不存在的命令断言只是装饰。
    #[test]
    fn the_columns_the_commands_query_actually_exist() {
        let path = std::env::temp_dir().join(format!(
            "omnix_cols_{}_{}.sqlite",
            std::process::id(),
            chrono::Utc::now().timestamp_micros()
        ));
        let db = crate::db::DbManager::new_with_path(path.clone());
        db.init_schema().expect("init_schema");
        let conn = db.get_connection().expect("连接");
        for sql in [
            "INSERT INTO task_dependencies (task_id, blocks_id) VALUES ('a', 'b')",
            "SELECT task_id FROM task_dependencies WHERE blocks_id = ''",
        ] {
            conn.execute(sql, []).unwrap_or_else(|e| panic!("{sql}\n  → {e}"));
        }
        drop(conn);
        let _ = std::fs::remove_file(&path);
    }
}

#[cfg(test)]
mod migration_tests {
    use super::{is_duplicate_column, schema_version, COLUMN_MIGRATIONS};
    use crate::db::DbManager;

    fn fresh_db(tag: &str) -> (DbManager, std::path::PathBuf) {
        use std::sync::atomic::{AtomicU32, Ordering};
        static N: AtomicU32 = AtomicU32::new(0);
        let path = std::env::temp_dir().join(format!(
            "omnix_migrate_{tag}_{}_{}.db",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_file(&path);
        (DbManager::new_with_path(path.clone()), path)
    }

    /// 从 `ALTER TABLE <表> ADD COLUMN <列> …` 里取出表名和列名。
    fn table_and_column(sql: &str) -> (&str, &str) {
        let rest = sql
            .strip_prefix("ALTER TABLE ")
            .unwrap_or_else(|| panic!("不认识的迁移语句：{sql}"));
        let mut parts = rest.split_whitespace();
        let table = parts.next().expect(sql);
        assert_eq!(parts.next(), Some("ADD"), "{sql}");
        assert_eq!(parts.next(), Some("COLUMN"), "{sql}");
        (table, parts.next().expect(sql))
    }

    /// **每一条迁移的列，在全新安装的库里都必须真的存在。**
    ///
    /// 这条是冲着一个已经发生过的 bug 写的：`ALTER TABLE cron_runs ADD COLUMN
    /// action_summary` 排在 `CREATE TABLE cron_runs` **前面**，新库上报
    /// `no such table` 被 `let _ =` 吞掉，于是全新安装的库里没有这一列，而升级
    /// 上来的库有。定时任务的动作摘要对所有新用户是写进虚空的，一声不吭。
    ///
    /// 语句从 `COLUMN_MIGRATIONS` 读，库从真实的 `init_schema` 建——把新加的
    /// ALTER 放错位置，这条就红。
    #[test]
    fn every_migrated_column_exists_in_a_fresh_database() {
        let (db, path) = fresh_db("fresh");
        let conn = db.get_connection().unwrap();
        let mut missing = Vec::new();
        for sql in COLUMN_MIGRATIONS {
            let (table, column) = table_and_column(sql);
            if conn
                .prepare(&format!("SELECT {column} FROM {table} LIMIT 0"))
                .is_err()
            {
                missing.push(format!("{table}.{column}"));
            }
        }
        drop(conn);
        let _ = std::fs::remove_file(&path);
        assert!(
            missing.is_empty(),
            "全新安装的库里缺这些列（多半是 ALTER 排在了它的 CREATE TABLE 前面）：{missing:?}"
        );
    }

    /// 同一份迁移清单跑第二遍必须一条不错——**「列已经有了」是升级路径的常态**。
    ///
    /// 顺带钉住分类没写反：把「已存在」也当成失败，升级时每次启动都会刷一屏错。
    #[test]
    fn rerunning_the_migrations_reports_no_failures() {
        let (db, path) = fresh_db("rerun");
        let conn = db.get_connection().unwrap();
        let mut unexpected = Vec::new();
        for sql in COLUMN_MIGRATIONS {
            if let Err(error) = conn.execute(sql, []) {
                if !is_duplicate_column(&error) {
                    unexpected.push(format!("{sql} —— {error}"));
                }
            }
        }
        drop(conn);
        let _ = std::fs::remove_file(&path);
        assert!(unexpected.is_empty(), "第二遍不该出现这些错：{unexpected:?}");
    }

    /// 真失败不能被当成「已经跑过了」。
    ///
    /// 这是整件事的要害：`let _ =` 把列名写错、表不存在、默认值不合法全压成和
    /// 「已存在」同一个结果。分类必须只认那一种。
    #[test]
    fn a_real_failure_is_not_mistaken_for_an_applied_migration() {
        let (db, path) = fresh_db("classify");
        let conn = db.get_connection().unwrap();
        // 表不存在——顺序错了的那种
        let err = conn
            .execute("ALTER TABLE 根本没有这张表 ADD COLUMN x TEXT", [])
            .unwrap_err();
        assert!(!is_duplicate_column(&err), "表不存在被当成了已跑过：{err}");
        // 语句本身不合法
        let err = conn
            .execute("ALTER TABLE skills ADD COLUMN", [])
            .unwrap_err();
        assert!(!is_duplicate_column(&err), "语法错被当成了已跑过：{err}");
        // 而「已存在」要认得出来
        let err = conn
            .execute("ALTER TABLE skills ADD COLUMN starred INTEGER", [])
            .unwrap_err();
        assert!(is_duplicate_column(&err), "「列已存在」没认出来：{err}");
        drop(conn);
        let _ = std::fs::remove_file(&path);
    }

    /// 建好的库要把版本号写进 `PRAGMA user_version`，值等于迁移条数。
    ///
    /// 版本号就是条数，没有第二个需要人手同步的数字：加一条 ALTER 自动换一个
    /// 新版本，老库据此重跑一遍把列补上。
    #[test]
    fn a_fresh_database_records_the_schema_version() {
        let (db, path) = fresh_db("version");
        let conn = db.get_connection().unwrap();
        let stored: i32 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        drop(conn);
        let _ = std::fs::remove_file(&path);
        assert_eq!(stored, schema_version());
        assert_eq!(schema_version(), COLUMN_MIGRATIONS.len() as i32);
    }

    /// 清单里不能有重复：同一列写两遍，第二遍永远是「已存在」，看不出是笔误
    /// 还是有意为之。
    #[test]
    fn no_column_is_migrated_twice() {
        let mut seen = std::collections::HashSet::new();
        let mut dupes = Vec::new();
        for sql in COLUMN_MIGRATIONS {
            let (table, column) = table_and_column(sql);
            if !seen.insert((table, column)) {
                dupes.push(format!("{table}.{column}"));
            }
        }
        assert!(dupes.is_empty(), "重复的列迁移：{dupes:?}");
    }
}

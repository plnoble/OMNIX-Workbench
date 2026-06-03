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

    fn get_connection(&self) -> Result<Connection> {
        Connection::open(&self.db_path)
    }

    fn init_schema(&self) -> Result<()> {
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
}

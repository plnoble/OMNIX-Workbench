use crate::db::DbManager;
use rusqlite::params;
use std::sync::Arc;
use tauri::State;

// ── Data Backup ──────────────────────────────────────────

/// Backup info DTO — table name and row count
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BackupTableInfo {
    pub table_name: String,
    pub row_count: i64,
}

/// Backup export result
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BackupExport {
    pub version: String,
    pub timestamp: String,
    pub source: String,
    pub tables: std::collections::HashMap<String, serde_json::Value>,
}

/// Import result DTO
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ImportResult {
    pub tables_restored: Vec<(String, usize)>,
    pub total_rows: usize,
}

/// Get backup info — row counts for all tables.
#[tauri::command]
pub fn get_backup_info(db: State<'_, Arc<DbManager>>) -> Result<Vec<BackupTableInfo>, String> {
    let counts = db.get_table_row_counts().map_err(|e| e.to_string())?;
    Ok(counts
        .into_iter()
        .map(|(table_name, row_count)| BackupTableInfo {
            table_name,
            row_count,
        })
        .collect())
}

/// Export database tables to a JSON string.
#[tauri::command]
pub fn export_backup(
    tables: Option<Vec<String>>,
    db: State<'_, Arc<DbManager>>,
) -> Result<String, String> {
    // 单一来源：界面列表、默认导出、白名单共用 crate::db::BACKUP_TABLES。
    let all_tables: Vec<&str> = crate::db::BACKUP_TABLES.to_vec();
    let selected: Vec<&str> = if let Some(t) = &tables {
        all_tables
            .into_iter()
            .filter(|name| t.iter().any(|s| s == name))
            .collect()
    } else {
        all_tables
    };

    let mut backup_tables = std::collections::HashMap::new();
    for table in &selected {
        match db.export_table_as_json(table) {
            Ok(json_str) => {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&json_str) {
                    backup_tables.insert(table.to_string(), val);
                }
            }
            Err(e) => log::warn!("[Backup] Skipping table {}: {}", table, e),
        }
    }

    let export = BackupExport {
        version: "1.0".to_string(),
        timestamp: chrono::Utc::now().to_rfc3339(),
        source: "OMNIX Workbench".to_string(),
        tables: backup_tables,
    };
    serde_json::to_string_pretty(&export).map_err(|e| e.to_string())
}

/// Import database from a JSON backup string.
#[tauri::command]
pub fn import_backup(
    json_str: String,
    tables: Option<Vec<String>>,
    db: State<'_, Arc<DbManager>>,
) -> Result<ImportResult, String> {
    let backup: BackupExport =
        serde_json::from_str(&json_str).map_err(|e| format!("Invalid backup format: {}", e))?;
    if backup.version != "1.0" {
        return Err(format!("Unsupported backup version: {}", backup.version));
    }

    let mut results = Vec::new();
    let mut total_rows = 0usize;

    for (table_name, data) in &backup.tables {
        if let Some(ref t) = tables {
            if !t.contains(table_name) {
                continue;
            }
        }
        let rows_json = serde_json::to_string(data)
            .map_err(|e| format!("Failed to serialize table {}: {}", table_name, e))?;
        match db.import_table_from_json(table_name, &rows_json) {
            Ok(count) => {
                total_rows += count;
                results.push((table_name.clone(), count));
            }
            Err(e) => log::warn!("[Backup] Failed to import table {}: {}", table_name, e),
        }
    }

    Ok(ImportResult {
        tables_restored: results,
        total_rows,
    })
}

// ── Prompt Library ──────────────────────────────────────

// ── Activity Log ────────────────────────────────────────

// ══════════════════════════════════════════════════
// MCP Presets
// ══════════════════════════════════════════════════

/// A single MCP server preset entry
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct McpPresetServer {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub env: std::collections::HashMap<String, String>,
    pub url: String,
    pub server_type: String,
    pub description: String,
}

/// A complete MCP preset (a named collection of MCP servers)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct McpPreset {
    pub id: String,
    pub name: String,
    pub description: String,
    pub category: String,
    pub servers: Vec<McpPresetServer>,
}

/// Get all built-in MCP presets
#[tauri::command]
pub fn get_mcp_presets() -> Vec<McpPreset> {
    vec![
        McpPreset {
            id: "web-dev".into(),
            name: "Web Development".into(),
            description: "Essential MCP servers for web development workflows".into(),
            category: "development".into(),
            servers: vec![
                McpPresetServer {
                    name: "filesystem".into(),
                    command: "npx".into(),
                    args: vec![
                        "-y".into(),
                        "@modelcontextprotocol/server-filesystem".into(),
                        "/tmp".into(),
                    ],
                    env: std::collections::HashMap::new(),
                    url: String::new(),
                    server_type: "stdio".into(),
                    description: "File system operations (read/write/search)".into(),
                },
                McpPresetServer {
                    name: "fetch".into(),
                    command: "npx".into(),
                    args: vec!["-y".into(), "@modelcontextprotocol/server-fetch".into()],
                    env: std::collections::HashMap::new(),
                    url: String::new(),
                    server_type: "stdio".into(),
                    description: "HTTP request fetching for web APIs".into(),
                },
            ],
        },
        McpPreset {
            id: "data-analysis".into(),
            name: "Data Analysis".into(),
            description: "MCP servers for data processing and analysis".into(),
            category: "data".into(),
            servers: vec![McpPresetServer {
                name: "sqlite".into(),
                command: "npx".into(),
                args: vec!["-y".into(), "@modelcontextprotocol/server-sqlite".into()],
                env: std::collections::HashMap::new(),
                url: String::new(),
                server_type: "stdio".into(),
                description: "SQLite database operations".into(),
            }],
        },
        McpPreset {
            id: "search-tools".into(),
            name: "Search & Research".into(),
            description: "MCP servers for searching and research".into(),
            category: "search".into(),
            servers: vec![McpPresetServer {
                name: "brave-search".into(),
                command: "npx".into(),
                args: vec![
                    "-y".into(),
                    "@modelcontextprotocol/server-brave-search".into(),
                ],
                env: vec![("BRAVE_API_KEY".into(), "".into())]
                    .into_iter()
                    .collect(),
                url: String::new(),
                server_type: "stdio".into(),
                description: "Web search via Brave Search API".into(),
            }],
        },
        McpPreset {
            id: "memory".into(),
            name: "Knowledge & Memory".into(),
            description: "Persistent memory and knowledge management".into(),
            category: "productivity".into(),
            servers: vec![McpPresetServer {
                name: "memory".into(),
                command: "npx".into(),
                args: vec!["-y".into(), "@modelcontextprotocol/server-memory".into()],
                env: std::collections::HashMap::new(),
                url: String::new(),
                server_type: "stdio".into(),
                description: "Persistent knowledge graph memory".into(),
            }],
        },
    ]
}

/// Apply an MCP preset — adds all servers from the preset to the MCP servers table
#[tauri::command]
pub fn apply_mcp_preset(preset_id: String, db: State<'_, Arc<DbManager>>) -> Result<u32, String> {
    let presets = get_mcp_presets();
    let preset = presets
        .iter()
        .find(|p| p.id == preset_id)
        .ok_or_else(|| format!("Unknown MCP preset: {}", preset_id))?;

    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let mut count = 0u32;
    for server in &preset.servers {
        let id = format!("mcp_{}", chrono::Utc::now().timestamp_millis());
        let env_json = serde_json::to_string(&server.env).unwrap_or_else(|_| "{}".into());
        let args_json = serde_json::to_string(&server.args).unwrap_or_else(|_| "[]".into());
        conn.execute(
            "INSERT OR IGNORE INTO mcp_servers (id, name, command, args, env, url, server_type, is_enabled)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 1)",
            params![id, server.name, server.command, args_json, env_json, server.url, server.server_type],
        ).map_err(|e| e.to_string())?;
        count += 1;
    }
    Ok(count)
}

// ══════════════════════════════════════════════════
// Output Styles
// ══════════════════════════════════════════════════


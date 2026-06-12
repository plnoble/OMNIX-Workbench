use tauri::State;
use std::sync::Arc;
use rusqlite::params;
use crate::db::DbManager;

use log::warn;
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
pub fn get_backup_info(
    db: State<'_, Arc<DbManager>>,
) -> Result<Vec<BackupTableInfo>, String> {
    let counts = db.get_table_row_counts().map_err(|e| e.to_string())?;
    Ok(counts.into_iter().map(|(table_name, row_count)| {
        BackupTableInfo { table_name, row_count }
    }).collect())
}

/// Export database tables to a JSON string.
#[tauri::command]
pub fn export_backup(
    tables: Option<Vec<String>>,
    db: State<'_, Arc<DbManager>>,
) -> Result<String, String> {
    let all_tables = vec![
        "settings", "agents", "conversations", "messages", "skills", "memories",
        "agent_accounts", "custom_models", "model_platforms", "platform_models",
        "tasks", "cron_tasks", "cron_runs", "kb_documents", "kb_chunks",
        "kb_embeddings", "selection_history", "translation_history",
        "mcp_servers", "prompt_library", "search_providers", "search_history",
        "activity_log",
    ];
    let selected: Vec<&str> = if let Some(t) = &tables {
        all_tables.into_iter().filter(|name| t.iter().any(|s| s == name)).collect()
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
        source: "OMNIX DevFlow".to_string(),
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
    let backup: BackupExport = serde_json::from_str(&json_str)
        .map_err(|e| format!("Invalid backup format: {}", e))?;
    if backup.version != "1.0" {
        return Err(format!("Unsupported backup version: {}", backup.version));
    }

    let mut results = Vec::new();
    let mut total_rows = 0usize;

    for (table_name, data) in &backup.tables {
        if let Some(ref t) = tables {
            if !t.contains(table_name) { continue; }
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

/// Prompt library entry DTO
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PromptEntry {
    pub id: String,
    pub title: String,
    pub content: String,
    pub category: String,
    pub order_key: i32,
    pub created_at: String,
}

/// Get all prompt library entries.
#[tauri::command]
pub fn get_prompt_library(
    db: State<'_, Arc<DbManager>>,
) -> Result<Vec<PromptEntry>, String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let mut stmt = conn.prepare(
        "SELECT id, title, content, category, order_key, created_at FROM prompt_library ORDER BY category, order_key"
    ).map_err(|e| e.to_string())?;
    let rows = stmt.query_map([], |row| {
        Ok(PromptEntry {
            id: row.get::<_, String>(0)?,
            title: row.get::<_, String>(1)?,
            content: row.get::<_, String>(2)?,
            category: row.get::<_, String>(3)?,
            order_key: row.get::<_, i32>(4)?,
            created_at: row.get::<_, String>(5).unwrap_or_default(),
        })
    }).map_err(|e| e.to_string())?;
    let mut result = Vec::new();
    for r in rows {
        if let Ok(item) = r { result.push(item); }
    }
    Ok(result)
}

/// Save (upsert) a prompt library entry.
#[tauri::command]
pub fn save_prompt_entry(
    entry: PromptEntry,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO prompt_library (id, title, content, category, order_key) VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(id) DO UPDATE SET title=?2, content=?3, category=?4, order_key=?5",
        params![entry.id, entry.title, entry.content, entry.category, entry.order_key],
    ).map_err(|e| e.to_string())?;
    Ok(())
}

/// Delete a prompt library entry.
#[tauri::command]
pub fn delete_prompt_entry(
    id: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM prompt_library WHERE id = ?1", params![id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

// ── Activity Log ────────────────────────────────────────

/// Activity log entry DTO
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ActivityLogEntry {
    pub id: String,
    pub action: String,
    pub target: String,
    pub details: String,
    pub created_at: String,
}

/// Log an activity.
#[tauri::command]
pub fn log_activity(
    action: String,
    target: String,
    details: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let id = format!("act_{}", chrono::Utc::now().timestamp_millis());
    conn.execute(
        "INSERT INTO activity_log (id, action, target, details) VALUES (?1, ?2, ?3, ?4)",
        params![id, action, target, details],
    ).map_err(|e| e.to_string())?;
    Ok(())
}

/// Get recent activity log entries.
#[tauri::command]
pub fn get_activity_log(
    limit: u32,
    db: State<'_, Arc<DbManager>>,
) -> Result<Vec<ActivityLogEntry>, String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let mut stmt = conn.prepare(
        "SELECT id, action, target, details, created_at FROM activity_log ORDER BY created_at DESC LIMIT ?1"
    ).map_err(|e| e.to_string())?;
    let rows = stmt.query_map(params![limit], |row| {
        Ok(ActivityLogEntry {
            id: row.get::<_, String>(0)?,
            action: row.get::<_, String>(1)?,
            target: row.get::<_, String>(2)?,
            details: row.get::<_, String>(3)?,
            created_at: row.get::<_, String>(4).unwrap_or_default(),
        })
    }).map_err(|e| e.to_string())?;
    let mut result = Vec::new();
    for r in rows {
        if let Ok(item) = r { result.push(item); }
    }
    Ok(result)
}

// ══════════════════════════════════════════════════
// MCP Presets (ZCF inspired)
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
                    args: vec!["-y".into(), "@modelcontextprotocol/server-filesystem".into(), "/tmp".into()],
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
            servers: vec![
                McpPresetServer {
                    name: "sqlite".into(),
                    command: "npx".into(),
                    args: vec!["-y".into(), "@modelcontextprotocol/server-sqlite".into()],
                    env: std::collections::HashMap::new(),
                    url: String::new(),
                    server_type: "stdio".into(),
                    description: "SQLite database operations".into(),
                },
            ],
        },
        McpPreset {
            id: "search-tools".into(),
            name: "Search & Research".into(),
            description: "MCP servers for searching and research".into(),
            category: "search".into(),
            servers: vec![
                McpPresetServer {
                    name: "brave-search".into(),
                    command: "npx".into(),
                    args: vec!["-y".into(), "@modelcontextprotocol/server-brave-search".into()],
                    env: vec![("BRAVE_API_KEY".into(), "".into())].into_iter().collect(),
                    url: String::new(),
                    server_type: "stdio".into(),
                    description: "Web search via Brave Search API".into(),
                },
            ],
        },
        McpPreset {
            id: "memory".into(),
            name: "Knowledge & Memory".into(),
            description: "Persistent memory and knowledge management".into(),
            category: "productivity".into(),
            servers: vec![
                McpPresetServer {
                    name: "memory".into(),
                    command: "npx".into(),
                    args: vec!["-y".into(), "@modelcontextprotocol/server-memory".into()],
                    env: std::collections::HashMap::new(),
                    url: String::new(),
                    server_type: "stdio".into(),
                    description: "Persistent knowledge graph memory".into(),
                },
            ],
        },
    ]
}

/// Apply an MCP preset — adds all servers from the preset to the MCP servers table
#[tauri::command]
pub fn apply_mcp_preset(
    preset_id: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<u32, String> {
    let presets = get_mcp_presets();
    let preset = presets.iter().find(|p| p.id == preset_id)
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
// Output Styles (ZCF inspired)
// ══════════════════════════════════════════════════

/// Output style configuration for controlling response formatting
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OutputStyle {
    pub id: String,
    pub name: String,
    pub description: String,
    pub format: String,
    pub options: serde_json::Value,
}

/// Get all available output styles
#[tauri::command]
pub fn get_output_styles() -> Vec<OutputStyle> {
    vec![
        OutputStyle {
            id: "markdown".into(), name: "Markdown".into(),
            description: "Standard Markdown with headers, code blocks, emphasis".into(),
            format: "markdown".into(),
            options: serde_json::json!({"headings": true, "code_blocks": true}),
        },
        OutputStyle {
            id: "bullet".into(), name: "Bullet Points".into(),
            description: "Concise bullet-point format for quick scanning".into(),
            format: "bullet".into(),
            options: serde_json::json!({"max_bullets": 20, "max_depth": 2}),
        },
        OutputStyle {
            id: "table".into(), name: "Table".into(),
            description: "Structured table format for comparison and data".into(),
            format: "table".into(),
            options: serde_json::json!({"columns": "auto", "sort": false}),
        },
        OutputStyle {
            id: "json".into(), name: "JSON".into(),
            description: "Machine-readable JSON output format".into(),
            format: "json".into(),
            options: serde_json::json!({"pretty": true}),
        },
        OutputStyle {
            id: "compact".into(), name: "Compact".into(),
            description: "Minimal formatting, maximum density".into(),
            format: "compact".into(),
            options: serde_json::json!({"max_lines": 50, "abbreviate": true}),
        },
    ]
}

/// Get the system prompt fragment for a given output style
#[tauri::command]
pub fn get_output_style_prompt(style_id: String) -> Result<String, String> {
    let styles = get_output_styles();
    let style = styles.iter().find(|s| s.id == style_id)
        .ok_or_else(|| format!("Unknown output style: {}", style_id))?;

    let prompt = match style.format.as_str() {
        "markdown" => "Format your response using Markdown: use ## for sections, ``` for code blocks, **bold** for key terms. Include a brief summary at the top.",
        "bullet" => "Format your response as concise bullet points. Use - for items, indent for sub-points. Maximum 20 bullets, no more than 2 levels deep. Start with a one-line summary.",
        "table" => "Format your response as Markdown tables where applicable. Use | column | column | syntax. Include a summary row. If tabular format doesn't fit, fall back to bullet points.",
        "json" => "Format your response as valid JSON. Use a structured schema: {\"summary\": string, \"details\": [...], \"metadata\": {...}}. Make it machine-parseable.",
        "compact" => "Format your response for maximum density. No decorative formatting. Use abbreviations where clear. Maximum 50 lines. Skip elaboration, keep only actionable content.",
        _ => "Format your response clearly and concisely.",
    };
    Ok(prompt.to_string())
}

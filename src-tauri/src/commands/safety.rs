use tauri::{AppHandle, Emitter, State};
use std::sync::Arc;
use std::path::PathBuf;
use std::fs;
use rusqlite::params;
use crate::db::DbManager;
use super::*;

// ══════════════════════════════════════════════════
// Security & safety features — Tauri Commands
// ══════════════════════════════════════════════════

/// Wrap untrusted content in safety tags (Prompt Injection Guard)
#[tauri::command]
pub fn wrap_untrusted_content(content: String, source: String) -> String {
    crate::prompt_guard::wrap_untrusted(&content, &source)
}

/// Scan content for prompt injection patterns (Prompt Injection Guard — Layer 1)
#[tauri::command]
pub fn scan_prompt_injection(content: String) -> crate::prompt_guard::InjectionScanResult {
    crate::prompt_guard::scan_for_injection(&content)
}

// ── Development Checklist ─────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChecklistItem {
    pub id: String,
    pub session_id: String,
    pub title: String,
    pub status: String,
    pub priority: i32,
    pub source: String,
    pub created_at: String,
    pub completed_at: Option<String>,
}

#[tauri::command]
pub fn checklist_add(
    session_id: String, title: String, priority: Option<i32>, source: Option<String>,
    db: State<'_, Arc<DbManager>>,
) -> Result<ChecklistItem, String> {
    let conn = db.get_connection().map_err(|e: rusqlite::Error| e.to_string())?;
    // Table is created in init_schema — no runtime CREATE needed
    let id = format!("chk_{}", chrono::Utc::now().timestamp_millis());
    let src = source.unwrap_or_else(|| "manual".into());
    let pri = priority.unwrap_or(3);
    conn.execute(
        "INSERT INTO dev_checklist (id, session_id, title, priority, source) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![id, session_id, title, pri, src],
    ).map_err(|e: rusqlite::Error| e.to_string())?;
    Ok(ChecklistItem {
        id, session_id, title, status: "pending".into(), priority: pri, source: src,
        created_at: chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        completed_at: None,
    })
}

#[tauri::command]
pub fn checklist_update(item_id: String, status: String, db: State<'_, Arc<DbManager>>) -> Result<(), String> {
    let conn = db.get_connection().map_err(|e: rusqlite::Error| e.to_string())?;
    if status == "done" {
        conn.execute("UPDATE dev_checklist SET status = ?1, completed_at = datetime('now') WHERE id = ?2", params![status, item_id])
    } else {
        conn.execute("UPDATE dev_checklist SET status = ?1, completed_at = NULL WHERE id = ?2", params![status, item_id])
    }.map_err(|e: rusqlite::Error| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn checklist_get(
    session_id: Option<String>, include_done: Option<bool>,
    db: State<'_, Arc<DbManager>>,
) -> Result<Vec<ChecklistItem>, String> {
    let conn = db.get_connection().map_err(|e: rusqlite::Error| e.to_string())?;
    // Table is created in init_schema — no runtime CREATE needed
    let show_done = include_done.unwrap_or(true);
    let base = "SELECT id, session_id, title, status, priority, source, created_at, completed_at FROM dev_checklist";
    let sql = match (&session_id, show_done) {
        (Some(_), true) => format!("{} WHERE session_id = ?1 ORDER BY priority DESC, created_at ASC", base),
        (Some(_), false) => format!("{} WHERE session_id = ?1 AND status != 'done' ORDER BY priority DESC, created_at ASC", base),
        (None, true) => format!("{} ORDER BY priority DESC, created_at ASC", base),
        (None, false) => format!("{} WHERE status != 'done' ORDER BY priority DESC, created_at ASC", base),
    };
    let mut stmt = conn.prepare(&sql).map_err(|e: rusqlite::Error| e.to_string())?;
    let parse_row = |row: &rusqlite::Row| -> rusqlite::Result<ChecklistItem> {
        Ok(ChecklistItem {
            id: row.get(0)?, session_id: row.get(1)?, title: row.get(2)?, status: row.get(3)?,
            priority: row.get(4)?, source: row.get(5)?, created_at: row.get(6)?, completed_at: row.get(7)?,
        })
    };
    let rows = if let Some(ref sid) = session_id {
        stmt.query_map(params![sid], parse_row)
    } else {
        stmt.query_map([], parse_row)
    }.map_err(|e: rusqlite::Error| e.to_string())?;
    Ok(rows.flatten().collect())
}

#[tauri::command]
pub fn checklist_summary(session_id: String, db: State<'_, Arc<DbManager>>) -> Result<String, String> {
    let items = checklist_get(Some(session_id), Some(false), db)?;
    if items.is_empty() { return Ok(String::new()); }
    let mut s = String::from("You have the following incomplete tasks:\n");
    for item in &items {
        let icon = if item.status == "in_progress" { "[>]" } else { "[ ]" };
        let pri = match item.priority { 5 => "CRITICAL", 4 => "HIGH", 3 => "MEDIUM", 2 => "LOW", _ => "MINOR" };
        s.push_str(&format!("{} [{}] {} ({})\n", icon, pri, item.title, item.id));
    }
    Ok(s)
}

// ── Context Budget ────────────────────────────────

/// CJK-aware token estimate: ASCII text is ~4 chars/token, CJK ~2 chars/token.
/// Shared by `estimate_tokens` and `get_context_budget` so the meter is accurate.
pub fn estimate_text_tokens(text: &str) -> u32 {
    let ascii = text.chars().filter(|c| c.is_ascii()).count() as u32;
    let cjk = text.chars().filter(|c| !c.is_ascii()).count() as u32;
    ascii / 4 + cjk / 2
}

#[tauri::command]
pub fn estimate_tokens(text: String) -> u32 {
    estimate_text_tokens(&text) + 1
}

/// Context-window budget over the OMNIX-stored conversation transcript (the
/// `messages` table — i.e. what OMNIX would replay). Accurate as a measure of
/// the stored transcript; per-message tokens use the CJK-aware estimate.
#[tauri::command]
pub fn get_context_budget(
    conversation_id: String, model_context_limit: Option<u32>,
    db: State<'_, Arc<DbManager>>,
) -> Result<serde_json::Value, String> {
    let conn = db.get_connection().map_err(|e: rusqlite::Error| e.to_string())?;
    let limit = model_context_limit.unwrap_or(128000);
    let mut stmt = conn
        .prepare("SELECT content FROM messages WHERE conversation_id = ?1")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![conversation_id], |r| r.get::<_, String>(0))
        .map_err(|e| e.to_string())?;
    let mut est: u32 = 0;
    let mut count: u32 = 0;
    for content in rows.flatten() {
        est = est.saturating_add(estimate_text_tokens(&content));
        count += 1;
    }
    // Small per-message structural overhead (role markers, separators).
    est = est.saturating_add(count.saturating_mul(4));
    let remaining = limit.saturating_sub(est);
    let pct = if limit > 0 { est as f64 / limit as f64 * 100.0 } else { 0.0 };
    Ok(serde_json::json!({
        "model_limit": limit, "estimated_tokens": est, "message_count": count,
        "remaining_tokens": remaining, "usage_percent": (pct * 100.0).round() / 100.0,
        "status": if pct > 90.0 { "critical" } else if pct > 70.0 { "warning" } else { "ok" },
    }))
}

/// Compact conversation context — summarize old messages and keep recent ones.
/// Returns the number of messages compacted.
#[tauri::command]
pub fn compact_conversation_context(
    conversation_id: String,
    keep_recent: Option<usize>,
    db: State<'_, Arc<DbManager>>,
) -> Result<serde_json::Value, String> {
    let keep = keep_recent.unwrap_or(20);
    let conn = db.get_connection().map_err(|e: rusqlite::Error| e.to_string())?;

    // Get total message count
    let total: i64 = conn.query_row(
        "SELECT COUNT(*) FROM messages WHERE conversation_id = ?1",
        params![conversation_id], |r| r.get(0),
    ).unwrap_or(0);

    if total <= keep as i64 {
        return Ok(serde_json::json!({
            "compacted": 0,
            "total": total,
            "summary": null,
            "message": "Not enough messages to compact"
        }));
    }

    // Get old messages (to be summarized)
    let mut stmt = conn.prepare(
        "SELECT role, content FROM messages WHERE conversation_id = ?1
         ORDER BY timestamp ASC LIMIT ?2"
    ).map_err(|e: rusqlite::Error| e.to_string())?;
    let cutoff = total - keep as i64;
    let old_messages: Vec<(String, String)> = stmt.query_map(
        params![conversation_id, cutoff as i32],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
    ).map_err(|e: rusqlite::Error| e.to_string())?.flatten().collect();

    // Build summary from old messages
    let mut summary_parts = Vec::new();
    for (role, content) in &old_messages {
        let truncated: String = content.chars().take(200).collect();
        summary_parts.push(format!("[{}]: {}", role, truncated));
    }
    let summary = format!(
        "=== CONVERSATION SUMMARY ({} older messages compacted) ===\n{}\n=== END SUMMARY ===",
        old_messages.len(),
        summary_parts.join("\n")
    );

    // Delete old messages
    conn.execute(
        "DELETE FROM messages WHERE conversation_id = ?1 AND id NOT IN (
            SELECT id FROM messages WHERE conversation_id = ?1 ORDER BY timestamp DESC LIMIT ?2
        )",
        params![conversation_id, keep as i32],
    ).map_err(|e: rusqlite::Error| e.to_string())?;

    // Insert summary as first message
    let summary_id = format!("summary_{}", chrono::Utc::now().timestamp_millis());
    conn.execute(
        "INSERT INTO messages (id, conversation_id, role, content, timestamp) VALUES (?1, ?2, 'system', ?3, datetime('now'))",
        params![summary_id, conversation_id, summary],
    ).map_err(|e: rusqlite::Error| e.to_string())?;

    Ok(serde_json::json!({
        "compacted": old_messages.len(),
        "total": keep as i64 + 1,
        "summary": summary,
        "message": format!("Compacted {} messages into summary", old_messages.len())
    }))
}

// ── Skill Audit ───────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillAuditResult {
    pub skill_name: String, pub score: u32, pub issues: Vec<String>,
    pub suggestion: String, pub auto_fixed: bool,
}

#[tauri::command]
pub fn run_skill_audit(db: State<'_, Arc<DbManager>>) -> Result<Vec<SkillAuditResult>, String> {
    let conn = db.get_connection().map_err(|e: rusqlite::Error| e.to_string())?;
    let _ = conn.execute(
        "CREATE TABLE IF NOT EXISTS skill_audit_log (id INTEGER PRIMARY KEY AUTOINCREMENT, skill_name TEXT, score INTEGER, issues TEXT, audited_at DATETIME DEFAULT CURRENT_TIMESTAMP)", [],
    );
    let mut stmt = conn.prepare("SELECT name, file_path FROM skills WHERE is_active = 1")
        .map_err(|e: rusqlite::Error| e.to_string())?;
    let skills: Vec<(String, String)> = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    }).map_err(|e: rusqlite::Error| e.to_string())?.flatten().collect();
    let mut results = Vec::new();
    for (name, file_path) in skills {
        let mut core_path = PathBuf::from(&file_path);
        core_path.set_file_name(format!("{}_core.md", name));
        let content = match fs::read_to_string(&core_path) { Ok(c) => c, Err(_) => continue };
        let mut issues = Vec::new();
        let mut score: u32 = 10;
        if content.len() < 100 { issues.push("Content too short".into()); score -= 3; }
        if !content.contains('#') { issues.push("No headings".into()); score -= 2; }
        if !content.contains("```") && content.len() > 500 { issues.push("No code blocks".into()); score -= 1; }
        if content.contains("TODO") || content.contains("FIXME") { issues.push("Has TODO/FIXME".into()); score -= 1; }
        let issues_str = issues.join("; ");
        let _ = conn.execute("INSERT INTO skill_audit_log (skill_name, score, issues) VALUES (?1, ?2, ?3)", params![name, score.max(1), issues_str]);
        results.push(SkillAuditResult {
            skill_name: name, score: score.max(1), issues,
            suggestion: if score < 7 { "Expand with more instructions".into() } else { "Quality OK".into() },
            auto_fixed: false,
        });
    }
    Ok(results)
}

// ── Event Bus ─────────────────────────────────────

#[tauri::command]
pub fn register_event_trigger(event_type: String, threshold: u32, task_id: String, db: State<'_, Arc<DbManager>>) -> Result<String, String> {
    crate::event_bus::register_trigger(&db, &event_type, threshold, &task_id)
}

#[tauri::command]
pub fn get_event_triggers(db: State<'_, Arc<DbManager>>) -> Vec<crate::event_bus::EventTrigger> {
    crate::event_bus::list_triggers(&db)
}

// ── Encryption ────────────────────────────────────

#[tauri::command]
pub fn encrypt_value(plaintext: String) -> String {
    crate::crypto::encrypt(&plaintext)
}

#[tauri::command]
pub fn decrypt_value(encrypted: String) -> String {
    crate::crypto::decrypt(&encrypted)
}

// ── Desktop Notification ──────────────────────────

#[tauri::command]
pub fn send_desktop_notification(title: String, body: String, app_handle: AppHandle) -> Result<(), String> {
    app_handle.emit("omnix-notification", serde_json::json!({ "title": title, "body": body }))
        .map_err(|e| e.to_string())?;
    Ok(())
}

// ── ntfy Push ─────────────────────────────────────

#[tauri::command]
pub async fn send_ntfy_notification(
    server: String, topic: String, title: String, message: String, priority: Option<String>,
) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());
    let pri = priority.unwrap_or_else(|| "default".into());
    let res = client.post(format!("{}/{}", server.trim_end_matches('/'), topic))
        .header("Title", &title).header("Priority", &pri)
        .body(message).send().await
        .map_err(|e| format!("ntfy request failed: {}", e))?;
    if !res.status().is_success() { return Err(format!("ntfy HTTP {}", res.status())); }
    Ok(())
}

// ══════════════════════════════════════════════════
// Cookbook Model Recommendation
// ══════════════════════════════════════════════════

/// Detect hardware and recommend models that fit
#[tauri::command]
pub fn get_model_recommendations() -> serde_json::Value {
    let hw = crate::model_knowledge::detect_hardware();
    let recommendations = crate::model_knowledge::recommend_models(&hw);
    serde_json::json!({
        "hardware": hw,
        "recommendations": recommendations,
    })
}

/// Get the full model knowledge base
#[tauri::command]
pub fn get_model_database() -> Vec<crate::model_knowledge::ModelEntry> {
    crate::model_knowledge::get_model_database()
}

/// Simulate recommendations for a hypothetical GPU
#[tauri::command]
pub fn recommend_for_gpu(gpu_name: String) -> Result<serde_json::Value, String> {
    let recommendations = crate::model_knowledge::recommend_for_gpu(&gpu_name)?;
    let gpu = crate::model_knowledge::simulate_gpu(&gpu_name);
    Ok(serde_json::json!({
        "gpu": gpu,
        "recommendations": recommendations,
    }))
}

/// Get the full GPU database
#[tauri::command]
pub fn get_gpu_database() -> Vec<crate::model_knowledge::GpuSpec> {
    crate::model_knowledge::get_gpu_database()
}

// ══════════════════════════════════════════════════
// Code Deep Analysis
// ══════════════════════════════════════════════════

/// Analyze a codebase directory — returns file statistics and structure
#[tauri::command]
pub fn analyze_codebase(path: String) -> Result<serde_json::Value, String> {
    let dir = PathBuf::from(&path);
    if !dir.exists() || !dir.is_dir() {
        return Err(format!("Path does not exist or is not a directory: {}", path));
    }

    let mut file_count = 0u32;
    let mut total_lines = 0u32;
    let mut languages: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
    let mut largest_files: Vec<(String, u64)> = Vec::new();

    fn walk_dir(
        dir: &PathBuf,
        file_count: &mut u32,
        total_lines: &mut u32,
        languages: &mut std::collections::HashMap<String, u32>,
        largest_files: &mut Vec<(String, u64)>,
    ) {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    let name = path.file_name().unwrap_or_default().to_string_lossy();
                    // Skip common non-source directories
                    if name == "node_modules" || name == ".git" || name == "target" || name == "dist" || name == ".next" {
                        continue;
                    }
                    walk_dir(&path, file_count, total_lines, languages, largest_files);
                } else {
                    *file_count += 1;
                    let size = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                    let name = path.to_string_lossy().to_string();

                    // Track largest files
                    largest_files.push((name.clone(), size));
                    if largest_files.len() > 100 {
                        largest_files.sort_by(|a, b| b.1.cmp(&a.1));
                        largest_files.truncate(50);
                    }

                    // Count lines for text files
                    if let Ok(content) = fs::read_to_string(&path) {
                        *total_lines += content.lines().count() as u32;
                    }

                    // Detect language by extension
                    if let Some(ext) = path.extension() {
                        let ext_str = ext.to_string_lossy().to_lowercase();
                        let lang = match ext_str.as_str() {
                            "rs" => "Rust",
                            "ts" | "tsx" => "TypeScript",
                            "js" | "jsx" => "JavaScript",
                            "py" => "Python",
                            "go" => "Go",
                            "java" => "Java",
                            "cpp" | "cc" | "cxx" => "C++",
                            "c" => "C",
                            "cs" => "C#",
                            "rb" => "Ruby",
                            "swift" => "Swift",
                            "kt" => "Kotlin",
                            "html" | "htm" => "HTML",
                            "css" | "scss" | "sass" => "CSS",
                            "json" => "JSON",
                            "md" => "Markdown",
                            "yaml" | "yml" => "YAML",
                            "toml" => "TOML",
                            "sql" => "SQL",
                            _ => "Other",
                        };
                        *languages.entry(lang.to_string()).or_insert(0) += 1;
                    }
                }
            }
        }
    }

    walk_dir(&dir, &mut file_count, &mut total_lines, &mut languages, &mut largest_files);
    largest_files.sort_by(|a, b| b.1.cmp(&a.1));
    largest_files.truncate(20);

    Ok(serde_json::json!({
        "path": path,
        "total_files": file_count,
        "total_lines": total_lines,
        "languages": languages,
        "largest_files": largest_files.iter().map(|(name, size)| {
            serde_json::json!({ "name": name, "size_bytes": size })
        }).collect::<Vec<_>>(),
    }))
}

// ══════════════════════════════════════════════════
// Configuration Backup
// ══════════════════════════════════════════════════

/// Backup a file before modification
#[tauri::command]
pub fn backup_config_file(file_path: String, category: String) -> Result<Option<String>, String> {
    let path = PathBuf::from(&file_path);
    crate::backup::backup_file(&path, &category).map(|p| p.map(|p| p.to_string_lossy().to_string()))
}

/// List backups for a category
#[tauri::command]
pub fn list_backups(category: String) -> Vec<crate::backup::BackupEntry> {
    crate::backup::list_backups(&category)
}

/// Restore a backup
#[tauri::command]
pub fn restore_backup(backup_path: String, target_path: String) -> Result<(), String> {
    crate::backup::restore_backup(&backup_path, &target_path)
}

// ══════════════════════════════════════════════════
// API Provider Preset Management
// ══════════════════════════════════════════════════

/// Apply an API provider preset — creates or updates a model platform
#[tauri::command]
pub fn apply_api_preset(
    preset_id: String,
    api_key: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<String, String> {
    // Preset definitions (mirrored from frontend constants)
    let presets: Vec<(&str, &str, &str, &str, &str)> = vec![
        ("openai",        "OpenAI",              "openai",    "https://api.openai.com/v1",                       "gpt-4o"),
        ("anthropic",     "Anthropic",           "anthropic", "https://api.anthropic.com",                       "claude-sonnet-4-20250514"),
        ("openrouter",    "OpenRouter",          "openai",    "https://openrouter.ai/api/v1",                    "anthropic/claude-sonnet-4-20250514"),
        ("deepseek",      "DeepSeek",            "openai",    "https://api.deepseek.com/v1",                     "deepseek-chat"),
        ("siliconflow",   "硅基流动 SiliconFlow", "openai",    "https://api.siliconflow.cn/v1",                   "Qwen/Qwen2.5-7B-Instruct"),
        ("zhipu",         "智谱 GLM",            "openai",    "https://open.bigmodel.cn/api/paas/v4",            "glm-4-flash"),
        ("moonshot",      "月之暗面 Kimi",       "openai",    "https://api.moonshot.cn/v1",                      "moonshot-v1-8k"),
        ("minimax",       "MiniMax",             "openai",    "https://api.minimax.chat/v1",                     "MiniMax-Text-01"),
        ("bailian",       "百炼 Bailian",        "openai",    "https://dashscope.aliyuncs.com/compatible-mode/v1","qwen-plus"),
        ("volcengine",    "火山引擎",            "openai",    "https://ark.cn-beijing.volces.com/api/v3",        "doubao-pro-32k"),
        ("ollama",        "Ollama (本地)",        "ollama",    "http://localhost:11434",                           "qwen2.5:7b"),
        ("lmstudio",      "LM Studio (本地)",    "openai",    "http://localhost:1234/v1",                         "local-model"),
    ];

    let preset = presets.iter().find(|(id, _, _, _, _)| *id == preset_id)
        .ok_or_else(|| format!("Unknown preset: {}", preset_id))?;

    let (id, name, api_type, api_address, default_model) = *preset;
    let conn = db.get_connection().map_err(|e: rusqlite::Error| e.to_string())?;

    // Check if platform already exists
    let exists: bool = conn.query_row(
        "SELECT COUNT(*) FROM model_platforms WHERE id = ?1",
        params![id],
        |r| r.get::<_, i64>(0),
    ).unwrap_or(0) > 0;

    if exists {
        // Update existing
        conn.execute(
            "UPDATE model_platforms SET api_key = ?1, api_address = ?2, api_type = ?3, is_enabled = 1 WHERE id = ?4",
            params![api_key, api_address, api_type, id],
        ).map_err(|e: rusqlite::Error| e.to_string())?;
    } else {
        // Insert new
        conn.execute(
            "INSERT INTO model_platforms (id, name, api_type, api_key, api_address, is_enabled, weight, priority) VALUES (?1, ?2, ?3, ?4, ?5, 1, 1, 0)",
            params![id, name, api_type, api_key, api_address],
        ).map_err(|e: rusqlite::Error| e.to_string())?;

        // Add default model
        let model_id = format!("{}:{}", id, default_model);
        conn.execute(
            "INSERT OR IGNORE INTO platform_models (id, platform_id, model_name, is_enabled) VALUES (?1, ?2, ?3, 1)",
            params![model_id, id, default_model],
        ).map_err(|e: rusqlite::Error| e.to_string())?;
    }

    Ok(format!("{}: {}", name, if exists { "已更新" } else { "已添加" }))
}

// ══════════════════════════════════════════════════
// Architecture Knowledge Graph
// ══════════════════════════════════════════════════

/// Build architecture graph for a project directory
#[tauri::command]
pub fn build_architecture_graph(project_path: String) -> Result<crate::code_graph::ArchitectureGraph, String> {
    crate::code_graph::build_graph(&project_path)
}

/// Save architecture graph to disk
#[tauri::command]
pub fn save_architecture_graph(graph: crate::code_graph::ArchitectureGraph) -> Result<String, String> {
    crate::code_graph::save_graph(&graph)
}

/// Load a saved architecture graph
#[tauri::command]
pub fn load_architecture_graph(project_name: String) -> Result<crate::code_graph::ArchitectureGraph, String> {
    crate::code_graph::load_graph(&project_name)
}

/// Get .omnixignore patterns for a project
#[tauri::command]
pub fn get_ignore_patterns(project_path: String) -> Vec<String> {
    let path = PathBuf::from(&project_path);
    crate::code_graph::load_omnixignore(&path)
}

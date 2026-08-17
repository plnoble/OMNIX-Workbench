use tauri::{AppHandle, Emitter, State};
use tokio::io::AsyncBufReadExt;
use crate::proc::NoWindow;
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

/// 每条旧消息在摘录里保留的字符数。
const DIGEST_CHARS_PER_MESSAGE: usize = 200;

/// 压缩前把原文写到备份目录，返回文件路径。
///
/// 复用 `storage::backups_dir()`（用户可在「设置 → 存储位置」改），不新起一套目录。
/// 返回 `Err` 时调用方必须中止压缩——没有备份就不能删。
fn write_compaction_backup(
    conversation_id: &str,
    messages: &[(String, String)],
) -> Result<PathBuf, String> {
    let dir = crate::storage::backups_dir().join("compaction");
    fs::create_dir_all(&dir).map_err(|e| format!("建备份目录失败：{e}"))?;
    let stamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
    // 会话 id 由 OMNIX 生成（`conv_*`），但仍按路径分量校验一次——它要拼进文件名。
    crate::input_validation::validate_path_component(conversation_id, "conversation_id")?;
    let path = dir.join(format!("{conversation_id}_{stamp}.json"));
    let payload = serde_json::json!({
        "conversation_id": conversation_id,
        "compacted_at": chrono::Local::now().to_rfc3339(),
        "messages": messages.iter().map(|(role, content)| {
            serde_json::json!({ "role": role, "content": content })
        }).collect::<Vec<_>>(),
    });
    fs::write(&path, serde_json::to_string_pretty(&payload).unwrap_or_default())
        .map_err(|e| format!("写备份失败：{e}"))?;
    Ok(path)
}

/// 压缩对话上下文：把早期消息换成一份**截断摘录**，只保留最近若干条。
///
/// **不是 AI 摘要**——每条只留前 200 字后拼接。原文在删除前会写进
/// `<备份目录>/compaction/`，所以这个操作是可逆的（手工恢复）。
#[tauri::command]
pub fn compact_conversation_context(
    conversation_id: String,
    keep_recent: Option<usize>,
    db: State<'_, Arc<DbManager>>,
) -> Result<serde_json::Value, String> {
    compact_core(&db, &conversation_id, keep_recent.unwrap_or(20))
}

pub(crate) fn compact_core(
    db: &DbManager,
    conversation_id: &str,
    keep: usize,
) -> Result<serde_json::Value, String> {
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

    // 删之前先把原文落盘。
    //
    // 这一步以前没有：旧消息被 DELETE 掉，只留下每条前 200 字的拼接，**不可恢复**。
    // 截断本身是有意的（要的就是把上下文缩小），但「缩小」不该等于「销毁」——
    // 备份让这个操作变成可逆的，代价只是一个 JSON 文件。
    //
    // 写失败就**中止整个压缩**：宁可这次没压成，也不能在没有退路的情况下删。
    let backup_path = write_compaction_backup(conversation_id, &old_messages)?;

    // 把旧消息压成「每条前 200 字」的清单。
    //
    // **这不是摘要**——没有模型参与，就是截断后拼接。函数名和界面文案都必须这么说，
    // 否则用户以为拿到的是浓缩过的要点，实际拿到的是一堆残句。
    let mut digest_parts = Vec::new();
    for (role, content) in &old_messages {
        let truncated: String = content.chars().take(DIGEST_CHARS_PER_MESSAGE).collect();
        let elided = content.chars().count() > DIGEST_CHARS_PER_MESSAGE;
        digest_parts.push(format!(
            "[{}]: {}{}",
            role,
            truncated,
            if elided { "…（已截断）" } else { "" }
        ));
    }
    let summary = format!(
        "=== 早期对话摘录（{} 条，每条保留前 {} 字，非摘要）===\n{}\n=== 原文备份：{} ===",
        old_messages.len(),
        DIGEST_CHARS_PER_MESSAGE,
        digest_parts.join("\n"),
        backup_path.display()
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
        // 这里曾把每条结果也写进 skill_audit_log。那张表**没有任何读取方**——审计
        // 结果是靠这个函数的返回值到界面的（SkillTab 的「质量审计」按钮直接用
        // `skillAuditApi.run()` 的返回），表里那份从来没人看过。
        //
        // 313 个技能点一次审计就是 313 行，纯浪费。删写入、留表：删表不可逆，而一张
        // 空表无害（同 success_count / priority_score 那两列的处置）。
        results.push(SkillAuditResult {
            skill_name: name, score: score.max(1), issues,
            suggestion: if score < 7 { "Expand with more instructions".into() } else { "Quality OK".into() },
            auto_fixed: false,
        });
    }
    Ok(results)
}

// ── Event Bus ─────────────────────────────────────

// 这里曾经把 `encrypt_value` / `decrypt_value` 两个命令暴露给前端，零消费方。
// 删掉不是因为它没人用，是因为**它不该存在**：渲染进程一旦被注入脚本，就能
// 逐个解出库里所有密文。v0.28.0 刚把加密密钥绑到 Windows 账号（DPAPI），
// 再从前端开一个万能解密口子方向正好相反。需要密文的地方都在后端自己解。

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

/// 检测硬件并给出推荐。
///
/// 先尝试刷新远程目录（失败静默，内置副本兜底），再把**本机已装但不在目录里**
/// 的 Ollama 模型并进来——用户自己 pull 过的东西不该在这一页凭空消失。
#[tauri::command]
pub async fn get_model_recommendations() -> serde_json::Value {
    // 拉不到就用内置副本，不打断这次请求。
    if let Err(error) = crate::model_knowledge::refresh_remote_catalog(None).await {
        log::debug!("模型目录用内置副本：{error}");
    }

    let hw = crate::model_knowledge::detect_hardware();
    let mut recommendations = crate::model_knowledge::recommend_models(&hw);

    let known: std::collections::HashSet<String> = recommendations
        .iter()
        .map(|r| r.model.name.clone())
        .collect();
    for installed in list_installed_ollama_models().await.unwrap_or_default() {
        if !known.contains(&installed) {
            recommendations.push(crate::model_knowledge::entry_for_installed(&installed, &hw));
        }
    }

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

/// 统计一个代码库：文件数、行数、语言分布、最大的若干文件。
///
/// 三处以前会出事的地方：
///
/// 1. **软链环导致无限递归。** 原来用 `path.is_dir()` 判断——它**跟随**软链，
///    于是工作区里一个指向祖先目录的软链（或 Windows junction）就会让 `walk_dir`
///    一路递归到爆栈。现在用 `entry.file_type()`（不跟随软链）明确排除软链，
///    另外加一道深度上限兜底：判类型这一层万一被绕过，深度也拦得住。
/// 2. **为数行数把整个文件读进内存。** `read_to_string` 碰上一个几百 MB 的日志或
///    数据集就是几百 MB 驻留。改成流式数换行符，并对超大文件直接跳过计行——
///    它对「这个库多大」这个问题没有信息量，却能拖垮统计本身。
/// 3. **输出绝对路径。** 泄露本机目录结构，界面上也是一长串没法看。改成相对根目录。
#[tauri::command]
pub fn analyze_codebase(path: String) -> Result<serde_json::Value, String> {
    crate::input_validation::validate_workspace_path(&path, "path")?;
    let dir = PathBuf::from(&path);
    if !dir.exists() || !dir.is_dir() {
        return Err(format!("Path does not exist or is not a directory: {}", path));
    }
    let stats = scan_codebase(&dir);
    Ok(serde_json::json!({
        "path": path,
        "total_files": stats.file_count,
        "total_lines": stats.total_lines,
        "languages": stats.languages,
        "largest_files": stats.largest_files.iter().map(|(name, size)| {
            serde_json::json!({ "name": name, "size_bytes": size })
        }).collect::<Vec<_>>(),
    }))
}

/// 递归深度上限。软链已经在类型判断那层挡掉了，这是第二道闸——真实代码库不会有
/// 这么深的目录，撞到上限本身就说明碰上了病态结构。
const MAX_SCAN_DEPTH: usize = 64;

/// 超过这个大小就不数行数了（仍然计入文件数和「最大文件」榜）。
const MAX_LINE_COUNT_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Default)]
pub(crate) struct CodebaseStats {
    pub file_count: u32,
    pub total_lines: u64,
    pub languages: std::collections::HashMap<String, u32>,
    /// (相对根目录的路径, 字节数)，按大小降序，最多 20 条。
    pub largest_files: Vec<(String, u64)>,
}

/// 流式数行数：按换行符计，不把文件读进内存。读不出 UTF-8（二进制）时返回 0。
fn count_lines(path: &PathBuf) -> u64 {
    use std::io::{BufRead, BufReader};
    let Ok(file) = fs::File::open(path) else { return 0 };
    let mut reader = BufReader::new(file);
    let mut lines = 0u64;
    let mut buf = Vec::new();
    loop {
        buf.clear();
        match reader.read_until(b'\n', &mut buf) {
            Ok(0) => break,
            Ok(_) => lines += 1,
            Err(_) => break, // 读坏了就按已数到的算，不让统计整个失败
        }
    }
    lines
}

fn language_of(ext: &str) -> &'static str {
    match ext {
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
    }
}

/// 不参与统计的目录：构建产物和依赖树。它们的体量会把真实代码淹没。
fn is_skipped_dir(name: &str) -> bool {
    matches!(
        name,
        "node_modules" | ".git" | "target" | "dist" | ".next"
            | ".venv" | "venv" | "__pycache__" | "vendor" | "build" | "out" | "coverage"
    )
}

pub(crate) fn scan_codebase(root: &PathBuf) -> CodebaseStats {
    let mut stats = CodebaseStats::default();
    walk(root, root, 0, &mut stats);
    stats.largest_files.sort_by(|a, b| b.1.cmp(&a.1));
    stats.largest_files.truncate(20);
    stats
}

fn walk(root: &PathBuf, dir: &PathBuf, depth: usize, stats: &mut CodebaseStats) {
    if depth > MAX_SCAN_DEPTH {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        // `entry.file_type()` **不跟随**软链——这是防环的第一道，也是主要一道。
        let Ok(file_type) = entry.file_type() else { continue };
        if file_type.is_symlink() {
            continue;
        }
        let path = entry.path();
        if file_type.is_dir() {
            let name = entry.file_name().to_string_lossy().to_string();
            if is_skipped_dir(&name) {
                continue;
            }
            walk(root, &path, depth + 1, stats);
            continue;
        }

        stats.file_count += 1;
        let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
        let rel = path.strip_prefix(root).unwrap_or(&path).to_string_lossy().to_string();

        stats.largest_files.push((rel, size));
        if stats.largest_files.len() > 100 {
            stats.largest_files.sort_by(|a, b| b.1.cmp(&a.1));
            stats.largest_files.truncate(50);
        }

        if size <= MAX_LINE_COUNT_BYTES {
            stats.total_lines += count_lines(&path);
        }

        if let Some(ext) = path.extension() {
            let lang = language_of(&ext.to_string_lossy().to_lowercase());
            *stats.languages.entry(lang.to_string()).or_insert(0) += 1;
        }
    }
}

// ══════════════════════════════════════════════════
// Configuration Backup
// ══════════════════════════════════════════════════

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
    apply_api_preset_core(&db, &preset_id, &api_key)
}

/// 命令体本身。抽出来是为了能测——`State<…>` 在单测里构造不出来。
pub(crate) fn apply_api_preset_core(
    db: &DbManager,
    preset_id: &str,
    api_key: &str,
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

    // Key **不写** `model_platforms.api_key`。那一列是遗留列：`platform_api_keys`
    // 才是现在的存放处，而且存的是密文。这里以前直接把明文 UPDATE/INSERT 进旧列，
    // 同时坏两件事——绕开加密策略；以及平台一旦已有 `platform_api_keys` 行，
    // 运行时根本不读那一列，预设 Key「保存成功、实际没用」。
    if exists {
        conn.execute(
            "UPDATE model_platforms SET api_address = ?1, api_type = ?2, is_enabled = 1 WHERE id = ?3",
            params![api_address, api_type, id],
        ).map_err(|e: rusqlite::Error| e.to_string())?;
    } else {
        // 旧列是 NOT NULL，必须显式给空串——省略它会直接违反约束。
        // 它只是遗留列，真正的 Key 在下面写进 `platform_api_keys`。
        conn.execute(
            "INSERT INTO model_platforms (id, name, api_type, api_key, api_address, is_enabled, weight, priority) VALUES (?1, ?2, ?3, '', ?4, 1, 1, 0)",
            params![id, name, api_type, api_address],
        ).map_err(|e: rusqlite::Error| e.to_string())?;

        // Add default model
        let model_id = format!("{}:{}", id, default_model);
        conn.execute(
            "INSERT OR IGNORE INTO platform_models (id, platform_id, model_name, is_enabled) VALUES (?1, ?2, ?3, 1)",
            params![model_id, id, default_model],
        ).map_err(|e: rusqlite::Error| e.to_string())?;
    }

    // Key 进加密表。给的是空串就只建平台不建 Key（用户可能只想先加上地址）。
    // Key 进加密表，**不写** `model_platforms.api_key`。那一列是遗留列：
    // 直接写明文会绕开加密策略，而且平台一旦已有 `platform_api_keys` 行，
    // 运行时根本不读那一列——预设 Key 会「保存成功、实际没用」。
    // ollama 这类本地平台不需要 Key，空串就不建条目。
    let trimmed_key = api_key.trim();
    if !trimmed_key.is_empty() {
        let already_active: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM platform_api_keys WHERE platform_id = ?1 AND is_active = 1",
                params![id],
                |r| r.get(0),
            )
            .unwrap_or(0);
        let key_id = format!("key_preset_{}_{}", id, chrono::Utc::now().timestamp_millis());
        conn.execute(
            "INSERT INTO platform_api_keys (id, platform_id, encrypted_key, label, is_active)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                key_id,
                id,
                crate::crypto::encrypt(trimmed_key),
                "一键预设",
                // 已经有活跃 Key 就不抢——用户在多 Key 里选过的那个应当保持生效。
                if already_active == 0 { 1 } else { 0 }
            ],
        )
        .map_err(|e: rusqlite::Error| e.to_string())?;
    }

    Ok(format!("{}: {}", name, if exists { "已更新" } else { "已添加" }))
}

// ══════════════════════════════════════════════════
// Architecture Knowledge Graph
// ══════════════════════════════════════════════════




// ══════════════════════════════════════════════════
// 本地模型下载（ollama pull）
// ══════════════════════════════════════════════════

/// 正在跑的 `ollama pull`，按模型名索引，用来取消。
static PULLING: std::sync::OnceLock<
    std::sync::Mutex<std::collections::HashMap<String, tokio::sync::oneshot::Sender<()>>>,
> = std::sync::OnceLock::new();

fn pulling() -> &'static std::sync::Mutex<
    std::collections::HashMap<String, tokio::sync::oneshot::Sender<()>>,
> {
    PULLING.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

/// 下载一个本地模型。
///
/// 「本地模型选型」以前只会给建议——看得到、装不了，用户还得自己开终端敲
/// `ollama pull`。这里把最后一步补上：跑真正的拉取，进度按行推给前端，可中途取消。
///
/// 进度事件：`local-model-pull`，payload `{ model, line, done, ok }`。
#[tauri::command]
pub async fn pull_local_model(app: tauri::AppHandle, model: String) -> Result<(), String> {
    crate::input_validation::validate_path_component(&model.replace(':', "_"), "模型名")
        .map_err(|_| format!("模型名不合法：{model}"))?;

    {
        let guard = pulling().lock().map_err(|_| "内部状态锁失败".to_string())?;
        if guard.contains_key(&model) {
            return Err(format!("{model} 正在下载中"));
        }
    }

    let mut child = tokio::process::Command::new("ollama")
        .arg("pull")
        .arg(&model)
        .no_window()
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|error| {
            format!("启动 ollama 失败（本机可能没装 Ollama）：{error}")
        })?;

    let (cancel_tx, mut cancel_rx) = tokio::sync::oneshot::channel::<()>();
    {
        let mut guard = pulling().lock().map_err(|_| "内部状态锁失败".to_string())?;
        guard.insert(model.clone(), cancel_tx);
    }

    // ollama 把进度写在 stderr（回车刷新的进度条），stdout 只有零星几行。
    //
    // `lines()` 只按 `\n` 切，而进度条整段刷新都靠 `\r`——所以一「行」里可能塞着
    // 几十个历史状态，直接发给前端会显示成一条几百字的乱码。只取最后一段：
    // 那就是**当前**进度。
    let emit = |app: &tauri::AppHandle, model: &str, line: String| {
        let current = line.rsplit('\r').find(|s| !s.trim().is_empty()).unwrap_or("").trim();
        if current.is_empty() {
            return;
        }
        let _ = app.emit(
            "local-model-pull",
            serde_json::json!({ "model": model, "line": current, "done": false, "ok": false }),
        );
    };
    for pipe in [child.stdout.take().map(Either::Out), child.stderr.take().map(Either::Err)]
        .into_iter()
        .flatten()
    {
        let app = app.clone();
        let model = model.clone();
        tokio::spawn(async move {
            match pipe {
                Either::Out(out) => {
                    let mut lines = tokio::io::BufReader::new(out).lines();
                    while let Ok(Some(line)) = lines.next_line().await {
                        emit(&app, &model, line);
                    }
                }
                Either::Err(err) => {
                    let mut lines = tokio::io::BufReader::new(err).lines();
                    while let Ok(Some(line)) = lines.next_line().await {
                        emit(&app, &model, line);
                    }
                }
            }
        });
    }

    let status = tokio::select! {
        result = child.wait() => result.map_err(|e| e.to_string())?,
        _ = &mut cancel_rx => {
            let _ = child.kill().await;
            pulling().lock().ok().map(|mut g| g.remove(&model));
            let _ = app.emit(
                "local-model-pull",
                serde_json::json!({ "model": model, "line": "已取消", "done": true, "ok": false }),
            );
            return Err("已取消".into());
        }
    };

    pulling().lock().ok().map(|mut g| g.remove(&model));
    let ok = status.success();
    let _ = app.emit(
        "local-model-pull",
        serde_json::json!({
            "model": model,
            "line": if ok { "下载完成" } else { "下载失败" },
            "done": true,
            "ok": ok,
        }),
    );
    if ok { Ok(()) } else { Err(format!("ollama pull {model} 失败")) }
}

enum Either {
    Out(tokio::process::ChildStdout),
    Err(tokio::process::ChildStderr),
}

/// 取消一个正在跑的下载。
#[tauri::command]
pub fn cancel_local_model_pull(model: String) -> Result<(), String> {
    let mut guard = pulling().lock().map_err(|_| "内部状态锁失败".to_string())?;
    match guard.remove(&model) {
        Some(tx) => {
            let _ = tx.send(());
            Ok(())
        }
        None => Err(format!("{model} 没有在下载")),
    }
}

/// 本机 Ollama 已经装了哪些模型——推荐列表据此标出「已安装」。
#[tauri::command]
pub async fn list_installed_ollama_models() -> Result<Vec<String>, String> {
    let output = tokio::process::Command::new("ollama")
        .arg("list")
        .no_window()
        .output()
        .await
        .map_err(|error| format!("ollama 不可用：{error}"))?;
    if !output.status.success() {
        return Err("ollama list 执行失败".into());
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .skip(1) // 表头
        .filter_map(|line| line.split_whitespace().next())
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .collect())
}

#[cfg(test)]
mod preset_key_tests {
    use crate::db::DbManager;
    use rusqlite::params;

    fn test_db(tag: &str) -> DbManager {
        use std::sync::atomic::{AtomicU32, Ordering};
        static N: AtomicU32 = AtomicU32::new(0);
        let path = std::env::temp_dir().join(format!(
            "omnix_preset_{tag}_{}_{}.db",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_file(&path);
        DbManager::new_with_path(path)
    }

    /// 一键预设的 Key 必须进加密表，而且**不能**在旧的明文列里留一份。
    ///
    /// 这里以前直接 `UPDATE model_platforms SET api_key = <明文>`，同时坏两件事：
    /// 绕开加密策略；以及平台一旦已有 `platform_api_keys` 行，运行时根本不读那一
    /// 列，预设 Key「保存成功、实际没用」。
    #[test]
    fn preset_key_lands_encrypted_and_leaves_no_plaintext() {
        let db = test_db("enc");
        {
            let conn = db.get_connection().unwrap();
            conn.execute("DELETE FROM model_platforms", []).unwrap();
            conn.execute("DELETE FROM platform_api_keys", []).unwrap();
        }
        super::apply_api_preset_core(&db, "deepseek", "sk-preset-secret-123").expect("应用预设");

        let conn = db.get_connection().unwrap();
        let legacy: String = conn
            .query_row(
                "SELECT COALESCE(api_key, '') FROM model_platforms WHERE id = 'deepseek'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(
            !legacy.contains("sk-preset-secret-123"),
            "明文 Key 落进了遗留列：{legacy}"
        );

        let stored: String = conn
            .query_row(
                "SELECT encrypted_key FROM platform_api_keys WHERE platform_id = 'deepseek'",
                [],
                |r| r.get(0),
            )
            .expect("Key 应该进了加密表");
        assert!(!stored.contains("sk-preset-secret-123"), "加密表里存的是明文");
        assert_eq!(
            crate::crypto::decrypt(&stored),
            "sk-preset-secret-123",
            "解出来必须还是原来那把 Key"
        );
    }

    /// 平台已经有用户自己选的活跃 Key 时，预设不该抢占活跃位。
    #[test]
    fn preset_does_not_steal_the_active_slot() {
        let db = test_db("active");
        {
            let conn = db.get_connection().unwrap();
            conn.execute("DELETE FROM model_platforms", []).unwrap();
            conn.execute("DELETE FROM platform_api_keys", []).unwrap();
            conn.execute(
                "INSERT INTO model_platforms (id, name, api_type, api_key, api_address, is_enabled)
                 VALUES ('deepseek', 'DeepSeek', 'openai', '', 'https://x', 1)",
                [],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO platform_api_keys (id, platform_id, encrypted_key, label, is_active)
                 VALUES ('mine', 'deepseek', ?1, '我自己的', 1)",
                params![crate::crypto::encrypt("sk-mine")],
            )
            .unwrap();
        }
        super::apply_api_preset_core(&db, "deepseek", "sk-from-preset").expect("应用预设");

        let conn = db.get_connection().unwrap();
        // 「活跃 Key 有且只有一个」本身就是不变量——只查 WHERE is_active = 1 取一行
        // 的话，两个都活跃时也可能碰巧返回 'mine'，测不出坏状态。
        let active: Vec<String> = conn
            .prepare("SELECT id FROM platform_api_keys WHERE platform_id = 'deepseek' AND is_active = 1")
            .unwrap()
            .query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();
        assert_eq!(active, vec!["mine".to_string()], "活跃 Key 应当仍然只有用户自己选的那把");
    }
}

#[cfg(test)]
mod codebase_scan_tests {
    use super::*;
    use std::fs;

    fn temp_root(tag: &str) -> PathBuf {
        use std::sync::atomic::{AtomicU32, Ordering};
        static N: AtomicU32 = AtomicU32::new(0);
        let p = std::env::temp_dir().join(format!(
            "omnix_scan_{tag}_{}_{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    /// 最大文件榜里给的是**相对根目录**的路径。
    ///
    /// 以前给的是绝对路径——泄露本机目录结构，界面上还是一长串没法看的东西。
    #[test]
    fn largest_files_are_relative_to_the_scanned_root() {
        let root = temp_root("rel");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();

        let stats = scan_codebase(&root);
        let (name, _) = &stats.largest_files[0];
        assert!(!name.contains(root.to_str().unwrap()), "不该出现绝对路径：{name}");
        assert!(name.ends_with("main.rs"), "路径应相对根目录，实际 {name}");
    }

    /// 行数按换行符流式统计，不把文件读进内存。
    #[test]
    fn counts_lines_without_reading_whole_files() {
        let root = temp_root("lines");
        fs::write(root.join("a.rs"), "one\ntwo\nthree\n").unwrap();
        assert_eq!(scan_codebase(&root).total_lines, 3);
    }

    /// 超过阈值的文件**仍然计入文件数和最大文件榜**，只是不数行数。
    ///
    /// 这条守的是「跳过」的分寸：跳过计行是为了不被一个几百 MB 的数据集拖垮，
    /// 但那个文件本身恰恰是「这个库里什么最大」最该报出来的答案。
    #[test]
    fn oversized_files_still_count_but_skip_line_counting() {
        let root = temp_root("big");
        let big = vec![b'x'; (MAX_LINE_COUNT_BYTES + 1) as usize];
        fs::write(root.join("big.bin"), &big).unwrap();
        fs::write(root.join("small.rs"), "a\nb\n").unwrap();

        let stats = scan_codebase(&root);
        assert_eq!(stats.file_count, 2, "超大文件也要计入文件数");
        assert_eq!(stats.total_lines, 2, "超大文件不该参与计行");
        assert_eq!(stats.largest_files[0].0, "big.bin", "超大文件该排在最大文件榜首");
    }

    /// 深度上限兜底：病态嵌套不会一路递归下去。
    ///
    /// 主防线是「不跟随软链」（`entry.file_type()`），但那一层依赖平台行为；
    /// 深度上限是不依赖平台的第二道。这里造一个超过上限的目录链，断言扫描正常
    /// 返回、并且确实**没有**把上限之外的文件算进来。
    #[test]
    fn pathological_nesting_stops_at_the_depth_cap() {
        let root = temp_root("deep");
        let mut p = root.clone();
        for i in 0..(MAX_SCAN_DEPTH + 5) {
            p = p.join(format!("d{i}"));
        }
        fs::create_dir_all(&p).unwrap();
        fs::write(p.join("deep.rs"), "x\n").unwrap();
        fs::write(root.join("shallow.rs"), "y\n").unwrap();

        let stats = scan_codebase(&root);
        assert_eq!(stats.file_count, 1, "只该数到浅层那个，深处的被上限挡住");
        assert_eq!(stats.largest_files[0].0, "shallow.rs");
    }

    /// 构建产物目录不参与统计。
    #[test]
    fn build_output_directories_are_skipped() {
        let root = temp_root("skip");
        for d in ["node_modules", "target", "__pycache__", ".venv"] {
            fs::create_dir_all(root.join(d)).unwrap();
            fs::write(root.join(d).join("junk.js"), "noise\n").unwrap();
        }
        fs::write(root.join("real.rs"), "code\n").unwrap();

        let stats = scan_codebase(&root);
        assert_eq!(stats.file_count, 1, "只有 real.rs 该被统计");
        assert_eq!(stats.languages.get("Rust"), Some(&1));
        assert_eq!(stats.languages.get("JavaScript"), None, "依赖树里的 js 不该算进语言分布");
    }
}

#[cfg(test)]
mod compaction_tests {
    use super::*;
    use crate::db::DbManager;

    fn test_db(tag: &str) -> Arc<DbManager> {
        use std::sync::atomic::{AtomicU32, Ordering};
        static N: AtomicU32 = AtomicU32::new(0);
        let path = std::env::temp_dir().join(format!(
            "omnix_compact_{tag}_{}_{}.db",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_file(&path);
        Arc::new(DbManager::new_with_path(path))
    }

    /// 塞 n 条消息，第 i 条内容是 `<i>` 后跟 len 个 'x'。
    fn seed(db: &DbManager, conv: &str, n: usize, len: usize) {
        let conn = db.get_connection().unwrap();
        conn.execute(
            "INSERT INTO conversations (id, title, workspace_path, active_agent)
             VALUES (?1, 'T', '', 'claude')",
            params![conv],
        )
        .unwrap();
        for i in 0..n {
            conn.execute(
                "INSERT INTO messages (id, conversation_id, role, content, timestamp)
                 VALUES (?1, ?2, 'user', ?3, datetime('now', ?4))",
                params![
                    format!("m{i}"),
                    conv,
                    format!("{i}{}", "x".repeat(len)),
                    format!("-{} minutes", n - i)
                ],
            )
            .unwrap();
        }
    }

    fn message_count(db: &DbManager, conv: &str) -> i64 {
        db.get_connection()
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM messages WHERE conversation_id = ?1",
                params![conv],
                |r| r.get(0),
            )
            .unwrap()
    }

    /// 压缩会**删掉**原始消息，只留最近 N 条 + 一条汇总。
    #[test]
    fn compaction_removes_the_original_messages() {
        let db = test_db("removes");
        seed(&db, "c1", 30, 10);
        assert_eq!(message_count(&db, "c1"), 30);

        compact_core(&db, "c1", 20).expect("压缩");

        // 20 条保留 + 1 条汇总
        assert_eq!(message_count(&db, "c1"), 21, "原始消息应已被删除");
    }

    /// **原文在删除前必须落盘，且内容完整**。
    ///
    /// 这是这次修复的核心：截断是有意的，销毁不是。备份让操作可逆——所以这条
    /// 断言的不只是「文件存在」，而是**未截断的全文**都在里面。
    #[test]
    fn originals_are_backed_up_in_full_before_deletion() {
        let db = test_db("backup");
        let long = format!("0{}", "x".repeat(500));
        seed(&db, "c3", 25, 500);

        let before = std::time::SystemTime::now();
        compact_core(&db, "c3", 20).expect("压缩");

        let dir = crate::storage::backups_dir().join("compaction");
        let mut found: Option<String> = None;
        for entry in std::fs::read_dir(&dir).expect("备份目录应已建立").flatten() {
            let meta = entry.metadata().unwrap();
            if meta.modified().map(|m| m >= before).unwrap_or(false)
                && entry.file_name().to_string_lossy().starts_with("c3_")
            {
                found = std::fs::read_to_string(entry.path()).ok();
            }
        }
        let body = found.expect("应写出本次压缩的备份文件");
        assert!(
            body.contains(&long),
            "备份里应是**未截断**的全文（500+ 字符），实际没找到"
        );
        let parsed: serde_json::Value = serde_json::from_str(&body).expect("备份应是合法 JSON");
        assert_eq!(
            parsed["messages"].as_array().unwrap().len(),
            5,
            "25 条留 20 条，应备份被删掉的 5 条"
        );
    }

    /// 摘录里每条只保留前 200 字符，并明确标注被截断过。
    ///
    /// 这条测的是「口径」而不是「对错」：截断本身是有意的（要的就是缩小上下文），
    /// 但产出必须说自己是摘录，不能自称摘要。
    #[test]
    fn the_digest_truncates_each_message_at_200_chars() {
        let db = test_db("truncate");
        seed(&db, "c2", 25, 500); // 每条 500+ 字符
        compact_core(&db, "c2", 20).expect("压缩");

        let digest: String = db
            .get_connection()
            .unwrap()
            .query_row(
                "SELECT content FROM messages WHERE conversation_id = ?1 AND role = 'system'",
                params!["c2"],
                |r| r.get(0),
            )
            .unwrap();

        // 被压掉的是最早 5 条（25 - 20）。取第一条看它被截到多长。
        let line = digest
            .lines()
            .find(|l| l.starts_with("[user]:"))
            .expect("汇总里应有 user 行");
        let body = line.trim_start_matches("[user]: ");
        assert!(
            body.ends_with("…（已截断）"),
            "被截断的条目必须自己说出来，否则读的人会以为那就是全文：{body}"
        );
        let kept = body.trim_end_matches("…（已截断）").chars().count();
        assert_eq!(kept, 200, "每条应保留前 200 字符，实际 {kept}");

        // 产出不能自称「摘要」——没有模型参与，它只是摘录。
        assert!(digest.contains("非摘要"), "摘录必须标明自己不是摘要");
        assert!(digest.contains("原文备份："), "摘录里要指出原文备份在哪");
    }
}

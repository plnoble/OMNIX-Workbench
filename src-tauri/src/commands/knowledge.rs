use super::*;
use crate::db::DbManager;
use crate::input_validation;
use crate::knowledge::{self, ChunkConfig, RagResponse, SearchResult};
use rusqlite::params;
use std::sync::Arc;
use tauri::{Emitter, State};

/// Validate a file path to prevent directory traversal attacks.
/// Delegates to the shared input_validation module.
fn validate_file_path(path: &std::path::Path) -> Result<(), String> {
    input_validation::validate_relative_path(path)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreviewFile {
    pub path: String,
    pub name: String,
    pub ext: String,
    pub modified: u64,
}

#[tauri::command]
pub fn get_previewable_files(workspace_path: String) -> Result<Vec<PreviewFile>, String> {
    use std::fs;
    use std::path::Path;

    let workspace = Path::new(&workspace_path);
    if !workspace.exists() || !workspace.is_dir() {
        return Err("Workspace directory does not exist".to_string());
    }

    let mut files = Vec::new();

    fn scan_dir(dir: &Path, depth: usize, files: &mut Vec<PreviewFile>) {
        if depth > 4 {
            return;
        }
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_string();

                if name.starts_with('.')
                    || name == "node_modules"
                    || name == "target"
                    || name == "dist"
                    || name == "build"
                {
                    continue;
                }

                if path.is_dir() {
                    scan_dir(&path, depth + 1, files);
                } else if path.is_file() {
                    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                        let ext_lower = ext.to_lowercase();
                        if ext_lower == "html"
                            || ext_lower == "md"
                            || ext_lower == "png"
                            || ext_lower == "jpg"
                            || ext_lower == "jpeg"
                            || ext_lower == "gif"
                            || ext_lower == "svg"
                        {
                            let modified = path
                                .metadata()
                                .and_then(|m| m.modified())
                                .and_then(|t| {
                                    t.duration_since(std::time::SystemTime::UNIX_EPOCH).map_err(
                                        |e| std::io::Error::new(std::io::ErrorKind::Other, e),
                                    )
                                })
                                .map(|d| d.as_secs())
                                .unwrap_or(0);

                            files.push(PreviewFile {
                                path: path.to_string_lossy().to_string(),
                                name,
                                ext: ext_lower,
                                modified,
                            });
                        }
                    }
                }
            }
        }
    }

    scan_dir(workspace, 0, &mut files);
    files.sort_by(|a, b| b.modified.cmp(&a.modified));
    if files.len() > 50 {
        files.truncate(50);
    }

    Ok(files)
}

#[tauri::command]
pub fn read_file_content_utf8(file_path: String) -> Result<String, String> {
    use std::fs;
    use std::path::Path;
    let path = Path::new(&file_path);
    validate_file_path(&path)?;
    if !path.exists() || !path.is_file() {
        return Err("File does not exist".to_string());
    }
    // Size limit: 2 MB to prevent memory exhaustion
    if let Ok(meta) = fs::metadata(&path) {
        if meta.len() > 2 * 1024 * 1024 {
            return Err("File too large (max 2 MB)".to_string());
        }
    }
    fs::read_to_string(path).map_err(|e| format!("Failed to read file: {}", e))
}

fn base64_encode(data: &[u8]) -> String {
    const CHARSET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::with_capacity((data.len() + 2) / 3 * 4);
    let mut i = 0;
    while i < data.len() {
        let chunk = &data[i..std::cmp::min(i + 3, data.len())];
        let val = match chunk.len() {
            3 => ((chunk[0] as u32) << 16) | ((chunk[1] as u32) << 8) | (chunk[2] as u32),
            2 => ((chunk[0] as u32) << 16) | ((chunk[1] as u32) << 8),
            1 => (chunk[0] as u32) << 16,
            _ => unreachable!(),
        };

        let enc1 = (val >> 18) & 63;
        let enc2 = (val >> 12) & 63;
        let enc3 = (val >> 6) & 63;
        let enc4 = val & 63;

        result.push(CHARSET[enc1 as usize] as char);
        result.push(CHARSET[enc2 as usize] as char);
        if chunk.len() >= 2 {
            result.push(CHARSET[enc3 as usize] as char);
        } else {
            result.push('=');
        }
        if chunk.len() == 3 {
            result.push(CHARSET[enc4 as usize] as char);
        } else {
            result.push('=');
        }
        i += 3;
    }
    result
}

#[tauri::command]
pub fn read_file_as_base64(file_path: String) -> Result<String, String> {
    use std::fs;
    use std::path::Path;
    let path = Path::new(&file_path);
    validate_file_path(&path)?;
    if !path.exists() || !path.is_file() {
        return Err("File does not exist".to_string());
    }
    // Size limit: 5 MB for binary preview
    if let Ok(meta) = fs::metadata(&path) {
        if meta.len() > 5 * 1024 * 1024 {
            return Err("File too large (max 5 MB)".to_string());
        }
    }
    let bytes = fs::read(path).map_err(|e| format!("Failed to read file: {}", e))?;
    Ok(base64_encode(&bytes))
}

#[tauri::command]
pub fn get_workspace_git_diff(workspace_path: String) -> Result<String, String> {
    use std::path::Path;
    let workspace = Path::new(&workspace_path);
    if !workspace.exists() || !workspace.is_dir() {
        return Err("Workspace directory does not exist".to_string());
    }

    let output = std::process::Command::new("git")
        .arg("diff")
        .current_dir(workspace)
        .output()
        .map_err(|e| format!("Failed to run git diff: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if !output.status.success() {
        return Err(format!("git diff error: {}", stderr));
    }

    Ok(stdout)
}

/// Run environment diagnostics — returns a flat string map so the
/// frontend `Record<string, string>` type matches exactly (no boolean
/// values that would cause `version.toLowerCase is not a function`).
#[tauri::command]
pub fn run_env_diagnostics() -> Result<std::collections::HashMap<String, String>, String> {
    let mut map = std::collections::HashMap::new();

    // Node.js
    match std::process::Command::new("node").arg("-v").output() {
        Ok(out) => {
            let v = String::from_utf8_lossy(&out.stdout).trim().to_string();
            map.insert("Node.js".into(), v);
        }
        Err(_) => {
            map.insert("Node.js".into(), "not found".into());
        }
    }

    // Git
    match std::process::Command::new("git").arg("--version").output() {
        Ok(out) => {
            let v = String::from_utf8_lossy(&out.stdout).trim().to_string();
            map.insert("Git".into(), v);
        }
        Err(_) => {
            map.insert("Git".into(), "not found".into());
        }
    }

    // Ripgrep
    match std::process::Command::new("rg").arg("--version").output() {
        Ok(out) => {
            let full_out = String::from_utf8_lossy(&out.stdout);
            let first_line = full_out.lines().next().unwrap_or("rg").to_string();
            map.insert("Ripgrep".into(), first_line);
        }
        Err(_) => {
            map.insert("Ripgrep".into(), "not found".into());
        }
    }

    // CLI agents — just check existence
    for (name, cmds) in [
        ("Claude Code", &["claude", "claude.cmd"] as &[&str]),
        ("OpenCode", &["opencode", "opencode.cmd"]),
        ("Codex", &["codex", "codex.cmd"]),
        ("Gemini CLI", &["gemini-cli", "gemini-cli.cmd"]),
    ] {
        let found = cmds.iter().any(|c| which::which(c).is_ok());
        map.insert(
            name.into(),
            if found {
                "✓ installed".into()
            } else {
                "not found".into()
            },
        );
    }

    Ok(map)
}

#[tauri::command]
pub async fn repair_env_tool(app: tauri::AppHandle, tool_name: String) -> Result<(), String> {
    let (cmd, args) = match tool_name.as_str() {
        "claude" => ("npm", vec!["install", "-g", "@anthropic-ai/claude-code"]),
        "gemini" => ("npm", vec!["install", "-g", "@google/gemini-cli"]),
        "opencode" => ("npm", vec!["install", "-g", "opencode-cli"]),
        "ripgrep" => {
            #[cfg(target_os = "windows")]
            {
                (
                    "powershell",
                    vec!["-Command", "winget install BurntSushi.ripgrep --silent"],
                )
            }
            #[cfg(not(target_os = "windows"))]
            {
                ("brew", vec!["install", "ripgrep"])
            }
        }
        _ => return Err(format!("Unsupported repair tool: {}", tool_name)),
    };

    let mut child = tokio::process::Command::new(cmd)
        .args(&args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to spawn repair command: {}", e))?;

    use tokio::io::AsyncBufReadExt;

    let stdout = child
        .stdout
        .take()
        .expect("child process stdout was already taken");
    let stderr = child
        .stderr
        .take()
        .expect("child process stderr was already taken");

    let app_clone1 = app.clone();
    tokio::spawn(async move {
        let mut reader = tokio::io::BufReader::new(stdout).lines();
        while let Ok(Some(line)) = reader.next_line().await {
            let _ = app_clone1.emit("omnix-repair-log", format!("[STDOUT] {}", line));
        }
    });

    let app_clone2 = app.clone();
    tokio::spawn(async move {
        let mut reader = tokio::io::BufReader::new(stderr).lines();
        while let Ok(Some(line)) = reader.next_line().await {
            let _ = app_clone2.emit("omnix-repair-log", format!("[STDERR] {}", line));
        }
    });

    let status = child
        .wait()
        .await
        .map_err(|e| format!("Wait failed: {}", e))?;
    if status.success() {
        let _ = app.emit(
            "omnix-repair-log",
            format!("[SUCCESS] {} 修复安装成功！", tool_name),
        );
        Ok(())
    } else {
        let _ = app.emit(
            "omnix-repair-log",
            format!(
                "[ERROR] {} 修复安装失败，退出码: {:?}",
                tool_name,
                status.code()
            ),
        );
        Err(format!("Command exited with status: {:?}", status))
    }
}

// ── Knowledge Base DTOs ────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KbDocument {
    pub id: String,
    pub knowledge_base_id: String,
    pub title: String,
    pub source_path: String,
    pub file_type: String,
    pub file_hash: String,
    pub chunk_count: i32,
    pub total_chars: i32,
    pub embedding_model: String,
    pub embedding_status: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeBase {
    pub id: String,
    pub name: String,
    pub description: String,
    pub document_count: i32,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KbChunk {
    pub id: String,
    pub document_id: String,
    pub chunk_index: i32,
    pub content: String,
    pub char_start: i32,
    pub char_end: i32,
    pub metadata: serde_json::Value,
    pub has_embedding: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkConfigPayload {
    pub max_chunk_chars: Option<usize>,
    pub overlap_chars: Option<usize>,
    pub respect_boundaries: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingProgress {
    pub document_id: String,
    pub total_chunks: i32,
    pub embedded_chunks: i32,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingModelInfo {
    pub model_name: String,
    pub platform_id: String,
    pub platform_name: String,
    pub api_type: String,
}

// ── Knowledge Base Commands ────────────────────────────

#[tauri::command]
pub fn kb_list_bases(db: State<'_, Arc<DbManager>>) -> Result<Vec<KnowledgeBase>, String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT b.id, b.name, b.description, COUNT(d.id), b.created_at, b.updated_at
             FROM knowledge_bases b
             LEFT JOIN kb_documents d ON d.knowledge_base_id = b.id
             GROUP BY b.id, b.name, b.description, b.created_at, b.updated_at
             ORDER BY CASE WHEN b.id = 'default' THEN 0 ELSE 1 END, b.name",
        )
        .map_err(|e| e.to_string())?;
    let bases = stmt
        .query_map([], |row| {
            Ok(KnowledgeBase {
                id: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                document_count: row.get(3)?,
                created_at: row.get(4)?,
                updated_at: row.get(5)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(bases)
}

#[tauri::command]
pub fn kb_create_base(
    name: String,
    description: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<KnowledgeBase, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("知识库名称不能为空".into());
    }
    input_validation::validate_name(name, "knowledge_base_name")?;
    let id = format!("kb_{}", uuid_simple());
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO knowledge_bases (id, name, description) VALUES (?1, ?2, ?3)",
        params![id, name, description.trim()],
    )
    .map_err(|e| format!("创建知识库失败: {e}"))?;
    drop(conn);
    kb_list_bases(db)?
        .into_iter()
        .find(|base| base.id == id)
        .ok_or_else(|| "知识库创建后读取失败".into())
}

#[tauri::command]
pub fn kb_update_base(
    knowledge_base_id: String,
    name: String,
    description: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    if name.trim().is_empty() {
        return Err("知识库名称不能为空".into());
    }
    input_validation::validate_name(name.trim(), "knowledge_base_name")?;
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let changed = conn
        .execute(
            "UPDATE knowledge_bases SET name = ?1, description = ?2, updated_at = CURRENT_TIMESTAMP WHERE id = ?3",
            params![name.trim(), description.trim(), knowledge_base_id],
        )
        .map_err(|e| e.to_string())?;
    if changed == 0 {
        Err("知识库不存在".into())
    } else {
        Ok(())
    }
}

#[tauri::command]
pub fn kb_delete_base(
    knowledge_base_id: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    if knowledge_base_id == "default" {
        return Err("默认知识库不能删除".into());
    }
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let document_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM kb_documents WHERE knowledge_base_id = ?1",
            params![knowledge_base_id],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    if document_count > 0 {
        return Err("请先删除或移动该知识库中的文档".into());
    }
    conn.execute(
        "DELETE FROM knowledge_bases WHERE id = ?1",
        params![knowledge_base_id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn kb_list_documents(
    knowledge_base_id: Option<String>,
    db: State<'_, Arc<DbManager>>,
) -> Result<Vec<KbDocument>, String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let sql = if knowledge_base_id.is_some() {
        "SELECT id, knowledge_base_id, title, source_path, file_type, file_hash, chunk_count, total_chars, embedding_model, embedding_status, created_at, updated_at FROM kb_documents WHERE knowledge_base_id = ?1 ORDER BY updated_at DESC"
    } else {
        "SELECT id, knowledge_base_id, title, source_path, file_type, file_hash, chunk_count, total_chars, embedding_model, embedding_status, created_at, updated_at FROM kb_documents ORDER BY updated_at DESC"
    };
    let mut stmt = conn.prepare(sql).map_err(|e| e.to_string())?;

    let map_row = |row: &rusqlite::Row<'_>| {
        Ok(KbDocument {
            id: row.get(0)?,
            knowledge_base_id: row.get(1)?,
            title: row.get(2)?,
            source_path: row.get(3)?,
            file_type: row.get(4)?,
            file_hash: row.get(5)?,
            chunk_count: row.get(6)?,
            total_chars: row.get(7)?,
            embedding_model: row.get(8)?,
            embedding_status: row.get(9)?,
            created_at: row.get(10)?,
            updated_at: row.get(11)?,
        })
    };
    let docs = if let Some(base_id) = knowledge_base_id {
        stmt.query_map(params![base_id], map_row)
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?
    } else {
        stmt.query_map([], map_row)
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?
    };

    Ok(docs)
}

#[tauri::command]
pub async fn kb_import_document(
    knowledge_base_id: Option<String>,
    title: String,
    source_path: String,
    file_type: String,
    content: String,
    chunk_config: Option<ChunkConfigPayload>,
    db: State<'_, Arc<DbManager>>,
) -> Result<KbDocument, String> {
    let knowledge_base_id = knowledge_base_id.unwrap_or_else(|| "default".into());
    {
        let conn = db.get_connection().map_err(|e| e.to_string())?;
        let exists: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM knowledge_bases WHERE id = ?1",
                params![knowledge_base_id],
                |row| row.get(0),
            )
            .map_err(|e| e.to_string())?;
        if exists == 0 {
            return Err("选择的知识库不存在".into());
        }
    }
    let config = match chunk_config {
        Some(c) => ChunkConfig {
            max_chunk_chars: c.max_chunk_chars.unwrap_or(512),
            overlap_chars: c.overlap_chars.unwrap_or(64),
            respect_boundaries: c.respect_boundaries.unwrap_or(true),
        },
        None => ChunkConfig::default(),
    };

    // Generate document ID
    let doc_id = format!("doc_{}", uuid_simple());

    // Compute SHA-256 hash
    let hash = content_hash_hex(&content);

    // Dedup: check if same file_hash already exists for same source_path
    {
        let conn = db.get_connection().map_err(|e| e.to_string())?;
        let existing: Option<String> = conn
            .query_row(
                "SELECT id FROM kb_documents WHERE knowledge_base_id = ?1 AND source_path = ?2 AND file_hash = ?3 LIMIT 1",
                params![knowledge_base_id, source_path, hash],
                |row| row.get(0),
            )
            .ok();
        if let Some(existing_id) = existing {
            return Err(format!("文档已存在 (id: {}), 内容未变更", existing_id));
        }
    }

    // Auto-detect file_type from source_path extension if file_type is empty or "auto"
    let resolved_file_type = if file_type.is_empty() || file_type == "auto" {
        let ext = source_path.rsplit('.').next().unwrap_or("").to_lowercase();
        match ext.as_str() {
            "md" | "markdown" => "markdown".to_string(),
            "rs" | "py" | "js" | "ts" | "tsx" | "jsx" | "go" | "java" | "c" | "cpp" | "h"
            | "rb" | "swift" | "kt" => "code".to_string(),
            _ => "text".to_string(),
        }
    } else {
        file_type.clone()
    };

    // Chunk the document
    let chunks = knowledge::chunk_document(&content, &resolved_file_type, &config);

    let chunk_count = chunks.len() as i32;
    let total_chars = content.len() as i32;

    // Insert document and chunks
    let conn = db.get_connection().map_err(|e| e.to_string())?;

    conn.execute(
        "INSERT INTO kb_documents (id, knowledge_base_id, title, source_path, file_type, file_hash, chunk_count, total_chars, embedding_status)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'pending')",
        params![doc_id, knowledge_base_id, title, source_path, resolved_file_type, hash, chunk_count, total_chars],
    )
    .map_err(|e| e.to_string())?;

    for chunk in &chunks {
        let chunk_id = format!("chunk_{}_{}", doc_id, chunk.index);
        let metadata_str =
            serde_json::to_string(&chunk.metadata).unwrap_or_else(|_| "{}".to_string());
        conn.execute(
            "INSERT INTO kb_chunks (id, document_id, chunk_index, content, char_start, char_end, metadata)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![chunk_id, doc_id, chunk.index as i32, chunk.content, chunk.char_start as i32, chunk.char_end as i32, metadata_str],
        )
        .map_err(|e| e.to_string())?;
    }

    // Read back the document
    let doc = conn
        .prepare("SELECT id, knowledge_base_id, title, source_path, file_type, file_hash, chunk_count, total_chars, embedding_model, embedding_status, created_at, updated_at FROM kb_documents WHERE id = ?1")
        .map_err(|e| e.to_string())?
        .query_row(params![doc_id], |row| {
            Ok(KbDocument {
                id: row.get(0)?,
                knowledge_base_id: row.get(1)?,
                title: row.get(2)?,
                source_path: row.get(3)?,
                file_type: row.get(4)?,
                file_hash: row.get(5)?,
                chunk_count: row.get(6)?,
                total_chars: row.get(7)?,
                embedding_model: row.get(8)?,
                embedding_status: row.get(9)?,
                created_at: row.get(10)?,
                updated_at: row.get(11)?,
            })
        })
        .map_err(|e| e.to_string())?;

    Ok(doc)
}

#[tauri::command]
pub fn kb_delete_document(
    document_id: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;

    // Delete embeddings for chunks in this document
    conn.execute(
        "DELETE FROM kb_embeddings WHERE chunk_id IN (SELECT id FROM kb_chunks WHERE document_id = ?1)",
        params![document_id],
    )
    .map_err(|e| e.to_string())?;

    // Delete FTS entries (trigger handles it, but be explicit for external content)
    conn.execute(
        "DELETE FROM kb_chunks_fts WHERE chunk_id IN (SELECT id FROM kb_chunks WHERE document_id = ?1)",
        params![document_id],
    )
    .map_err(|e| e.to_string())?;

    // Delete chunks (trigger handles FTS cleanup too)
    conn.execute(
        "DELETE FROM kb_chunks WHERE document_id = ?1",
        params![document_id],
    )
    .map_err(|e| e.to_string())?;

    // Delete document
    conn.execute(
        "DELETE FROM kb_documents WHERE id = ?1",
        params![document_id],
    )
    .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub fn kb_get_chunks(
    document_id: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<Vec<KbChunk>, String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;

    let mut stmt = conn
        .prepare(
            "SELECT c.id, c.document_id, c.chunk_index, c.content, c.char_start, c.char_end, c.metadata,
                    CASE WHEN e.chunk_id IS NOT NULL THEN 1 ELSE 0 END AS has_emb
             FROM kb_chunks c
             LEFT JOIN kb_embeddings e ON c.id = e.chunk_id
             WHERE c.document_id = ?1
             ORDER BY c.chunk_index",
        )
        .map_err(|e| e.to_string())?;

    let chunks = stmt
        .query_map(params![document_id], |row| {
            let metadata_str: String = row.get(6)?;
            let has_emb: i32 = row.get(7)?;
            Ok(KbChunk {
                id: row.get(0)?,
                document_id: row.get(1)?,
                chunk_index: row.get(2)?,
                content: row.get(3)?,
                char_start: row.get(4)?,
                char_end: row.get(5)?,
                metadata: serde_json::from_str(&metadata_str).unwrap_or(serde_json::Value::Null),
                has_embedding: has_emb != 0,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    Ok(chunks)
}

#[tauri::command]
pub async fn kb_generate_embeddings(
    document_id: String,
    model_name: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<EmbeddingProgress, String> {
    // ── Phase 1: Synchronous data extraction (must complete before any await) ──
    let (chunk_ids, chunk_texts, total_chunks) = {
        let conn = db.get_connection().map_err(|e| e.to_string())?;

        let chunks: Vec<(String, String)> = conn
            .prepare(
                "SELECT c.id, c.content FROM kb_chunks c
                 LEFT JOIN kb_embeddings e ON c.id = e.chunk_id
                 WHERE c.document_id = ?1 AND e.chunk_id IS NULL
                 ORDER BY c.chunk_index",
            )
            .map_err(|e| e.to_string())?
            .query_map(params![document_id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;

        let total_chunks: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM kb_chunks WHERE document_id = ?1",
                params![document_id],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;

        if !chunks.is_empty() {
            conn.execute(
                "UPDATE kb_documents SET embedding_status = 'in_progress', embedding_model = ?1, updated_at = CURRENT_TIMESTAMP WHERE id = ?2",
                params![model_name, document_id],
            )
            .map_err(|e| e.to_string())?;
        }

        let ids: Vec<String> = chunks.iter().map(|(id, _)| id.clone()).collect();
        let texts: Vec<String> = chunks.iter().map(|(_, c)| c.clone()).collect();
        (ids, texts, total_chunks)
    }; // conn dropped here — safe to await below

    if chunk_ids.is_empty() {
        return Ok(EmbeddingProgress {
            document_id,
            total_chunks,
            embedded_chunks: total_chunks,
            status: "completed".to_string(),
        });
    }

    // ── Phase 2: Async embedding generation ──
    let batch_size = 32;
    let mut embedded_count = 0usize;

    for (batch_idx, batch) in chunk_texts.chunks(batch_size).enumerate() {
        let batch_texts: Vec<String> = batch.to_vec();
        let embeddings =
            knowledge::generate_embeddings(&*db, batch_texts, &model_name, None).await?;

        let conn = db.get_connection().map_err(|e| e.to_string())?;
        for (i, embedding) in embeddings.iter().enumerate() {
            let global_idx = batch_idx * batch_size + i;
            if global_idx >= chunk_ids.len() {
                break;
            }
            let chunk_id = &chunk_ids[global_idx];
            let blob = knowledge::vec_f32_to_blob(embedding);
            let dimensions = embedding.len() as i32;
            conn.execute(
                "INSERT OR REPLACE INTO kb_embeddings (chunk_id, embedding, model, dimensions) VALUES (?1, ?2, ?3, ?4)",
                params![chunk_id, blob, model_name, dimensions],
            )
            .map_err(|e| e.to_string())?;
            embedded_count += 1;
        }
    }

    // Update status to completed
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE kb_documents SET embedding_status = 'completed', updated_at = CURRENT_TIMESTAMP WHERE id = ?1",
        params![document_id],
    )
    .map_err(|e| e.to_string())?;

    Ok(EmbeddingProgress {
        document_id,
        total_chunks,
        embedded_chunks: embedded_count as i32,
        status: "completed".to_string(),
    })
}

#[tauri::command]
pub async fn kb_hybrid_search(
    query: String,
    embedding_model: String,
    limit: Option<usize>,
    knowledge_base_ids: Option<Vec<String>>,
    db: State<'_, Arc<DbManager>>,
) -> Result<Vec<SearchResult>, String> {
    let limit = limit.unwrap_or(10);
    knowledge::hybrid_search(
        &*db,
        &query,
        &embedding_model,
        limit,
        20,
        20,
        60,
        knowledge_base_ids.as_deref(),
    )
    .await
}

#[tauri::command]
pub async fn kb_rag_query(
    query: String,
    embedding_model: String,
    chat_model: String,
    top_k: Option<usize>,
    knowledge_base_ids: Option<Vec<String>>,
    db: State<'_, Arc<DbManager>>,
) -> Result<RagResponse, String> {
    let top_k = top_k.unwrap_or(5);
    knowledge::rag_query(
        &*db,
        &query,
        &embedding_model,
        &chat_model,
        top_k,
        None,
        knowledge_base_ids.as_deref(),
    )
    .await
}

#[tauri::command]
pub fn kb_get_embedding_models(
    db: State<'_, Arc<DbManager>>,
) -> Result<Vec<EmbeddingModelInfo>, String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;

    let mut stmt = conn
        .prepare(
            "SELECT pm.model_name, pm.platform_id, mp.name, mp.api_type
             FROM platform_models pm
             JOIN model_platforms mp ON pm.platform_id = mp.id
             WHERE pm.has_embedding = 1 AND pm.is_enabled = 1 AND mp.is_enabled = 1",
        )
        .map_err(|e| e.to_string())?;

    let models = stmt
        .query_map([], |row| {
            Ok(EmbeddingModelInfo {
                model_name: row.get(0)?,
                platform_id: row.get(1)?,
                platform_name: row.get(2)?,
                api_type: row.get(3)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    Ok(models)
}

/// Import a file from the local filesystem into the Knowledge Base.
/// Reads the file, auto-detects type, chunks, and stores.
#[tauri::command]
pub async fn kb_import_file(
    file_path: String,
    knowledge_base_id: Option<String>,
    chunk_config: Option<ChunkConfigPayload>,
    db: State<'_, Arc<DbManager>>,
) -> Result<KbDocument, String> {
    use std::path::Path;

    let path = Path::new(&file_path);
    validate_file_path(&path)?;
    if !path.exists() || !path.is_file() {
        return Err(format!("文件不存在: {}", file_path));
    }
    // Size limit: 10 MB for KB import
    if let Ok(meta) = std::fs::metadata(&path) {
        if meta.len() > 10 * 1024 * 1024 {
            return Err("文件过大 (最大 10 MB)".to_string());
        }
    }

    // Read file content
    let content = std::fs::read_to_string(path).map_err(|e| format!("无法读取文件: {}", e))?;

    // Extract title from filename
    let title = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("untitled")
        .to_string();

    // file_type will be auto-detected in kb_import_document
    kb_import_document(
        knowledge_base_id,
        title,
        file_path,
        "auto".to_string(),
        content,
        chunk_config,
        db,
    )
    .await
}

/// Batch import multiple files from a directory.
#[tauri::command]
pub async fn kb_import_directory(
    directory_path: String,
    extensions: Option<String>, // comma-separated, e.g. "md,txt,rs,py"
    knowledge_base_id: Option<String>,
    db: State<'_, Arc<DbManager>>,
) -> Result<Vec<KbDocument>, String> {
    use std::path::Path;

    let dir = Path::new(&directory_path);
    if !dir.is_dir() {
        return Err(format!("目录不存在: {}", directory_path));
    }

    let ext_filter: Vec<String> = extensions
        .map(|e| e.split(',').map(|s| s.trim().to_lowercase()).collect())
        .unwrap_or_else(|| {
            vec![
                "md".into(),
                "txt".into(),
                "rs".into(),
                "py".into(),
                "js".into(),
                "ts".into(),
            ]
        });

    let mut results = Vec::new();
    let entries = std::fs::read_dir(dir).map_err(|e| format!("无法读取目录: {}", e))?;

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        if !ext_filter.contains(&ext) {
            continue;
        }

        let file_path_str = path.to_string_lossy().to_string();
        match kb_import_file(
            file_path_str.clone(),
            knowledge_base_id.clone(),
            None,
            db.clone(),
        )
        .await
        {
            Ok(doc) => results.push(doc),
            Err(e) => {
                // Skip duplicates, log other errors
                if !e.contains("文档已存在") {
                    log::warn!(
                        "[kb_import_directory] Failed to import {}: {}",
                        file_path_str,
                        e
                    );
                }
            }
        }
    }

    Ok(results)
}

// ── Utility Functions ──────────────────────────────────

/// Generate a simple UUID-like string (no external dependency).
fn uuid_simple() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{:016x}", ts)
}

/// Compute FNV-1a hex digest of a string (non-cryptographic, for change detection only).
fn content_hash_hex(input: &str) -> String {
    use std::fmt::Write;
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in input.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    let mut result = String::with_capacity(16);
    write!(result, "{:016x}", hash).expect("writing to String should never fail");
    result
}

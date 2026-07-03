//! Evolution loop — preview & maintain the experience that is fed back to agents.
//!
//! Feedback (回注) is delivered by [`crate::agent::AgentManager::inject_workspace_memories`],
//! which writes a managed memory block (`<!--- OMNIX MEMORY START/END --->`) into each
//! agent's auto-loaded context file (CLAUDE.md / AGENTS.md / GEMINI.md …) on every spawn.
//! That is the single, reliable feedback channel.
//!
//! As the memory bank grows, injecting the 20 *most recent* memories stops being right —
//! we inject the 20 most *relevant* to the current workspace. Relevance is lexical
//! (workspace stack signals vs. memory tags/text — always available, no network) plus an
//! optional embedding-cosine bonus when both the memory and a cached workspace profile
//! have embeddings. `build_memory_block` is the single source for both the live injection
//! and the UI preview.

use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

use rusqlite::{params, OptionalExtension};
use serde::Serialize;
use tauri::State;

use crate::db::DbManager;
use crate::knowledge::{blob_to_vec_f32, cosine_similarity, generate_embeddings, vec_f32_to_blob};

const MAX_INJECT: usize = 20;

#[derive(Serialize)]
pub struct LessonsInfo {
    pub count: usize,
    pub content: String,
}

// ─────────────────────────── helpers ───────────────────────────

fn tokenize(text: &str) -> HashSet<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.len() >= 2)
        .map(|t| t.to_string())
        .collect()
}

/// Pure-local signals describing a workspace's stack (no network) — manifests,
/// the directory name, and top-level entries.
fn derive_workspace_signals(workspace_path: &str) -> String {
    if workspace_path.trim().is_empty() || workspace_path == "direct" {
        return String::new();
    }
    let root = Path::new(workspace_path);
    let mut sig: Vec<String> = Vec::new();
    if let Some(name) = root.file_name().and_then(|n| n.to_str()) {
        sig.push(name.to_string());
    }
    let markers = [
        ("Cargo.toml", "rust cargo tokio"),
        ("package.json", "javascript typescript node npm"),
        ("tsconfig.json", "typescript"),
        ("pyproject.toml", "python"),
        ("requirements.txt", "python pip"),
        ("go.mod", "go golang"),
        ("pom.xml", "java maven"),
        ("build.gradle", "java kotlin gradle"),
        ("Gemfile", "ruby rails"),
        ("composer.json", "php"),
        ("CMakeLists.txt", "cpp c cmake"),
        ("tauri.conf.json", "tauri rust"),
    ];
    for (file, tags) in markers {
        if root.join(file).exists() {
            sig.push(tags.to_string());
        }
    }
    if let Ok(entries) = std::fs::read_dir(root) {
        for entry in entries.flatten().take(40) {
            if let Some(n) = entry.file_name().to_str() {
                if !n.starts_with('.') {
                    sig.push(n.to_string());
                }
            }
        }
    }
    sig.join(" ")
}

/// Lightweight language/stack tags derived from a memory's text.
fn derive_stack_tags(text: &str) -> String {
    let t = text.to_lowercase();
    let mut tags: Vec<&str> = Vec::new();
    for (kw, tag) in [
        ("rust", "rust"), ("tokio", "rust"), ("cargo", "rust"),
        ("python", "python"), ("pip", "python"),
        ("typescript", "typescript"), ("javascript", "javascript"),
        ("react", "react"), ("node", "node"),
        ("golang", "go"), ("java", "java"), ("tauri", "tauri"),
        ("sql", "sql"), ("cors", "web"), ("fetch", "web"), ("git", "git"),
    ] {
        if t.contains(kw) && !tags.contains(&tag) {
            tags.push(tag);
        }
    }
    tags.join(",")
}

/// The embedding model to use: the user's explicit choice (setting `embedding_model`)
/// if set, otherwise the first enabled embedding-capable model — excluding rerankers,
/// which are sometimes mis-tagged with `has_embedding = 1` but can't produce embeddings.
fn default_embedding_model(db: &DbManager) -> Option<String> {
    // 1) The user's explicit pick (from the evolution hub / settings).
    if let Ok(Some(m)) = db.get_setting("embedding_model") {
        let m = m.trim().to_string();
        if !m.is_empty() {
            return Some(m);
        }
    }
    // 2) Fallback: first enabled true embedding model.
    db.get_connection().ok().and_then(|conn| {
        conn.query_row(
            "SELECT pm.model_name FROM platform_models pm
             JOIN model_platforms mp ON pm.platform_id = mp.id
             WHERE pm.has_embedding = 1 AND pm.is_enabled = 1 AND mp.is_enabled = 1
               AND lower(pm.model_name) NOT LIKE '%rerank%' LIMIT 1",
            [],
            |r| r.get::<_, String>(0),
        )
        .ok()
    })
}

fn experience_count(db: &DbManager) -> usize {
    db.get_connection()
        .ok()
        .and_then(|conn| {
            conn.query_row(
                "SELECT COUNT(*) FROM memories WHERE type = 'experience'",
                [],
                |r| r.get::<_, i64>(0),
            )
            .ok()
        })
        .unwrap_or(0) as usize
}

// ─────────────────── memory block (relevance-aware) ───────────────────

struct ScoredMemory {
    desc: String,
    pattern: String,
    remediation: String,
    keywords: String,
    score: f64,
}

/// Build the managed OMNIX memory injection block from experience memories,
/// ranked by relevance to `workspace_path` (empty = recency only). Returns `None`
/// when there are no active experience memories. Shared by the live injection
/// (`agent::inject_workspace_memories`) and the evolution preview command.
pub fn build_memory_block(db: &DbManager, workspace_path: &str) -> Result<Option<String>, String> {
    let ws_tokens = tokenize(&derive_workspace_signals(workspace_path));

    // Read everything in a scope so the connection + statement drop before scoring.
    #[allow(clippy::type_complexity)]
    let (ws_embedding, raw): (
        Option<Vec<f32>>,
        Vec<(String, String, String, String, String, f64, Option<Vec<u8>>, i64)>,
    ) = {
        let conn = db.get_connection().map_err(|e| e.to_string())?;
        let ws_embedding: Option<Vec<f32>> = if workspace_path.is_empty() {
            None
        } else {
            conn.query_row(
                "SELECT embedding, dimensions FROM workspace_profiles WHERE workspace_path = ?1",
                params![workspace_path],
                |r| Ok((r.get::<_, Option<Vec<u8>>>(0)?, r.get::<_, i64>(1)?)),
            )
            .optional()
            .ok()
            .flatten()
            .and_then(|(blob, dim)| {
                blob.filter(|b| !b.is_empty() && dim > 0)
                    .map(|b| blob_to_vec_f32(&b, dim as usize))
            })
        };

        let mut stmt = conn
            .prepare(
                "SELECT incident_desc, code_pattern, remediation, keywords, stack_tags, confidence, embedding, dimensions
                 FROM memories
                 WHERE type = 'experience' AND (status = 'active' OR status IS NULL OR status = '')
                 ORDER BY created_at DESC LIMIT 500",
            )
            .map_err(|e| e.to_string())?;
        let raw = stmt
            .query_map([], |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get::<_, Option<String>>(4)?.unwrap_or_default(),
                    r.get::<_, Option<f64>>(5)?.unwrap_or(1.0),
                    r.get::<_, Option<Vec<u8>>>(6)?,
                    r.get::<_, i64>(7)?,
                ))
            })
            .map_err(|e| e.to_string())?
            .flatten()
            .collect();
        (ws_embedding, raw)
    };

    if raw.is_empty() {
        return Ok(None);
    }

    let use_relevance = !ws_tokens.is_empty();
    let mut scored: Vec<ScoredMemory> = raw
        .into_iter()
        .enumerate()
        .map(
            |(idx, (desc, pattern, remediation, keywords, stack_tags, confidence, emb_blob, dim))| {
                let recency = 1.0 / (1.0 + idx as f64); // rows are created_at DESC
                let mut score = recency * 0.3 + (confidence - 1.0).max(0.0) * 0.2;
                if use_relevance {
                    let mem_tokens =
                        tokenize(&format!("{keywords} {stack_tags} {desc} {pattern}"));
                    let overlap =
                        ws_tokens.iter().filter(|t| mem_tokens.contains(*t)).count() as f64;
                    score += overlap / (ws_tokens.len() as f64).max(1.0);
                    if let (Some(ws_emb), Some(blob)) = (&ws_embedding, emb_blob) {
                        if dim > 0 && !blob.is_empty() {
                            let mem_emb = blob_to_vec_f32(&blob, dim as usize);
                            score += cosine_similarity(ws_emb, &mem_emb);
                        }
                    }
                }
                ScoredMemory { desc, pattern, remediation, keywords, score }
            },
        )
        .collect();

    if use_relevance {
        scored.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }

    let mut md = String::new();
    md.push_str("\n<!--- OMNIX MEMORY START --->\n");
    md.push_str("## 🧠 OMNIX Anti-Failure Guidelines & Memory Bank\n");
    md.push_str(
        "以下是历史项目踩坑事故记录与规约，请在此工作区内严加防范，避免重犯相同错误：\n\n",
    );
    for (i, m) in scored.into_iter().take(MAX_INJECT).enumerate() {
        md.push_str(&format!("### ❌ 坑点 {}: {}\n", i + 1, m.desc));
        md.push_str(&format!("* **危险模式/命令**: `{}`\n", m.pattern));
        md.push_str(&format!("* **安全修复方案**: {}\n", m.remediation));
        md.push_str(&format!("* **相关标签**: `{}`\n\n", m.keywords));
    }
    md.push_str("<!--- OMNIX MEMORY END --->\n");
    Ok(Some(md))
}

// ─────────────────────────── commands ───────────────────────────

/// Preview the memory block OMNIX auto-injects into agents' context files.
#[tauri::command]
pub fn get_lessons_preview(
    workspace_path: Option<String>,
    db: State<'_, Arc<DbManager>>,
) -> Result<LessonsInfo, String> {
    let ws = workspace_path.unwrap_or_default();
    let content =
        build_memory_block(&db, &ws)?.unwrap_or_else(|| "（暂无可注入经验）".to_string());
    Ok(LessonsInfo { count: experience_count(&db), content })
}

/// Embed experience memories that lack an embedding (+ derive stack tags), so the
/// injection path can rank them by semantic relevance. Best-effort; returns count.
#[tauri::command]
pub async fn reindex_memory_embeddings(
    model_name: Option<String>,
    db: State<'_, Arc<DbManager>>,
) -> Result<usize, String> {
    let model = model_name
        .or_else(|| default_embedding_model(&db))
        .ok_or_else(|| "没有可用的嵌入模型（请在模型中心启用一个支持 embedding 的模型）".to_string())?;

    // Gather rows lacking embeddings — DB guard dropped before the await.
    let rows: Vec<(String, String)> = {
        let conn = db.get_connection().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT id, incident_desc || ' ' || code_pattern || ' ' || remediation || ' ' || keywords
                 FROM memories
                 WHERE type = 'experience' AND (embedding IS NULL OR dimensions = 0)",
            )
            .map_err(|e| e.to_string())?;
        let collected: Vec<(String, String)> = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            .map_err(|e| e.to_string())?
            .flatten()
            .collect();
        collected
    };
    if rows.is_empty() {
        return Ok(0);
    }

    let texts: Vec<String> = rows.iter().map(|(_, t)| t.clone()).collect();
    let embeddings = generate_embeddings(&db, texts, &model, None).await?;

    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let mut n = 0;
    for ((id, text), emb) in rows.iter().zip(embeddings.iter()) {
        let blob = vec_f32_to_blob(emb);
        let tags = derive_stack_tags(text);
        let _ = conn.execute(
            "UPDATE memories SET embedding = ?1, dimensions = ?2, stack_tags = ?3 WHERE id = ?4",
            params![blob, emb.len() as i64, tags, id],
        );
        n += 1;
    }
    Ok(n)
}

fn union_keywords(a: &str, b: &str) -> String {
    let mut set: Vec<String> = Vec::new();
    for kw in a.split(',').chain(b.split(',')) {
        let k = kw.trim().to_string();
        if !k.is_empty() && !set.iter().any(|e| e.eq_ignore_ascii_case(&k)) {
            set.push(k);
        }
    }
    set.join(",")
}

/// Merge near-duplicate active memories using cosine over their stored embeddings.
/// Keeps the higher-confidence memory, unions keywords, bumps its confidence, and
/// marks the duplicate `status='merged'` (kept, not deleted, so it can be audited).
/// Requires embeddings — run `reindex_memory_embeddings` first. Returns merged count.
#[tauri::command]
pub fn consolidate_memories(db: State<'_, Arc<DbManager>>) -> Result<usize, String> {
    consolidate_core(&db)
}

pub(crate) fn consolidate_core(db: &DbManager) -> Result<usize, String> {
    const THRESHOLD: f64 = 0.92;
    let conn = db.get_connection().map_err(|e| e.to_string())?;

    struct M {
        id: String,
        keywords: String,
        confidence: f64,
        emb: Vec<f32>,
    }
    let mut mems: Vec<M> = {
        let mut stmt = conn
            .prepare(
                "SELECT id, keywords, confidence, embedding, dimensions FROM memories
                 WHERE type = 'experience' AND (status = 'active' OR status IS NULL OR status = '')
                   AND embedding IS NOT NULL AND dimensions > 0
                 ORDER BY confidence DESC, created_at DESC",
            )
            .map_err(|e| e.to_string())?;
        let collected: Vec<M> = stmt
            .query_map([], |r| {
                let blob: Vec<u8> = r.get(3)?;
                let dim: i64 = r.get(4)?;
                Ok(M {
                    id: r.get(0)?,
                    keywords: r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                    confidence: r.get::<_, Option<f64>>(2)?.unwrap_or(1.0),
                    emb: blob_to_vec_f32(&blob, dim as usize),
                })
            })
            .map_err(|e| e.to_string())?
            .flatten()
            .collect();
        collected
    };

    let mut merged: Vec<bool> = vec![false; mems.len()];
    let mut ops: Vec<(usize, usize)> = Vec::new(); // (keep, drop)
    for i in 0..mems.len() {
        if merged[i] {
            continue;
        }
        for j in (i + 1)..mems.len() {
            if merged[j] {
                continue;
            }
            if cosine_similarity(&mems[i].emb, &mems[j].emb) >= THRESHOLD {
                merged[j] = true;
                ops.push((i, j));
            }
        }
    }

    let mut count = 0;
    for (keep, drop) in ops {
        let union = union_keywords(&mems[keep].keywords.clone(), &mems[drop].keywords.clone());
        let new_conf = mems[keep].confidence + 0.5;
        mems[keep].confidence = new_conf;
        mems[keep].keywords = union.clone();
        let _ = conn.execute(
            "UPDATE memories SET keywords = ?1, confidence = ?2, seen_count = seen_count + 1 WHERE id = ?3",
            params![union, new_conf, mems[keep].id],
        );
        let _ = conn.execute(
            "UPDATE memories SET status = 'merged' WHERE id = ?1",
            params![mems[drop].id],
        );
        count += 1;
    }
    Ok(count)
}

/// Compute & cache a workspace's embedding from its local stack signals, so the
/// synchronous inject path can score relevance without a network call.
#[tauri::command]
pub async fn refresh_workspace_profile(
    workspace_path: String,
    model_name: Option<String>,
    db: State<'_, Arc<DbManager>>,
) -> Result<bool, String> {
    let signals = derive_workspace_signals(&workspace_path);
    if signals.trim().is_empty() {
        return Ok(false);
    }
    let model = model_name.or_else(|| default_embedding_model(&db));
    let (blob, dim): (Option<Vec<u8>>, i64) = match model {
        Some(model) => match generate_embeddings(&db, vec![signals.clone()], &model, None).await {
            Ok(mut embs) if !embs.is_empty() => {
                let e = embs.remove(0);
                (Some(vec_f32_to_blob(&e)), e.len() as i64)
            }
            _ => (None, 0),
        },
        None => (None, 0),
    };

    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO workspace_profiles (workspace_path, embedding, dimensions, signals, updated_at)
         VALUES (?1, ?2, ?3, ?4, datetime('now'))
         ON CONFLICT(workspace_path) DO UPDATE SET
            embedding = excluded.embedding,
            dimensions = excluded.dimensions,
            signals = excluded.signals,
            updated_at = datetime('now')",
        params![workspace_path, blob, dim, signals],
    )
    .map_err(|e| e.to_string())?;
    Ok(true)
}

#[cfg(test)]
mod evolution_tests {
    use super::*;

    // ───────── fast unit tests (no DB) ─────────

    #[test]
    fn union_keywords_dedups_case_insensitive() {
        assert_eq!(union_keywords("rust, async", "Async,deadlock"), "rust,async,deadlock");
    }

    #[test]
    fn tokenize_splits_and_filters() {
        let t = tokenize("Tokio::sync, deadlock!");
        assert!(t.contains("tokio") && t.contains("sync") && t.contains("deadlock"));
        assert!(!t.contains("")); // 1-char/empty filtered
    }

    #[test]
    fn workspace_signals_detect_language() {
        let ws = std::env::temp_dir().join(format!("omnix_sig_{}_{}", std::process::id(), now_nanos()));
        std::fs::create_dir_all(&ws).unwrap();
        std::fs::write(ws.join("Cargo.toml"), "[package]").unwrap();
        let sig = derive_workspace_signals(ws.to_str().unwrap());
        assert!(sig.to_lowercase().contains("rust"), "got: {sig}");
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn stack_tags_from_text() {
        assert!(derive_stack_tags("std::sync::Mutex across await in tokio").contains("rust"));
        assert!(derive_stack_tags("pip install python venv").contains("python"));
    }

    // ───────── DB-backed integration (full schema; run with --ignored) ─────────

    fn now_nanos() -> u128 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    }

    // Minimal schema (no full seed — the full-seed path is slow/heavy in tests).
    fn temp_db() -> Arc<DbManager> {
        let p = std::env::temp_dir()
            .join(format!("omnix_evo_{}_{}.db", std::process::id(), now_nanos()));
        let _ = std::fs::remove_file(&p);
        let db = DbManager::new_runtime_test(p);
        db.get_connection()
            .unwrap()
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS memories (
                    id TEXT PRIMARY KEY, incident_desc TEXT, code_pattern TEXT, remediation TEXT,
                    keywords TEXT, type TEXT DEFAULT 'experience', status TEXT DEFAULT 'active',
                    confidence REAL DEFAULT 1, seen_count INTEGER DEFAULT 0,
                    repeated_count INTEGER DEFAULT 0, last_matched_at TEXT,
                    stack_tags TEXT DEFAULT '', embedding BLOB, dimensions INTEGER DEFAULT 0,
                    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
                );
                CREATE TABLE IF NOT EXISTS workspace_profiles (
                    workspace_path TEXT PRIMARY KEY, embedding BLOB, dimensions INTEGER DEFAULT 0,
                    signals TEXT DEFAULT '', updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
                );",
            )
            .unwrap();
        Arc::new(db)
    }

    fn insert_mem(db: &DbManager, id: &str, desc: &str, pattern: &str, keywords: &str) {
        let conn = db.get_connection().unwrap();
        conn.execute(
            "INSERT INTO memories (id, incident_desc, code_pattern, remediation, keywords, type, status)
             VALUES (?1, ?2, ?3, ?4, 'fix it', 'experience', 'active')",
            params![id, desc, pattern, keywords],
        )
        .unwrap();
    }

    fn mk_workspace(marker: &str, content: &str) -> std::path::PathBuf {
        let ws = std::env::temp_dir()
            .join(format!("omnix_ws_{}_{}", std::process::id(), now_nanos()));
        std::fs::create_dir_all(&ws).unwrap();
        std::fs::write(ws.join(marker), content).unwrap();
        ws
    }

    #[test]
    fn relevance_prefers_matching_stack() {
        let db = temp_db();
        db.get_connection().unwrap().execute("DELETE FROM memories", []).unwrap();
        insert_mem(&db, "m_rust", "Tokio deadlock", "std::sync::MutexGuard across await", "rust,tokio,async,deadlock");
        insert_mem(&db, "m_py", "Python venv issue", "pip install wrong env", "python,pip,venv");

        let rust_ws = mk_workspace("Cargo.toml", "[package]\nname=\"x\"");
        let py_ws = mk_workspace("requirements.txt", "flask\n");

        let rblock = build_memory_block(&db, rust_ws.to_str().unwrap()).unwrap().unwrap();
        let pblock = build_memory_block(&db, py_ws.to_str().unwrap()).unwrap().unwrap();

        assert!(
            rblock.find("Tokio deadlock").unwrap() < rblock.find("Python venv").unwrap(),
            "rust workspace should rank the rust memory first:\n{rblock}"
        );
        assert!(
            pblock.find("Python venv").unwrap() < pblock.find("Tokio deadlock").unwrap(),
            "python workspace should rank the python memory first:\n{pblock}"
        );

        let _ = std::fs::remove_dir_all(&rust_ws);
        let _ = std::fs::remove_dir_all(&py_ws);
    }

    #[test]
    fn consolidate_merges_identical_embeddings() {
        let db = temp_db();
        db.get_connection().unwrap().execute("DELETE FROM memories", []).unwrap();
        insert_mem(&db, "a", "deadlock A", "p", "rust");
        insert_mem(&db, "b", "deadlock B", "p", "tokio");
        // identical embeddings → cosine 1.0 → must merge
        let emb = vec_f32_to_blob(&[1.0f32, 0.0, 0.0]);
        db.get_connection()
            .unwrap()
            .execute("UPDATE memories SET embedding=?1, dimensions=3", params![emb])
            .unwrap();

        let merged = consolidate_core(&db).unwrap();
        assert_eq!(merged, 1, "exactly one near-duplicate should be merged");

        let conn = db.get_connection().unwrap();
        let active: i64 = conn
            .query_row("SELECT COUNT(*) FROM memories WHERE status='active'", [], |r| r.get(0))
            .unwrap();
        let merged_cnt: i64 = conn
            .query_row("SELECT COUNT(*) FROM memories WHERE status='merged'", [], |r| r.get(0))
            .unwrap();
        assert_eq!((active, merged_cnt), (1, 1));
    }
}

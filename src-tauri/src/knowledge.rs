/**
 * OMNIX Knowledge Base — Chunking Engine + Embedding Client + Hybrid Search + RAG Orchestrator
 *
 * This module implements the full RAG pipeline:
 *   1. Document chunking (boundary-aware for markdown, code, and plain text)
 *   2. Embedding generation via Ollama / OpenAI-compatible APIs
 *   3. BM25 full-text search via SQLite FTS5
 *   4. Vector similarity search (brute-force cosine)
 *   5. Reciprocal Rank Fusion (RRF) to merge BM25 + vector results
 *   6. RAG query orchestration (retrieve → augment → generate)
 */
use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::db::DbManager;

// ── Chunking Engine ─────────────────────────────────────

/// Configuration for the chunking strategy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkConfig {
    pub max_chunk_chars: usize,
    pub overlap_chars: usize,
    pub respect_boundaries: bool,
}

impl Default for ChunkConfig {
    fn default() -> Self {
        Self {
            max_chunk_chars: 512,
            overlap_chars: 64,
            respect_boundaries: true,
        }
    }
}

/// A single chunk produced from a document
#[derive(Debug, Clone)]
pub struct Chunk {
    pub index: usize,
    pub content: String,
    pub char_start: usize,
    pub char_end: usize,
    pub metadata: serde_json::Value,
}

/// Chunk a document's text content into overlapping pieces.
///
/// - **Markdown**: split at `##` headings (heading becomes metadata),
///   then by paragraph `\n\n` within each section.
/// - **Code**: split at function/class/impl boundaries, language stored in metadata.
/// - **Text**: sliding window with paragraph-boundary awareness.
pub fn chunk_document(content: &str, file_type: &str, config: &ChunkConfig) -> Vec<Chunk> {
    match file_type {
        "markdown" | "md" => chunk_markdown(content, config),
        "code" => chunk_code(content, config),
        _ => chunk_text(content, config),
    }
}

fn chunk_markdown(content: &str, config: &ChunkConfig) -> Vec<Chunk> {
    let mut chunks = Vec::new();
    let mut current_heading;

    // Split by heading lines (## ...)
    let lines: Vec<&str> = content.lines().collect();
    let mut section_breaks: Vec<(usize, String)> = vec![(0, String::new())]; // (char_offset, heading)

    let mut char_offset = 0usize;
    for line in &lines {
        let trimmed = line.trim_start();
        if trimmed.starts_with("##") && config.respect_boundaries {
            let heading = trimmed.trim_start_matches('#').trim().to_string();
            section_breaks.push((char_offset, heading));
        }
        char_offset += line.len() + 1; // +1 for \n
    }
    // Add end boundary
    section_breaks.push((content.len(), String::new()));

    // For each section, split by paragraphs if too long
    for i in 0..section_breaks.len() - 1 {
        let (start, heading) = &section_breaks[i];
        let (end, _) = &section_breaks[i + 1];
        if start >= end {
            continue;
        }
        current_heading = heading.clone();
        let section = &content[*start..*end];

        if section.len() <= config.max_chunk_chars {
            chunks.push(Chunk {
                index: chunks.len(),
                content: section.to_string(),
                char_start: *start,
                char_end: *end,
                metadata: serde_json::json!({ "heading": current_heading }),
            });
        } else {
            // Split by paragraphs
            let sub_chunks = split_by_paragraphs(section, *start, &current_heading, config);
            chunks.extend(sub_chunks);
        }
    }

    apply_overlap(&mut chunks, config);
    chunks
}

fn chunk_code(content: &str, config: &ChunkConfig) -> Vec<Chunk> {
    let mut chunks = Vec::new();
    let lines: Vec<&str> = content.lines().collect();

    let mut current_block_char_start = 0usize;
    let mut current_block_lines: Vec<&str> = Vec::new();
    let mut char_offset = 0usize;

    // Heuristic boundary patterns for function/class/impl definitions
    let boundary_prefixes = [
        "fn ",
        "pub fn ",
        "async fn ",
        "pub async fn ",
        "def ",
        "class ",
        "func ",
        "impl ",
        "pub impl ",
        "interface ",
        "type ",
        "pub type ",
        "enum ",
        "pub enum ",
        "struct ",
        "pub struct ",
        "mod ",
        "pub mod ",
    ];

    for (line_idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        let is_boundary = config.respect_boundaries
            && line_idx > 0
            && boundary_prefixes.iter().any(|p| trimmed.starts_with(p));

        if is_boundary && !current_block_lines.is_empty() {
            let block_content = current_block_lines.join("\n");
            if !block_content.trim().is_empty() {
                chunks.push(Chunk {
                    index: chunks.len(),
                    content: block_content.clone(),
                    char_start: current_block_char_start,
                    char_end: current_block_char_start + block_content.len(),
                    metadata: serde_json::json!({ "language": "code" }),
                });
            }
            current_block_lines.clear();
            current_block_char_start = char_offset;
        }

        if current_block_lines.is_empty() {
            current_block_char_start = char_offset;
        }
        current_block_lines.push(line);

        // If single block is too long, force split at blank lines
        let block_len: usize = current_block_lines.iter().map(|l| l.len() + 1).sum();
        if block_len > config.max_chunk_chars * 2 && trimmed.is_empty() {
            let block_content = current_block_lines.join("\n");
            if !block_content.trim().is_empty() {
                chunks.push(Chunk {
                    index: chunks.len(),
                    content: block_content.clone(),
                    char_start: current_block_char_start,
                    char_end: current_block_char_start + block_content.len(),
                    metadata: serde_json::json!({ "language": "code" }),
                });
            }
            current_block_lines.clear();
        }

        char_offset += line.len() + 1;
    }

    // Flush remaining
    if !current_block_lines.is_empty() {
        let block_content = current_block_lines.join("\n");
        if !block_content.trim().is_empty() {
            chunks.push(Chunk {
                index: chunks.len(),
                content: block_content.clone(),
                char_start: current_block_char_start,
                char_end: current_block_char_start + block_content.len(),
                metadata: serde_json::json!({ "language": "code" }),
            });
        }
    }

    apply_overlap(&mut chunks, config);
    chunks
}

fn chunk_text(content: &str, config: &ChunkConfig) -> Vec<Chunk> {
    let chunks = split_by_paragraphs(content, 0, "", config);
    let mut chunks = chunks;
    apply_overlap(&mut chunks, config);
    chunks
}

/// Split text into chunks at paragraph boundaries, staying under max_chunk_chars.
fn split_by_paragraphs(
    text: &str,
    base_offset: usize,
    heading: &str,
    config: &ChunkConfig,
) -> Vec<Chunk> {
    let mut chunks = Vec::new();
    let paragraphs: Vec<&str> = text.split("\n\n").collect();

    let mut current_content = String::new();
    let mut para_start_offset = base_offset;

    // Calculate absolute offsets for each paragraph
    let mut para_offsets = Vec::new();
    let mut off = base_offset;
    for para in &paragraphs {
        para_offsets.push(off);
        off += para.len() + 2; // +2 for "\n\n"
    }

    for (i, para) in paragraphs.iter().enumerate() {
        if current_content.is_empty() {
            para_start_offset = para_offsets[i];
        }

        if current_content.len() + para.len() + 2 > config.max_chunk_chars
            && !current_content.is_empty()
        {
            let end = para_start_offset + current_content.len();
            chunks.push(Chunk {
                index: chunks.len(),
                content: current_content.trim().to_string(),
                char_start: para_start_offset,
                char_end: end,
                metadata: serde_json::json!({ "heading": heading }),
            });
            current_content = para.to_string();
            para_start_offset = para_offsets[i];
        } else {
            if !current_content.is_empty() {
                current_content.push_str("\n\n");
            }
            current_content.push_str(para);
        }
    }

    // Flush remaining
    if !current_content.trim().is_empty() {
        let end = para_start_offset + current_content.len();
        chunks.push(Chunk {
            index: chunks.len(),
            content: current_content.trim().to_string(),
            char_start: para_start_offset,
            char_end: end,
            metadata: serde_json::json!({ "heading": heading }),
        });
    }

    chunks
}

/// Apply overlap: prepend trailing text from previous chunk to current chunk.
fn apply_overlap(chunks: &mut [Chunk], config: &ChunkConfig) {
    if config.overlap_chars == 0 || chunks.len() <= 1 {
        return;
    }
    for i in 1..chunks.len() {
        let prev = &chunks[i - 1].content;
        let overlap_text = prev
            .chars()
            .rev()
            .take(config.overlap_chars)
            .collect::<String>();
        let overlap_text: String = overlap_text.chars().rev().collect();
        // Prepend overlap to current chunk
        chunks[i].content = format!("{}…\n{}", overlap_text, chunks[i].content);
    }
}

// ── Vector Serialization ────────────────────────────────

/// Serialize a Vec<f32> into little-endian bytes (4 bytes per f32).
pub fn vec_f32_to_blob(vec: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(vec.len() * 4);
    for &v in vec {
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    bytes
}

/// Deserialize little-endian bytes into a Vec<f32>.
pub fn blob_to_vec_f32(blob: &[u8], dimensions: usize) -> Vec<f32> {
    blob.chunks_exact(4)
        .take(dimensions)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

/// Compute cosine similarity between two f32 vectors.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let dot: f64 = a
        .iter()
        .zip(b.iter())
        .map(|(x, y)| (*x as f64) * (*y as f64))
        .sum();
    let norm_a: f64 = a.iter().map(|x| (*x as f64).powi(2)).sum::<f64>().sqrt();
    let norm_b: f64 = b.iter().map(|x| (*x as f64).powi(2)).sum::<f64>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    dot / (norm_a * norm_b)
}

// ── Embedding Client ────────────────────────────────────

/// Resolve the best available embedding platform + API details.
pub fn resolve_embedding_platform(
    db: &DbManager,
    model_name: &str,
    platform_id: Option<&str>,
) -> Result<(String, String, String, String), String> {
    // (api_key, api_address, api_type, actual_model_name)
    let conn = db.get_connection().map_err(|e| e.to_string())?;

    if let Some(pid) = platform_id {
        // Use specified platform
        let (api_key, api_address, api_type) = conn
            .prepare(
                "SELECT COALESCE(
                    (SELECT encrypted_key FROM platform_api_keys
                     WHERE platform_id = mp.id AND is_active = 1 AND is_enabled = 1
                     ORDER BY priority DESC, created_at ASC LIMIT 1),
                    mp.api_key
                 ), mp.api_address, mp.api_type
                 FROM model_platforms mp WHERE mp.id = ?1 AND mp.is_enabled = 1",
            )
            .map_err(|e| e.to_string())?
            .query_row(params![pid], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(|e| format!("Platform '{}' not found or disabled: {}", pid, e))?;
        return Ok((api_key, api_address, api_type, model_name.to_string()));
    }

    // Auto-detect: find platform_models where has_embedding = 1
    let result = conn
        .prepare(
            "SELECT pm.model_name, mp.id, mp.api_key, mp.api_address, mp.api_type
             FROM platform_models pm
             JOIN model_platforms mp ON pm.platform_id = mp.id
             WHERE pm.has_embedding = 1 AND pm.is_enabled = 1 AND mp.is_enabled = 1
               AND pm.model_name = ?1
             LIMIT 1",
        )
        .map_err(|e| e.to_string())?
        .query_row(params![model_name], |row| {
            Ok((
                row.get::<_, String>(2)?, // api_key
                row.get::<_, String>(3)?, // api_address
                row.get::<_, String>(4)?, // api_type
                row.get::<_, String>(0)?, // model_name
            ))
        })
        .map_err(|e| {
            format!(
                "No enabled embedding platform found for model '{}': {}",
                model_name, e
            )
        })?;

    Ok(result)
}

/// Generate embeddings for a batch of texts using the specified model.
///
/// Supports:
/// - **Ollama**: `POST {api_address}/api/embeddings` (single text per call)
/// - **OpenAI-compatible**: `POST {api_address}/embeddings` (batch up to 64 texts)
pub async fn generate_embeddings(
    db: &DbManager,
    texts: Vec<String>,
    model_name: &str,
    platform_id: Option<&str>,
) -> Result<Vec<Vec<f32>>, String> {
    let (api_key, api_address, api_type, actual_model) =
        resolve_embedding_platform(db, model_name, platform_id)?;

    // 上游地址是用户配置的，可能是 localhost（Ollama / 本地 vLLM）：
    // 回环绕开系统代理，公网保留——写死任何一边都会错一半。
    let client = crate::storage::client_for_url(&api_address, std::time::Duration::from_secs(30));

    let mut all_embeddings: Vec<Vec<f32>> = Vec::with_capacity(texts.len());

    match api_type.as_str() {
        "ollama" => {
            // Ollama's /api/embeddings takes a single prompt
            for text in &texts {
                let url = format!("{}/api/embeddings", api_address.trim_end_matches('/'));
                let body = serde_json::json!({
                    "model": actual_model,
                    "prompt": text,
                });
                let mut req = client.post(&url).json(&body);
                if !api_key.trim().is_empty() {
                    req = req.header("Authorization", format!("Bearer {}", api_key.trim()));
                }
                let resp = req
                    .send()
                    .await
                    .map_err(|e| format!("Ollama embedding request failed: {}", e))?;
                if !resp.status().is_success() {
                    let status = resp.status();
                    let body = resp.text().await.unwrap_or_default();
                    return Err(format!("Ollama embedding API error ({}): {}", status, body));
                }
                let json: serde_json::Value = resp
                    .json()
                    .await
                    .map_err(|e| format!("Failed to parse Ollama embedding response: {}", e))?;
                let embedding = json["embedding"]
                    .as_array()
                    .ok_or("Missing 'embedding' array in Ollama response")?
                    .iter()
                    .map(|v| {
                        v.as_f64()
                            .map(|f| f as f32)
                            .ok_or("Invalid f32 in embedding")
                    })
                    .collect::<Result<Vec<f32>, _>>()
                    .map_err(|_| "Invalid embedding value")?;
                all_embeddings.push(embedding);
            }
        }
        _ => {
            // OpenAI-compatible: batch up to 64 texts per request
            let batch_size = 64;
            for chunk in texts.chunks(batch_size) {
                let url = format!("{}/embeddings", api_address.trim_end_matches('/'));
                let body = serde_json::json!({
                    "model": actual_model,
                    "input": chunk,
                });
                let mut req = client.post(&url).json(&body);
                if !api_key.trim().is_empty() {
                    req = req.header("Authorization", format!("Bearer {}", api_key.trim()));
                }
                let resp = req
                    .send()
                    .await
                    .map_err(|e| format!("Embedding request failed: {}", e))?;
                if !resp.status().is_success() {
                    let status = resp.status();
                    let body = resp.text().await.unwrap_or_default();
                    return Err(format!("Embedding API error ({}): {}", status, body));
                }
                let json: serde_json::Value = resp
                    .json()
                    .await
                    .map_err(|e| format!("Failed to parse embedding response: {}", e))?;
                let data = json["data"]
                    .as_array()
                    .ok_or("Missing 'data' array in embedding response")?;
                // Results may not be in order; sort by index
                let mut indexed: Vec<(usize, Vec<f32>)> = Vec::new();
                for item in data {
                    let idx = item["index"]
                        .as_u64()
                        .ok_or("Missing 'index' in embedding data")?
                        as usize;
                    let embedding = item["embedding"]
                        .as_array()
                        .ok_or("Missing 'embedding' array in data item")?
                        .iter()
                        .map(|v| {
                            v.as_f64()
                                .map(|f| f as f32)
                                .ok_or("Invalid f32 in embedding")
                        })
                        .collect::<Result<Vec<f32>, _>>()
                        .map_err(|_| "Invalid embedding value")?;
                    indexed.push((idx, embedding));
                }
                indexed.sort_by_key(|(i, _)| *i);
                for (_, emb) in indexed {
                    all_embeddings.push(emb);
                }
            }
        }
    }

    Ok(all_embeddings)
}

// ── Search Engine ───────────────────────────────────────

/// A single search result from hybrid search
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub chunk_id: String,
    pub document_id: String,
    pub document_title: String,
    pub knowledge_base_id: String,
    pub knowledge_base_name: String,
    pub content: String,
    pub metadata: serde_json::Value,
    pub bm25_score: Option<f64>,
    pub vector_score: Option<f64>,
    pub rrf_score: f64,
    pub rank: usize,
}

/// BM25 full-text search using FTS5.
///
/// 这一路曾经**对任何语言都返回空**，而且看不出来。两个原因叠在一起：
///
/// 1. `kb_chunks_fts` 是外部内容表，取列值要回 `kb_chunks` 按 rowid 查同名列，
///    而 fts 的列叫 `chunk_id`、源表主键叫 `id`，于是每一行都报
///    `no such column: T.chunk_id`。`hybrid_search` 又用 `.unwrap_or_default()`
///    把这个硬错误吞成了「没有关键词命中」。
/// 2. `unicode61` 把一整段中文当成**一个** token，所以即便查询能跑，
///    「量子计算」也匹配不到「量子计算的进展很快」。
///
/// 现在 fts 是独立表（`chunk_id` 是它自己的 UNINDEXED 列，不再回源），索引和
/// 查询都先过 `segment_for_index`——CJK 连写段切成二元组，英文原样走 porter。
/// 两字词、四字词、句中词都能命中（见 `bm25_chinese_matches_words_not_just_whole_runs`），
/// 不相干的词不会命中（`bm25_chinese_does_not_match_unrelated_words`）。
///
/// 代价：二元切分的精度不如真正的分词器（相邻二元组可能跨词），偶尔会多召回。
/// BM25 只是混合检索的一路，RRF 融合和向量那一路会把排序拉回来。要更准就得上
/// ICU 分词器（需要带 ICU 的 SQLite 构建）或自带词典，那是另一件事。
///
/// Returns (chunk_id, bm25_score) pairs. FTS5's rank is negative (more negative = better),
/// so we negate it for consistency.
///
/// `knowledge_base_ids` restricts the search **inside SQL**. It used to be a
/// post-filter over a globally-ranked top-5000: with a corpus bigger than that,
/// a small selected base whose chunks all ranked below 5000 silently returned
/// nothing. The join makes `limit` mean what it says.
pub fn bm25_search(
    db: &DbManager,
    query: &str,
    knowledge_base_ids: Option<&[String]>,
    limit: usize,
) -> Result<Vec<(String, f64)>, String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;

    // Escape special FTS5 characters in query
    let safe_query = query
        .replace('"', "")
        .replace("'", "")
        .replace(":", " ")
        .replace("*", " ")
        .replace("OR", "")
        .replace("AND", "")
        .replace("NOT", "");

    let scope = knowledge_base_ids.filter(|ids| !ids.is_empty());
    // `chunk_id` 现在是 fts 表自己的 UNINDEXED 列，直接取，不再回源表。
    let mut sql = String::from("SELECT f.chunk_id, f.rank AS bm25_score FROM kb_chunks_fts f");
    if scope.is_some() {
        sql.push_str(
            " JOIN kb_chunks c ON c.id = f.chunk_id
              JOIN kb_documents d ON d.id = c.document_id",
        );
    }
    sql.push_str(" WHERE kb_chunks_fts MATCH ?");
    // 查询要按索引时同样的规则切，否则中文永远对不上。
    let mut args: Vec<rusqlite::types::Value> = vec![segment_for_index(&safe_query).into()];
    if let Some(ids) = scope {
        sql.push_str(&format!(" AND d.knowledge_base_id IN ({})", sql_placeholders(ids.len())));
        args.extend(ids.iter().map(|id| rusqlite::types::Value::from(id.clone())));
    }
    sql.push_str(" ORDER BY f.rank LIMIT ?");
    args.push((limit as i64).into());

    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let results = stmt
        .query_map(rusqlite::params_from_iter(args), |row| {
            let chunk_id: String = row.get(0)?;
            let score: f64 = row.get(1)?;
            Ok((chunk_id, -score)) // Negate: FTS5 rank is negative
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    Ok(results)
}

/// 判断一个字符是否属于「连写不分词」的东亚文字（中日韩）。
fn is_cjk(c: char) -> bool {
    matches!(c as u32,
        0x3400..=0x4DBF      // CJK 扩展 A
        | 0x4E00..=0x9FFF    // CJK 基本区
        | 0xF900..=0xFAFF    // 兼容表意文字
        | 0x3040..=0x30FF    // 日文假名
        | 0xAC00..=0xD7AF    // 韩文音节
        | 0x20000..=0x2FA1F  // CJK 扩展 B~F
    )
}

/// 把文本切成 FTS5 能索引的形式：**CJK 连写段落改成二元组**，其余原样。
///
/// FTS5 内置的 `unicode61` 把一整段中文当成**一个** token，于是「量子计算」
/// 匹配不到「量子计算的进展很快」——只有整段一模一样才命中。内置分词器里没有
/// 能用的替代：`trigram` 要三字以上，而中文里最常见的恰恰是两字词。
///
/// 二元切分是 CJK 无分词器时的经典做法：
/// 「量子计算」→「量子 子计 计算」。查询按同样规则切，于是
/// 「量子计算」的三个二元组都能在正文里找到，两字词「量子」也能命中。
/// 代价是精度略降（相邻二元组可能跨词），但 BM25 只是混合检索的一路，
/// RRF 融合和向量那一路会把它拉回来。
///
/// 英文不动：仍然走 `unicode61 + porter`，保留词干还原。
pub fn segment_for_index(text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 2);
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if is_cjk(chars[i]) {
            let start = i;
            while i < chars.len() && is_cjk(chars[i]) {
                i += 1;
            }
            let run = &chars[start..i];
            if run.len() == 1 {
                // 单字成段（如「书」）：没有二元组可切，就索引这个字本身。
                out.push(run[0]);
                out.push(' ');
            } else {
                for pair in run.windows(2) {
                    out.push(pair[0]);
                    out.push(pair[1]);
                    out.push(' ');
                }
            }
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

/// 把一个 chunk 写进全文索引。**所有 `kb_chunks` 插入点都要调它**——
/// 索引不再靠 SQL 触发器同步（触发器调不到 Rust 的分词），所以漏调一处就等于
/// 那批内容搜不到。
pub fn index_chunk(conn: &rusqlite::Connection, chunk_id: &str, content: &str) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO kb_chunks_fts (chunk_id, content) VALUES (?1, ?2)",
        params![chunk_id, segment_for_index(content)],
    )?;
    Ok(())
}

/// 删掉一篇文档所有 chunk 的索引项。删 `kb_chunks` 时同步调用。
pub fn unindex_document(conn: &rusqlite::Connection, document_id: &str) -> rusqlite::Result<()> {
    conn.execute(
        "DELETE FROM kb_chunks_fts
         WHERE chunk_id IN (SELECT id FROM kb_chunks WHERE document_id = ?1)",
        params![document_id],
    )?;
    Ok(())
}

/// `?,?,?` for an `IN (...)` clause.
fn sql_placeholders(n: usize) -> String {
    std::iter::repeat_n("?", n).collect::<Vec<_>>().join(",")
}

/// Vector similarity search using brute-force cosine similarity.
///
/// Scores only the rows that are actually comparable to the query vector, and
/// says so in SQL rather than after loading the table:
///
/// - `model` — **cosine between two different models' vectors is meaningless.**
///   `kb_embeddings.model` had been written since the table existed and never
///   read back; same-dimension models (plenty of them are 1536) scored against
///   each other produced confident-looking numbers that RRF then fused into the
///   RAG context. Different-dimension ones scored a silent 0.0. Neither warned.
/// - `dimensions` — belt to the model's braces, and it keeps a corrupt row from
///   poisoning a whole search.
/// - `knowledge_base_ids` — the scope used to be applied *after* loading every
///   embedding in the database and sorting all of them.
///
/// Brute force is still brute force; what changed is how much it chews through.
pub fn vector_search(
    db: &DbManager,
    query_embedding: &[f32],
    model: &str,
    knowledge_base_ids: Option<&[String]>,
    limit: usize,
) -> Result<Vec<(String, f64)>, String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;

    let scope = knowledge_base_ids.filter(|ids| !ids.is_empty());
    let mut sql = String::from("SELECT e.chunk_id, e.embedding, e.dimensions FROM kb_embeddings e");
    if scope.is_some() {
        sql.push_str(
            " JOIN kb_chunks c ON c.id = e.chunk_id
              JOIN kb_documents d ON d.id = c.document_id",
        );
    }
    sql.push_str(" WHERE e.model = ? AND e.dimensions = ?");
    let mut args: Vec<rusqlite::types::Value> = vec![
        model.to_string().into(),
        (query_embedding.len() as i64).into(),
    ];
    if let Some(ids) = scope {
        sql.push_str(&format!(" AND d.knowledge_base_id IN ({})", sql_placeholders(ids.len())));
        args.extend(ids.iter().map(|id| rusqlite::types::Value::from(id.clone())));
    }

    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let mut scored: Vec<(String, f64)> = stmt
        .query_map(rusqlite::params_from_iter(args), |row| {
            let chunk_id: String = row.get(0)?;
            let blob: Vec<u8> = row.get(1)?;
            let dimensions: i32 = row.get(2)?;
            Ok((chunk_id, blob, dimensions))
        })
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .filter_map(|(chunk_id, blob, dimensions)| {
            let vec = blob_to_vec_f32(&blob, dimensions as usize);
            // `dimensions` is what the row claims; the blob is what it has. A
            // short blob would otherwise score a silent 0.0 and rank as a real
            // (bad) hit — drop it instead.
            (vec.len() == query_embedding.len())
                .then(|| (chunk_id, cosine_similarity(query_embedding, &vec)))
        })
        .collect();

    // Partition around the k-th score instead of ordering the whole candidate
    // set; only the survivors get sorted.
    let by_score_desc =
        |a: &(String, f64), b: &(String, f64)| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal);
    if scored.len() > limit {
        scored.select_nth_unstable_by(limit, by_score_desc);
        scored.truncate(limit);
    }
    scored.sort_by(by_score_desc);

    Ok(scored)
}

/// How many embeddings live in scope, and how many of them the query model can
/// actually read. Used to tell "this base has no embeddings yet" (fine, BM25
/// carries the search) apart from "this base is embedded with another model"
/// (not fine — the vector half is dead and nothing would have said so).
fn embedding_coverage(
    db: &DbManager,
    model: &str,
    knowledge_base_ids: Option<&[String]>,
) -> Result<(i64, i64, Vec<String>), String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let scope = knowledge_base_ids.filter(|ids| !ids.is_empty());

    let mut sql = String::from(
        "SELECT COUNT(*), SUM(CASE WHEN e.model = ? THEN 1 ELSE 0 END) FROM kb_embeddings e",
    );
    let mut args: Vec<rusqlite::types::Value> = vec![model.to_string().into()];
    if let Some(ids) = scope {
        sql.push_str(
            " JOIN kb_chunks c ON c.id = e.chunk_id
              JOIN kb_documents d ON d.id = c.document_id",
        );
        sql.push_str(&format!(" WHERE d.knowledge_base_id IN ({})", sql_placeholders(ids.len())));
        args.extend(ids.iter().map(|id| rusqlite::types::Value::from(id.clone())));
    }
    let (total, matching): (i64, i64) = conn
        .prepare(&sql)
        .map_err(|e| e.to_string())?
        .query_row(rusqlite::params_from_iter(args.clone()), |row| {
            Ok((row.get(0)?, row.get::<_, Option<i64>>(1)?.unwrap_or(0)))
        })
        .map_err(|e| e.to_string())?;

    if total == 0 || matching > 0 {
        return Ok((total, matching, Vec::new()));
    }

    // Nothing matched — name the models that are actually in there, so the
    // error can tell the user what to switch to (or re-embed from).
    let mut names_sql = String::from("SELECT DISTINCT e.model FROM kb_embeddings e");
    let mut names_args: Vec<rusqlite::types::Value> = Vec::new();
    if let Some(ids) = scope {
        names_sql.push_str(
            " JOIN kb_chunks c ON c.id = e.chunk_id
              JOIN kb_documents d ON d.id = c.document_id",
        );
        names_sql.push_str(&format!(" WHERE d.knowledge_base_id IN ({})", sql_placeholders(ids.len())));
        names_args.extend(ids.iter().map(|id| rusqlite::types::Value::from(id.clone())));
    }
    let stored = conn
        .prepare(&names_sql)
        .map_err(|e| e.to_string())?
        .query_map(rusqlite::params_from_iter(names_args), |row| {
            row.get::<_, String>(0)
        })
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();

    Ok((total, matching, stored))
}

/// Reciprocal Rank Fusion: merge BM25 and vector search results.
///
/// For each unique chunk_id across both result sets:
///   rrf_score = 1/(k + bm25_rank) + 1/(k + vector_rank)
///
/// Where rank is 1-based; chunks not present in a list get rank = infinity (contribute 0).
pub fn rrf_fuse(
    bm25_results: Vec<(String, f64)>,
    vector_results: Vec<(String, f64)>,
    k: u32,
    limit: usize,
) -> Vec<SearchResult> {
    use std::collections::HashMap;

    let mut rrf_scores: HashMap<String, (f64, Option<f64>, Option<f64>)> = HashMap::new();

    // BM25 rankings (1-based)
    for (rank_idx, (chunk_id, score)) in bm25_results.iter().enumerate() {
        let rank = (rank_idx + 1) as u32;
        let entry = rrf_scores
            .entry(chunk_id.clone())
            .or_insert((0.0, None, None));
        entry.0 += 1.0 / (k as f64 + rank as f64);
        entry.1 = Some(*score);
    }

    // Vector rankings (1-based)
    for (rank_idx, (chunk_id, score)) in vector_results.iter().enumerate() {
        let rank = (rank_idx + 1) as u32;
        let entry = rrf_scores
            .entry(chunk_id.clone())
            .or_insert((0.0, None, None));
        entry.0 += 1.0 / (k as f64 + rank as f64);
        entry.2 = Some(*score);
    }

    // Sort by RRF score descending
    let mut results: Vec<(String, f64, Option<f64>, Option<f64>)> = rrf_scores
        .into_iter()
        .map(|(chunk_id, (rrf, bm25, vec))| (chunk_id, rrf, bm25, vec))
        .collect();
    results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    results.truncate(limit);

    results
        .into_iter()
        .enumerate()
        .map(
            |(rank, (chunk_id, rrf_score, bm25_score, vector_score))| SearchResult {
                chunk_id,
                document_id: String::new(), // Filled by caller
                document_title: String::new(),
                knowledge_base_id: String::new(),
                knowledge_base_name: String::new(),
                content: String::new(), // Filled by caller
                metadata: serde_json::Value::Null,
                bm25_score,
                vector_score,
                rrf_score,
                rank: rank + 1,
            },
        )
        .collect()
}

/// Top-level hybrid search: BM25 + Vector + RRF.
///
/// 1. Runs BM25 search via FTS5
/// 2. Generates query embedding
/// 3. Runs vector similarity search
/// 4. Fuses results via RRF
/// 5. Enriches results with chunk content and document metadata
#[allow(clippy::too_many_arguments)]  // 混合检索可选参数集，已有调用方依赖此签名
pub async fn hybrid_search(
    db: &DbManager,
    query: &str,
    embedding_model: &str,
    limit: usize,
    bm25_limit: usize,
    vector_limit: usize,
    rrf_k: u32,
    knowledge_base_ids: Option<&[String]>,
) -> Result<Vec<SearchResult>, String> {
    // 1. BM25 search (synchronous SQLite) — scoped in SQL.
    //    Half the search failing is a degraded mode, not a dead end, so it stays
    //    non-fatal — but it no longer happens quietly. A bare `unwrap_or_default`
    //    here is what let a broken FTS5 query pass for "no keyword matches".
    let bm25_results = bm25_search(db, query, knowledge_base_ids, bm25_limit)
        .inspect_err(|e| log::error!("BM25 检索失败，本次只用向量结果：{e}"))
        .unwrap_or_default();

    // 2. Can the vector half run here at all? Two very different "no vector
    //    results" cases used to look identical from the outside.
    let (total_embeddings, usable_embeddings, stored_models) =
        embedding_coverage(db, embedding_model, knowledge_base_ids)?;
    if total_embeddings > 0 && usable_embeddings == 0 {
        return Err(format!(
            "向量检索没法做：所选范围里的 {total_embeddings} 条嵌入是用「{}」生成的，\
             而这次检索用的是「{embedding_model}」。不同模型的向量之间算余弦没有意义\
             （维度相同也一样），所以这里不给你一个看着像模像样的排序。\
             请换回同一个嵌入模型检索，或者用当前模型重新生成嵌入。",
            stored_models.join("、"),
        ));
    }

    // 3. Query embedding + vector search. Skipped entirely when nothing in
    //    scope is embedded yet — that's a normal state for a freshly indexed
    //    base, and BM25 carries the search. (It also saves an API round-trip
    //    that used to be spent producing a vector with nothing to compare to.)
    let vector_results = if usable_embeddings > 0 {
        let query_embeddings =
            generate_embeddings(db, vec![query.to_string()], embedding_model, None).await?;
        let query_embedding = query_embeddings
            .into_iter()
            .next()
            .ok_or("Failed to generate query embedding")?;
        // 这一段搬到阻塞线程池上跑。暴力余弦的开销**随知识库大小无上限地涨**：
        // 范围内每条嵌入都要把 blob 读出来、反序列化成 f32、算一遍点积。放在
        // 异步线程上就是拿一个几十毫秒起步的纯计算去堵整个 runtime——同一个
        // runtime 上还挂着网关的请求转发和会话事件。
        //
        // `DbManager` 的 Clone 很便宜（r2d2 连接池内部是引用计数的），它那条
        // 注释写的就是这个用途。
        //
        // BM25 那半边留在原地：它走 FTS5 索引，结果集有上限，代价不随语料线性
        // 涨。真正需要让开的是这一段。
        let db_for_search = db.clone();
        let model = embedding_model.to_string();
        let scope = knowledge_base_ids.map(|ids| ids.to_vec());
        tokio::task::spawn_blocking(move || {
            vector_search(
                &db_for_search,
                &query_embedding,
                &model,
                scope.as_deref(),
                vector_limit,
            )
        })
        .await
        .map_err(|e| format!("向量检索线程异常退出：{e}"))?
        .inspect_err(|e| log::error!("向量检索失败，本次只用 BM25 结果：{e}"))
        .unwrap_or_default()
    } else {
        Vec::new()
    };

    // 4. RRF fusion
    let mut results = rrf_fuse(bm25_results, vector_results, rrf_k, limit);

    // 5. Enrich with chunk content and document metadata
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    for result in &mut results {
        let chunk_id = &result.chunk_id;
        // Get chunk content + document_id
        let (content, document_id, document_title, base_id, base_name, metadata_str): (
            String,
            String,
            String,
            String,
            String,
            String,
        ) = conn
            .prepare(
                "SELECT c.content, c.document_id, d.title, d.knowledge_base_id,
                        COALESCE(b.name, '默认知识库'), c.metadata
                 FROM kb_chunks c
                 JOIN kb_documents d ON d.id = c.document_id
                 LEFT JOIN knowledge_bases b ON b.id = d.knowledge_base_id
                 WHERE c.id = ?1",
            )
            .map_err(|e| e.to_string())?
            .query_row(params![chunk_id], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            })
            .map_err(|e| e.to_string())?;

        result.content = content;
        result.document_id = document_id;
        result.document_title = document_title;
        result.knowledge_base_id = base_id;
        result.knowledge_base_name = base_name;
        result.metadata = serde_json::from_str(&metadata_str).unwrap_or(serde_json::Value::Null);
    }

    Ok(results)
}

// ── RAG Orchestrator ────────────────────────────────────

/// RAG query response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RagResponse {
    pub answer: String,
    pub sources: Vec<SearchResult>,
    pub query: String,
}

/// RAG query: retrieve relevant chunks and generate an answer via LLM.
///
/// 1. Call hybrid_search to get top-k relevant chunks
/// 2. Construct augmented prompt with context
/// 3. Call LLM to generate answer
/// 4. Return answer with source citations
pub async fn rag_query(
    db: &DbManager,
    query: &str,
    embedding_model: &str,
    chat_model: &str,
    top_k: usize,
    system_prompt: Option<&str>,
    knowledge_base_ids: Option<&[String]>,
) -> Result<RagResponse, String> {
    // 1. Retrieve relevant chunks
    let sources = hybrid_search(
        db,
        query,
        embedding_model,
        top_k,
        20,
        20,
        60,
        knowledge_base_ids,
    )
    .await?;

    // 2. Construct augmented prompt
    let context = sources
        .iter()
        .enumerate()
        .map(|(i, r)| {
            format!(
                "[{}] 知识库：{}；文档：{}\n{}",
                i + 1,
                r.knowledge_base_name,
                r.document_title,
                r.content
            )
        })
        .collect::<Vec<_>>()
        .join("\n---\n");

    let default_system = "你是一个知识库助手。请根据以下上下文回答用户的问题。如果上下文中没有相关信息，请明确说明。引用来源时请使用 [1], [2] 等标记。";
    let system = system_prompt.unwrap_or(default_system);

    let user_message = format!("上下文：\n{}\n\n问题：{}", context, query);

    // 3. Resolve chat model platform
    let (api_key, api_address, api_type, actual_model) = resolve_chat_platform(db, chat_model)?;

    // 上游地址是用户配置的，可能是 localhost（Ollama / 本地 vLLM）：
    // 回环绕开系统代理，公网保留——写死任何一边都会错一半。
    let client = crate::storage::client_for_url(&api_address, std::time::Duration::from_secs(120));

    // 4. Call LLM
    let answer = match api_type.as_str() {
        "anthropic" => {
            let url = format!("{}/v1/messages", api_address.trim_end_matches('/'));
            let body = serde_json::json!({
                "model": actual_model,
                "max_tokens": 4096,
                "system": system,
                "messages": [{"role": "user", "content": user_message}],
            });
            let mut req = client.post(&url).json(&body);
            req = req
                .header("x-api-key", api_key.trim())
                .header("anthropic-version", "2023-06-01")
                .header("content-type", "application/json");
            let resp = req
                .send()
                .await
                .map_err(|e| format!("LLM request failed: {}", e))?;
            if !resp.status().is_success() {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                return Err(format!("LLM API error ({}): {}", status, body));
            }
            let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
            json["content"][0]["text"]
                .as_str()
                .unwrap_or("No answer generated")
                .to_string()
        }
        _ => {
            // OpenAI-compatible
            let url = format!("{}/chat/completions", api_address.trim_end_matches('/'));
            let body = serde_json::json!({
                "model": actual_model,
                "messages": [
                    {"role": "system", "content": system},
                    {"role": "user", "content": user_message},
                ],
            });
            let mut req = client.post(&url).json(&body);
            if !api_key.trim().is_empty() {
                req = req.header("Authorization", format!("Bearer {}", api_key.trim()));
            }
            let resp = req
                .send()
                .await
                .map_err(|e| format!("LLM request failed: {}", e))?;
            if !resp.status().is_success() {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                return Err(format!("LLM API error ({}): {}", status, body));
            }
            let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
            json["choices"][0]["message"]["content"]
                .as_str()
                .unwrap_or("No answer generated")
                .to_string()
        }
    };

    Ok(RagResponse {
        answer,
        sources,
        query: query.to_string(),
    })
}

/// Resolve the chat model's platform + API details.
pub fn resolve_chat_platform(
    db: &DbManager,
    model_name: &str,
) -> Result<(String, String, String, String), String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;

    // Try platform_id:model_name format first
    if let Some(colon_pos) = model_name.find(':') {
        let pid = &model_name[..colon_pos];
        let mname = &model_name[colon_pos + 1..];
        let (api_address, api_type) = conn
            .prepare("SELECT api_address, api_type FROM model_platforms WHERE id = ?1 AND is_enabled = 1")
            .map_err(|e| e.to_string())?
            .query_row(params![pid], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|e| format!("Platform '{}' not found: {}", pid, e))?;
        // 和网关、健康检测同一套 Key 解析（新表优先、活跃在前）。
        let key = crate::commands::platform_keys(db, pid)
            .0
            .into_iter()
            .next()
            .unwrap_or_default();
        return Ok((key, api_address, api_type, mname.to_string()));
    }

    // 裸模型名：交给和路由**同一个**挑选函数。
    //
    // 以前这里是 `... WHERE pm.model_name = ?1 ... LIMIT 1`，**没有 ORDER BY**——
    // 同名模型挂在多个平台上时，SQLite 返回谁就是谁，和网关路由挑的那个可以
    // 是两回事。于是「网关能跑通、技能融合却失败」这种事就说得通了。
    let platform_id = crate::proxy::winning_platform_for_model(db, model_name).ok_or_else(|| {
        format!("没有已启用的平台提供模型 '{model_name}'。请到「模型」页启用一个。")
    })?;
    let (api_address, api_type) = conn
        .prepare("SELECT api_address, api_type FROM model_platforms WHERE id = ?1")
        .map_err(|e| e.to_string())?
        .query_row(params![platform_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|e| format!("平台 '{platform_id}' 读取失败: {e}"))?;
    let key = crate::commands::platform_keys(db, &platform_id)
        .0
        .into_iter()
        .next()
        .unwrap_or_default();
    Ok((key, api_address, api_type, model_name.to_string()))
}

/// One-shot, non-streaming chat call against a gateway model
/// (`platform_id:model_name` or bare model name). Shared by features that need
/// a single structured reply (PPT generation/editing, skill review, 技能融合…).
///
/// **走 OMNIX 自己的网关**，不再直连平台。以前这里是第五套并行实现：自己拼
/// Anthropic / OpenAI 两种协议、自己解析 Key（只读 `model_platforms.api_key` 旧列）、
/// 裸模型名用 `LIMIT 1` 且**没有 ORDER BY**——同名模型挂在多个平台上时，
/// 它挑的那个和路由挑的那个可以是两回事。技能融合就是这么失败的。
///
/// 走网关之后自动获得：统一的 Key 解析、失败落 `request_logs`、错误信封、
/// 熔断计数、用量计费。
pub async fn chat_once(
    db: &DbManager,
    chat_model: &str,
    prompt: &str,
) -> Result<String, String> {
    let port = db
        .get_setting("proxy_port")
        .ok()
        .flatten()
        .unwrap_or_else(|| "1421".to_string());

    let response = crate::storage::loopback_client(std::time::Duration::from_secs(180))
        .post(format!("http://127.0.0.1:{port}/v1/chat/completions"))
        // 这条路由不看 body 里的 `model`（它给外部 CLI 用），内部调用靠这个头指名。
        .header("x-omnix-model", chat_model)
        .json(&serde_json::json!({
            "model": chat_model,
            "messages": [{"role": "user", "content": prompt}],
            "stream": false,
        }))
        .send()
        .await
        .map_err(|e| format!("模型请求失败: {e}"))?;

    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    let snippet = |text: &str| text.chars().take(300).collect::<String>();
    if !status.is_success() {
        return Err(format!(
            "模型 API 错误 {status}（模型 {chat_model}）: {}",
            snippet(&body)
        ));
    }

    let json: serde_json::Value = serde_json::from_str(&body)
        .map_err(|e| format!("解析模型响应失败: {e} — 原始响应: {}", snippet(&body)))?;
    let answer = json["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or_default()
        .to_string();

    if answer.trim().is_empty() {
        // 以前这里只说「模型没有返回内容」，真正的原因（响应形状不对、上游把
        // 错误塞在 200 里、模型拒答）一个字都看不到。
        return Err(format!(
            "模型 '{chat_model}' 没有返回内容。原始响应: {}",
            snippet(&body)
        ));
    }
    Ok(answer)
}

#[cfg(test)]
mod search_scoping_tests {
    use super::*;

    fn test_db(tag: &str) -> DbManager {
        use std::sync::atomic::{AtomicU32, Ordering};
        static N: AtomicU32 = AtomicU32::new(0);
        // Unique per test — cargo runs these in parallel.
        let path = std::env::temp_dir().join(format!(
            "omnix_kbsearch_{tag}_{}_{}.db",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_file(&path);
        DbManager::new_with_path(path)
    }

    /// One base, one document, and a chunk per (id, text) with an embedding
    /// tagged `model`. `vec` is stored verbatim so tests control the scores.
    fn seed(db: &DbManager, base: &str, chunks: &[(&str, &str, &str, &[f32])]) {
        let conn = db.get_connection().unwrap();
        conn.execute(
            "INSERT OR IGNORE INTO knowledge_bases (id, name) VALUES (?1, ?1)",
            params![base],
        )
        .unwrap();
        let doc = format!("doc-{base}");
        conn.execute(
            "INSERT OR IGNORE INTO kb_documents (id, knowledge_base_id, title, source_path)
             VALUES (?1, ?2, ?1, '')",
            params![doc, base],
        )
        .unwrap();
        for (i, (id, text, model, vec)) in chunks.iter().enumerate() {
            conn.execute(
                "INSERT INTO kb_chunks (id, document_id, chunk_index, content)
                 VALUES (?1, ?2, ?3, ?4)",
                params![id, doc, i as i64, text],
            )
            .unwrap();
            // 全文索引不再由触发器同步——夹具也得走和生产同一条路。
            index_chunk(&conn, id, text).unwrap();
            if !model.is_empty() {
                conn.execute(
                    "INSERT INTO kb_embeddings (chunk_id, embedding, model, dimensions)
                     VALUES (?1, ?2, ?3, ?4)",
                    params![id, vec_f32_to_blob(vec), model, vec.len() as i64],
                )
                .unwrap();
            }
        }
    }

    fn ids(results: &[(String, f64)]) -> Vec<&str> {
        results.iter().map(|(id, _)| id.as_str()).collect()
    }

    /// The bug this whole change exists for: `kb_embeddings.model` was written
    /// from day one and never read, so a base embedded with another model got
    /// scored anyway — silently, and plausibly, when the dimensions happened to
    /// line up (1536 is a very popular number).
    #[test]
    fn vector_search_ignores_other_models() {
        let db = test_db("model");
        seed(
            &db,
            "b1",
            &[
                ("mine", "x", "model-a", &[1.0, 0.0]),
                // Same dimensions, different model: cosine against it returns a
                // perfectly respectable 1.0 that means nothing at all.
                ("theirs", "x", "model-b", &[1.0, 0.0]),
            ],
        );
        let hits = vector_search(&db, &[1.0, 0.0], "model-a", None, 10).unwrap();
        assert_eq!(ids(&hits), vec!["mine"]);
    }

    /// Rows the query vector cannot be compared against, dropped two different
    /// ways: a different declared dimension never leaves SQL, and a row whose
    /// blob disagrees with its own `dimensions` column is thrown out in Rust.
    /// The second one is the nasty case — it used to score a silent 0.0 and take
    /// a slot in the ranking as if it were a real (bad) hit.
    #[test]
    fn vector_search_drops_wrong_dimensions() {
        let db = test_db("dims");
        seed(
            &db,
            "b1",
            &[
                ("ok", "x", "model-a", &[1.0, 0.0]),
                ("other-dim", "x", "model-a", &[1.0, 0.0, 0.0]),
                ("corrupt", "x", "", &[]),
            ],
        );
        // Claims 2 dimensions, carries 1 — passes the SQL filter, and only the
        // in-Rust length check keeps it out.
        db.get_connection()
            .unwrap()
            .execute(
                "INSERT INTO kb_embeddings (chunk_id, embedding, model, dimensions)
                 VALUES ('corrupt', ?1, 'model-a', 2)",
                params![vec_f32_to_blob(&[1.0])],
            )
            .unwrap();

        let hits = vector_search(&db, &[1.0, 0.0], "model-a", None, 10).unwrap();
        assert_eq!(ids(&hits), vec!["ok"]);
    }

    #[test]
    fn vector_search_scopes_to_selected_bases() {
        let db = test_db("scope");
        seed(&db, "b1", &[("in", "x", "m", &[1.0, 0.0])]);
        seed(&db, "b2", &[("out", "x", "m", &[1.0, 0.0])]);

        let scoped = vector_search(&db, &[1.0, 0.0], "m", Some(&["b1".into()]), 10).unwrap();
        assert_eq!(ids(&scoped), vec!["in"]);

        let all = vector_search(&db, &[1.0, 0.0], "m", None, 10).unwrap();
        assert_eq!(all.len(), 2);
    }

    /// Ordering has to survive the switch from "sort everything" to "partition,
    /// then sort the survivors".
    #[test]
    fn vector_search_returns_best_k_in_order() {
        let db = test_db("topk");
        seed(
            &db,
            "b1",
            &[
                ("far", "x", "m", &[0.0, 1.0]),
                ("near", "x", "m", &[1.0, 0.0]),
                ("mid", "x", "m", &[1.0, 1.0]),
            ],
        );
        let hits = vector_search(&db, &[1.0, 0.0], "m", None, 2).unwrap();
        assert_eq!(ids(&hits), vec!["near", "mid"]);
        assert!(hits[0].1 > hits[1].1);
    }

    /// Also proves the FTS5 join compiles and `rank` is still reachable through
    /// it — the scope used to be applied after a global top-5000 cut.
    #[test]
    fn bm25_search_scopes_in_sql() {
        let db = test_db("bm25");
        seed(&db, "b1", &[("in", "quantum computing", "", &[])]);
        seed(&db, "b2", &[("out", "quantum computing", "", &[])]);

        let scoped = bm25_search(&db, "quantum", Some(&["b1".into()]), 10).unwrap();
        assert_eq!(ids(&scoped), vec!["in"]);

        let all = bm25_search(&db, "quantum", None, 10).unwrap();
        assert_eq!(all.len(), 2);
    }

    /// 中文现在真的能搜了。
    ///
    /// 这条测试的上一版断言的是**坏掉的行为**：四字词 0 条、两字词 0 条、
    /// 只有整段一模一样才命中 1 条——因为 `unicode61` 把一整段中文当成一个
    /// token。二元切分之后三种都该命中。
    #[test]
    fn bm25_chinese_matches_words_not_just_whole_runs() {
        let db = test_db("zh");
        seed(&db, "b1", &[("zh", "量子计算的进展很快", "", &[])]);
        assert_eq!(bm25_search(&db, "量子计算", None, 10).unwrap().len(), 1, "四字词");
        assert_eq!(bm25_search(&db, "量子", None, 10).unwrap().len(), 1, "两字词");
        assert_eq!(bm25_search(&db, "进展", None, 10).unwrap().len(), 1, "词在句中");
        assert_eq!(
            bm25_search(&db, "量子计算的进展很快", None, 10).unwrap().len(),
            1,
            "整段"
        );
    }

    /// 不相干的词不该命中——二元切分换来的召回不能以「什么都能搜到」为代价。
    #[test]
    fn bm25_chinese_does_not_match_unrelated_words() {
        let db = test_db("zh2");
        seed(&db, "b1", &[("zh", "量子计算的进展很快", "", &[])]);
        assert_eq!(bm25_search(&db, "红烧肉", None, 10).unwrap().len(), 0);
        assert_eq!(bm25_search(&db, "光刻机", None, 10).unwrap().len(), 0);
    }

    /// 老库升级：外部内容表 + 触发器那一套要能就地换掉，且**已有内容立刻可搜**。
    ///
    /// 这条很重要——旧索引从来没查出过结果，所以迁移不是「保持现状」，而是
    /// 第一次真的把索引建起来。如果只换表不回灌，老用户的知识库会从
    /// 「搜不到（因为坏）」变成「搜不到（因为空）」，从外面看一模一样。
    #[test]
    fn legacy_fts_is_rebuilt_and_becomes_searchable() {
        let db = test_db("migrate");
        {
            let conn = db.get_connection().unwrap();
            // 退回旧结构：外部内容表 + 三个触发器
            conn.execute("DROP TABLE IF EXISTS kb_chunks_fts", []).unwrap();
            conn.execute(
                "CREATE VIRTUAL TABLE kb_chunks_fts USING fts5(
                    chunk_id, content, content='kb_chunks', content_rowid=rowid,
                    tokenize='porter unicode61')",
                [],
            )
            .unwrap();
            conn.execute(
                "CREATE TRIGGER kb_chunks_ai AFTER INSERT ON kb_chunks BEGIN
                    INSERT INTO kb_chunks_fts(rowid, chunk_id, content)
                    VALUES (new.rowid, new.id, new.content);
                 END",
                [],
            )
            .unwrap();
            // 旧库里已有内容
            conn.execute(
                "INSERT OR IGNORE INTO knowledge_bases (id, name) VALUES ('old', 'old')",
                [],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO kb_documents (id, knowledge_base_id, title, source_path)
                 VALUES ('d', 'old', 'd', '')",
                [],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO kb_chunks (id, document_id, chunk_index, content)
                 VALUES ('old-chunk', 'd', 0, '量子计算的进展很快')",
                [],
            )
            .unwrap();
        }

        // 再跑一次 init_schema = 应用升级后第一次启动
        db.init_schema().expect("迁移");

        let sql: String = db
            .get_connection()
            .unwrap()
            .query_row(
                "SELECT sql FROM sqlite_master WHERE type='table' AND name='kb_chunks_fts'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(!sql.contains("content='kb_chunks'"), "旧的外部内容表应已换掉：{sql}");

        let hits = bm25_search(&db, "量子计算", None, 10).unwrap();
        assert_eq!(ids(&hits), vec!["old-chunk"], "老内容迁移后必须立刻可搜");
    }

    #[test]
    fn segment_for_index_makes_bigrams_of_cjk_only() {
        assert_eq!(segment_for_index("量子计算"), "量子 子计 计算 ");
        assert_eq!(segment_for_index("书"), "书 ", "单字成段时索引这个字本身");
        // 英文原样，仍然交给 unicode61 + porter
        assert_eq!(segment_for_index("quantum computing"), "quantum computing");
        // 中英混排：CJK 段尾补的空格会和原文的空格叠成两个。对 FTS 没有影响
        // （分词器折叠空白），断言就照实写，不为了好看去 trim。
        assert_eq!(segment_for_index("量子 computing"), "量子  computing");
    }

    /// English is the control: it proves the half works now. It did not before —
    /// the id column could not be read off the external-content FTS5 table, so
    /// every query errored and `hybrid_search` swallowed it into an empty list.
    #[test]
    fn bm25_english_actually_returns_hits() {
        let db = test_db("en");
        seed(&db, "b1", &[("en", "quantum computing advances", "", &[])]);
        assert_eq!(ids(&bm25_search(&db, "quantum", None, 10).unwrap()), vec!["en"]);
    }

    #[test]
    fn coverage_separates_not_embedded_from_wrong_model() {
        let db = test_db("cover");
        seed(&db, "empty", &[("plain", "x", "", &[])]);
        seed(&db, "other", &[("theirs", "x", "model-b", &[1.0, 0.0])]);

        let (total, usable, _) =
            embedding_coverage(&db, "model-a", Some(&["empty".into()])).unwrap();
        assert_eq!((total, usable), (0, 0), "未生成嵌入不该被当成模型不匹配");

        let (total, usable, stored) =
            embedding_coverage(&db, "model-a", Some(&["other".into()])).unwrap();
        assert_eq!((total, usable), (1, 0));
        assert_eq!(stored, vec!["model-b".to_string()]);
    }

    /// The mismatch is refused before any embedding API call, so this needs no
    /// network: `hybrid_search` must not hand back a confident-looking ranking
    /// built from incomparable vectors.
    #[tokio::test]
    async fn hybrid_search_refuses_a_model_mismatch() {
        let db = test_db("mismatch");
        seed(&db, "b1", &[("theirs", "quantum", "model-b", &[1.0, 0.0])]);

        let err = hybrid_search(
            &db,
            "quantum",
            "model-a",
            10,
            20,
            20,
            60,
            Some(&["b1".to_string()]),
        )
        .await
        .expect_err("跨模型检索必须报错，而不是给一个像模像样的排序");
        assert!(err.contains("model-a"), "错误里要写清当前用的模型: {err}");
        assert!(err.contains("model-b"), "错误里要写清库里存的模型: {err}");
    }

    /// A base that is indexed but not embedded yet is a normal state — BM25
    /// still carries the search, and no embedding API call is made (this test
    /// would hang or fail on a network call, since no platform is configured).
    #[tokio::test]
    async fn hybrid_search_runs_bm25_only_when_nothing_is_embedded() {
        let db = test_db("bm25only");
        seed(&db, "b1", &[("plain", "quantum computing", "", &[])]);

        let hits = hybrid_search(
            &db,
            "quantum",
            "model-a",
            10,
            20,
            20,
            60,
            Some(&["b1".to_string()]),
        )
        .await
        .expect("没生成嵌入不该让整个检索失败");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].chunk_id, "plain");
        assert!(hits[0].vector_score.is_none());
    }
}

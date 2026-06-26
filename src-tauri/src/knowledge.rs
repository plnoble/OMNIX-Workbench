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
fn apply_overlap(chunks: &mut Vec<Chunk>, config: &ChunkConfig) {
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

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;

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
/// Returns (chunk_id, bm25_score) pairs. FTS5's rank is negative (more negative = better),
/// so we negate it for consistency.
pub fn bm25_search(
    db: &DbManager,
    query: &str,
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

    let mut stmt = conn
        .prepare(
            "SELECT chunk_id, rank AS bm25_score FROM kb_chunks_fts WHERE kb_chunks_fts MATCH ?1 ORDER BY rank LIMIT ?2",
        )
        .map_err(|e| e.to_string())?;

    let results = stmt
        .query_map(params![safe_query, limit as i64], |row| {
            let chunk_id: String = row.get(0)?;
            let score: f64 = row.get(1)?;
            Ok((chunk_id, -score)) // Negate: FTS5 rank is negative
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    Ok(results)
}

/// Vector similarity search using brute-force cosine similarity.
///
/// Loads all embeddings from the database, computes cosine similarity against
/// the query embedding, and returns the top-k results.
pub fn vector_search(
    db: &DbManager,
    query_embedding: &[f32],
    limit: usize,
) -> Result<Vec<(String, f64)>, String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;

    let mut stmt = conn
        .prepare("SELECT chunk_id, embedding, dimensions FROM kb_embeddings")
        .map_err(|e| e.to_string())?;

    let mut scored: Vec<(String, f64)> = stmt
        .query_map([], |row| {
            let chunk_id: String = row.get(0)?;
            let blob: Vec<u8> = row.get(1)?;
            let dimensions: i32 = row.get(2)?;
            Ok((chunk_id, blob, dimensions))
        })
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .map(|(chunk_id, blob, dimensions)| {
            let vec = blob_to_vec_f32(&blob, dimensions as usize);
            let score = cosine_similarity(query_embedding, &vec);
            (chunk_id, score)
        })
        .collect();

    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(limit);

    Ok(scored)
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
    let allowed_chunks = if let Some(base_ids) = knowledge_base_ids.filter(|ids| !ids.is_empty()) {
        let conn = db.get_connection().map_err(|e| e.to_string())?;
        let placeholders = std::iter::repeat("?")
            .take(base_ids.len())
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "SELECT c.id FROM kb_chunks c
             JOIN kb_documents d ON d.id = c.document_id
             WHERE d.knowledge_base_id IN ({placeholders})"
        );
        let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
        let params = rusqlite::params_from_iter(base_ids.iter());
        let ids = stmt
            .query_map(params, |row| row.get::<_, String>(0))
            .map_err(|e| e.to_string())?
            .collect::<Result<std::collections::HashSet<_>, _>>()
            .map_err(|e| e.to_string())?;
        Some(ids)
    } else {
        None
    };

    // 1. BM25 search (synchronous SQLite)
    let mut bm25_results = bm25_search(
        db,
        query,
        if allowed_chunks.is_some() {
            5_000
        } else {
            bm25_limit
        },
    )
    .unwrap_or_default();
    if let Some(allowed) = &allowed_chunks {
        filter_ranked_results(&mut bm25_results, allowed, bm25_limit);
    }

    // 2. Generate query embedding
    let query_embeddings =
        generate_embeddings(db, vec![query.to_string()], embedding_model, None).await?;

    let query_embedding = query_embeddings
        .into_iter()
        .next()
        .ok_or("Failed to generate query embedding")?;

    // 3. Vector search
    let mut vector_results = vector_search(
        db,
        &query_embedding,
        if allowed_chunks.is_some() {
            usize::MAX
        } else {
            vector_limit
        },
    )
    .unwrap_or_default();
    if let Some(allowed) = &allowed_chunks {
        filter_ranked_results(&mut vector_results, allowed, vector_limit);
    }

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

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| e.to_string())?;

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
        let (api_key, api_address, api_type) = conn
            .prepare("SELECT api_key, api_address, api_type FROM model_platforms WHERE id = ?1 AND is_enabled = 1")
            .map_err(|e| e.to_string())?
            .query_row(params![pid], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?))
            })
            .map_err(|e| format!("Platform '{}' not found: {}", pid, e))?;
        return Ok((
            crate::crypto::decrypt(&api_key),
            api_address,
            api_type,
            mname.to_string(),
        ));
    }

    // Search by model name
    let result = conn
        .prepare(
            "SELECT COALESCE(
                    (SELECT encrypted_key FROM platform_api_keys
                     WHERE platform_id = mp.id AND is_active = 1 AND is_enabled = 1
                     ORDER BY priority DESC, created_at ASC LIMIT 1),
                    mp.api_key
                 ), mp.api_address, mp.api_type, pm.model_name
             FROM platform_models pm
             JOIN model_platforms mp ON pm.platform_id = mp.id
             WHERE pm.model_name = ?1 AND pm.is_enabled = 1 AND mp.is_enabled = 1
             LIMIT 1",
        )
        .map_err(|e| e.to_string())?
        .query_row(params![model_name], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })
        .map_err(|e| {
            format!(
                "No enabled platform found for chat model '{}': {}",
                model_name, e
            )
        })?;

    Ok((
        crate::crypto::decrypt(&result.0),
        result.1,
        result.2,
        result.3,
    ))
}

fn filter_ranked_results(
    results: &mut Vec<(String, f64)>,
    allowed: &std::collections::HashSet<String>,
    limit: usize,
) {
    results.retain(|(chunk_id, _)| allowed.contains(chunk_id));
    results.truncate(limit);
}

#[cfg(test)]
mod knowledge_base_filter_tests {
    use std::collections::HashSet;

    use super::filter_ranked_results;

    #[test]
    fn selected_knowledge_bases_exclude_unbound_chunks() {
        let mut ranked = vec![
            ("allowed-1".into(), 1.0),
            ("foreign".into(), 0.9),
            ("allowed-2".into(), 0.8),
        ];
        let allowed = HashSet::from(["allowed-1".to_string(), "allowed-2".to_string()]);
        filter_ranked_results(&mut ranked, &allowed, 1);
        assert_eq!(ranked, vec![("allowed-1".to_string(), 1.0)]);
    }
}

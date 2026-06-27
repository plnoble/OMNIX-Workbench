//! Knowledge-base portability (export / import). A knowledge base lives in
//! SQLite (`knowledge_bases` + `kb_documents` + `kb_chunks` + `kb_embeddings`).
//! This serializes a whole base — documents, chunks, AND their embeddings (with
//! the embedding model + dimensions) — to a self-describing JSON so it can move
//! to another OMNIX install (or be read by other software).
//!
//! Embeddings are model-specific: they are tagged with the model that produced
//! them, so the importing machine knows whether its embedding model matches.
//! BM25 (keyword) search is reproduced automatically on import via the
//! `kb_chunks` → `kb_chunks_fts` trigger; vector search only stays meaningful if
//! the same embedding model is used.

use std::sync::Arc;

use base64::Engine;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::commands::KnowledgeBase;
use crate::db::DbManager;

const FORMAT: &str = "omnix-kb";
const VERSION: u32 = 1;

#[derive(Debug, Serialize, Deserialize)]
struct KbExport {
    format: String,
    version: u32,
    exported_at: String,
    base_name: String,
    base_description: String,
    /// Distinct embedding models present, for a quick compatibility check.
    embedding_models: Vec<String>,
    documents: Vec<DocExport>,
}

#[derive(Debug, Serialize, Deserialize)]
struct DocExport {
    title: String,
    source_path: String,
    file_type: String,
    file_hash: String,
    chunk_count: i32,
    total_chars: i32,
    embedding_model: String,
    embedding_status: String,
    chunks: Vec<ChunkExport>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ChunkExport {
    chunk_index: i32,
    content: String,
    char_start: i32,
    char_end: i32,
    metadata: String,
    embedding: Option<EmbeddingExport>,
}

#[derive(Debug, Serialize, Deserialize)]
struct EmbeddingExport {
    model: String,
    dimensions: i32,
    /// base64 of the raw little-endian f32 blob.
    data_b64: String,
}

/// Serialize a whole knowledge base (docs + chunks + embeddings) to JSON.
#[tauri::command]
pub fn kb_export_base(
    knowledge_base_id: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<String, String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;

    let (base_name, base_description): (String, String) = conn
        .query_row(
            "SELECT name, description FROM knowledge_bases WHERE id = ?1",
            params![knowledge_base_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|e| format!("找不到知识库: {e}"))?;

    // Documents
    let mut doc_stmt = conn
        .prepare("SELECT id, title, source_path, file_type, file_hash, chunk_count, total_chars, embedding_model, embedding_status FROM kb_documents WHERE knowledge_base_id = ?1 ORDER BY created_at ASC")
        .map_err(|e| e.to_string())?;
    let doc_rows = doc_stmt
        .query_map(params![knowledge_base_id], |row| {
            Ok((
                row.get::<_, String>(0)?, // id
                row.get::<_, String>(1)?, // title
                row.get::<_, String>(2)?, // source_path
                row.get::<_, String>(3)?, // file_type
                row.get::<_, String>(4)?, // file_hash
                row.get::<_, i32>(5)?,     // chunk_count
                row.get::<_, i32>(6)?,     // total_chars
                row.get::<_, String>(7)?, // embedding_model
                row.get::<_, String>(8)?, // embedding_status
            ))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    let mut models = std::collections::BTreeSet::new();
    let mut documents = Vec::new();
    for (doc_id, title, source_path, file_type, file_hash, chunk_count, total_chars, embedding_model, embedding_status) in doc_rows {
        if !embedding_model.is_empty() {
            models.insert(embedding_model.clone());
        }
        // Chunks + their embeddings (LEFT JOIN so unembedded chunks are kept).
        let mut chunk_stmt = conn
            .prepare(
                "SELECT c.chunk_index, c.content, c.char_start, c.char_end, c.metadata, e.embedding, e.model, e.dimensions
                 FROM kb_chunks c LEFT JOIN kb_embeddings e ON c.id = e.chunk_id
                 WHERE c.document_id = ?1 ORDER BY c.chunk_index ASC",
            )
            .map_err(|e| e.to_string())?;
        let chunks = chunk_stmt
            .query_map(params![doc_id], |row| {
                let blob: Option<Vec<u8>> = row.get(5)?;
                let model: Option<String> = row.get(6)?;
                let dims: Option<i32> = row.get(7)?;
                let embedding = match (blob, model, dims) {
                    (Some(blob), Some(model), Some(dims)) => Some(EmbeddingExport {
                        model,
                        dimensions: dims,
                        data_b64: base64::engine::general_purpose::STANDARD.encode(&blob),
                    }),
                    _ => None,
                };
                Ok(ChunkExport {
                    chunk_index: row.get(0)?,
                    content: row.get(1)?,
                    char_start: row.get(2)?,
                    char_end: row.get(3)?,
                    metadata: row.get::<_, Option<String>>(4)?.unwrap_or_else(|| "{}".into()),
                    embedding,
                })
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;

        documents.push(DocExport {
            title,
            source_path,
            file_type,
            file_hash,
            chunk_count,
            total_chars,
            embedding_model,
            embedding_status,
            chunks,
        });
    }

    let export = KbExport {
        format: FORMAT.into(),
        version: VERSION,
        exported_at: chrono::Utc::now().to_rfc3339(),
        base_name,
        base_description,
        embedding_models: models.into_iter().collect(),
        documents,
    };
    serde_json::to_string(&export).map_err(|e| e.to_string())
}

/// Import a knowledge base from exported JSON into a brand-new base.
#[tauri::command]
pub fn kb_import_base(
    data: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<KnowledgeBase, String> {
    let export: KbExport = serde_json::from_str(&data).map_err(|e| format!("不是有效的知识库文件: {e}"))?;
    if export.format != FORMAT {
        return Err("文件格式不是 omnix-kb".into());
    }

    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let base_id = format!("kb_{}", chrono::Utc::now().timestamp_micros());
    let base_name = format!("{} (导入)", export.base_name);
    conn.execute(
        "INSERT INTO knowledge_bases (id, name, description) VALUES (?1, ?2, ?3)",
        params![base_id, base_name, export.base_description],
    )
    .map_err(|e| format!("创建知识库失败: {e}"))?;

    for (di, doc) in export.documents.iter().enumerate() {
        let doc_id = format!("doc_{}_{}", base_id, di);
        conn.execute(
            "INSERT INTO kb_documents (id, knowledge_base_id, title, source_path, file_type, file_hash, chunk_count, total_chars, embedding_model, embedding_status)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                doc_id, base_id, doc.title, doc.source_path, doc.file_type, doc.file_hash,
                doc.chunk_count, doc.total_chars, doc.embedding_model, doc.embedding_status
            ],
        )
        .map_err(|e| e.to_string())?;

        for chunk in &doc.chunks {
            let chunk_id = format!("chunk_{}_{}", doc_id, chunk.chunk_index);
            // Inserting into kb_chunks fires the FTS trigger → BM25 search works.
            conn.execute(
                "INSERT INTO kb_chunks (id, document_id, chunk_index, content, char_start, char_end, metadata)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![chunk_id, doc_id, chunk.chunk_index, chunk.content, chunk.char_start, chunk.char_end, chunk.metadata],
            )
            .map_err(|e| e.to_string())?;

            if let Some(emb) = &chunk.embedding {
                let blob = base64::engine::general_purpose::STANDARD
                    .decode(&emb.data_b64)
                    .map_err(|e| format!("嵌入解码失败: {e}"))?;
                conn.execute(
                    "INSERT OR REPLACE INTO kb_embeddings (chunk_id, embedding, model, dimensions) VALUES (?1, ?2, ?3, ?4)",
                    params![chunk_id, blob, emb.model, emb.dimensions],
                )
                .map_err(|e| e.to_string())?;
            }
        }
    }
    drop(conn);

    crate::commands::kb_list_bases(db)?
        .into_iter()
        .find(|base| base.id == base_id)
        .ok_or_else(|| "导入后读取知识库失败".into())
}

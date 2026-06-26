use super::*;
use crate::db::DbManager;
use crate::knowledge;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QaResponse {
    pub answer: String,
    pub sources: Vec<knowledge::SearchResult>,
    pub used_kb: bool,
}

#[tauri::command]
pub async fn qa_query(
    query: String,
    use_kb: bool,
    chat_model: String,
    embedding_model: Option<String>,
    db: State<'_, Arc<DbManager>>,
) -> Result<QaResponse, String> {
    if use_kb {
        // Use RAG pipeline
        let emb_model = embedding_model.unwrap_or_else(|| "nomic-embed-text".to_string());
        let rag_result =
            knowledge::rag_query(&*db, &query, &emb_model, &chat_model, 5, None, None).await?;
        Ok(QaResponse {
            answer: rag_result.answer,
            sources: rag_result.sources,
            used_kb: true,
        })
    } else {
        // Direct LLM call (no knowledge base)
        let (api_key, api_address, api_type, actual_model) =
            knowledge::resolve_chat_platform(&*db, &chat_model)?;

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .map_err(|e| e.to_string())?;

        let system = "你是一个智能助手，请简洁准确地回答用户的问题。";
        let answer = match api_type.as_str() {
            "anthropic" => {
                let url = format!("{}/v1/messages", api_address.trim_end_matches('/'));
                let body = serde_json::json!({
                    "model": actual_model,
                    "max_tokens": 2048,
                    "system": system,
                    "messages": [{"role": "user", "content": query}],
                });
                let mut req = client.post(&url).json(&body);
                req = req
                    .header("x-api-key", api_key.trim())
                    .header("anthropic-version", "2023-06-01");
                let resp = req
                    .send()
                    .await
                    .map_err(|e| format!("LLM request failed: {}", e))?;
                if !resp.status().is_success() {
                    return Err(format!("LLM API error: {}", resp.status()));
                }
                let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
                json["content"][0]["text"]
                    .as_str()
                    .unwrap_or("No answer")
                    .to_string()
            }
            _ => {
                let url = format!("{}/chat/completions", api_address.trim_end_matches('/'));
                let body = serde_json::json!({
                    "model": actual_model,
                    "messages": [
                        {"role": "system", "content": system},
                        {"role": "user", "content": query},
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
                    return Err(format!("LLM API error: {}", resp.status()));
                }
                let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
                json["choices"][0]["message"]["content"]
                    .as_str()
                    .unwrap_or("No answer")
                    .to_string()
            }
        };

        Ok(QaResponse {
            answer,
            sources: vec![],
            used_kb: false,
        })
    }
}

/// Streaming QA query — emits chunks via Tauri events for incremental rendering.
/// Events: `qa-stream-chunk` (text chunk), `qa-stream-done` (final with sources), `qa-stream-error`
#[tauri::command]
pub async fn qa_query_stream(
    query: String,
    use_kb: bool,
    chat_model: String,
    embedding_model: Option<String>,
    db: State<'_, Arc<DbManager>>,
    app_handle: AppHandle,
) -> Result<String, String> {
    // For RAG queries, we still use the non-streaming path (RAG pipeline is complex to stream)
    // and emit the full result as a single chunk.
    if use_kb {
        let emb_model = embedding_model.unwrap_or_else(|| "nomic-embed-text".to_string());
        let rag_result =
            knowledge::rag_query(&*db, &query, &emb_model, &chat_model, 5, None, None).await?;

        // Emit full answer as one chunk then done
        let _ = app_handle.emit("qa-stream-chunk", rag_result.answer.clone());
        let _ = app_handle.emit(
            "qa-stream-done",
            serde_json::json!({
                "sources": rag_result.sources,
                "used_kb": true,
            }),
        );
        return Ok("streamed".to_string());
    }

    // Direct LLM call with streaming
    let (api_key, api_address, api_type, actual_model) =
        knowledge::resolve_chat_platform(&*db, &chat_model)?;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| e.to_string())?;

    let system = "你是一个智能助手，请简洁准确地回答用户的问题。";

    match api_type.as_str() {
        "anthropic" => {
            // Anthropic streaming via SSE
            let url = format!("{}/v1/messages", api_address.trim_end_matches('/'));
            let body = serde_json::json!({
                "model": actual_model,
                "max_tokens": 2048,
                "stream": true,
                "system": system,
                "messages": [{"role": "user", "content": query}],
            });
            let mut req = client.post(&url).json(&body);
            req = req
                .header("x-api-key", api_key.trim())
                .header("anthropic-version", "2023-06-01");

            let resp = req
                .send()
                .await
                .map_err(|e| format!("LLM request failed: {}", e))?;
            if !resp.status().is_success() {
                let status = resp.status();
                let err_body = resp.text().await.unwrap_or_default();
                let _ = app_handle.emit(
                    "qa-stream-error",
                    format!("API error {}: {}", status, err_body),
                );
                return Err(format!("LLM API error: {}", status));
            }

            // Parse SSE stream
            let mut stream = resp.bytes_stream();
            use futures::StreamExt;
            while let Some(chunk_result) = stream.next().await {
                match chunk_result {
                    Ok(bytes) => {
                        let text = String::from_utf8_lossy(&bytes);
                        // Parse SSE lines looking for content_block_delta events
                        for line in text.lines() {
                            if let Some(data) = line.strip_prefix("data: ") {
                                if data == "[DONE]" {
                                    continue;
                                }
                                if let Ok(json) = serde_json::from_str::<serde_json::Value>(data) {
                                    if json["type"] == "content_block_delta" {
                                        if let Some(content) = json["delta"]["text"].as_str() {
                                            if !content.is_empty() {
                                                let _ = app_handle
                                                    .emit("qa-stream-chunk", content.to_string());
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        let _ = app_handle.emit("qa-stream-error", format!("Stream error: {}", e));
                        break;
                    }
                }
            }
        }
        _ => {
            // OpenAI-compatible streaming via SSE
            let url = format!("{}/chat/completions", api_address.trim_end_matches('/'));
            let body = serde_json::json!({
                "model": actual_model,
                "stream": true,
                "messages": [
                    {"role": "system", "content": system},
                    {"role": "user", "content": query},
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
                let err_body = resp.text().await.unwrap_or_default();
                let _ = app_handle.emit(
                    "qa-stream-error",
                    format!("API error {}: {}", status, err_body),
                );
                return Err(format!("LLM API error: {}", status));
            }

            // Parse SSE stream
            let mut stream = resp.bytes_stream();
            use futures::StreamExt;
            while let Some(chunk_result) = stream.next().await {
                match chunk_result {
                    Ok(bytes) => {
                        let text = String::from_utf8_lossy(&bytes);
                        for line in text.lines() {
                            if let Some(data) = line.strip_prefix("data: ") {
                                if data == "[DONE]" {
                                    continue;
                                }
                                if let Ok(json) = serde_json::from_str::<serde_json::Value>(data) {
                                    if let Some(content) =
                                        json["choices"][0]["delta"]["content"].as_str()
                                    {
                                        if !content.is_empty() {
                                            let _ = app_handle
                                                .emit("qa-stream-chunk", content.to_string());
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        let _ = app_handle.emit("qa-stream-error", format!("Stream error: {}", e));
                        break;
                    }
                }
            }
        }
    }

    // Signal completion
    let _ = app_handle.emit(
        "qa-stream-done",
        serde_json::json!({
            "sources": [],
            "used_kb": false,
        }),
    );

    Ok("streamed".to_string())
}

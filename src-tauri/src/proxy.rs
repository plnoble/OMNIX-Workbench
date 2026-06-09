use axum::{
    body::Body,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use futures::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::sync::oneshot;
use tower_http::cors::CorsLayer;
use rusqlite::params;

use crate::db::DbManager;

// Define sharing state
pub struct ProxyState {
    pub db: Arc<DbManager>,
    pub agent_manager: Arc<crate::agent::AgentManager>,
    pub http_client: Client,
    pub request_counter: AtomicUsize,
    pub concurrency_semaphore: Arc<tokio::sync::Semaphore>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AnthropicMessageContent {
    String(String),
    Blocks(Vec<AnthropicContentBlock>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnthropicContentBlock {
    #[serde(rename = "type")]
    pub block_type: String,
    pub text: Option<String>,
}

impl AnthropicMessageContent {
    pub fn to_string_content(&self) -> String {
        match self {
            AnthropicMessageContent::String(s) => s.clone(),
            AnthropicMessageContent::Blocks(blocks) => {
                let mut text_parts = Vec::new();
                for block in blocks {
                    if block.block_type == "text" {
                        if let Some(ref t) = block.text {
                            text_parts.push(t.as_str());
                        }
                    }
                }
                text_parts.join("\n")
            }
        }
    }
}

// Anthropic Request format
#[derive(Debug, Deserialize, Serialize)]
pub struct AnthropicMessage {
    pub role: String,
    pub content: AnthropicMessageContent,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct AnthropicRequest {
    pub model: String,
    pub messages: Vec<AnthropicMessage>,
    pub max_tokens: Option<u32>,
    pub system: Option<AnthropicMessageContent>,
    pub temperature: Option<f32>,
    pub stream: Option<bool>,
    /// Reasoning effort control (New API inspired): "low" | "medium" | "high"
    /// Maps to budget_tokens for Anthropic extended thinking
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<String>,
}

// OpenAI Request format
#[derive(Debug, Serialize, Deserialize)]
struct OpenAIRequestMessage {
    role: String,
    content: String,
}

#[derive(Debug, Serialize)]
struct OpenAIRequest {
    model: String,
    messages: Vec<OpenAIRequestMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
}

// Structs for parsing OpenAI responses
#[derive(Debug, Deserialize)]
struct OpenAIChoiceDelta {
    content: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAIChoice {
    delta: OpenAIChoiceDelta,
}

#[derive(Debug, Deserialize)]
struct OpenAIStreamChunk {
    choices: Vec<OpenAIChoice>,
}

pub struct ProxyServer {
    shutdown_tx: Option<oneshot::Sender<()>>,
}

impl ProxyServer {
    pub fn new() -> Self {
        Self { shutdown_tx: None }
    }

    pub fn start(&mut self, db: Arc<DbManager>, agent_manager: Arc<crate::agent::AgentManager>, port: u16) {
        let (tx, rx) = oneshot::channel::<()>();
        self.shutdown_tx = Some(tx);

        let use_wsl = db.get_setting("use_wsl").unwrap_or(None).unwrap_or_else(|| "false".to_string()) == "true";
        let bind_ip = if use_wsl {
            [0, 0, 0, 0]
        } else {
            [127, 0, 0, 1]
        };
        let addr = SocketAddr::from((bind_ip, port));

        let client = Client::builder()
            .connect_timeout(std::time::Duration::from_secs(15))
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .unwrap_or_else(|_| Client::new());

        let state = Arc::new(ProxyState {
            db,
            agent_manager,
            http_client: client,
            request_counter: AtomicUsize::new(0),
            concurrency_semaphore: Arc::new(tokio::sync::Semaphore::new(20)), // max 20 concurrent proxy requests
        });

        let app = Router::new()
            .route("/v1/messages", post(handle_messages))
            .route("/v1/chat/completions", post(handle_openai_forward))
            .route("/v1/embeddings", post(handle_embeddings))
            .route("/agent/:agent_name/v1/messages", post(handle_messages_for_agent))
            .route("/agent/:agent_name/v1/chat/completions", post(handle_openai_forward_for_agent))
            .route("/remote", axum::routing::get(serve_remote_html))
            .route("/api/remote/status", axum::routing::get(get_remote_status))
            .route("/api/remote/approve", axum::routing::post(post_remote_approve))
            .route("/api/remote/cron_trigger", axum::routing::post(post_remote_cron_trigger))
            .route("/health", axum::routing::get(handle_health))
            .layer(CorsLayer::permissive())
            .layer(axum::extract::DefaultBodyLimit::max(10 * 1024 * 1024)) // 10MB request body limit
            .with_state(state);

        println!("Starting OMNIX DevFlow HTTP Proxy on {}", addr);

        tauri::async_runtime::handle().spawn(async move {
            let listener = match tokio::net::TcpListener::bind(addr).await {
                Ok(l) => l,
                Err(e) => {
                    eprintln!("OMNIX Proxy: Failed to bind to {}: {}. Port may be in use.", addr, e);
                    return;
                }
            };
            if let Err(e) = axum::serve(listener, app)
                .with_graceful_shutdown(async move {
                    let _ = rx.await;
                    println!("OMNIX DevFlow HTTP Proxy shutting down gracefully...");
                })
                .await
            {
                eprintln!("OMNIX Proxy: Server error: {}", e);
            }
        });
    }

    pub fn stop(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
    }
}

// Main handler for /v1/messages (Claude format -> OpenAI format)
async fn handle_messages_for_agent(
    State(state): State<Arc<ProxyState>>,
    axum::extract::Path(agent_name): axum::extract::Path<String>,
    headers: axum::http::HeaderMap,
    Json(payload): Json<AnthropicRequest>,
) -> impl IntoResponse {
    let agent_name_decoded = agent_name.replace('_', " ");
    handle_messages_impl(state, Some(agent_name_decoded), headers, payload).await
}

async fn handle_messages(
    State(state): State<Arc<ProxyState>>,
    headers: axum::http::HeaderMap,
    Json(payload): Json<AnthropicRequest>,
) -> impl IntoResponse {
    handle_messages_impl(state, None, headers, payload).await
}

async fn handle_messages_impl(
    state: Arc<ProxyState>,
    agent_name_opt: Option<String>,
    headers: axum::http::HeaderMap,
    payload: AnthropicRequest,
) -> impl IntoResponse {
    // Concurrency limiting (New API/Sub2API inspired)
    let _permit = match state.concurrency_semaphore.try_acquire() {
        Ok(p) => p,
        Err(_) => {
            return (StatusCode::TOO_MANY_REQUESTS, "Too many concurrent requests. Please retry later.").into_response();
        }
    };

    let start_time = std::time::Instant::now();

    let target_account_id = headers.get("x-omnix-account-id")
        .and_then(|v| v.to_str().ok().map(|s| s.to_string()));

    let active_acc = if let Some(ref acc_id) = target_account_id {
        state.db.get_account_by_id(acc_id).unwrap_or(None)
    } else {
        let agent_name = agent_name_opt.unwrap_or_else(|| "Claude Code".to_string());
        state.db.get_active_account_for_agent(&agent_name).unwrap_or(None)
    };

    let target_model_name = if let Some(ref acc) = active_acc {
        acc.target_model.clone()
    } else {
        state.db.get_setting("target_model").unwrap_or(None).unwrap_or_else(|| "deepseek-chat".to_string())
    };

    let mut resolved_model = target_model_name.clone();
    if resolved_model == "Auto" {
        let (need_vis, need_reas, need_cod, need_spd) = classify_anthropic_capabilities(&payload);
        println!("OMNIX Router: Classification result -> Need Vision: {}, Reasoning: {}, Coding: {}, Speedy: {}", need_vis, need_reas, need_cod, need_spd);
        
        if let Ok(active_models) = state.db.get_connection().and_then(|conn| {
            let mut stmt = conn.prepare(
                "SELECT pm.model_name, pm.platform_id, pm.has_vision, pm.has_reasoning, pm.has_coding, mp.api_key, mp.api_address, mp.api_type
                 FROM platform_models pm
                 JOIN model_platforms mp ON pm.platform_id = mp.id
                 WHERE pm.is_enabled = 1 AND mp.is_enabled = 1"
            )?;
            let rows = stmt.query_map([], |row| {
                let has_vis: i32 = row.get(2)?;
                let has_reas: i32 = row.get(3)?;
                let has_cod: i32 = row.get(4)?;
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    has_vis != 0,
                    has_reas != 0,
                    has_cod != 0,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                ))
            })?;
            let mut res = Vec::new();
            for r in rows {
                if let Ok(item) = r {
                    res.push(item);
                }
            }
            Ok(res)
        }) {
            let mut best_model = None;
            let mut highest_score = -1;
            for (model_name, platform_id, vis, reas, cod, api_key, _api_address, api_type) in active_models {
                if api_key.trim().is_empty() && api_type != "ollama" {
                    continue;
                }
                let mut score = 0;
                if need_vis && vis { score += 10; }
                if need_reas && reas { score += 10; }
                if need_cod && cod { score += 5; }
                if !need_vis && !need_reas && !need_cod && vis { score -= 2; }
                
                if score > highest_score {
                    highest_score = score;
                    best_model = Some(format!("{}:{}", platform_id, model_name));
                }
            }
            if let Some(m) = best_model {
                resolved_model = m;
            }
        }
    }

    let (api_key_raw, api_host, api_type, actual_model_name) = match resolve_model_upstream(&state.db, &resolved_model) {
        Ok(res) => res,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to resolve model upstream: {}", e),
            ).into_response();
        }
    };

    let keys: Vec<&str> = api_key_raw.split(',').map(|k| k.trim()).filter(|k| !k.is_empty()).collect();
    if keys.is_empty() && api_type != "ollama" {
        return (
            StatusCode::UNAUTHORIZED,
            "API Key is not configured for this model platform.",
        ).into_response();
    }
    let api_key = if keys.is_empty() { "" } else { keys[state.request_counter.fetch_add(1, Ordering::Relaxed) % keys.len()] };

    let request_model = payload.model.clone();

    if api_type == "anthropic" {
        let mut native_req = payload;
        native_req.model = actual_model_name;
        
        let upstream_url = join_url(&api_host, "/v1/messages");
        let is_stream = native_req.stream.unwrap_or(false);
        
        println!("OMNIX Proxy (Anthropic Route Native): Forwarding to {} (stream={})", upstream_url, is_stream);
        
        let mut req_builder = state.http_client.post(&upstream_url)
            .header("Content-Type", "application/json")
            .header("anthropic-version", "2023-06-01")
            .json(&native_req);
            
        if !api_key.is_empty() {
            req_builder = req_builder.header("x-api-key", api_key);
        }
        
        let upstream_res = match req_builder.send().await {
            Ok(res) => res,
            Err(e) => return (StatusCode::BAD_GATEWAY, format!("Upstream failed: {}", e)).into_response(),
        };
        
        let status = upstream_res.status();
        if !status.is_success() {
            let err_body = upstream_res.text().await.unwrap_or_default();
            return (status, err_body).into_response();
        }
        
        // Log request (non-blocking)
        let log_db = state.db.clone();
        let log_model = resolved_model.clone();
        let log_latency = start_time.elapsed().as_millis() as i64;
        tokio::spawn(async move {
            log_request(&log_db, &log_model, Some("anthropic"), 0, 0, log_latency, status.as_u16() as i32, is_stream, !status.is_success(), None, None, "proxy");
        });

        if is_stream {
            let stream = upstream_res.bytes_stream().map(|r| r.map_err(|e| axum::Error::new(e)));
            Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "text/event-stream")
                .header("Cache-Control", "no-cache")
                .header("Connection", "keep-alive")
                .body(Body::from_stream(stream))
                .unwrap_or_else(|_| Response::builder().status(StatusCode::INTERNAL_SERVER_ERROR).body(Body::empty()).unwrap())
        } else {
            let bytes = match upstream_res.bytes().await {
                Ok(b) => b,
                Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
            };
            Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "application/json")
                .body(Body::from(bytes))
                .unwrap_or_else(|_| Response::builder().status(StatusCode::INTERNAL_SERVER_ERROR).body(Body::empty()).unwrap())
        }
    } else {
        // Translate to OpenAI format (For OpenAI or Ollama)
        let mut messages = Vec::new();
        if let Some(sys_prompt) = payload.system {
            messages.push(OpenAIRequestMessage {
                role: "system".to_string(),
                content: sys_prompt.to_string_content(),
            });
        }
        for msg in payload.messages {
            messages.push(OpenAIRequestMessage {
                role: msg.role,
                content: msg.content.to_string_content(),
            });
        }

        let openai_req = OpenAIRequest {
            model: actual_model_name,
            messages,
            max_tokens: payload.max_tokens,
            temperature: payload.temperature,
            stream: payload.stream,
        };

        let upstream_url = if api_type == "ollama" {
            join_url(&api_host, "/v1/chat/completions")
        } else {
            join_url(&api_host, "/chat/completions")
        };

        let is_stream = payload.stream.unwrap_or(false);
        println!("OMNIX Proxy (Claude Route to OpenAI): Forwarding to {} (stream={})", upstream_url, is_stream);
        
        let mut req_builder = state.http_client.post(&upstream_url)
            .header("Content-Type", "application/json")
            .json(&openai_req);
            
        if !api_key.is_empty() {
            req_builder = req_builder.header("Authorization", format!("Bearer {}", api_key));
        }

        let upstream_res = match req_builder.send().await {
            Ok(res) => res,
            Err(e) => return (StatusCode::BAD_GATEWAY, format!("Upstream request failed: {}", e)).into_response(),
        };

        let status = upstream_res.status();
        if !status.is_success() {
            let err_body = upstream_res.text().await.unwrap_or_default();
            return (status, err_body).into_response();
        }

        if is_stream {
            let stream = upstream_res.bytes_stream();
            let mut buffer_bytes = Vec::new();
            
            let anthropic_stream = stream.map(move |result| {
                match result {
                    Ok(bytes) => {
                        buffer_bytes.extend_from_slice(&bytes);
                        let mut output_bytes = Vec::new();
                        
                        while let Some(pos) = buffer_bytes.iter().position(|&b| b == b'\n') {
                            let line_bytes = &buffer_bytes[..pos];
                            let line = String::from_utf8_lossy(line_bytes).trim().to_string();
                            buffer_bytes.drain(..pos + 1);
                            
                            if line.starts_with("data: ") {
                                let data_content = &line[6..];
                                if data_content == "[DONE]" {
                                    output_bytes.extend_from_slice(b"event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n");
                                    break;
                                }
                                if let Ok(chunk_json) = serde_json::from_str::<OpenAIStreamChunk>(data_content) {
                                    if let Some(choice) = chunk_json.choices.first() {
                                        if let Some(delta_text) = &choice.delta.content {
                                            let anthropic_event = serde_json::json!({
                                                "type": "content_block_delta",
                                                "index": 0,
                                                "delta": {
                                                    "type": "text_delta",
                                                    "text": delta_text
                                                }
                                            });
                                            let formatted_line = format!("event: content_block_delta\ndata: {}\n\n", anthropic_event.to_string());
                                            output_bytes.extend_from_slice(formatted_line.as_bytes());
                                        }
                                    }
                                }
                            }
                        }
                        Ok::<_, axum::Error>(axum::body::Bytes::from(output_bytes))
                    }
                    Err(e) => Err(axum::Error::new(e)),
                }
            });

            Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "text/event-stream")
                .header("Cache-Control", "no-cache")
                .header("Connection", "keep-alive")
                .body(Body::from_stream(anthropic_stream))
                .unwrap_or_else(|_| Response::builder().status(StatusCode::INTERNAL_SERVER_ERROR).body(Body::empty()).unwrap())
        } else {
            #[derive(Debug, Deserialize)]
            struct OpenAIChoiceNonStream {
                message: OpenAIRequestMessage,
            }
            #[derive(Debug, Deserialize)]
            struct OpenAIResponse {
                choices: Vec<OpenAIChoiceNonStream>,
            }

            let res_bytes = match upstream_res.bytes().await {
                Ok(b) => b,
                Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
            };

            // Log request (non-blocking)
            let log_db = state.db.clone();
            let log_model = resolved_model.clone();
            let log_latency = start_time.elapsed().as_millis() as i64;
            tokio::spawn(async move {
                log_request(&log_db, &log_model, Some("openai"), 0, 0, log_latency, 200, false, false, None, None, "proxy");
            });

            if let Ok(openai_res) = serde_json::from_slice::<OpenAIResponse>(&res_bytes) {
                if let Some(choice) = openai_res.choices.first() {
                    let text_content = &choice.message.content;
                    let anthropic_res = serde_json::json!({
                        "id": "msg_local_proxy",
                        "type": "message",
                        "role": "assistant",
                        "content": [
                            {
                                "type": "text",
                                "text": text_content
                            }
                        ],
                        "model": request_model,
                        "stop_reason": "end_turn",
                        "stop_sequence": null,
                        "usage": {
                            "input_tokens": 0,
                            "output_tokens": 0
                        }
                    });
                    return Json(anthropic_res).into_response();
                }
            }
            (StatusCode::INTERNAL_SERVER_ERROR, "Failed to parse OpenAI response.").into_response()
        }
    }
}

// Forward direct OpenAI requests (e.g. for agents that request /v1/chat/completions directly)
async fn handle_openai_forward_for_agent(
    State(state): State<Arc<ProxyState>>,
    axum::extract::Path(agent_name): axum::extract::Path<String>,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> impl IntoResponse {
    let agent_name_decoded = agent_name.replace('_', " ");
    handle_openai_forward_impl(state, Some(agent_name_decoded), headers, payload).await
}

async fn handle_openai_forward(
    State(state): State<Arc<ProxyState>>,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> impl IntoResponse {
    handle_openai_forward_impl(state, None, headers, payload).await
}

async fn handle_openai_forward_impl(
    state: Arc<ProxyState>,
    agent_name_opt: Option<String>,
    headers: HeaderMap,
    payload: Value,
) -> impl IntoResponse {
    // Concurrency limiting (New API/Sub2API inspired)
    let _permit = match state.concurrency_semaphore.try_acquire() {
        Ok(p) => p,
        Err(_) => {
            return (StatusCode::TOO_MANY_REQUESTS, "Too many concurrent requests. Please retry later.").into_response();
        }
    };

    let start_time = std::time::Instant::now();
    let mut payload = payload;
    let target_account_id = headers.get("x-omnix-account-id")
        .and_then(|v| v.to_str().ok().map(|s| s.to_string()));

    let active_acc = if let Some(ref acc_id) = target_account_id {
        state.db.get_account_by_id(acc_id).unwrap_or(None)
    } else {
        let agent_name = agent_name_opt.unwrap_or_else(|| "Codex".to_string());
        state.db.get_active_account_for_agent(&agent_name).unwrap_or(None)
    };

    let target_model_name = if let Some(ref acc) = active_acc {
        acc.target_model.clone()
    } else {
        state.db.get_setting("target_model").unwrap_or(None).unwrap_or_else(|| "deepseek-chat".to_string())
    };

    let mut resolved_model = target_model_name.clone();
    if resolved_model == "Auto" {
        let mut messages = Vec::new();
        if let Some(payload_obj) = payload.as_object() {
            if let Some(msgs_val) = payload_obj.get("messages") {
                if let Some(msgs_arr) = msgs_val.as_array() {
                    for m in msgs_arr {
                        let role = m["role"].as_str().unwrap_or("user").to_string();
                        let content = if let Some(content_str) = m["content"].as_str() {
                            content_str.to_string()
                        } else {
                            m["content"].to_string()
                        };
                        messages.push(OpenAIRequestMessage { role, content });
                    }
                }
            }
        }

        let (need_vis, need_reas, need_cod, need_spd) = classify_request_capabilities(&messages);
        println!("OMNIX OpenAI Router: Classification result -> Need Vision: {}, Reasoning: {}, Coding: {}, Speedy: {}", need_vis, need_reas, need_cod, need_spd);
        
        if let Ok(active_models) = state.db.get_connection().and_then(|conn| {
            let mut stmt = conn.prepare(
                "SELECT pm.model_name, pm.platform_id, pm.has_vision, pm.has_reasoning, pm.has_coding, mp.api_key, mp.api_address, mp.api_type
                 FROM platform_models pm
                 JOIN model_platforms mp ON pm.platform_id = mp.id
                 WHERE pm.is_enabled = 1 AND mp.is_enabled = 1"
            )?;
            let rows = stmt.query_map([], |row| {
                let has_vis: i32 = row.get(2)?;
                let has_reas: i32 = row.get(3)?;
                let has_cod: i32 = row.get(4)?;
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    has_vis != 0,
                    has_reas != 0,
                    has_cod != 0,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                ))
            })?;
            let mut res = Vec::new();
            for r in rows {
                if let Ok(item) = r {
                    res.push(item);
                }
            }
            Ok(res)
        }) {
            let mut best_model = None;
            let mut highest_score = -1;
            for (model_name, platform_id, vis, reas, cod, api_key, _api_address, api_type) in active_models {
                if api_key.trim().is_empty() && api_type != "ollama" {
                    continue;
                }
                let mut score = 0;
                if need_vis && vis { score += 10; }
                if need_reas && reas { score += 10; }
                if need_cod && cod { score += 5; }
                if !need_vis && !need_reas && !need_cod && vis { score -= 2; }
                
                if score > highest_score {
                    highest_score = score;
                    best_model = Some(format!("{}:{}", platform_id, model_name));
                }
            }
            if let Some(m) = best_model {
                resolved_model = m;
            }
        }
    }

    let (api_key_raw, api_host, api_type, actual_model_name) = match resolve_model_upstream(&state.db, &resolved_model) {
        Ok(res) => res,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to resolve model upstream: {}", e),
            ).into_response();
        }
    };

    let keys: Vec<&str> = api_key_raw.split(',').map(|k| k.trim()).filter(|k| !k.is_empty()).collect();
    if keys.is_empty() && api_type != "ollama" {
        return (
            StatusCode::UNAUTHORIZED,
            "API Key is not configured for this model platform.",
        ).into_response();
    }
    let api_key = if keys.is_empty() { "" } else { keys[state.request_counter.fetch_add(1, Ordering::Relaxed) % keys.len()] };

    if api_type != "anthropic" {
        if let Some(payload_obj) = payload.as_object_mut() {
            payload_obj.insert("model".to_string(), serde_json::Value::String(actual_model_name.clone()));
        }

        let upstream_url = if api_type == "ollama" {
            join_url(&api_host, "/v1/chat/completions")
        } else {
            join_url(&api_host, "/chat/completions")
        };

        let is_stream = headers.get("accept").and_then(|v| v.to_str().ok()).map(|s| s.contains("text/event-stream")).unwrap_or(false)
            || payload.get("stream").and_then(|v| v.as_bool()).unwrap_or(false);

        println!("OMNIX Proxy (OpenAI Route): Forwarding request to {} (stream={})", upstream_url, is_stream);

        let mut req_builder = state.http_client.post(&upstream_url)
            .header("Content-Type", "application/json")
            .json(&payload);

        if !api_key.is_empty() {
            req_builder = req_builder.header("Authorization", format!("Bearer {}", api_key));
        }

        let upstream_res = match req_builder.send().await {
            Ok(res) => {
                println!("OMNIX Proxy (OpenAI Route): Upstream returned status {}", res.status());
                res
            }
            Err(e) => {
                eprintln!("OMNIX Proxy (OpenAI Route): Upstream request failed: {}", e);
                return (
                    StatusCode::BAD_GATEWAY,
                    format!("Failed to connect to upstream LLM API: {}", e),
                )
                    .into_response();
            }
        };

        let status = upstream_res.status();
        if !status.is_success() {
            let err_body = upstream_res.text().await.unwrap_or_default();
            eprintln!("OMNIX Proxy (OpenAI Route): Upstream non-success payload (status {}): {}", status, err_body);
            return (status, err_body).into_response();
        }

        // Log request (non-blocking)
        let log_db = state.db.clone();
        let log_model = resolved_model.clone();
        let log_latency = start_time.elapsed().as_millis() as i64;
        tokio::spawn(async move {
            log_request(&log_db, &log_model, Some("openai"), 0, 0, log_latency, status.as_u16() as i32, is_stream, !status.is_success(), None, None, "proxy");
        });

        if is_stream {
            let stream = upstream_res.bytes_stream().map(|r| r.map_err(|e| axum::Error::new(e)));
            Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "text/event-stream")
                .header("Cache-Control", "no-cache")
                .header("Connection", "keep-alive")
                .body(Body::from_stream(stream))
                .unwrap_or_else(|_| Response::builder().status(StatusCode::INTERNAL_SERVER_ERROR).body(Body::empty()).unwrap())
                .into_response()
        } else {
            let bytes = match upstream_res.bytes().await {
                Ok(b) => b,
                Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
            };
            Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "application/json")
                .body(Body::from(bytes))
                .unwrap_or_else(|_| Response::builder().status(StatusCode::INTERNAL_SERVER_ERROR).body(Body::empty()).unwrap())
                .into_response()
        }
    } else {
        #[derive(Debug, Deserialize)]
        struct OpenAIRequestPayload {
            messages: Vec<OpenAIMessage>,
            temperature: Option<f32>,
            max_tokens: Option<u32>,
            stream: Option<bool>,
        }

        #[derive(Debug, Deserialize, Serialize, Clone)]
        struct OpenAIMessage {
            role: String,
            content: String,
        }

        let openai_req: OpenAIRequestPayload = match serde_json::from_value(payload) {
            Ok(req) => req,
            Err(e) => return (StatusCode::BAD_REQUEST, format!("Invalid OpenAI request: {}", e)).into_response(),
        };

        let mut system_prompt = None;
        let mut anthropic_messages = Vec::new();
        for msg in openai_req.messages {
            if msg.role == "system" {
                system_prompt = Some(AnthropicMessageContent::String(msg.content));
            } else {
                anthropic_messages.push(AnthropicMessage {
                    role: msg.role,
                    content: AnthropicMessageContent::String(msg.content),
                });
            }
        }

        let native_req = AnthropicRequest {
            model: actual_model_name,
            messages: anthropic_messages,
            max_tokens: Some(openai_req.max_tokens.unwrap_or(4096)),
            system: system_prompt,
            temperature: openai_req.temperature,
            stream: openai_req.stream,
            reasoning_effort: None,
        };

        let upstream_url = join_url(&api_host, "/v1/messages");
        let is_stream = native_req.stream.unwrap_or(false);

        println!("OMNIX Proxy (OpenAI to Anthropic Route): Forwarding request to {} (stream={})", upstream_url, is_stream);

        let mut req_builder = state.http_client.post(&upstream_url)
            .header("Content-Type", "application/json")
            .header("anthropic-version", "2023-06-01")
            .json(&native_req);
            
        if !api_key.is_empty() {
            req_builder = req_builder.header("x-api-key", api_key);
        }

        let upstream_res = match req_builder.send().await {
            Ok(res) => res,
            Err(e) => return (StatusCode::BAD_GATEWAY, format!("Upstream failed: {}", e)).into_response(),
        };
        
        let status = upstream_res.status();
        if !status.is_success() {
            let err_body = upstream_res.text().await.unwrap_or_default();
            return (status, err_body).into_response();
        }

        if !is_stream {
            #[derive(Debug, Deserialize)]
            struct AnthropicContentBlock {
                text: String,
            }
            #[derive(Debug, Deserialize)]
            struct AnthropicResponse {
                id: String,
                content: Vec<AnthropicContentBlock>,
            }
            
            let res_bytes = match upstream_res.bytes().await {
                Ok(b) => b,
                Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
            };
            
            match serde_json::from_slice::<AnthropicResponse>(&res_bytes) {
                Ok(anthropic_res) => {
                    let text = anthropic_res.content.iter()
                        .map(|b| b.text.as_str())
                        .collect::<Vec<&str>>()
                        .join("\n");
                        
                    let openai_res = serde_json::json!({
                        "id": format!("chatcmpl-{}", anthropic_res.id),
                        "object": "chat.completion",
                        "created": std::time::SystemTime::now().duration_since(std::time::SystemTime::UNIX_EPOCH).unwrap_or_default().as_secs(),
                        "model": resolved_model,
                        "choices": [
                            {
                                "index": 0,
                                "message": {
                                    "role": "assistant",
                                    "content": text
                                },
                                "finish_reason": "stop"
                            }
                        ]
                    });
                    Json(openai_res).into_response()
                }
                Err(_) => {
                    (StatusCode::INTERNAL_SERVER_ERROR, "Failed to parse Anthropic response").into_response()
                }
            }
        } else {
            let stream = upstream_res.bytes_stream();
            let mut buffer_bytes = Vec::new();
            let model_for_chunk = resolved_model.clone();
            
            let openai_stream = stream.map(move |result| {
                match result {
                    Ok(bytes) => {
                        buffer_bytes.extend_from_slice(&bytes);
                        let mut output_bytes = Vec::new();
                        
                        while let Some(pos) = buffer_bytes.iter().position(|&b| b == b'\n') {
                            let line_bytes = &buffer_bytes[..pos];
                            let line = String::from_utf8_lossy(line_bytes).trim().to_string();
                            buffer_bytes.drain(..pos + 1);
                            
                            if line.starts_with("data: ") {
                                let data_content = &line[6..];
                                
                                #[derive(Debug, Deserialize)]
                                #[serde(tag = "type")]
                                enum AnthropicStreamEvent {
                                    #[serde(rename = "message_start")]
                                    MessageStart,
                                    #[serde(rename = "content_block_start")]
                                    ContentBlockStart,
                                    #[serde(rename = "content_block_delta")]
                                    ContentBlockDelta {
                                        delta: AnthropicDelta,
                                    },
                                    #[serde(rename = "content_block_stop")]
                                    ContentBlockStop,
                                    #[serde(rename = "message_delta")]
                                    MessageDelta,
                                    #[serde(rename = "message_stop")]
                                    MessageStop,
                                    #[serde(other)]
                                    Other,
                                }
                                
                                #[derive(Debug, Deserialize)]
                                struct AnthropicDelta {
                                    text: String,
                                }
                                
                                if let Ok(event) = serde_json::from_str::<AnthropicStreamEvent>(data_content) {
                                    match event {
                                        AnthropicStreamEvent::ContentBlockDelta { delta } => {
                                            let chunk = serde_json::json!({
                                                "id": "chatcmpl-stream",
                                                "object": "chat.completion.chunk",
                                                "created": 0,
                                                "model": model_for_chunk,
                                                "choices": [
                                                    {
                                                        "index": 0,
                                                        "delta": {
                                                            "content": delta.text
                                                        },
                                                        "finish_reason": null
                                                    }
                                                ]
                                            });
                                            output_bytes.extend_from_slice(format!("data: {}\n\n", chunk.to_string()).as_bytes());
                                        }
                                        AnthropicStreamEvent::MessageStop => {
                                            output_bytes.extend_from_slice(b"data: [DONE]\n\n");
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }
                        Ok::<_, axum::Error>(axum::body::Bytes::from(output_bytes))
                    }
                    Err(e) => Err(axum::Error::new(e)),
                }
            });

            Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "text/event-stream")
                .header("Cache-Control", "no-cache")
                .header("Connection", "keep-alive")
                .body(Body::from_stream(openai_stream))
                .unwrap_or_else(|_| Response::builder().status(StatusCode::INTERNAL_SERVER_ERROR).body(Body::empty()).unwrap())
                .into_response()
        }
    }
}

// --- 4. Remote Dashboard API Handlers ---

async fn serve_remote_html(
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
    State(state): State<Arc<ProxyState>>,
) -> impl IntoResponse {
    let token = params.get("token").cloned().unwrap_or_default();
    let expected_token = state.db.get_setting("remote_token")
        .unwrap_or(None)
        .unwrap_or_default();

    if token.is_empty() || token != expected_token {
        return axum::response::Html("<h1>401 Unauthorized - Invalid Access Token</h1>").into_response();
    }

    let html = include_str!("remote_dashboard.html");
    let parsed_html = html.replace("{{TOKEN}}", &token);
    axum::response::Html(parsed_html).into_response()
}

#[derive(Debug, Serialize)]
struct RemoteStatus {
    api_host: String,
    target_model: String,
    active_sessions: Vec<String>,
    tasks: Vec<crate::commands::DbTask>,
    cron_tasks: Vec<CronTaskInfo>,
}

#[derive(Debug, Serialize)]
struct CronTaskInfo {
    id: String,
    title: String,
    schedule: String,
    agent_name: String,
    is_active: bool,
    last_run: Option<String>,
}

async fn get_remote_status(
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
    State(state): State<Arc<ProxyState>>,
) -> impl IntoResponse {
    let token = params.get("token").cloned().unwrap_or_default();
    let expected_token = state.db.get_setting("remote_token")
        .unwrap_or(None)
        .unwrap_or_default();

    if token.is_empty() || token != expected_token {
        return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
    }

    let active_sessions = state.agent_manager.get_active_session_ids();

    let mut tasks = Vec::new();
    if let Some(session_id) = active_sessions.first() {
        if let Ok(conn) = state.db.get_connection() {
            if let Ok(mut stmt) = conn.prepare(
                "SELECT id, conversation_id, title, status, order_num, dependencies 
                 FROM tasks WHERE conversation_id = ?1 ORDER BY order_num ASC"
            ) {
                let rows = stmt.query_map(params![session_id], |row| {
                    let deps_str: String = row.get(5)?;
                    let dependencies: Vec<String> = serde_json::from_str(&deps_str).unwrap_or_default();
                    Ok(crate::commands::DbTask {
                        id: row.get(0)?,
                        conversation_id: row.get(1)?,
                        title: row.get(2)?,
                        status: row.get(3)?,
                        order_num: row.get(4)?,
                        dependencies,
                    })
                });
                if let Ok(rows) = rows {
                    tasks = rows.flatten().collect();
                }
            }
        }
    }

    let mut cron_tasks = Vec::new();
    if let Ok(conn) = state.db.get_connection() {
        if let Ok(mut stmt) = conn.prepare(
            "SELECT id, title, schedule, agent_name, is_active, last_run FROM cron_tasks"
        ) {
            let rows = stmt.query_map([], |row| {
                let is_active_int: i32 = row.get(4)?;
                Ok(CronTaskInfo {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    schedule: row.get(2)?,
                    agent_name: row.get(3)?,
                    is_active: is_active_int != 0,
                    last_run: row.get(5)?,
                })
            });
            if let Ok(rows) = rows {
                cron_tasks = rows.flatten().collect();
            }
        }
    }

    let api_host = state.db.get_setting("api_host").unwrap_or(None).unwrap_or_default();
    let target_model = state.db.get_setting("target_model").unwrap_or(None).unwrap_or_default();

    Json(RemoteStatus {
        api_host,
        target_model,
        active_sessions,
        tasks,
        cron_tasks,
    }).into_response()
}

#[derive(Debug, Deserialize)]
struct ApprovePayload {
    session_id: String,
    input: String,
}

async fn post_remote_approve(
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
    State(state): State<Arc<ProxyState>>,
    Json(body): Json<ApprovePayload>,
) -> impl IntoResponse {
    let token = params.get("token").cloned().unwrap_or_default();
    let expected_token = state.db.get_setting("remote_token")
        .unwrap_or(None)
        .unwrap_or_default();

    if token.is_empty() || token != expected_token {
        return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
    }

    match state.agent_manager.send_stdin(&body.session_id, body.input.clone()) {
        Ok(_) => (StatusCode::OK, "Success").into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, format!("Failed: {}", e)).into_response(),
    }
}

#[derive(Debug, Deserialize)]
struct CronTriggerPayload {
    task_id: String,
}

async fn post_remote_cron_trigger(
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
    State(state): State<Arc<ProxyState>>,
    Json(body): Json<CronTriggerPayload>,
) -> impl IntoResponse {
    let token = params.get("token").cloned().unwrap_or_default();
    let expected_token = state.db.get_setting("remote_token")
        .unwrap_or(None)
        .unwrap_or_default();

    if token.is_empty() || token != expected_token {
        return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
    }

    let db = Arc::clone(&state.db);
    let conn_res = db.get_connection();
    if let Ok(conn) = conn_res {
        let mut stmt = match conn.prepare("SELECT id, title, agent_name, args, workspace_dir FROM cron_tasks WHERE id = ?1") {
            Ok(s) => s,
            Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "DB Error").into_response(),
        };
        
        let row_res = stmt.query_row(params![body.task_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        });

        if let Ok((id, _title, agent_name, args_str, workspace_dir)) = row_res {
            tauri::async_runtime::handle().spawn(async move {
                let _ = crate::agent::run_cron_task(db, id, agent_name, args_str, workspace_dir).await;
            });
            return (StatusCode::OK, "Task triggered").into_response();
        }
    }

    (StatusCode::BAD_REQUEST, "Task not found").into_response()
}

// --- Dynamic Capability Routing Helpers ---


fn classify_request_capabilities(messages: &[OpenAIRequestMessage]) -> (bool, bool, bool, bool) {
    let mut need_vision = false;
    let mut need_reasoning = false;
    let mut need_coding = false;

    for msg in messages {
        let content_lower = msg.content.to_lowercase();
        if content_lower.contains("data:image/") || content_lower.contains("[image]") || content_lower.contains("图片") || content_lower.contains("图像") {
            need_vision = true;
        }
        if content_lower.contains("prove") 
            || content_lower.contains("proof") 
            || content_lower.contains("math") 
            || content_lower.contains("算法") 
            || content_lower.contains("algorithm") 
            || content_lower.contains("deadlock") 
            || content_lower.contains("死锁") 
            || content_lower.contains("性能优化")
            || content_lower.contains("explain step-by-step")
            || content_lower.contains("思维链") 
        {
            need_reasoning = true;
        }
        if content_lower.contains("```") 
            || content_lower.contains("code") 
            || content_lower.contains("代码") 
            || content_lower.contains("write a") 
            || content_lower.contains("refactor") 
            || content_lower.contains("重构") 
            || content_lower.contains("implement") 
            || content_lower.contains("编写") 
            || content_lower.contains(".rs") 
            || content_lower.contains(".tsx") 
            || content_lower.contains(".ts") 
            || content_lower.contains(".js") 
            || content_lower.contains(".py") 
        {
            need_coding = true;
        }
    }

    let total_len: usize = messages.iter().map(|m| m.content.len()).sum();
    let need_speedy = !need_reasoning && !need_vision && total_len < 300;

    (need_vision, need_reasoning, need_coding, need_speedy)
}

fn classify_anthropic_capabilities(payload: &AnthropicRequest) -> (bool, bool, bool, bool) {
    let mut need_vision = false;
    let mut need_reasoning = false;
    let mut need_coding = false;

    for msg in &payload.messages {
        let content_str = msg.content.to_string_content();
        let content_lower = content_str.to_lowercase();
        if content_lower.contains("image") || content_lower.contains("图片") || content_lower.contains("图像") {
            need_vision = true;
        }
        if content_lower.contains("prove") 
            || content_lower.contains("proof") 
            || content_lower.contains("math") 
            || content_lower.contains("算法") 
            || content_lower.contains("algorithm") 
            || content_lower.contains("deadlock") 
            || content_lower.contains("死锁") 
            || content_lower.contains("性能优化")
            || content_lower.contains("思维链") 
        {
            need_reasoning = true;
        }
        if content_lower.contains("```") 
            || content_lower.contains("code") 
            || content_lower.contains("代码") 
            || content_lower.contains("write a") 
            || content_lower.contains("refactor") 
            || content_lower.contains("重构") 
            || content_lower.contains("implement") 
            || content_lower.contains("编写") 
        {
            need_coding = true;
        }
    }

    let total_len: usize = payload.messages.iter().map(|m| m.content.to_string_content().len()).sum();
    let need_speedy = !need_reasoning && !need_vision && total_len < 300;

    (need_vision, need_reasoning, need_coding, need_speedy)
}


fn resolve_model_upstream(
    db: &DbManager,
    target_model_name: &str,
) -> Result<(String, String, String, String), String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;

    // 1. If target_model_name has platform prefix (e.g. "platform_id:model_name")
    if let Some(pos) = target_model_name.find(':') {
        let platform_id = &target_model_name[..pos];
        let model_name = &target_model_name[pos + 1..];

        let mut stmt = conn.prepare(
            "SELECT api_key, api_address, api_type FROM model_platforms WHERE id = ?1 AND is_enabled = 1"
        ).map_err(|e| e.to_string())?;

        let platform_opt = stmt.query_row(params![platform_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        }).ok();

        if let Some((api_key, api_address, api_type)) = platform_opt {
            return Ok((api_key, api_address, api_type, model_name.to_string()));
        }
    }

    // 2. Weighted selection from matching platforms (New API/Sub2API inspired)
    //    Find all platforms that serve this model, ordered by priority DESC, weight DESC
    //    Only consider healthy and enabled platforms
    let mut stmt = conn.prepare(
        "SELECT mp.id, mp.api_key, mp.api_address, mp.api_type, mp.weight, mp.priority,
                pm.model_name, mp.consecutive_failures
         FROM platform_models pm
         JOIN model_platforms mp ON pm.platform_id = mp.id
         WHERE pm.model_name = ?1 AND pm.is_enabled = 1 AND mp.is_enabled = 1 AND mp.is_healthy = 1
         ORDER BY mp.priority DESC, mp.weight DESC"
    ).map_err(|e| e.to_string())?;

    let candidates: Vec<(String, String, String, String, String, i32, String, i32)> = stmt
        .query_map(params![target_model_name], |row| {
            Ok((
                row.get::<_, String>(0)?,  // platform_id
                row.get::<_, String>(1)?,  // api_key
                row.get::<_, String>(2)?,  // api_address
                row.get::<_, String>(3)?,  // api_type
                row.get::<_, String>(4)?,  // weight (stored as TEXT in some configs)
                row.get::<_, i32>(5)?,     // priority
                row.get::<_, String>(6)?,  // model_name
                row.get::<_, i32>(7)?,     // consecutive_failures
            ))
        })
        .map_err(|e| e.to_string())?
        .flatten()
        .collect();

    if !candidates.is_empty() {
        // Weighted random selection: candidates are sorted by priority then weight.
        // Higher priority platforms are always preferred.
        // Within same priority, select based on weight (proportional).
        let highest_priority = candidates[0].5;
        let same_priority: Vec<_> = candidates.iter()
            .filter(|c| c.5 == highest_priority)
            .collect();

        // Calculate total weight for same-priority candidates
        let total_weight: i32 = same_priority.iter().map(|c| {
            c.4.parse::<i32>().unwrap_or(1).max(1)
        }).sum();

        // Weighted selection using request counter as deterministic random
        let counter = db.get_connection()
            .ok()
            .and_then(|c| c.query_row("SELECT COUNT(*) FROM request_logs", [], |r| r.get::<_, i64>(0)).ok())
            .unwrap_or(0) as i32;

        let mut pick = (counter as i32).rem_euclid(total_weight);
        for candidate in &same_priority {
            let w = candidate.4.parse::<i32>().unwrap_or(1).max(1);
            pick -= w;
            if pick < 0 {
                // Update last_used_at
                let _ = conn.execute(
                    "UPDATE model_platforms SET last_used_at = datetime('now') WHERE id = ?1",
                    params![candidate.0],
                );
                return Ok((candidate.1.clone(), candidate.2.clone(), candidate.3.clone(), candidate.6.clone()));
            }
        }

        // Fallback to first candidate
        let c = &same_priority[0];
        return Ok((c.1.clone(), c.2.clone(), c.3.clone(), c.6.clone()));
    }

    // 3. Fallback to any healthy active platform
    let mut stmt = conn.prepare(
        "SELECT api_key, api_address, api_type, name FROM model_platforms WHERE is_enabled = 1 AND is_healthy = 1 ORDER BY priority DESC, weight DESC LIMIT 1"
    ).map_err(|e| e.to_string())?;

    let fallback_opt = stmt.query_row([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
        ))
    }).ok();

    if let Some((api_key, api_address, api_type, _name)) = fallback_opt {
        return Ok((api_key, api_address, api_type, target_model_name.to_string()));
    }

    Err("No active model platforms configured in database.".to_string())
}

fn join_url(base: &str, path: &str) -> String {
    let base_trimmed = base.trim_end_matches('/');
    let path_trimmed = path.trim_start_matches('/');
    format!("{}/{}", base_trimmed, path_trimmed)
}

// ── Health Endpoint (New API/Sub2API inspired) ────────

/// GET /health — Returns proxy status and platform summary
async fn handle_health(
    State(state): State<Arc<ProxyState>>,
) -> impl IntoResponse {
    let conn = match state.db.get_connection() {
        Ok(c) => c,
        Err(_) => {
            return Json(serde_json::json!({
                "status": "error",
                "message": "Database connection failed"
            })).into_response();
        }
    };

    let total_platforms: i64 = conn.query_row(
        "SELECT COUNT(*) FROM model_platforms", [], |r| r.get(0)
    ).unwrap_or(0);

    let enabled_platforms: i64 = conn.query_row(
        "SELECT COUNT(*) FROM model_platforms WHERE is_enabled = 1", [], |r| r.get(0)
    ).unwrap_or(0);

    let healthy_platforms: i64 = conn.query_row(
        "SELECT COUNT(*) FROM model_platforms WHERE is_enabled = 1 AND is_healthy = 1", [], |r| r.get(0)
    ).unwrap_or(0);

    let total_models: i64 = conn.query_row(
        "SELECT COUNT(*) FROM platform_models WHERE is_enabled = 1", [], |r| r.get(0)
    ).unwrap_or(0);

    let total_requests: i64 = conn.query_row(
        "SELECT COUNT(*) FROM request_logs", [], |r| r.get(0)
    ).unwrap_or(0);

    let requests_today: i64 = conn.query_row(
        "SELECT COUNT(*) FROM request_logs WHERE date(timestamp) = date('now')", [], |r| r.get(0)
    ).unwrap_or(0);

    Json(serde_json::json!({
        "status": "ok",
        "proxy_port": 1421,
        "platforms": {
            "total": total_platforms,
            "enabled": enabled_platforms,
            "healthy": healthy_platforms,
            "unhealthy": enabled_platforms - healthy_platforms,
        },
        "models": {
            "total": total_models,
        },
        "requests": {
            "total": total_requests,
            "today": requests_today,
        }
    })).into_response()
}

// ── Platform Health Tracking (New API/Sub2API inspired) ──

/// Mark a platform as healthy after a successful request
#[allow(dead_code)]
pub fn mark_platform_healthy(db: &DbManager, platform_id: &str) {
    if let Ok(conn) = db.get_connection() {
        let _ = conn.execute(
            "UPDATE model_platforms SET is_healthy = 1, consecutive_failures = 0, last_error = NULL WHERE id = ?1",
            params![platform_id],
        );
    }
}

/// Mark a platform as unhealthy after consecutive failures
#[allow(dead_code)]
pub fn mark_platform_unhealthy(db: &DbManager, platform_id: &str, error: &str) {
    if let Ok(conn) = db.get_connection() {
        let _ = conn.execute(
            "UPDATE model_platforms SET consecutive_failures = consecutive_failures + 1, last_error = ?1 WHERE id = ?2",
            params![error, platform_id],
        );
        // Auto-disable after 5 consecutive failures
        let _ = conn.execute(
            "UPDATE model_platforms SET is_healthy = 0 WHERE id = ?1 AND consecutive_failures >= 5",
            params![platform_id],
        );
    }
}

// ── Request Logging (New API/Sub2API inspired) ───────

/// Write a request log entry to the database (async, non-blocking)
pub fn log_request(
    db: &DbManager,
    model: &str,
    platform: Option<&str>,
    prompt_tokens: i64,
    completion_tokens: i64,
    latency_ms: i64,
    status_code: i32,
    is_stream: bool,
    is_error: bool,
    error_message: Option<&str>,
    request_id: Option<&str>,
    source: &str,
) {
    let conn = match db.get_connection() {
        Ok(c) => c,
        Err(_) => return,
    };
    let total_tokens = prompt_tokens + completion_tokens;
    let _ = conn.execute(
        "INSERT INTO request_logs (model, platform, prompt_tokens, completion_tokens, total_tokens, latency_ms, status_code, is_stream, is_error, error_message, request_id, source)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        params![
            model,
            platform.unwrap_or(""),
            prompt_tokens,
            completion_tokens,
            total_tokens,
            latency_ms,
            status_code,
            is_stream as i32,
            is_error as i32,
            error_message.unwrap_or(""),
            request_id.unwrap_or(""),
            source,
        ],
    );
}

// ── Embeddings Handler ─────────────────────────────────
//
// Transparent proxy for /v1/embeddings requests.
// Resolves the model to its upstream platform, then forwards
// the request to the appropriate embedding API endpoint:
//   - Ollama:  POST {api_address}/api/embeddings
//   - Others:  POST {api_address}/embeddings

async fn handle_embeddings(
    State(state): State<Arc<ProxyState>>,
    _headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let model_name = match payload.get("model").and_then(|m| m.as_str()) {
        Some(m) => m.to_string(),
        None => {
            return (StatusCode::BAD_REQUEST, "Missing 'model' field in request body").into_response();
        }
    };

    // Resolve the model to an upstream platform
    let (api_key, api_address, api_type, actual_model) =
        match resolve_model_upstream(&state.db, &model_name) {
            Ok(res) => res,
            Err(e) => {
                return (StatusCode::NOT_FOUND, format!("Model resolution failed: {}", e)).into_response();
            }
        };

    // Build the upstream URL based on api_type
    let upstream_url = match api_type.as_str() {
        "ollama" => join_url(&api_address, "/api/embeddings"),
        _ => join_url(&api_address, "/embeddings"),
    };

    // Replace the model name in the payload with the actual model name
    let mut forwarded_payload = payload.clone();
    if let Some(obj) = forwarded_payload.as_object_mut() {
        obj.insert("model".to_string(), Value::String(actual_model.clone()));
    }

    // Ollama uses a different request format: {"model", "prompt"} instead of {"model", "input"}
    // Convert OpenAI format to Ollama format if needed
    if api_type.as_str() == "ollama" {
        if let Some(obj) = forwarded_payload.as_object_mut() {
            // Ollama only supports single-prompt embedding; extract first input string
            if let Some(input) = obj.remove("input") {
                let prompt = match input {
                    Value::String(s) => s,
                    Value::Array(arr) => arr.first()
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    _ => String::new(),
                };
                obj.insert("prompt".to_string(), Value::String(prompt));
            }
        }
    }

    // Forward the request
    let mut req = state.http_client.post(&upstream_url).json(&forwarded_payload);
    if !api_key.trim().is_empty() {
        req = req.header("Authorization", format!("Bearer {}", api_key.trim()));
    }

    match req.send().await {
        Ok(resp) => {
            let status = resp.status();
            let body = match resp.bytes().await {
                Ok(b) => b,
                Err(e) => {
                    return (StatusCode::BAD_GATEWAY, format!("Failed to read upstream response: {}", e)).into_response();
                }
            };
            // Return the upstream response as-is
            (status, body.to_vec()).into_response()
        }
        Err(e) => {
            (StatusCode::BAD_GATEWAY, format!("Upstream request failed: {}", e)).into_response()
        }
    }
}

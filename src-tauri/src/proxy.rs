use axum::{
    body::Body,
    extract::State,
    http::{HeaderMap, HeaderValue, StatusCode},
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

use crate::db::DbManager;

// Define sharing state
pub struct ProxyState {
    pub db: Arc<DbManager>,
    pub http_client: Client,
    pub request_counter: AtomicUsize,
}

// Anthropic Request format
#[derive(Debug, Deserialize, Serialize)]
pub struct AnthropicMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct AnthropicRequest {
    pub model: String,
    pub messages: Vec<AnthropicMessage>,
    pub max_tokens: Option<u32>,
    pub system: Option<String>,
    pub temperature: Option<f32>,
    pub stream: Option<bool>,
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
    max_tokens: Option<u32>,
    temperature: Option<f32>,
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

    pub fn start(&mut self, db: Arc<DbManager>, port: u16) {
        let (tx, rx) = oneshot::channel::<()>();
        self.shutdown_tx = Some(tx);

        let state = Arc::new(ProxyState {
            db,
            http_client: Client::new(),
            request_counter: AtomicUsize::new(0),
        });

        let app = Router::new()
            .route("/v1/messages", post(handle_messages))
            // Support transparent forwarding for normal OpenAI completions
            .route("/v1/chat/completions", post(handle_openai_forward))
            .layer(CorsLayer::permissive())
            .with_state(state);

        let addr = SocketAddr::from(([127, 0, 0, 1], port));
        println!("Starting OMNIX DevFlow HTTP Proxy on {}", addr);

        tokio::spawn(async move {
            let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
            axum::serve(listener, app)
                .with_graceful_shutdown(async move {
                    let _ = rx.await;
                    println!("OMNIX DevFlow HTTP Proxy shutting down gracefully...");
                })
                .await
                .unwrap();
        });
    }

    pub fn stop(self) {
        if let Some(tx) = self.shutdown_tx {
            let _ = tx.send(());
        }
    }
}

// Main handler for /v1/messages (Claude format -> OpenAI format)
async fn handle_messages(
    State(state): State<Arc<ProxyState>>,
    Json(payload): Json<AnthropicRequest>,
) -> impl IntoResponse {
    // 1. Fetch configurations from SQLite DB (supporting comma-separated load balancing)
    let api_key_raw = state.db.get_setting("api_key").unwrap_or(None).unwrap_or_default();
    let keys: Vec<&str> = api_key_raw.split(',').map(|k| k.trim()).filter(|k| !k.is_empty()).collect();

    if keys.is_empty() {
        return (
            StatusCode::UNAUTHORIZED,
            "API Key is not configured. Please set it in OMNIX Settings.",
        )
            .into_response();
    }

    let idx = state.request_counter.fetch_add(1, Ordering::Relaxed) % keys.len();
    let api_key = keys[idx];

    let api_host = state.db.get_setting("api_host").unwrap_or(None)
        .unwrap_or_else(|| "https://api.openai.com/v1".to_string());

    // 2. Map Anthropic schema to OpenAI messages layout
    let mut messages = Vec::new();
    
    // Inject system instruction if present
    if let Some(sys_prompt) = payload.system {
        messages.push(OpenAIRequestMessage {
            role: "system".to_string(),
            content: sys_prompt,
        });
    }

    // Map dialog messages
    for msg in payload.messages {
        messages.push(OpenAIRequestMessage {
            role: msg.role,
            content: msg.content,
        });
    }

    // Map configured model (or fallback to user model selection)
    // Some tools hardcode claude models, so we check and translate if needed
    let target_model = if payload.model.contains("claude-") {
        // If it's Claude model but we are targeting DeepSeek or other host, map it.
        // We will default to the model configured in the OMNIX settings if present.
        state.db.get_setting("target_model").unwrap_or(None)
            .unwrap_or_else(|| "deepseek-chat".to_string())
    } else {
        payload.model.clone()
    };

    let openai_req = OpenAIRequest {
        model: target_model,
        messages,
        max_tokens: payload.max_tokens,
        temperature: payload.temperature,
        stream: payload.stream,
    };

    let upstream_url = format!("{}/chat/completions", api_host.trim_end_matches('/'));

    // 3. Make HTTP request to upstream
    let mut req_builder = state.http_client.post(&upstream_url)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&openai_req);

    // Forward stream flag
    let is_stream = payload.stream.unwrap_or(false);

    let upstream_res = match req_builder.send().await {
        Ok(res) => res,
        Err(e) => {
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
        return (status, err_body).into_response();
    }

    if is_stream {
        // Stream response translation
        let stream = upstream_res.bytes_stream();
        let mut buffer = String::new();
        
        let anthropic_stream = stream.map(move |result| {
            match result {
                Ok(bytes) => {
                    let chunk = String::from_utf8_lossy(&bytes);
                    buffer.push_str(&chunk);
                    
                    let mut output_bytes = Vec::new();
                    
                    // Process complete lines in buffer
                    while let Some(pos) = buffer.find('\n') {
                        let line = buffer[..pos].trim().to_string();
                        buffer = buffer[pos + 1..].to_string();
                        
                        if line.starts_with("data: ") {
                            let data_content = &line[6..];
                            if data_content == "[DONE]" {
                                // Write Claude stream stop structure
                                output_bytes.extend_from_slice(b"event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n");
                                break;
                            }
                            
                            // Parse OpenAI JSON
                            if let Ok(chunk_json) = serde_json::from_str::<OpenAIStreamChunk>(data_content) {
                                if let Some(choice) = chunk_json.choices.first() {
                                    if let Some(delta_text) = &choice.delta.content {
                                        // Format as Anthropic content_block_delta event
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

        // Initialize header maps for SSE stream
        let mut headers = HeaderMap::new();
        headers.insert("Content-Type", HeaderValue::from_static("text/event-stream"));
        headers.insert("Cache-Control", HeaderValue::from_static("no-cache"));
        headers.insert("Connection", HeaderValue::from_static("keep-alive"));

        Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "text/event-stream")
            .header("Cache-Control", "no-cache")
            .header("Connection", "keep-alive")
            .body(Body::from_stream(anthropic_stream))
            .unwrap()
    } else {
        // Non-stream response translation
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
                    "model": payload.model,
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

        (StatusCode::INTERNAL_SERVER_ERROR, "Failed to parse upstream OpenAI response structure.").into_response()
    }
}

// Forward direct OpenAI requests (e.g. for agents that request /v1/chat/completions directly)
async fn handle_openai_forward(
    State(state): State<Arc<ProxyState>>,
    mut headers: HeaderMap,
    Json(mut payload): Json<Value>,
) -> impl IntoResponse {
    let api_key_raw = state.db.get_setting("api_key").unwrap_or(None).unwrap_or_default();
    let keys: Vec<&str> = api_key_raw.split(',').map(|k| k.trim()).filter(|k| !k.is_empty()).collect();

    if keys.is_empty() {
        return (
            StatusCode::UNAUTHORIZED,
            "API Key is not configured. Please set it in OMNIX Settings.",
        )
            .into_response();
    }

    let idx = state.request_counter.fetch_add(1, Ordering::Relaxed) % keys.len();
    let api_key = keys[idx];

    let api_host = state.db.get_setting("api_host").unwrap_or(None)
        .unwrap_or_else(|| "https://api.openai.com/v1".to_string());

    // Rewrite model if set in OMNIX settings
    if let Some(payload_obj) = payload.as_object_mut() {
        if let Some(model_val) = payload_obj.get("model") {
            if let Some(model_str) = model_val.as_str() {
                if model_str.contains("claude") || model_str.starts_with("gpt-") {
                    let target_model = state.db.get_setting("target_model").unwrap_or(None)
                        .unwrap_or_else(|| "deepseek-chat".to_string());
                    payload_obj.insert("model".to_string(), serde_json::Value::String(target_model));
                }
            }
        }
    }

    let upstream_url = format!("{}/chat/completions", api_host.trim_end_matches('/'));

    let mut req_builder = state.http_client.post(&upstream_url)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&payload);

    let upstream_res = match req_builder.send().await {
        Ok(res) => res,
        Err(e) => {
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
        return (status, err_body).into_response();
    }

    // Return upstream response directly (stream or non-stream)
    let is_stream = headers.get("accept").and_then(|v| v.to_str().ok()).map(|s| s.contains("text/event-stream")).unwrap_or(false)
        || payload.get("stream").and_then(|v| v.as_bool()).unwrap_or(false);

    if is_stream {
        let stream = upstream_res.bytes_stream().map(|r| r.map_err(|e| axum::Error::new(e)));
        Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "text/event-stream")
            .header("Cache-Control", "no-cache")
            .header("Connection", "keep-alive")
            .body(Body::from_stream(stream))
            .unwrap()
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
            .unwrap()
            .into_response()
    }
}

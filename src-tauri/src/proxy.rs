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
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::oneshot;
use tower_http::cors::CorsLayer;

use crate::db::DbManager;

// Remote phone-panel handlers live in proxy_remote.rs (child module so the
// split stays a pure move — it reuses this file's private items/imports).
#[path = "proxy_remote.rs"]
mod remote_panel;
use remote_panel::*;

// Define sharing state
pub struct ProxyState {
    pub db: Arc<DbManager>,
    pub agent_manager: Arc<crate::agent::AgentManager>,
    pub runtime_manager: Arc<crate::runtime_manager::RuntimeManager>,
    pub http_client: Client,
    pub request_counter: AtomicUsize,
    pub concurrency_semaphore: Arc<tokio::sync::Semaphore>,
}

// Wire DTOs (Anthropic/OpenAI request & response shapes) live in
// proxy_types.rs; re-exported so `crate::proxy::*` paths keep working.
pub use crate::proxy_types::*;

pub struct ProxyServer {
    shutdown_tx: Option<oneshot::Sender<()>>,
}

impl ProxyServer {
    pub fn new() -> Self {
        Self { shutdown_tx: None }
    }

    pub fn start(
        &mut self,
        db: Arc<DbManager>,
        agent_manager: Arc<crate::agent::AgentManager>,
        runtime_manager: Arc<crate::runtime_manager::RuntimeManager>,
        port: u16,
    ) {
        let (tx, rx) = oneshot::channel::<()>();
        self.shutdown_tx = Some(tx);

        let use_wsl = db
            .get_setting("use_wsl")
            .unwrap_or(None)
            .unwrap_or_else(|| "false".to_string())
            == "true";
        // Remote phone access: bind all interfaces only when the
        // user has explicitly enabled it, so the gateway stays localhost-only by
        // default. The remote endpoints are token-gated.
        let remote_enabled = db
            .get_setting("remote_access_enabled")
            .unwrap_or(None)
            .unwrap_or_else(|| "false".to_string())
            == "true";
        let bind_ip = if use_wsl || remote_enabled {
            [0, 0, 0, 0]
        } else {
            [127, 0, 0, 1]
        };
        let addr = SocketAddr::from((bind_ip, port));

        // CORS: restrict to localhost origins whenever we bind to 0.0.0.0 —
        // this must follow the *bind* decision, not just WSL. Previously only
        // `use_wsl` tightened it, so enabling 手机远程访问 exposed the gateway
        // with `CorsLayer::permissive()`, letting any web page a LAN browser
        // visits script requests against it. The remote panel is same-origin,
        // so restricting cross-origin here does not affect it.
        let cors_layer = if use_wsl || remote_enabled {
            CorsLayer::new()
                .allow_origin([
                    "http://localhost:1420"
                        .parse::<axum::http::HeaderValue>()
                        .expect("valid localhost URL"),
                    "http://127.0.0.1:1420"
                        .parse::<axum::http::HeaderValue>()
                        .expect("valid 127.0.0.1 URL"),
                    "tauri://localhost"
                        .parse::<axum::http::HeaderValue>()
                        .expect("valid tauri scheme"),
                ])
                .allow_methods(tower_http::cors::Any)
                .allow_headers(tower_http::cors::Any)
        } else {
            // 坑点1-ack: 走到这个分支时监听地址是 127.0.0.1，只有本机进程够得着；
            // 而且网关不使用 cookie 凭证（鉴权走 header/令牌），不构成
            // 「通配符 + credentials」的组合。绑 0.0.0.0 的分支在上面，是白名单。
            CorsLayer::permissive()
        };

        let client = Client::builder()
            .connect_timeout(std::time::Duration::from_secs(15))
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .unwrap_or_else(|_| Client::new());

        let state = Arc::new(ProxyState {
            db,
            agent_manager,
            runtime_manager,
            http_client: client,
            request_counter: AtomicUsize::new(0),
            concurrency_semaphore: Arc::new(tokio::sync::Semaphore::new(20)), // max 20 concurrent proxy requests
        });

        let app = Router::new()
            // P1：OMNIX 作为能力提供方。挂在网关上而不是另起一个进程，
            // 鉴权、CORS、绑定地址都沿用同一套（见 guard_gateway_access）。
            .route("/mcp", post(handle_mcp))
            .route("/v1/messages", post(handle_messages))
            .route("/v1/chat/completions", post(handle_openai_forward))
            .route("/v1/embeddings", post(handle_embeddings))
            .route(
                "/agent/:agent_name/v1/messages",
                post(handle_messages_for_agent),
            )
            .route(
                "/agent/:agent_name/v1/chat/completions",
                post(handle_openai_forward_for_agent),
            )
            .route(
                "/session/:session_key/v1/messages",
                post(handle_messages_for_session),
            )
            .route(
                "/session/:session_key/v1/responses",
                post(handle_responses_for_session),
            )
            .route("/remote", axum::routing::get(serve_remote_html))
            .route("/api/remote/status", axum::routing::get(get_remote_status))
            .route("/api/remote/conversations", axum::routing::get(get_remote_conversations))
            .route("/api/remote/messages", axum::routing::get(get_remote_messages))
            .route("/api/remote/chat", axum::routing::post(post_remote_chat))
            .route("/api/remote/agents", axum::routing::get(get_remote_agents))
            .route("/api/remote/workspaces", axum::routing::get(get_remote_workspaces))
            .route("/api/remote/new", axum::routing::post(post_remote_new))
            .route("/api/remote/pending", axum::routing::get(get_remote_pending))
            .route("/api/remote/respond", axum::routing::post(post_remote_respond))
            .route(
                "/api/remote/approve",
                axum::routing::post(post_remote_approve),
            )
            .route("/api/remote/send", axum::routing::post(post_remote_send))
            .route(
                "/api/remote/cron_trigger",
                axum::routing::post(post_remote_cron_trigger),
            )
            .route("/health", axum::routing::get(handle_health))
            // Gate /v1,/agent,/session to loopback-or-token so enabling LAN
            // remote access can't expose the raw model gateway (P1 fix).
            .layer(axum::middleware::from_fn_with_state(
                state.clone(),
                guard_gateway_access,
            ))
            .layer(cors_layer)
            .layer(axum::extract::DefaultBodyLimit::max(10 * 1024 * 1024)) // 10MB request body limit
            .with_state(state);

        println!("Starting OMNIX Workbench HTTP Proxy on {}", addr);

        tauri::async_runtime::handle().spawn(async move {
            let listener = match tokio::net::TcpListener::bind(addr).await {
                Ok(l) => l,
                Err(e) => {
                    log::warn!(
                        "OMNIX Proxy: Failed to bind to {}: {}. Port may be in use.",
                        addr,
                        e
                    );
                    return;
                }
            };
            if let Err(e) = axum::serve(
                listener,
                app.into_make_service_with_connect_info::<SocketAddr>(),
            )
                .with_graceful_shutdown(async move {
                    let _ = rx.await;
                    println!("OMNIX Workbench HTTP Proxy shutting down gracefully...");
                })
                .await
            {
                log::warn!("OMNIX Proxy: Server error: {}", e);
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
    handle_messages_impl(state, Some(agent_name_decoded), None, headers, payload).await
}

async fn handle_messages(
    State(state): State<Arc<ProxyState>>,
    headers: axum::http::HeaderMap,
    Json(payload): Json<AnthropicRequest>,
) -> impl IntoResponse {
    handle_messages_impl(state, None, None, headers, payload).await
}

async fn handle_messages_for_session(
    State(state): State<Arc<ProxyState>>,
    axum::extract::Path(session_key): axum::extract::Path<String>,
    headers: axum::http::HeaderMap,
    Json(payload): Json<AnthropicRequest>,
) -> Response {
    handle_messages_impl(state, None, Some(session_key), headers, payload).await
}

/// 正式池网关直调 (#3 技能池): append matched official-pool skills to the
/// request's system prompt. Every agent that talks through the gateway gets the
/// same approved skills with zero per-tool distribution. Pending-pool skills
/// are never injected. Disable via setting `skill_gateway_injection = "0"`.
fn inject_official_skills(db: &DbManager, payload: &mut AnthropicRequest) {
    let enabled = db
        .get_setting("skill_gateway_injection")
        .unwrap_or(None)
        .map(|v| v != "0")
        .unwrap_or(true);
    if !enabled {
        return;
    }
    let Some(user_text) = payload
        .messages
        .iter()
        .rev()
        .find(|m| m.role == "user")
        .map(|m| m.content.to_string_content())
    else {
        return;
    };
    let mut matches = crate::skill_library::match_skills_for_message(db, &user_text, true);
    matches.truncate(2); // strongest few, never a skill dump
    if matches.is_empty() {
        return;
    }
    let injection = crate::skill_library::build_skill_injection(&matches, db);
    if injection.is_empty() {
        return;
    }
    match payload.system.as_mut() {
        Some(AnthropicMessageContent::String(s)) => s.push_str(&injection),
        Some(AnthropicMessageContent::Blocks(blocks)) => blocks.push(AnthropicContentBlock {
            block_type: "text".to_string(),
            text: Some(injection),
            ..Default::default()
        }),
        None => payload.system = Some(AnthropicMessageContent::String(injection)),
    }
    // Compound-interest tracking: injected == used.
    if let Ok(conn) = db.get_connection() {
        for m in &matches {
            let _ = conn.execute(
                "UPDATE skills SET usage_count = usage_count + 1, last_used_at = CURRENT_TIMESTAMP WHERE name = ?1",
                params![m.skill_name],
            );
        }
    }
}

/// 记忆自动召回注入：取最近一条用户消息，词法匹配相关历史经验/教训，追加到
/// system。默认关（`memory_gateway_recall`）、最多 3 条、无命中不注——与技能注入
/// 同一套克制策略。借鉴 jcode 的「相关记忆自动浮现」，用 OMNIX 已有的记忆库实现。
fn inject_recalled_memory(db: &DbManager, payload: &mut AnthropicRequest) {
    let Some(user_text) = payload
        .messages
        .iter()
        .rev()
        .find(|m| m.role == "user")
        .map(|m| m.content.to_string_content())
    else {
        return;
    };
    let injection = crate::memory_recall::recall_injection(db, &user_text);
    if injection.is_empty() {
        return;
    }
    match payload.system.as_mut() {
        Some(AnthropicMessageContent::String(s)) => s.push_str(&injection),
        Some(AnthropicMessageContent::Blocks(blocks)) => blocks.push(AnthropicContentBlock {
            block_type: "text".to_string(),
            text: Some(injection),
            ..Default::default()
        }),
        None => payload.system = Some(AnthropicMessageContent::String(injection)),
    }
}

async fn handle_messages_impl(
    state: Arc<ProxyState>,
    agent_name_opt: Option<String>,
    session_key: Option<String>,
    headers: axum::http::HeaderMap,
    mut payload: AnthropicRequest,
) -> Response {
    // Q2′ 事后审计：请求历史里带着 agent 上一轮**真实调用过**的工具，记下来。
    // 放在最前面，因为下面的分支会把 payload 改成各家上游的形状。
    // 纯观察：不改请求、不拦截、出错咽掉——审计绝不能影响正在转发的请求。
    {
        let uses = crate::action_audit::extract_tool_uses(&payload);
        if !uses.is_empty() {
            let who = agent_name_opt.as_deref().unwrap_or("（未指定 agent）");
            crate::action_audit::record(&state.db, who, &uses);
        }
    }

    // Concurrency limiting
    let _permit = match state.concurrency_semaphore.try_acquire() {
        Ok(p) => p,
        Err(_) => {
            return (
                StatusCode::TOO_MANY_REQUESTS,
                "Too many concurrent requests. Please retry later.",
            )
                .into_response();
        }
    };

    let start_time = std::time::Instant::now();

    // Preserve agent_name before consuming agent_name_opt
    let agent_name_for_routing = agent_name_opt
        .clone()
        .unwrap_or_else(|| "Claude Code".to_string());

    let session_upstream = match session_key.as_deref() {
        Some(key) => match resolve_session_model_upstream(&state.db, key) {
            Ok(upstream) => Some(upstream),
            Err(error) => {
                return (StatusCode::BAD_REQUEST, error).into_response();
            }
        },
        None => None,
    };
    let mut key_health = session_upstream.as_ref().map(|upstream| KeyHealthContext {
        db: Arc::clone(&state.db),
        key_ids: upstream.key_ids.clone(),
        platform_id: Some(upstream.platform_id.clone()),
    });

    let target_account_id = headers
        .get("x-omnix-account-id")
        .and_then(|v| v.to_str().ok().map(|s| s.to_string()));

    let active_acc = if session_upstream.is_some() {
        None
    } else if let Some(ref acc_id) = target_account_id {
        state.db.get_account_by_id(acc_id).unwrap_or(None)
    } else {
        let agent_name = agent_name_opt.unwrap_or_else(|| "Claude Code".to_string());
        state
            .db
            .get_active_account_for_agent(&agent_name)
            .unwrap_or(None)
    };

    // 正式池技能注入 + 记忆自动召回——在进入两条上游分支前统一改写 system。
    inject_official_skills(&state.db, &mut payload);
    inject_recalled_memory(&state.db, &mut payload);

    let target_model_name = if let Some(ref upstream) = session_upstream {
        upstream.model_name.clone()
    } else if let Some(ref acc) = active_acc {
        acc.target_model.clone()
    } else {
        state
            .db
            .get_setting("target_model")
            .unwrap_or(None)
            .unwrap_or_else(|| "deepseek-chat".to_string())
    };

    let mut resolved_model = target_model_name.clone();
    if resolved_model == "Auto" {
        let (need_vis, need_reas, need_cod, need_spd) = classify_anthropic_capabilities(&payload);
        println!("OMNIX Router: Classification result -> Need Vision: {}, Reasoning: {}, Coding: {}, Speedy: {}", need_vis, need_reas, need_cod, need_spd);

        if let Ok(active_models) = state.db.get_connection().and_then(|conn| {
            let mut stmt = conn.prepare(
                "SELECT pm.model_name, pm.platform_id, pm.has_vision, pm.has_reasoning, pm.has_coding, mp.api_key, mp.api_address, mp.api_type, pm.has_speedy
                 FROM platform_models pm
                 JOIN model_platforms mp ON pm.platform_id = mp.id
                 WHERE pm.is_enabled = 1 AND mp.is_enabled = 1
                   AND (mp.is_healthy = 1 OR mp.circuit_opened_at <= datetime('now', '-60 seconds'))"
            )?;
            let rows = stmt.query_map([], |row| {
                let has_vis: i32 = row.get(2)?;
                let has_reas: i32 = row.get(3)?;
                let has_cod: i32 = row.get(4)?;
                // has_speedy is column index 8 (guaranteed present by schema + migration).
                let has_spd: bool = row.get::<_, i32>(8).unwrap_or(0) != 0;
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    has_vis != 0,
                    has_reas != 0,
                    has_cod != 0,
                    has_spd,
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
            for (model_name, platform_id, vis, reas, cod, spd, api_key, _api_address, api_type) in active_models {
                if api_key.trim().is_empty() && api_type != "ollama" {
                    continue;
                }
                let mut score = 0;
                if need_vis && vis { score += 10; }
                if need_reas && reas { score += 10; }
                if need_cod && cod { score += 5; }
                if need_spd && spd { score += 8; }
                if !need_vis && !need_reas && !need_cod && !need_spd && vis { score -= 2; }

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

    let (api_host, api_type, actual_model_name, keys, circuit_platform_id, upstream_is_oauth) = if let Some(upstream) = session_upstream {
        (
            upstream.api_address,
            upstream.api_type,
            upstream.model_name,
            upstream.keys,
            Some(upstream.platform_id),
            upstream.is_oauth,
        )
    } else {
        match resolve_model_upstream_for_agent(
            &state.db,
            &resolved_model,
            Some(&agent_name_for_routing),
        ) {
            Ok((api_key_raw, api_host, api_type, actual_model_name, platform_id)) => (
                api_host,
                api_type,
                actual_model_name,
                api_key_raw
                    .split(',')
                    .map(str::trim)
                    .filter(|key| !key.is_empty())
                    .map(str::to_string)
                    .collect(),
                Some(platform_id),
                false,
            ),
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to resolve model upstream: {}", e),
                )
                    .into_response();
            }
        }
    };
    // On the agent-routing path key_health was None (built only for sessions);
    // backfill it so circuit outcomes are recorded against the resolved platform.
    if key_health.is_none() {
        if let Some(platform_id) = circuit_platform_id.clone() {
            key_health = Some(KeyHealthContext {
                db: Arc::clone(&state.db),
                key_ids: Vec::new(),
                platform_id: Some(platform_id),
            });
        }
    }

    if keys.is_empty() && api_type != "ollama" {
        return (
            StatusCode::UNAUTHORIZED,
            "API Key is not configured for this model platform.",
        )
            .into_response();
    }
    let request_model = payload.model.clone();

    if api_type == "anthropic" {
        let mut native_req = payload;
        native_req.model = actual_model_name;

        let upstream_url = join_url(&api_host, "/v1/messages");
        let is_stream = native_req.stream.unwrap_or(false);

        println!(
            "OMNIX Proxy (Anthropic Route Native): Forwarding to {} (stream={})",
            upstream_url, is_stream
        );

        let mut req_builder = state
            .http_client
            .post(&upstream_url)
            .header("Content-Type", "application/json")
            .header("anthropic-version", "2023-06-01")
            .json(&native_req);
        // OAuth subscription tokens authenticate with Bearer + the OAuth beta,
        // not x-api-key (F1 active-account override; header live-verify pending).
        if upstream_is_oauth {
            req_builder = req_builder.header("anthropic-beta", "oauth-2025-04-20");
        }

        let upstream_res = match send_with_key_failover(
            req_builder,
            &keys,
            if upstream_is_oauth { ApiKeyHeader::Bearer } else { ApiKeyHeader::Anthropic },
            key_health.clone(),
        )
        .await
        {
            Ok(res) => res,
            Err(e) => return (StatusCode::BAD_GATEWAY, e).into_response(),
        };

        let status = upstream_res.status();
        if !status.is_success() {
            let err_body = upstream_res.text().await.unwrap_or_default();
            return (status, err_body).into_response();
        }

        // Log request (non-blocking, uses spawn_blocking for sync DB I/O)
        let log_db = state.db.clone();
        let log_model = resolved_model.clone();
        let log_latency = start_time.elapsed().as_millis() as i64;
        let log_status = status.as_u16() as i32;
        let log_is_err = !status.is_success();
        let evt_db = state.db.clone();
        tokio::task::spawn_blocking(move || {
            log_request(
                &log_db,
                &log_model,
                Some("anthropic"),
                0,
                0,
                log_latency,
                log_status,
                is_stream,
                log_is_err,
                None,
                None,
                "proxy",
            );
            // Emit message_sent event for event bus
            crate::event_bus::emit_event(&evt_db, crate::event_bus::EventType::MessageSent);
        });

        if is_stream {
            let stream = upstream_res
                .bytes_stream()
                .map(|r| r.map_err(|e| axum::Error::new(e)));
            Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "text/event-stream")
                .header("Cache-Control", "no-cache")
                .header("Connection", "keep-alive")
                .body(Body::from_stream(stream))
                .unwrap_or_else(|_| {
                    Response::builder()
                        .status(StatusCode::INTERNAL_SERVER_ERROR)
                        .body(Body::empty())
                        .unwrap()
                })
        } else {
            let bytes = match upstream_res.bytes().await {
                Ok(b) => b,
                Err(e) => {
                    return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
                }
            };
            Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "application/json")
                .body(Body::from(bytes))
                .unwrap_or_else(|_| {
                    Response::builder()
                        .status(StatusCode::INTERNAL_SERVER_ERROR)
                        .body(Body::empty())
                        .unwrap()
                })
        }
    } else {
        // Translate to OpenAI format (For OpenAI or Ollama). Content becomes a
        // parts array only when image blocks are present, so text-only flows
        // keep their old plain-string shape.
        let mut messages = Vec::new();
        if let Some(sys_prompt) = payload.system {
            messages.push(serde_json::json!({
                "role": "system",
                "content": sys_prompt.to_string_content(),
            }));
        }
        for msg in payload.messages {
            messages.push(serde_json::json!({
                "role": msg.role,
                "content": msg.content.to_openai_content(),
            }));
        }

        // 已知缺口：这条 Anthropic→OpenAI 翻译路径还不搬运工具。
        //
        // 不做「只翻译请求不翻译响应」的半吊子版本——那样模型会返回 tool_calls，
        // 而我们没有反向翻译，agent 收到的是一堆看不懂的东西，比它压根不知道有
        // 工具更糟。要做就得连响应（含流式 SSE）一起做。
        //
        // 在此之前至少让它可诊断：静默丢能力最难查。
        if payload.extra.contains_key("tools") {
            log::warn!(
                "网关：请求声明了工具，但上游是 {api_type} 类型，当前的 Anthropic→OpenAI \
                 翻译尚未搬运工具定义，本次请求的工具会失效。改用 anthropic 类型的上游可避免。"
            );
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
        println!(
            "OMNIX Proxy (Claude Route to OpenAI): Forwarding to {} (stream={})",
            upstream_url, is_stream
        );

        let req_builder = state
            .http_client
            .post(&upstream_url)
            .header("Content-Type", "application/json")
            .json(&openai_req);

        let upstream_res = match send_with_key_failover(
            req_builder,
            &keys,
            ApiKeyHeader::Bearer,
            key_health.clone(),
        )
        .await
        {
            Ok(res) => res,
            Err(e) => return (StatusCode::BAD_GATEWAY, e).into_response(),
        };

        let status = upstream_res.status();
        if !status.is_success() {
            let err_body = upstream_res.text().await.unwrap_or_default();
            return (status, err_body).into_response();
        }

        if is_stream {
            let stream = upstream_res.bytes_stream();
            let mut buffer_bytes = Vec::new();

            let anthropic_stream = stream.map(move |result| match result {
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
                                output_bytes.extend_from_slice(
                                    b"event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n",
                                );
                                break;
                            }
                            if let Ok(chunk_json) =
                                serde_json::from_str::<OpenAIStreamChunk>(data_content)
                            {
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
                                        let formatted_line = format!(
                                            "event: content_block_delta\ndata: {}\n\n",
                                            anthropic_event.to_string()
                                        );
                                        output_bytes.extend_from_slice(formatted_line.as_bytes());
                                    }
                                }
                            }
                        }
                    }
                    Ok::<_, axum::Error>(axum::body::Bytes::from(output_bytes))
                }
                Err(e) => Err(axum::Error::new(e)),
            });

            Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "text/event-stream")
                .header("Cache-Control", "no-cache")
                .header("Connection", "keep-alive")
                .body(Body::from_stream(anthropic_stream))
                .unwrap_or_else(|_| {
                    Response::builder()
                        .status(StatusCode::INTERNAL_SERVER_ERROR)
                        .body(Body::empty())
                        .unwrap()
                })
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
                Err(e) => {
                    return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
                }
            };

            // Log request (non-blocking, uses spawn_blocking for sync DB I/O)
            let log_db = state.db.clone();
            let log_model = resolved_model.clone();
            let log_latency = start_time.elapsed().as_millis() as i64;
            tokio::task::spawn_blocking(move || {
                log_request(
                    &log_db,
                    &log_model,
                    Some("openai"),
                    0,
                    0,
                    log_latency,
                    200,
                    false,
                    false,
                    None,
                    None,
                    "proxy",
                );
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
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to parse OpenAI response.",
            )
                .into_response()
        }
    }
}

#[derive(Clone, Copy)]
enum ApiKeyHeader {
    Anthropic,
    Bearer,
}

#[derive(Clone)]
struct KeyHealthContext {
    db: Arc<DbManager>,
    key_ids: Vec<Option<String>>,
    /// Platform behind this request, for per-platform circuit breaking. `None`
    /// when the upstream isn't an OMNIX-managed platform (e.g. bare relay).
    platform_id: Option<String>,
}

fn record_key_health(
    context: &KeyHealthContext,
    index: usize,
    status: &str,
    error: Option<&str>,
    latency_ms: i64,
) {
    let Some(Some(key_id)) = context.key_ids.get(index) else {
        return;
    };
    if let Ok(conn) = context.db.get_connection() {
        let _ = conn.execute(
            "UPDATE platform_api_keys
             SET last_status = ?1, last_error = ?2, latency_ms = ?3,
                 last_checked_at = datetime('now')
             WHERE id = ?4",
            params![status, error, latency_ms, key_id],
        );
    }
}

/// Feed a request's final outcome into the platform circuit breaker: a 2xx
/// closes/keeps the circuit healthy, a 5xx or network error trips it toward
/// open. 4xx (auth/rate/bad-request) is a key/client issue — left neutral so a
/// bad key never marks the whole platform down. Called once per request.
fn record_circuit_outcome(context: &KeyHealthContext, status: Option<reqwest::StatusCode>, error: Option<&str>) {
    let Some(platform_id) = context.platform_id.as_deref() else {
        return;
    };
    match status {
        Some(code) if code.is_success() => {
            crate::circuit_breaker::record_success(&context.db, platform_id);
        }
        Some(code) if code.is_server_error() => {
            crate::circuit_breaker::record_failure(
                &context.db,
                platform_id,
                &format!("HTTP {code}"),
            );
        }
        Some(_) => {} // 4xx: not a platform-health signal.
        None => {
            crate::circuit_breaker::record_failure(
                &context.db,
                platform_id,
                error.unwrap_or("upstream network error"),
            );
        }
    }
}

async fn send_with_key_failover(
    request: reqwest::RequestBuilder,
    keys: &[String],
    header: ApiKeyHeader,
    health: Option<KeyHealthContext>,
) -> Result<reqwest::Response, String> {
    let attempts: Vec<Option<&str>> = if keys.is_empty() {
        vec![None]
    } else {
        keys.iter().map(|key| Some(key.as_str())).collect()
    };
    let mut last_error = None;
    for (index, key) in attempts.iter().enumerate() {
        let started_at = std::time::Instant::now();
        let mut attempt = request
            .try_clone()
            .ok_or_else(|| "Unable to clone upstream request for key failover".to_string())?;
        if let Some(key) = key {
            attempt = match header {
                ApiKeyHeader::Anthropic => attempt.header("x-api-key", *key),
                ApiKeyHeader::Bearer => attempt.header("Authorization", format!("Bearer {key}")),
            };
        }
        match attempt.send().await {
            Ok(response) => {
                let status = response.status();
                let latency_ms = started_at.elapsed().as_millis() as i64;
                let can_retry = matches!(
                    status.as_u16(),
                    401 | 403 | 408 | 409 | 425 | 429 | 500 | 502 | 503 | 504
                );
                if let Some(context) = health.as_ref() {
                    if status.is_success() {
                        record_key_health(context, index, "success", None, latency_ms);
                    } else {
                        let message = format!("HTTP {status}");
                        record_key_health(context, index, "error", Some(&message), latency_ms);
                    }
                }
                if can_retry && index + 1 < attempts.len() {
                    last_error = Some(format!("upstream returned {status}"));
                    continue;
                }
                if let Some(context) = health.as_ref() {
                    record_circuit_outcome(context, Some(status), None);
                }
                return Ok(response);
            }
            Err(error) if index + 1 < attempts.len() => {
                if let Some(context) = health.as_ref() {
                    record_key_health(
                        context,
                        index,
                        "error",
                        Some(&error.to_string()),
                        started_at.elapsed().as_millis() as i64,
                    );
                }
                last_error = Some(format!("upstream network error: {error}"));
            }
            Err(error) => {
                if let Some(context) = health.as_ref() {
                    record_key_health(
                        context,
                        index,
                        "error",
                        Some(&error.to_string()),
                        started_at.elapsed().as_millis() as i64,
                    );
                    record_circuit_outcome(context, None, Some(&error.to_string()));
                }
                return Err(format!("Upstream request failed: {error}"));
            }
        }
    }
    // Every attempt exhausted without a returnable response (all keys failed).
    if let Some(context) = health.as_ref() {
        record_circuit_outcome(context, None, last_error.as_deref());
    }
    Err(last_error.unwrap_or_else(|| "No upstream API key attempt was made".into()))
}

async fn handle_responses_for_session(
    State(state): State<Arc<ProxyState>>,
    axum::extract::Path(session_key): axum::extract::Path<String>,
    Json(mut payload): Json<Value>,
) -> Response {
    let _permit = match state.concurrency_semaphore.try_acquire() {
        Ok(permit) => permit,
        Err(_) => {
            return (
                StatusCode::TOO_MANY_REQUESTS,
                "Too many concurrent requests. Please retry later.",
            )
                .into_response();
        }
    };
    let upstream = match resolve_session_model_upstream(&state.db, &session_key) {
        Ok(upstream) => upstream,
        Err(error) => return (StatusCode::BAD_REQUEST, error).into_response(),
    };
    let health = KeyHealthContext {
        db: Arc::clone(&state.db),
        key_ids: upstream.key_ids.clone(),
        platform_id: Some(upstream.platform_id.clone()),
    };

    if upstream.api_type == "openai-response" {
        // Provider speaks the Responses API natively: forward verbatim.
        if let Some(object) = payload.as_object_mut() {
            object.insert("model".into(), Value::String(upstream.model_name.clone()));
        }
        let upstream_url = join_url(&upstream.api_address, "/responses");
        let request = state
            .http_client
            .post(upstream_url)
            .header("Content-Type", "application/json")
            .json(&payload);
        let response = match send_with_key_failover(
            request,
            &upstream.keys,
            ApiKeyHeader::Bearer,
            Some(health),
        )
        .await
        {
            Ok(response) => response,
            Err(error) => return (StatusCode::BAD_GATEWAY, error).into_response(),
        };
        return forward_event_stream(response);
    }

    // Provider only speaks Chat Completions: translate Responses <-> Chat so
    // Codex can use any model the user configured (DeepSeek, Volcano, etc.).
    let chat_body =
        crate::responses_bridge::responses_request_to_chat(&payload, &upstream.model_name);
    let upstream_url = join_url(&upstream.api_address, "/chat/completions");
    let request = state
        .http_client
        .post(upstream_url)
        .header("Content-Type", "application/json")
        .json(&chat_body);
    let response =
        match send_with_key_failover(request, &upstream.keys, ApiKeyHeader::Bearer, Some(health))
            .await
        {
            Ok(response) => response,
            Err(error) => return (StatusCode::BAD_GATEWAY, error).into_response(),
        };
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return (status, body).into_response();
    }
    let response_id = format!("resp_{}", chrono::Utc::now().timestamp_micros());
    let is_sse = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(|content_type| content_type.contains("event-stream"))
        .unwrap_or(false);
    if is_sse {
        translated_responses_stream(response, response_id)
    } else {
        // Upstream ignored `stream`: translate the whole completion at once.
        let completion = response
            .json::<Value>()
            .await
            .unwrap_or_else(|_| serde_json::json!({}));
        let mut translator =
            crate::responses_bridge::ResponsesStreamTranslator::new(response_id);
        let body = translator.translate_full(&completion).concat();
        Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "text/event-stream")
            .body(Body::from(body))
            .unwrap_or_else(|_| {
                Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(Body::empty())
                    .expect("static response")
            })
    }
}

/// Forward an upstream SSE response stream verbatim (native Responses provider).
fn forward_event_stream(response: reqwest::Response) -> Response {
    let status = response.status();
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("text/event-stream")
        .to_string();
    let stream = response
        .bytes_stream()
        .map(|item| item.map_err(axum::Error::new));
    Response::builder()
        .status(status)
        .header("Content-Type", content_type)
        .body(Body::from_stream(stream))
        .unwrap_or_else(|_| {
            Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::empty())
                .expect("static response")
        })
}

/// Translate an upstream Chat Completions SSE stream into Responses SSE events.
fn translated_responses_stream(response: reqwest::Response, response_id: String) -> Response {
    let status = response.status();
    let upstream = response.bytes_stream().boxed();
    let translator = crate::responses_bridge::ResponsesStreamTranslator::new(response_id);
    let init = (
        upstream,
        String::new(),
        translator,
        std::collections::VecDeque::<String>::new(),
        false, // upstream_done
        false, // finished
    );
    let stream = futures::stream::unfold(
        init,
        |(mut upstream, mut buf, mut translator, mut queue, mut upstream_done, mut finished)| async move {
            loop {
                if let Some(chunk) = queue.pop_front() {
                    return Some((
                        Ok::<_, std::io::Error>(chunk),
                        (upstream, buf, translator, queue, upstream_done, finished),
                    ));
                }
                if finished {
                    return None;
                }
                if upstream_done {
                    for event in translator.finish() {
                        queue.push_back(event);
                    }
                    finished = true;
                    continue;
                }
                match upstream.next().await {
                    Some(Ok(bytes)) => {
                        buf.push_str(&String::from_utf8_lossy(&bytes));
                        drain_chat_sse_lines(&mut buf, &mut translator, &mut queue);
                    }
                    Some(Err(_)) | None => {
                        upstream_done = true;
                    }
                }
            }
        },
    );
    Response::builder()
        .status(status)
        .header("Content-Type", "text/event-stream")
        .header("Cache-Control", "no-cache")
        .body(Body::from_stream(stream))
        .unwrap_or_else(|_| {
            Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::empty())
                .expect("static response")
        })
}

/// Pull complete `data:` SSE lines out of the buffer and feed parsed Chat
/// Completions chunks to the translator, queueing the emitted Responses events.
fn drain_chat_sse_lines(
    buf: &mut String,
    translator: &mut crate::responses_bridge::ResponsesStreamTranslator,
    queue: &mut std::collections::VecDeque<String>,
) {
    while let Some(pos) = buf.find('\n') {
        let line = buf[..pos].trim_end_matches('\r').to_string();
        buf.drain(..=pos);
        let trimmed = line.trim();
        let Some(rest) = trimmed.strip_prefix("data:") else {
            continue;
        };
        let data = rest.trim();
        if data.is_empty() || data == "[DONE]" {
            continue;
        }
        if let Ok(chunk) = serde_json::from_str::<Value>(data) {
            for event in translator.push_chunk(&chunk) {
                queue.push_back(event);
            }
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
    // Concurrency limiting
    let _permit = match state.concurrency_semaphore.try_acquire() {
        Ok(p) => p,
        Err(_) => {
            return (
                StatusCode::TOO_MANY_REQUESTS,
                "Too many concurrent requests. Please retry later.",
            )
                .into_response();
        }
    };

    let agent_name_for_routing = agent_name_opt
        .clone()
        .unwrap_or_else(|| "Codex".to_string());

    let start_time = std::time::Instant::now();
    let mut payload = payload;
    let target_account_id = headers
        .get("x-omnix-account-id")
        .and_then(|v| v.to_str().ok().map(|s| s.to_string()));

    let active_acc = if let Some(ref acc_id) = target_account_id {
        state.db.get_account_by_id(acc_id).unwrap_or(None)
    } else {
        let agent_name = agent_name_opt.unwrap_or_else(|| "Codex".to_string());
        state
            .db
            .get_active_account_for_agent(&agent_name)
            .unwrap_or(None)
    };

    let target_model_name = if let Some(ref acc) = active_acc {
        acc.target_model.clone()
    } else {
        state
            .db
            .get_setting("target_model")
            .unwrap_or(None)
            .unwrap_or_else(|| "deepseek-chat".to_string())
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
                "SELECT pm.model_name, pm.platform_id, pm.has_vision, pm.has_reasoning, pm.has_coding, mp.api_key, mp.api_address, mp.api_type, pm.has_speedy
                 FROM platform_models pm
                 JOIN model_platforms mp ON pm.platform_id = mp.id
                 WHERE pm.is_enabled = 1 AND mp.is_enabled = 1
                   AND (mp.is_healthy = 1 OR mp.circuit_opened_at <= datetime('now', '-60 seconds'))"
            )?;
            let rows = stmt.query_map([], |row| {
                let has_vis: i32 = row.get(2)?;
                let has_reas: i32 = row.get(3)?;
                let has_cod: i32 = row.get(4)?;
                // has_speedy is column index 8 (guaranteed present by schema + migration).
                let has_spd: bool = row.get::<_, i32>(8).unwrap_or(0) != 0;
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    has_vis != 0,
                    has_reas != 0,
                    has_cod != 0,
                    has_spd,
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
            for (model_name, platform_id, vis, reas, cod, spd, api_key, _api_address, api_type) in active_models {
                if api_key.trim().is_empty() && api_type != "ollama" {
                    continue;
                }
                let mut score = 0;
                if need_vis && vis { score += 10; }
                if need_reas && reas { score += 10; }
                if need_cod && cod { score += 5; }
                if need_spd && spd { score += 8; }
                if !need_vis && !need_reas && !need_cod && !need_spd && vis { score -= 2; }

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

    let (api_key_raw, api_host, api_type, actual_model_name, circuit_platform_id) =
        match resolve_model_upstream_for_agent(
            &state.db,
            &resolved_model,
            Some(&agent_name_for_routing),
        ) {
            Ok(res) => res,
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to resolve model upstream: {}", e),
                )
                    .into_response();
            }
        };

    let keys: Vec<&str> = api_key_raw
        .split(',')
        .map(|k| k.trim())
        .filter(|k| !k.is_empty())
        .collect();
    if keys.is_empty() && api_type != "ollama" {
        return (
            StatusCode::UNAUTHORIZED,
            "API Key is not configured for this model platform.",
        )
            .into_response();
    }
    let api_key = if keys.is_empty() {
        ""
    } else {
        keys[state.request_counter.fetch_add(1, Ordering::Relaxed) % keys.len()]
    };

    if api_type != "anthropic" {
        if let Some(payload_obj) = payload.as_object_mut() {
            payload_obj.insert(
                "model".to_string(),
                serde_json::Value::String(actual_model_name.clone()),
            );
        }

        let upstream_url = if api_type == "ollama" {
            join_url(&api_host, "/v1/chat/completions")
        } else {
            join_url(&api_host, "/chat/completions")
        };

        let is_stream = headers
            .get("accept")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.contains("text/event-stream"))
            .unwrap_or(false)
            || payload
                .get("stream")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

        println!(
            "OMNIX Proxy (OpenAI Route): Forwarding request to {} (stream={})",
            upstream_url, is_stream
        );

        // ── Prompt Injection Guard ──
        if let Some(msgs) = payload.get("messages").and_then(|m| m.as_array()) {
            if let Some(last_msg) = msgs.last() {
                if last_msg.get("role").and_then(|r| r.as_str()) == Some("user") {
                    if let Some(content) = last_msg.get("content").and_then(|c| c.as_str()) {
                        let (wrapped, scan_result) =
                            crate::prompt_guard::scan_and_wrap(content, "user_message");
                        if scan_result.risk_score > 0.7 {
                            log::warn!("[omnix::proxy] High injection risk ({:.0}%) in user message — {} pattern(s): {:?}",
                                scan_result.risk_score * 100.0,
                                scan_result.detected_patterns.len(),
                                scan_result.detected_patterns
                            );
                        }
                        if wrapped != content {
                            if let Some(obj) = payload.as_object_mut() {
                                if let Some(msgs_arr) =
                                    obj.get_mut("messages").and_then(|m| m.as_array_mut())
                                {
                                    if let Some(last) = msgs_arr.last_mut() {
                                        if let Some(last_obj) = last.as_object_mut() {
                                            last_obj.insert(
                                                "content".to_string(),
                                                serde_json::Value::String(wrapped),
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        let mut req_builder = state
            .http_client
            .post(&upstream_url)
            .header("Content-Type", "application/json")
            .json(&payload);

        if !api_key.is_empty() {
            req_builder = req_builder.header("Authorization", format!("Bearer {}", api_key));
        }

        let upstream_res = match req_builder.send().await {
            Ok(res) => {
                println!(
                    "OMNIX Proxy (OpenAI Route): Upstream returned status {}",
                    res.status()
                );
                res
            }
            Err(e) => {
                log::warn!("OMNIX Proxy (OpenAI Route): Upstream request failed: {}", e);
                crate::circuit_breaker::record_failure(&state.db, &circuit_platform_id, &e.to_string());
                return (
                    StatusCode::BAD_GATEWAY,
                    format!("Failed to connect to upstream LLM API: {}", e),
                )
                    .into_response();
            }
        };

        let status = upstream_res.status();
        // Feed the platform circuit breaker: 2xx heals, 5xx trips; 4xx is neutral.
        if status.is_success() {
            crate::circuit_breaker::record_success(&state.db, &circuit_platform_id);
        } else if status.is_server_error() {
            crate::circuit_breaker::record_failure(&state.db, &circuit_platform_id, &format!("HTTP {status}"));
        }
        if !status.is_success() {
            let err_body = upstream_res.text().await.unwrap_or_default();
            log::warn!(
                "OMNIX Proxy (OpenAI Route): Upstream non-success payload (status {}): {}",
                status,
                err_body
            );
            return (status, err_body).into_response();
        }

        // Log request (non-blocking, uses spawn_blocking for sync DB I/O)
        let log_db = state.db.clone();
        let log_model = resolved_model.clone();
        let log_latency = start_time.elapsed().as_millis() as i64;
        let log_status = status.as_u16() as i32;
        let log_is_err = !status.is_success();
        tokio::task::spawn_blocking(move || {
            log_request(
                &log_db,
                &log_model,
                Some("openai"),
                0,
                0,
                log_latency,
                log_status,
                is_stream,
                log_is_err,
                None,
                None,
                "proxy",
            );
        });

        if is_stream {
            let stream = upstream_res
                .bytes_stream()
                .map(|r| r.map_err(|e| axum::Error::new(e)));
            Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "text/event-stream")
                .header("Cache-Control", "no-cache")
                .header("Connection", "keep-alive")
                .body(Body::from_stream(stream))
                .unwrap_or_else(|_| {
                    Response::builder()
                        .status(StatusCode::INTERNAL_SERVER_ERROR)
                        .body(Body::empty())
                        .unwrap()
                })
                .into_response()
        } else {
            let bytes = match upstream_res.bytes().await {
                Ok(b) => b,
                Err(e) => {
                    return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
                }
            };
            Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "application/json")
                .body(Body::from(bytes))
                .unwrap_or_else(|_| {
                    Response::builder()
                        .status(StatusCode::INTERNAL_SERVER_ERROR)
                        .body(Body::empty())
                        .unwrap()
                })
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
            Err(e) => {
                return (
                    StatusCode::BAD_REQUEST,
                    format!("Invalid OpenAI request: {}", e),
                )
                    .into_response()
            }
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
            ..Default::default()
        };

        let upstream_url = join_url(&api_host, "/v1/messages");
        let is_stream = native_req.stream.unwrap_or(false);

        println!(
            "OMNIX Proxy (OpenAI to Anthropic Route): Forwarding request to {} (stream={})",
            upstream_url, is_stream
        );

        let mut req_builder = state
            .http_client
            .post(&upstream_url)
            .header("Content-Type", "application/json")
            .header("anthropic-version", "2023-06-01")
            .json(&native_req);

        if !api_key.is_empty() {
            req_builder = req_builder.header("x-api-key", api_key);
        }

        let upstream_res = match req_builder.send().await {
            Ok(res) => res,
            Err(e) => {
                return (StatusCode::BAD_GATEWAY, format!("Upstream failed: {}", e)).into_response()
            }
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
                Err(e) => {
                    return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
                }
            };

            match serde_json::from_slice::<AnthropicResponse>(&res_bytes) {
                Ok(anthropic_res) => {
                    let text = anthropic_res
                        .content
                        .iter()
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
                Err(_) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Failed to parse Anthropic response",
                )
                    .into_response(),
            }
        } else {
            let stream = upstream_res.bytes_stream();
            let mut buffer_bytes = Vec::new();
            let model_for_chunk = resolved_model.clone();

            let openai_stream = stream.map(move |result| match result {
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
                                ContentBlockDelta { delta: AnthropicDelta },
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

                            if let Ok(event) =
                                serde_json::from_str::<AnthropicStreamEvent>(data_content)
                            {
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
                                        output_bytes.extend_from_slice(
                                            format!("data: {}\n\n", chunk.to_string()).as_bytes(),
                                        );
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
            });

            Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "text/event-stream")
                .header("Cache-Control", "no-cache")
                .header("Connection", "keep-alive")
                .body(Body::from_stream(openai_stream))
                .unwrap_or_else(|_| {
                    Response::builder()
                        .status(StatusCode::INTERNAL_SERVER_ERROR)
                        .body(Body::empty())
                        .unwrap()
                })
                .into_response()
        }
    }
}

// --- 4. Remote Dashboard API Handlers ---

static REMOTE_CLIENTS: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<String, i64>>> =
    std::sync::OnceLock::new();

#[derive(Debug, Clone, Serialize)]
pub struct RemoteClientInfo {
    pub ip: String,
    pub last_seen: i64,
}

fn record_remote_client(ip: String) {
    let map = REMOTE_CLIENTS.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()));
    if let Ok(mut m) = map.lock() {
        m.insert(ip, chrono::Utc::now().timestamp());
        if m.len() > 32 {
            if let Some(oldest) = m.iter().min_by_key(|(_, ts)| **ts).map(|(k, _)| k.clone()) {
                m.remove(&oldest);
            }
        }
    }
}

pub fn remote_clients_snapshot() -> Vec<RemoteClientInfo> {
    let map = REMOTE_CLIENTS.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()));
    map.lock()
        .map(|m| {
            let mut v: Vec<RemoteClientInfo> = m
                .iter()
                .map(|(ip, ts)| RemoteClientInfo { ip: ip.clone(), last_seen: *ts })
                .collect();
            v.sort_by(|a, b| b.last_seen.cmp(&a.last_seen));
            v
        })
        .unwrap_or_default()
}

/// Compare a presented credential against the expected one without leaking
/// length or position through timing. An empty `expected` never matches, so a
/// DB read failure can't turn into an open door.
fn token_matches(provided: &str, expected: &str) -> bool {
    if expected.is_empty() || provided.len() != expected.len() {
        return false;
    }
    let mut diff = 0u8;
    for (a, b) in provided.as_bytes().iter().zip(expected.as_bytes()) {
        diff |= a ^ b;
    }
    diff == 0
}

/// Extract a query-string parameter without pulling in a parser dependency.
/// Our tokens are plain `tok_<hex>` so no percent-decoding is needed.
fn query_param(query: Option<&str>, key: &str) -> Option<String> {
    for pair in query?.split('&') {
        let mut parts = pair.splitn(2, '=');
        if parts.next() == Some(key) {
            return Some(parts.next().unwrap_or("").to_string());
        }
    }
    None
}

/// Gate the raw model-gateway routes (`/v1/*`, `/agent/*`, `/session/*`) to the
/// local machine unless the caller presents the remote-access token. Local CLI
/// agents connect over loopback and are always allowed; when "手机远程访问"
/// binds the proxy to 0.0.0.0, a LAN client must send
/// `x-omnix-remote-token: <token>` or it gets 403 — otherwise anyone on the
/// same Wi-Fi could spend the user's API keys straight through /v1/messages.
///
/// The remote-panel surface (`/remote`, `/api/remote/*`) is ALSO enforced here
/// (token via `?token=` or the header, from any peer — matching the per-handler
/// checks), so a future panel route can't ship without auth. The per-handler
/// checks stay as defense in depth. Successful panel auths are recorded so the
/// desktop UI can list connected devices.
async fn guard_gateway_access(
    axum::extract::ConnectInfo(peer): axum::extract::ConnectInfo<SocketAddr>,
    State(state): State<Arc<ProxyState>>,
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    let path = req.uri().path();

    // Remote-panel surface: token required from every peer (query or header).
    let is_remote_surface = path == "/remote" || path.starts_with("/api/remote/");
    if is_remote_surface {
        let expected = state
            .db
            .get_setting("remote_token")
            .unwrap_or(None)
            .unwrap_or_default();
        // Header first: query strings leak into browser history, Referer and
        // screenshots. `?token=` stays as a fallback because the phone opens
        // the panel from a plain link/QR and can't set headers on that first
        // navigation — the served page then uses the header for its API calls.
        let provided = req
            .headers()
            .get("x-omnix-remote-token")
            .and_then(|v| v.to_str().ok())
            .map(|v| v.to_string())
            .or_else(|| query_param(req.uri().query(), "token"))
            .unwrap_or_default();
        if !token_matches(&provided, &expected) {
            return (StatusCode::FORBIDDEN, "远程访问需要有效令牌（在链接中携带 ?token=…）").into_response();
        }
        record_remote_client(peer.ip().to_string());
        return next.run(req).await;
    }

    // `/mcp` 必须在这一行里：它把技能库交给调用方，开了手机远程访问之后网关
    // 绑的是 0.0.0.0，漏掉它等于把整个技能库对局域网无鉴权敞开。
    let is_gateway = path.starts_with("/v1/")
        || path.starts_with("/agent/")
        || path.starts_with("/session/")
        || path == "/mcp";
    if is_gateway && !peer.ip().is_loopback() {
        // WSL agents reach the host over a non-loopback address and can't
        // easily carry the token, so WSL mode keeps the pre-existing local-dev
        // trust — but ONLY while 手机远程访问 is off. With both enabled the
        // listener is on 0.0.0.0 for the LAN, and a blanket WSL exemption would
        // hand every device on that network an unauthenticated model gateway.
        let use_wsl = state
            .db
            .get_setting("use_wsl")
            .unwrap_or(None)
            .as_deref()
            == Some("true");
        let remote_enabled = state
            .db
            .get_setting("remote_access_enabled")
            .unwrap_or(None)
            .as_deref()
            == Some("true");
        if !use_wsl || remote_enabled {
            let expected = state
                .db
                .get_setting("remote_token")
                .unwrap_or(None)
                .unwrap_or_default();
            let provided = req
                .headers()
                .get("x-omnix-remote-token")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("");
            if !token_matches(provided, &expected) {
                return (
                    StatusCode::FORBIDDEN,
                    "远程访问模型网关需要有效令牌 (x-omnix-remote-token)",
                )
                    .into_response();
            }
        }
    }
    next.run(req).await
}

fn classify_request_capabilities(messages: &[OpenAIRequestMessage]) -> (bool, bool, bool, bool) {
    let mut need_vision = false;
    let mut need_reasoning = false;
    let mut need_coding = false;

    for msg in messages {
        let content_lower = msg.content.to_lowercase();
        if content_lower.contains("data:image/")
            || content_lower.contains("[image]")
            || content_lower.contains("图片")
            || content_lower.contains("图像")
        {
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
        if content_lower.contains("image")
            || content_lower.contains("图片")
            || content_lower.contains("图像")
        {
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

    let total_len: usize = payload
        .messages
        .iter()
        .map(|m| m.content.to_string_content().len())
        .sum();
    let need_speedy = !need_reasoning && !need_vision && total_len < 300;

    (need_vision, need_reasoning, need_coding, need_speedy)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SessionUpstream {
    platform_id: String,
    model_name: String,
    api_address: String,
    api_type: String,
    keys: Vec<String>,
    key_ids: Vec<Option<String>>,
    /// True when `keys[0]` is an OAuth access token (Bearer + provider betas),
    /// not a platform api-key. Set by the F1 active-account override.
    is_oauth: bool,
}

fn resolve_session_model_upstream(
    db: &DbManager,
    session_key: &str,
) -> Result<SessionUpstream, String> {
    let conn = db.get_connection().map_err(|error| error.to_string())?;
    let session_row: Option<(String, String)> = conn
        .query_row(
            "SELECT model_json, agent_id
             FROM agent_sessions
             WHERE id = ?1 OR conversation_id = ?1
             ORDER BY CASE WHEN id = ?1 THEN 0 ELSE 1 END, created_at DESC
             LIMIT 1",
            params![session_key],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    let (model_json, agent_id) =
        session_row.ok_or_else(|| format!("Agent session not found: {session_key}"))?;
    let selection: crate::runtime::ModelSelection =
        serde_json::from_str(&model_json).map_err(|error| error.to_string())?;
    let (platform_id, model_name) = match selection {
        crate::runtime::ModelSelection::Omnix {
            platform_id,
            model_name,
        } => (platform_id, model_name),
        _ => return Err("该会话没有选择 OMNIX 模型，不应进入会话网关".into()),
    };
    // F1: if the agent switched its active upstream to a specific OAuth / api-key
    // account, use that account as the upstream — same conversation & session
    // gateway URL, only the next turn's upstream changes (context preserved).
    if let Some(upstream) = active_account_override(db, &agent_id, &model_name) {
        return Ok(upstream);
    }
    let platform: Option<(String, String, String)> = conn
        .query_row(
            "SELECT api_key, api_address, api_type
             FROM model_platforms
             WHERE id = ?1 AND is_enabled = 1",
            params![platform_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    let (legacy_key, api_address, api_type) =
        platform.ok_or_else(|| format!("Model platform is disabled or missing: {platform_id}"))?;

    let mut keys = Vec::new();
    let mut key_ids = Vec::new();
    if let Ok(mut statement) = conn.prepare(
        "SELECT id, encrypted_key
         FROM platform_api_keys
         WHERE platform_id = ?1
         ORDER BY is_active DESC, created_at ASC",
    ) {
        if let Ok(rows) = statement.query_map(params![platform_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        }) {
            for (id, encrypted) in rows.flatten() {
                let key = crate::crypto::decrypt(&encrypted);
                if !key.trim().is_empty() && !keys.contains(&key) {
                    keys.push(key);
                    key_ids.push(Some(id));
                }
            }
        }
    }
    if keys.is_empty() {
        for encrypted in legacy_key
            .split(',')
            .map(str::trim)
            .filter(|key| !key.is_empty())
        {
            let key = crate::crypto::decrypt(encrypted);
            if !key.trim().is_empty() && !keys.contains(&key) {
                keys.push(key);
                key_ids.push(None);
            }
        }
    }
    if keys.is_empty() && api_type != "ollama" {
        return Err(format!("Model platform has no API key: {platform_id}"));
    }
    Ok(SessionUpstream {
        platform_id,
        model_name,
        api_address,
        api_type,
        keys,
        key_ids,
        is_oauth: false,
    })
}

/// F1: resolve an agent's active upstream account (OAuth subscription or api-key
/// account) into a session upstream override. `None` = no override (use the
/// session's platform). Keeps the session's `model_name`.
fn active_account_override(
    db: &DbManager,
    agent_id: &str,
    model_name: &str,
) -> Option<SessionUpstream> {
    let active = db
        .get_setting(&crate::commands::active_upstream_setting_key(agent_id))
        .ok()
        .flatten()
        .filter(|value| !value.trim().is_empty())?;

    if let Some(oauth_id) = active.strip_prefix("oauth:") {
        let (kind, token) = crate::commands::resolve_oauth_access_token(db, oauth_id).ok()?;
        // Provider-native API base + type; Claude speaks anthropic, others openai.
        let (api_address, api_type) = match kind {
            crate::oauth::OAuthProviderKind::AnthropicClaude => {
                ("https://api.anthropic.com".to_string(), "anthropic".to_string())
            }
            crate::oauth::OAuthProviderKind::OpenAiCodex => {
                ("https://api.openai.com/v1".to_string(), "openai".to_string())
            }
            crate::oauth::OAuthProviderKind::GoogleGemini => (
                "https://generativelanguage.googleapis.com".to_string(),
                "openai".to_string(),
            ),
        };
        return Some(SessionUpstream {
            platform_id: active,
            model_name: model_name.to_string(),
            api_address,
            api_type,
            keys: vec![token],
            key_ids: vec![None],
            is_oauth: true,
        });
    }

    if let Some(apikey_id) = active.strip_prefix("apikey:") {
        let conn = db.get_connection().ok()?;
        let (api_key, api_host): (String, String) = conn
            .query_row(
                "SELECT api_key, api_host FROM agent_accounts WHERE id = ?1",
                params![apikey_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .ok()?;
        let key = crate::crypto::decrypt(&api_key);
        let key = if key.trim().is_empty() { api_key } else { key };
        return Some(SessionUpstream {
            platform_id: active,
            model_name: model_name.to_string(),
            api_address: api_host,
            api_type: "openai".to_string(),
            keys: vec![key],
            key_ids: vec![None],
            is_oauth: false,
        });
    }
    None
}

fn resolve_model_upstream(
    db: &DbManager,
    target_model_name: &str,
) -> Result<(String, String, String, String, String), String> {
    resolve_model_upstream_for_agent(db, target_model_name, None)
}

/// Resolve upstream with optional agent name for per-agent routing.
/// Returns `(api_key, api_host, api_type, actual_model_name, platform_id)` — the
/// trailing `platform_id` lets the caller attribute circuit-breaker outcomes.
fn resolve_model_upstream_for_agent(
    db: &DbManager,
    target_model_name: &str,
    agent_name: Option<&str>,
) -> Result<(String, String, String, String, String), String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;

    // 0. Check per-agent platform binding
    if let Some(agent) = agent_name {
        if let Ok(row) = conn.query_row(
            "SELECT apb.platform_id, COALESCE(apb.model_name, mp.name), mp.api_key, mp.api_address, mp.api_type
             FROM agent_platform_bindings apb
             JOIN model_platforms mp ON apb.platform_id = mp.id
             WHERE apb.agent_name = ?1 AND apb.enabled = 1
               AND COALESCE(apb.binding_kind, 'omnix') = 'omnix'
               AND mp.is_enabled = 1 AND mp.is_healthy = 1",
            params![agent],
            |r| Ok((
                r.get::<_, String>(0)?,  // platform_id
                r.get::<_, String>(1)?,  // model_name
                r.get::<_, String>(2)?,  // api_key
                r.get::<_, String>(3)?,  // api_address
                r.get::<_, String>(4)?,  // api_type
            )),
        ) {
            let (platform_id, model_name, api_key, api_address, api_type) = row;
            let decrypted_key = crate::crypto::decrypt(&api_key);
            println!("OMNIX Router: Agent '{}' bound to platform '{}' → {}", agent, platform_id, model_name);
            return Ok((decrypted_key, api_address, api_type, model_name, platform_id));
        }
    }

    // 1. If target_model_name has platform prefix (e.g. "platform_id:model_name")
    if let Some(pos) = target_model_name.find(':') {
        let platform_id = &target_model_name[..pos];
        let model_name = &target_model_name[pos + 1..];

        let mut stmt = conn.prepare(
            "SELECT api_key, api_address, api_type FROM model_platforms WHERE id = ?1 AND is_enabled = 1"
        ).map_err(|e| e.to_string())?;

        let platform_opt = stmt
            .query_row(params![platform_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .ok();

        // Decrypt API key if encrypted
        let platform_opt = platform_opt.map(|(k, a, t)| (crate::crypto::decrypt(&k), a, t));

        if let Some((api_key, api_address, api_type)) = platform_opt {
            return Ok((api_key, api_address, api_type, model_name.to_string(), platform_id.to_string()));
        }
    }

    // 2. Weighted selection from matching platforms
    //    Find all platforms that serve this model, ordered by priority DESC, weight DESC
    //    Only consider healthy and enabled platforms
    let mut stmt = conn
        .prepare(
            "SELECT mp.id, mp.api_key, mp.api_address, mp.api_type, mp.weight, mp.priority,
                pm.model_name, mp.consecutive_failures
         FROM platform_models pm
         JOIN model_platforms mp ON pm.platform_id = mp.id
         WHERE pm.model_name = ?1 AND pm.is_enabled = 1 AND mp.is_enabled = 1 AND mp.is_healthy = 1
         ORDER BY mp.priority DESC, mp.weight DESC",
        )
        .map_err(|e| e.to_string())?;

    let candidates: Vec<(String, String, String, String, String, i32, String, i32)> = stmt
        .query_map(params![target_model_name], |row| {
            Ok((
                row.get::<_, String>(0)?, // platform_id
                row.get::<_, String>(1)?, // api_key
                row.get::<_, String>(2)?, // api_address
                row.get::<_, String>(3)?, // api_type
                row.get::<_, String>(4)?, // weight (stored as TEXT in some configs)
                row.get::<_, i32>(5)?,    // priority
                row.get::<_, String>(6)?, // model_name
                row.get::<_, i32>(7)?,    // consecutive_failures
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
        let same_priority: Vec<_> = candidates
            .iter()
            .filter(|c| c.5 == highest_priority)
            .collect();

        // Calculate total weight for same-priority candidates
        let total_weight: i32 = same_priority
            .iter()
            .map(|c| c.4.parse::<i32>().unwrap_or(1).max(1))
            .sum();

        // Weighted selection using a simple counter (no DB query needed)
        // Use FNV hash of target model name as deterministic seed to spread picks
        let counter = {
            let mut h: u64 = 0xcbf29ce484222325;
            for b in target_model_name.bytes() {
                h ^= b as u64;
                h = h.wrapping_mul(0x100000001b3);
            }
            h as i32
        };

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
                return Ok((
                    candidate.1.clone(),
                    candidate.2.clone(),
                    candidate.3.clone(),
                    candidate.6.clone(),
                    candidate.0.clone(),
                ));
            }
        }

        // Fallback to first candidate
        let c = &same_priority[0];
        return Ok((c.1.clone(), c.2.clone(), c.3.clone(), c.6.clone(), c.0.clone()));
    }

    // 3. Fallback to any healthy active platform
    let mut stmt = conn.prepare(
        "SELECT id, api_key, api_address, api_type FROM model_platforms WHERE is_enabled = 1 AND is_healthy = 1 ORDER BY priority DESC, weight DESC LIMIT 1"
    ).map_err(|e| e.to_string())?;

    let fallback_opt = stmt
        .query_row([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })
        .ok();

    if let Some((platform_id, api_key, api_address, api_type)) = fallback_opt {
        return Ok((
            api_key,
            api_address,
            api_type,
            target_model_name.to_string(),
            platform_id,
        ));
    }

    Err("No active model platforms configured in database.".to_string())
}

fn join_url(base: &str, path: &str) -> String {
    let base_trimmed = base.trim_end_matches('/');
    let path_trimmed = path.trim_start_matches('/');
    format!("{}/{}", base_trimmed, path_trimmed)
}

// ── Health Endpoint ────────

/// GET /health — Returns proxy status and platform summary (single query)
async fn handle_health(State(state): State<Arc<ProxyState>>) -> impl IntoResponse {
    let conn = match state.db.get_connection() {
        Ok(c) => c,
        Err(_) => {
            return Json(serde_json::json!({
                "status": "error",
                "message": "Database connection failed"
            }))
            .into_response();
        }
    };

    // Single UNION ALL query instead of 6 separate queries
    let sql = "
        SELECT 'total_platforms' as k, COUNT(*) as v FROM model_platforms
        UNION ALL SELECT 'enabled_platforms', COUNT(*) FROM model_platforms WHERE is_enabled = 1
        UNION ALL SELECT 'healthy_platforms', COUNT(*) FROM model_platforms WHERE is_enabled = 1 AND is_healthy = 1
        UNION ALL SELECT 'total_models', COUNT(*) FROM platform_models WHERE is_enabled = 1
        UNION ALL SELECT 'total_requests', COUNT(*) FROM request_logs
        UNION ALL SELECT 'requests_today', COUNT(*) FROM request_logs WHERE date(timestamp) = date('now')
    ";

    let mut stats = std::collections::HashMap::new();
    if let Ok(mut stmt) = conn.prepare(sql) {
        if let Ok(rows) = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        }) {
            for r in rows.flatten() {
                stats.insert(r.0, r.1);
            }
        }
    }

    let enabled = stats.get("enabled_platforms").copied().unwrap_or(0);
    let healthy = stats.get("healthy_platforms").copied().unwrap_or(0);

    Json(serde_json::json!({
        "status": "ok",
        "proxy_port": 1421,
        "platforms": {
            "total": stats.get("total_platforms").copied().unwrap_or(0),
            "enabled": enabled,
            "healthy": healthy,
            "unhealthy": enabled - healthy,
        },
        "models": {
            "total": stats.get("total_models").copied().unwrap_or(0),
        },
        "requests": {
            "total": stats.get("total_requests").copied().unwrap_or(0),
            "today": stats.get("requests_today").copied().unwrap_or(0),
        }
    }))
    .into_response()
}

// ── Platform Health Tracking ──

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

// ── Request Logging ───────

/// Write a request log entry to the database. The INSERT (WAL write + fsync)
/// runs on the blocking pool so the per-request write never stalls a tokio
/// worker on the hot async path — callers fire-and-forget.
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
    let db = db.clone(); // shares the same r2d2 pool
    let model = model.to_string();
    let platform = platform.unwrap_or("").to_string();
    let error_message = error_message.unwrap_or("").to_string();
    let request_id = request_id.unwrap_or("").to_string();
    let source = source.to_string();
    let total_tokens = prompt_tokens + completion_tokens;
    tokio::task::spawn_blocking(move || {
        let conn = match db.get_connection() {
            Ok(c) => c,
            Err(_) => return,
        };
        let _ = conn.execute(
            "INSERT INTO request_logs (model, platform, prompt_tokens, completion_tokens, total_tokens, latency_ms, status_code, is_stream, is_error, error_message, request_id, source)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                model,
                platform,
                prompt_tokens,
                completion_tokens,
                total_tokens,
                latency_ms,
                status_code,
                is_stream as i32,
                is_error as i32,
                error_message,
                request_id,
                source,
            ],
        );
    });
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
            return (
                StatusCode::BAD_REQUEST,
                "Missing 'model' field in request body",
            )
                .into_response();
        }
    };

    // Resolve the model to an upstream platform
    let (api_key, api_address, api_type, actual_model, _circuit_platform_id) =
        match resolve_model_upstream(&state.db, &model_name) {
            Ok(res) => res,
            Err(e) => {
                return (
                    StatusCode::NOT_FOUND,
                    format!("Model resolution failed: {}", e),
                )
                    .into_response();
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
                    Value::Array(arr) => arr
                        .first()
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
    let mut req = state
        .http_client
        .post(&upstream_url)
        .json(&forwarded_payload);
    if !api_key.trim().is_empty() {
        req = req.header("Authorization", format!("Bearer {}", api_key.trim()));
    }

    match req.send().await {
        Ok(resp) => {
            let status = resp.status();
            let body = match resp.bytes().await {
                Ok(b) => b,
                Err(e) => {
                    return (
                        StatusCode::BAD_GATEWAY,
                        format!("Failed to read upstream response: {}", e),
                    )
                        .into_response();
                }
            };
            // Return the upstream response as-is
            (status, body.to_vec()).into_response()
        }
        Err(e) => (
            StatusCode::BAD_GATEWAY,
            format!("Upstream request failed: {}", e),
        )
            .into_response(),
    }
}

#[cfg(test)]
mod tests {
    use crate::db::DbManager;
    use crate::runtime::{
        create_agent_session_record, AgentId, AgentSessionConfig, ModelSelection, PermissionPolicy,
        WorkMode,
    };

    use super::resolve_session_model_upstream;

    #[test]
    fn query_param_extracts_token_and_ignores_others() {
        assert_eq!(
            super::query_param(Some("token=tok_abc&x=1"), "token").as_deref(),
            Some("tok_abc")
        );
        assert_eq!(
            super::query_param(Some("a=1&token=tok_xyz"), "token").as_deref(),
            Some("tok_xyz")
        );
        assert_eq!(super::query_param(Some("a=1&b=2"), "token"), None);
        assert_eq!(super::query_param(None, "token"), None);
        // Empty value stays empty (→ auth fails against a non-empty expected).
        assert_eq!(super::query_param(Some("token="), "token").as_deref(), Some(""));
    }

    #[test]
    fn token_matches_is_exact_and_rejects_empty_expected() {
        assert!(super::token_matches("tok_abc", "tok_abc"));
        assert!(!super::token_matches("tok_abd", "tok_abc"));
        assert!(!super::token_matches("tok_ab", "tok_abc"));
        assert!(!super::token_matches("tok_abcd", "tok_abc"));
        // A missing/unreadable stored token must never authenticate anyone.
        assert!(!super::token_matches("", ""));
        assert!(!super::token_matches("anything", ""));
    }

    #[test]
    fn remote_clients_recorded_and_sorted_recent_first() {
        super::record_remote_client("192.168.1.7".into());
        super::record_remote_client("192.168.1.9".into());
        let snapshot = super::remote_clients_snapshot();
        assert!(snapshot.iter().any(|c| c.ip == "192.168.1.7"));
        assert!(snapshot.iter().any(|c| c.ip == "192.168.1.9"));
        // Sorted by last_seen desc.
        for pair in snapshot.windows(2) {
            assert!(pair[0].last_seen >= pair[1].last_seen);
        }
    }

    #[test]
    fn session_upstream_uses_bound_platform_and_primary_key_first() {
        let db_path = std::env::temp_dir().join(format!(
            "omnix_session_gateway_{}.sqlite",
            chrono::Utc::now().timestamp_micros()
        ));
        let db = DbManager::new_runtime_test(db_path.clone());
        let conn = db.get_connection().expect("db connection");
        conn.execute_batch(
            "CREATE TABLE model_platforms (
                id TEXT PRIMARY KEY, name TEXT NOT NULL, api_type TEXT NOT NULL,
                api_key TEXT NOT NULL DEFAULT '', api_address TEXT NOT NULL DEFAULT '',
                is_enabled INTEGER NOT NULL DEFAULT 1
            );
            CREATE TABLE platform_models (
                id TEXT PRIMARY KEY, platform_id TEXT NOT NULL, model_name TEXT NOT NULL,
                is_enabled INTEGER NOT NULL DEFAULT 1, status TEXT NOT NULL DEFAULT 'success'
            );
            CREATE TABLE platform_api_keys (
                id TEXT PRIMARY KEY, platform_id TEXT NOT NULL, encrypted_key TEXT NOT NULL,
                label TEXT DEFAULT '', is_active INTEGER DEFAULT 0,
                created_at TEXT DEFAULT (datetime('now'))
            );
            INSERT INTO conversations (id, title, workspace_path, active_agent)
                VALUES ('conv-gateway', 'Gateway', 'D:/work/project', 'Claude Code');
            INSERT INTO model_platforms (id, name, api_type, api_address)
                VALUES ('volcano', 'Volcano', 'openai-compatible', 'https://example.test/api');
            INSERT INTO platform_models (id, platform_id, model_name)
                VALUES ('volcano:doubao', 'volcano', 'doubao-code');
            INSERT INTO platform_api_keys (id, platform_id, encrypted_key, is_active, created_at)
                VALUES ('backup', 'volcano', 'backup-key', 0, '2026-01-01'),
                       ('primary', 'volcano', 'primary-key', 1, '2026-01-02');",
        )
        .expect("gateway fixture");
        drop(conn);
        create_agent_session_record(
            &db,
            "session-gateway",
            &AgentSessionConfig {
                conversation_id: "conv-gateway".into(),
                agent: AgentId::ClaudeCode,
                executable_path: "claude.cmd".into(),
                workspace_path: "D:/work/project".into(),
                model: ModelSelection::Omnix {
                    platform_id: "volcano".into(),
                    model_name: "doubao-code".into(),
                },
                permission: PermissionPolicy::AskOnRisk,
                work_mode: WorkMode::Direct,
            },
        )
        .expect("session fixture");

        let upstream =
            resolve_session_model_upstream(&db, "session-gateway").expect("session upstream");
        assert_eq!(upstream.platform_id, "volcano");
        assert_eq!(upstream.model_name, "doubao-code");
        assert_eq!(upstream.api_type, "openai-compatible");
        assert_eq!(upstream.keys, vec!["primary-key", "backup-key"]);

        drop(db);
        let _ = std::fs::remove_file(db_path);
    }
}

/// P1：MCP 端点。JSON-RPC 2.0，通知不回响应（按规范返回 202 空体）。
async fn handle_mcp(
    State(state): State<Arc<ProxyState>>,
    Json(body): Json<serde_json::Value>,
) -> Response {
    // 批量请求：规范允许数组。逐条处理，过滤掉通知的空响应。
    if let Some(batch) = body.as_array() {
        let mut out = Vec::new();
        for item in batch {
            if let Ok(req) = serde_json::from_value::<crate::mcp_server::RpcRequest>(item.clone()) {
                if let Some(resp) = crate::mcp_server::handle_rpc(&state.db, req) {
                    out.push(serde_json::to_value(resp).unwrap_or(serde_json::Value::Null));
                }
            }
        }
        if out.is_empty() {
            return StatusCode::ACCEPTED.into_response();
        }
        return Json(out).into_response();
    }

    let req = match serde_json::from_value::<crate::mcp_server::RpcRequest>(body) {
        Ok(r) => r,
        Err(e) => {
            return Json(serde_json::json!({
                "jsonrpc": "2.0",
                "id": serde_json::Value::Null,
                "error": { "code": -32700, "message": format!("请求解析失败: {e}") }
            }))
            .into_response()
        }
    };
    match crate::mcp_server::handle_rpc(&state.db, req) {
        Some(resp) => Json(resp).into_response(),
        // 通知按规范不能回 body
        None => StatusCode::ACCEPTED.into_response(),
    }
}

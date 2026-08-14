//! Anthropic 路由（`/v1/messages` 及其 agent / session 变体），从 proxy.rs
//! 拆出（纯移动）。
//!
//! 这条路是网关的主干：收 Anthropic 形状的请求，选上游，按上游是 Anthropic 还是
//! OpenAI 决定原样转发还是翻译，流式与非流式各走一支。注入（正式池技能、召回
//! 记忆）也在这里——它们改的是 system prompt，必须在选型之后、转发之前。
//!
//! 判据是 `proxy_wire_tests.rs` 的大半：工具定义要原样到上游、Auto 选型只挑支持
//! 工具的模型且不把字面量 "Auto" 发出去、Anthropic→OpenAI 的工具翻译两个方向、
//! 流式透传仍要记账、用户消息**逐字节**到达上游（不被任何「安全改写」动过）。
//!
//! 作为子模块能看到父模块的私有项，`use super::*;` 把 imports 一并带过来。
#![allow(clippy::module_inception)]

use super::*;

// Main handler for /v1/messages (Claude format -> OpenAI format)
pub(super) async fn handle_messages_for_agent(
    State(state): State<Arc<ProxyState>>,
    axum::extract::Path(agent_name): axum::extract::Path<String>,
    headers: axum::http::HeaderMap,
    Json(payload): Json<AnthropicRequest>,
) -> impl IntoResponse {
    let agent_name_decoded = agent_name.replace('_', " ");
    handle_messages_impl(state, Some(agent_name_decoded), None, headers, payload).await
}

pub(super) async fn handle_messages(
    State(state): State<Arc<ProxyState>>,
    headers: axum::http::HeaderMap,
    Json(payload): Json<AnthropicRequest>,
) -> impl IntoResponse {
    handle_messages_impl(state, None, None, headers, payload).await
}

pub(super) async fn handle_messages_for_session(
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
    // 只记「注入过几次」，不是「有没有用」。OMNIX 是透传网关，真正的活在用户机器
    // 上的 CLI 里跑，没有任何结果回到这里——所以别拿它当质量分排序，界面上也只
    // 能写「注入」。想要真信号只有显式反馈一条路，那是另一个决定。
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

pub(super) async fn handle_messages_impl(
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
        // 声明了 tools 就是要用工具。这个信号比任何启发式都准。
        let need_tools = payload.extra.contains_key("tools");
        println!("OMNIX Router: Classification result -> Need Vision: {}, Reasoning: {}, Coding: {}, Speedy: {}", need_vis, need_reas, need_cod, need_spd);

        // 候选池和「有没有 Key」的判断都在 proxy::auto_route_candidates 里——见 openai 侧注释。
        {
            let mut best_model = None;
            let mut highest_score = -1;
            for c in crate::proxy::auto_route_candidates(&state.db) {
                if !c.has_key && c.api_type != "ollama" {
                    continue;
                }
                // R0：请求声明了工具，就**只在支持工具的模型里选**。视觉/推理/编码
                // 是偏好（打分），工具支持是资格——挑一个不会调工具的模型去跑工具
                // 任务，产出是废的，而且失败得很隐蔽。
                if need_tools && !c.has_tool_use {
                    continue;
                }
                let mut score = 0;
                if need_vis && c.has_vision { score += 10; }
                if need_reas && c.has_reasoning { score += 10; }
                if need_cod && c.has_coding { score += 5; }
                if need_spd && c.has_speedy { score += 8; }
                if !need_vis && !need_reas && !need_cod && !need_spd && c.has_vision { score -= 2; }

                if score > highest_score {
                    highest_score = score;
                    best_model = Some(format!("{}:{}", c.platform_id, c.model_name));
                }
            }
            match best_model {
                Some(m) => resolved_model = m,
                None if need_tools => {
                    return anthropic_error(
                        StatusCode::BAD_REQUEST,
                        "这次请求需要工具调用，但当前启用的模型里没有一个标记为支持工具。请到「模型中心」为要用的模型勾上「工具调用」，或改用支持工具的平台。",
                    );
                }
                // 挑不出模型时绝不能把字面量 "Auto" 当模型名发上去——上游会回
                // 一句「Model does not exist」，把「一个可聊天的模型都没有」
                // 伪装成「模型名写错了」。
                None => {
                    return anthropic_error(
                        StatusCode::SERVICE_UNAVAILABLE,
                        "Auto 路由没能选出可用的对话模型：请到「模型中心」确认至少有一个已启用、带 API Key 且非嵌入/语音类的模型，或在「设置 → 内置功能默认模型」直接指定一个。",
                    );
                }
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
            // 逗号切分已经在 `platform_keys` 里做完了（那是旧列的多 Key 写法）。
            // 在这里再切一次不但多余，还把新表来的 Key 数组压成长度 1。
            Ok((api_keys, api_host, api_type, actual_model_name, platform_id)) => (
                api_host,
                api_type,
                actual_model_name,
                api_keys,
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
            Err(e) => {
                log_failure(&state.db, &resolved_model, "anthropic", &start_time, StatusCode::BAD_GATEWAY, &e);
                return anthropic_error(
                    StatusCode::BAD_GATEWAY,
                    format!("连不上上游 {upstream_url}（模型 {resolved_model}）：{e}"),
                )
            }
        };

        let status = upstream_res.status();
        if !status.is_success() {
            let err_body = upstream_res.text().await.unwrap_or_default();
            log_failure(&state.db, &resolved_model, "anthropic", &start_time, status, &err_body);
            if err_body.trim().is_empty() {
                return anthropic_error(status, format!("上游返回 {status} 且无错误信息（模型 {resolved_model}）"));
            }
            return (status, err_body).into_response();
        }

        // 这里曾经给「事件总线」计数。那套机制到阈值只会把计数清零并返回一串
        // 待触发的 task_id，而**唯一的调用方（就是这里）把返回值丢了**——
        // 也就是说它只是个自己清零的计数器，补 UI 也不会有任何事发生。已删除。
        // 日志则必须等到读得到 usage 之后才写——见下面两个分支。
        let log_db = (*state.db).clone();
        let log_model = resolved_model.clone();
        let log_status = status.as_u16() as i32;

        if is_stream {
            // usage 要到 message_start / message_delta 才出现，所以边转发边扫，
            // 流结束（或客户端断开）时由 recorder 的 Drop 记账。
            let mut recorder =
                StreamUsageRecorder::new(log_db, log_model, "anthropic", start_time, log_status);
            let stream = upstream_res.bytes_stream().map(move |r| {
                if let Ok(bytes) = &r {
                    recorder.observe(bytes);
                }
                r.map_err(|e| axum::Error::new(e))
            });
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
            log_request(
                &log_db,
                &log_model,
                Some("anthropic"),
                crate::usage_meter::from_response_body(&bytes),
                start_time.elapsed().as_millis() as i64,
                log_status,
                false,
                false,
                None,
                None,
                "proxy",
            );
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
        // Translate to OpenAI format (For OpenAI or Ollama).
        //
        // R1：工具链在这里一并翻译（`crate::tool_translate`）。协议细节全在那个
        // 模块里做成纯函数——本机没有 OpenAI 兼容上游可打，端到端验不了，
        // 所以逻辑不能留在 handler 里。这里只负责接线。
        let system = payload
            .system
            .as_ref()
            .map(|s| serde_json::Value::String(s.to_string_content()));
        let anthropic_messages: Vec<(String, serde_json::Value)> = payload
            .messages
            .iter()
            .map(|m| {
                let content = serde_json::to_value(&m.content)
                    .unwrap_or_else(|_| serde_json::Value::String(String::new()));
                (m.role.clone(), content)
            })
            .collect();
        let messages =
            crate::tool_translate::messages_to_openai(system.as_ref(), &anthropic_messages);

        let tools = payload
            .extra
            .get("tools")
            .and_then(crate::tool_translate::tools_to_openai);
        let tool_choice = payload
            .extra
            .get("tool_choice")
            .and_then(crate::tool_translate::tool_choice_to_openai);

        let openai_req = OpenAIRequest {
            model: actual_model_name,
            messages,
            max_tokens: payload.max_tokens,
            temperature: payload.temperature,
            stream: payload.stream,
            tools,
            tool_choice,
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
            Err(e) => {
                log_failure(&state.db, &resolved_model, "openai", &start_time, StatusCode::BAD_GATEWAY, &e);
                return anthropic_error(
                    StatusCode::BAD_GATEWAY,
                    format!("连不上上游 {upstream_url}（模型 {resolved_model}）：{e}"),
                )
            }
        };

        let status = upstream_res.status();
        if !status.is_success() {
            let err_body = upstream_res.text().await.unwrap_or_default();
            log_failure(&state.db, &resolved_model, "openai", &start_time, status, &err_body);
            if err_body.trim().is_empty() {
                return anthropic_error(status, format!("上游返回 {status} 且无错误信息（模型 {resolved_model}）"));
            }
            return (status, err_body).into_response();
        }

        if is_stream {
            let stream = upstream_res.bytes_stream();
            // 这条路径要把 OpenAI SSE 改写成 Anthropic SSE，所以用量扫的是**上游
            // 原始字节**（改写后的事件里没有 usage，扫下游只会永远得零）。
            let mut recorder = StreamUsageRecorder::new(
                (*state.db).clone(),
                resolved_model.clone(),
                "openai",
                start_time,
                200,
            );
            // R1：跨 chunk 的工具调用状态机在 tool_translate 里，这里只喂字节。
            let mut translator = crate::tool_translate::StreamTranslator::new(request_model.clone());

            let anthropic_stream = stream.map(move |result| match result {
                Ok(bytes) => {
                    recorder.observe(&bytes);
                    let out = translator.push_bytes(&bytes);
                    Ok::<_, axum::Error>(axum::body::Bytes::from(out.into_bytes()))
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
            let res_bytes = match upstream_res.bytes().await {
                Ok(b) => b,
                Err(e) => {
                    return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
                }
            };

            let usage = crate::usage_meter::from_response_body(&res_bytes);
            log_request(
                &(*state.db).clone(),
                &resolved_model,
                Some("openai"),
                usage,
                start_time.elapsed().as_millis() as i64,
                200,
                false,
                false,
                None,
                None,
                "proxy",
            );

            // 整个响应按 Value 解析：`message` 里除了 content 还有 tool_calls，
            // 用固定结构体接会把它挡在门外——那正是工具链断掉的原因之一。
            let parsed: serde_json::Value = match serde_json::from_slice(&res_bytes) {
                Ok(v) => v,
                Err(_) => {
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "Failed to parse OpenAI response.",
                    )
                        .into_response()
                }
            };
            let Some(choice) = parsed.get("choices").and_then(|c| c.as_array()).and_then(|c| c.first())
            else {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Failed to parse OpenAI response.",
                )
                    .into_response();
            };
            let empty = serde_json::json!({});
            let message = choice.get("message").unwrap_or(&empty);
            // 上游报了多少就往回传多少。这里原本也是硬编码的零——
            // 客户端（Claude Code 等）读的是这个字段，写零等于告诉它这次没花钱。
            let reported = usage.unwrap_or_default();
            let anthropic_res = serde_json::json!({
                "id": "msg_local_proxy",
                "type": "message",
                "role": "assistant",
                "content": crate::tool_translate::content_blocks_from_openai_message(message),
                "model": request_model,
                // 工具调用必须报 tool_use，否则客户端当这一轮已经说完了，
                // 不会去执行工具，工具链在最后一步断掉。
                "stop_reason": crate::tool_translate::stop_reason_to_anthropic(
                    choice.get("finish_reason").and_then(|r| r.as_str())
                ),
                "stop_sequence": null,
                "usage": {
                    "input_tokens": reported.input,
                    "output_tokens": reported.output,
                    "cache_read_input_tokens": reported.cache_read,
                    "cache_creation_input_tokens": reported.cache_creation
                }
            });
            Json(anthropic_res).into_response()
        }
    }
}

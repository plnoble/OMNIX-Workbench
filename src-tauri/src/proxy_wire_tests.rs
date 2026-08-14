//! T0：请求级回归测试——把请求真穿过网关，断言**最后发给上游的是什么**。
//!
//! ## 为什么需要这一层
//!
//! 这一轮在网关注入路径上查出三个 bug：
//!
//! 1. `AnthropicRequest` 没有 `tools` 字段，重新序列化时被静默吞掉；
//! 2. 自动选型不读 `has_tool_use`，会把工具任务派给不支持工具的模型；
//! 3. Anthropic→OpenAI 那条路径根本不翻译工具。
//!
//! **三个都没被 300 多项单测拦住**，因为那些测的全是零件：serde 一个、
//! 选型一个、翻译一个，没有一条回答「一个请求进来，上游最终收到什么」。
//!
//! 借鉴 pi 的 evals（跑真实 session + 模型判分）的思路，但砍到本机可验的程度：
//! 起一个**假上游**记录请求体，让请求真穿过 `handle_messages_impl`，
//! 然后断言那个请求体。不需要 API key，不需要网络。
//!
//! 下面每条测试都对应上面一个已修的 bug——它们是现成的回归样本。

use super::*;
use crate::db::DbManager;
use axum::{extract::State as AxumState, routing::post, Json, Router};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

// ── 假上游 ────────────────────────────────────

#[derive(Clone)]
struct Recorder {
    seen: Arc<Mutex<Vec<Value>>>,
    /// 上游收到的鉴权头（x-api-key / Authorization），按到达顺序。
    auth: Arc<Mutex<Vec<String>>>,
    reply: Arc<Value>,
}

async fn record_and_reply(
    AxumState(rec): AxumState<Recorder>,
    headers: axum::http::HeaderMap,
    Json(body): Json<Value>,
) -> Json<Value> {
    // 坑点2：这里绝不能跨 await 持锁。lock → push → 立刻出作用域，
    // 中间没有任何 await，所以是安全的。
    {
        let mut seen = rec.seen.lock().expect("recorder lock");
        seen.push(body);
    }
    {
        let key = headers
            .get("x-api-key")
            .and_then(|v| v.to_str().ok())
            .map(str::to_string)
            .or_else(|| {
                headers
                    .get("authorization")
                    .and_then(|v| v.to_str().ok())
                    .map(|v| v.trim_start_matches("Bearer ").to_string())
            })
            .unwrap_or_default();
        rec.auth.lock().expect("auth lock").push(key);
    }
    Json((*rec.reply).clone())
}

struct FakeUpstream {
    base_url: String,
    seen: Arc<Mutex<Vec<Value>>>,
    auth: Arc<Mutex<Vec<String>>>,
}

impl FakeUpstream {
    /// 上游收到的第一个请求体。没收到就是网关压根没转发出去。
    fn first_request(&self) -> Value {
        self.seen
            .lock()
            .expect("recorder lock")
            .first()
            .cloned()
            .expect("上游没有收到任何请求——网关在转发前就返回了")
    }

    /// 上游第一次收到的鉴权头值（去掉 `Bearer ` 前缀）。
    fn first_auth_header(&self) -> Option<String> {
        self.auth.lock().expect("auth lock").first().cloned()
    }
}

/// 起一个只会记录请求体、返回固定响应的上游。
/// 同时挂 Anthropic 与 OpenAI 两个路径，这样一份夹具能测两条分支。
async fn start_fake_upstream(reply: Value) -> FakeUpstream {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let auth = Arc::new(Mutex::new(Vec::new()));
    let rec = Recorder { seen: Arc::clone(&seen), auth: Arc::clone(&auth), reply: Arc::new(reply) };
    let app = Router::new()
        .route("/v1/messages", post(record_and_reply))
        .route("/chat/completions", post(record_and_reply))
        .route("/v1/chat/completions", post(record_and_reply))
        .with_state(rec);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("绑定临时端口");
    let addr = listener.local_addr().expect("本地地址");
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    FakeUpstream { base_url: format!("http://{addr}"), seen, auth }
}

/// 会返回 SSE 的假上游。
///
/// 流式是 T0 第一版唯一没覆盖的分支，而它恰好是最难写对的：跨 chunk 的
/// 状态机、用量要到末尾事件才齐、`Drop` 才记账。用假上游把整条流跑一遍，
/// 就能在**没有真实上游**的情况下验到接线——剩下的才是真模型行为。
async fn start_streaming_upstream(sse_body: &'static str) -> FakeUpstream {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let captured = Arc::clone(&seen);
    let handler = move |Json(body): Json<Value>| {
        let captured = Arc::clone(&captured);
        async move {
            {
                // 坑点2：lock → push → 出作用域，中间无 await。
                captured.lock().expect("recorder lock").push(body);
            }
            axum::response::Response::builder()
                .header("Content-Type", "text/event-stream")
                .body(axum::body::Body::from(sse_body))
                .expect("SSE 响应")
        }
    };
    let app = Router::new()
        .route("/v1/messages", post(handler.clone()))
        .route("/chat/completions", post(handler.clone()))
        .route("/v1/chat/completions", post(handler));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("绑定临时端口");
    let addr = listener.local_addr().expect("本地地址");
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    FakeUpstream { base_url: format!("http://{addr}"), seen, auth: Arc::new(Mutex::new(Vec::new())) }
}

/// 一律拒绝的假上游：原样回指定状态码和错误体，用来验失败路径。
async fn start_rejecting_upstream(status: StatusCode, body: &'static str) -> FakeUpstream {
    let seen: Arc<std::sync::Mutex<Vec<Value>>> = Arc::new(std::sync::Mutex::new(Vec::new()));
    let recorder = Arc::clone(&seen);
    let handler = move |Json(payload): Json<Value>| {
        let recorder = Arc::clone(&recorder);
        async move {
            recorder.lock().expect("记录锁").push(payload);
            axum::response::Response::builder()
                .status(status)
                .header("Content-Type", "application/json")
                .body(axum::body::Body::from(body))
                .expect("拒绝响应")
        }
    };
    let app = Router::new()
        .route("/v1/messages", post(handler.clone()))
        .route("/chat/completions", post(handler.clone()))
        .route("/v1/chat/completions", post(handler));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("绑定临时端口");
    let addr = listener.local_addr().expect("本地地址");
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    FakeUpstream { base_url: format!("http://{addr}"), seen, auth: Arc::new(Mutex::new(Vec::new())) }
}

/// 把响应体读成文本（流式响应也是一段一段拼起来的字节）。
async fn body_text(response: axum::response::Response) -> String {
    let bytes = axum::body::to_bytes(response.into_body(), 4 * 1024 * 1024)
        .await
        .expect("读响应体");
    String::from_utf8_lossy(&bytes).to_string()
}

fn streaming_request() -> AnthropicRequest {
    serde_json::from_value(json!({
        "model": "claude-x", "max_tokens": 256, "stream": true,
        "tools": [{"name": "Read", "description": "读文件",
                   "input_schema": {"type": "object", "properties": {"path": {"type": "string"}}}}],
        "messages": [{"role": "user", "content": "看一下 a.txt"}]
    }))
    .expect("夹具请求可解析")
}

// ── 网关夹具 ──────────────────────────────────

fn temp_db(tag: &str) -> (Arc<DbManager>, std::path::PathBuf) {
    let path = std::env::temp_dir().join(format!(
        "omnix_wire_{tag}_{}_{}.sqlite",
        std::process::id(),
        chrono::Utc::now().timestamp_micros()
    ));
    let _ = std::fs::remove_file(&path);
    (Arc::new(DbManager::new_with_path(path.clone())), path)
}

fn proxy_state(db: Arc<DbManager>) -> Arc<ProxyState> {
    Arc::new(ProxyState {
        agent_manager: Arc::new(crate::agent::AgentManager::new(Arc::clone(&db))),
        runtime_manager: Arc::new(crate::runtime_manager::RuntimeManager::new(Arc::clone(&db))),
        db,
        http_client: reqwest::Client::new(),
        request_counter: std::sync::atomic::AtomicUsize::new(0),
        concurrency_semaphore: Arc::new(tokio::sync::Semaphore::new(4)),
    })
}

/// 把路由状态清干净。
///
/// `init_schema` 会种一批默认平台（含 Ollama），`agent_accounts` 里的活跃账号
/// 又**优先于** `target_model` 设置。不清就会打到 localhost:11434 而不是假上游——
/// 第一版测试就是这么全绿不了的。测路由就得先让路由只有一条路。
fn reset_routing(db: &DbManager) {
    let conn = db.get_connection().expect("db");
    for sql in [
        "DELETE FROM agent_accounts",
        "DELETE FROM agent_platform_bindings",
        "DELETE FROM platform_models",
        "DELETE FROM model_platforms",
    ] {
        let _ = conn.execute(sql, []);
    }
}

/// 装一个平台 + 一个模型，并把默认目标模型指向它。
fn install_model(db: &DbManager, platform: &str, api_type: &str, host: &str, model: &str, has_tool_use: i32) {
    let conn = db.get_connection().expect("db");
    conn.execute(
        "INSERT INTO model_platforms (id, name, api_type, api_key, api_address, is_enabled)
         VALUES (?1, ?1, ?2, 'test-key', ?3, 1)",
        rusqlite::params![platform, api_type, host],
    )
    .expect("插入平台");
    conn.execute(
        "INSERT INTO platform_models (id, platform_id, model_name, is_enabled, has_tool_use)
         VALUES (?1, ?2, ?3, 1, ?4)",
        rusqlite::params![format!("{platform}:{model}"), platform, model, has_tool_use],
    )
    .expect("插入模型");
}

fn set_target(db: &DbManager, target: &str) {
    db.set_setting("target_model", target).expect("设置目标模型");
}

/// Claude Code 一轮带工具往返的真实请求形状。
fn request_with_tools() -> AnthropicRequest {
    serde_json::from_value(json!({
        "model": "claude-x",
        "max_tokens": 1024,
        "tools": [{
            "name": "Read",
            "description": "读文件",
            "input_schema": {"type": "object", "properties": {"path": {"type": "string"}}}
        }],
        "tool_choice": {"type": "auto"},
        "messages": [
            {"role": "user", "content": "看一下 a.txt"},
            {"role": "assistant", "content": [
                {"type": "tool_use", "id": "tu_1", "name": "Read", "input": {"path": "a.txt"}}
            ]},
            {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "tu_1", "content": "文件内容"}
            ]}
        ]
    }))
    .expect("夹具请求可解析")
}

async fn drive(state: Arc<ProxyState>, payload: AnthropicRequest) -> axum::response::Response {
    handle_messages_impl(state, None, None, axum::http::HeaderMap::new(), payload).await
}

async fn body_json(response: axum::response::Response) -> Value {
    let bytes = axum::body::to_bytes(response.into_body(), 4 * 1024 * 1024)
        .await
        .expect("读响应体");
    serde_json::from_slice(&bytes).unwrap_or(Value::Null)
}

// ── 回归：P-1 工具定义被 serde 吞掉 ─────────────

/// 原 bug：`AnthropicRequest` 结构体没有 `tools` 字段，网关反序列化再重新
/// 序列化转发（`.json(&payload)`），工具定义在这一步静默消失。上游模型
/// 永远不知道有工具可用，agent 于是永远不会发起工具调用。
#[tokio::test]
async fn anthropic_upstream_receives_the_tools_the_client_declared() {
    let upstream = start_fake_upstream(json!({
        "type": "message", "role": "assistant",
        "content": [{"type": "text", "text": "好"}],
        "usage": {"input_tokens": 10, "output_tokens": 5}
    }))
    .await;
    let (db, path) = temp_db("anthropic_tools");
    reset_routing(&db);
    install_model(&db, "anth", "anthropic", &upstream.base_url, "claude-x", 1);
    set_target(&db, "anth:claude-x");

    let response = drive(proxy_state(Arc::clone(&db)), request_with_tools()).await;
    assert_eq!(response.status(), StatusCode::OK);

    let sent = upstream.first_request();
    assert!(sent.get("tools").is_some(), "工具定义没到上游：{sent}");
    assert_eq!(sent["tools"][0]["name"], "Read");
    assert_eq!(sent["tool_choice"]["type"], "auto");
    // tool_use / tool_result 的关联 id 也必须活着，否则工具链断在中间。
    assert_eq!(sent["messages"][1]["content"][0]["id"], "tu_1");
    assert_eq!(sent["messages"][2]["content"][0]["tool_use_id"], "tu_1");
    // 别往上游塞 null——`{"type":"tool_use","text":null}` 正是当初的现场。
    assert!(!sent.to_string().contains("null"), "转发的请求里有 null：{sent}");

    drop(db);
    let _ = std::fs::remove_file(path);
}

// ── 回归：R0 选型忽略 has_tool_use ──────────────

/// 原 bug：`has_tool_use` 列在、有维护、也给了前端，唯独自动选型的查询不读它。
/// 于是带工具的请求可能被派给一个不支持工具的模型——产出是废的，且失败得很隐蔽。
#[tokio::test]
async fn auto_routing_picks_a_tool_capable_model_when_tools_are_declared() {
    let upstream = start_fake_upstream(json!({
        "type": "message", "role": "assistant",
        "content": [{"type": "text", "text": "好"}],
        "usage": {"input_tokens": 1, "output_tokens": 1}
    }))
    .await;
    let (db, path) = temp_db("auto_tools");
    reset_routing(&db);
    // 两个都启用，只有 withtools 支持工具。
    install_model(&db, "notools", "anthropic", &upstream.base_url, "plain-model", 0);
    install_model(&db, "withtools", "anthropic", &upstream.base_url, "tool-model", 1);
    set_target(&db, "Auto");

    let response = drive(proxy_state(Arc::clone(&db)), request_with_tools()).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        upstream.first_request()["model"], "tool-model",
        "带工具的请求被派给了不支持工具的模型"
    );

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// 一个都不支持工具时要明确报错，而不是挑一个凑合上——静默降级最难查。
#[tokio::test]
async fn auto_routing_refuses_rather_than_silently_downgrading() {
    let upstream = start_fake_upstream(json!({"type": "message", "content": []})).await;
    let (db, path) = temp_db("auto_none");
    reset_routing(&db);
    install_model(&db, "notools", "anthropic", &upstream.base_url, "plain-model", 0);
    set_target(&db, "Auto");

    let response = drive(proxy_state(Arc::clone(&db)), request_with_tools()).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert!(
        upstream.seen.lock().unwrap().is_empty(),
        "既然选不出合格模型，就不该把请求发出去"
    );

    drop(db);
    let _ = std::fs::remove_file(path);
}

// ── 回归：R1 Anthropic→OpenAI 不翻译工具 ────────

/// 原 bug：绑 OpenAI 兼容上游时，这条翻译路径只翻文本，工具定义整个丢掉。
#[tokio::test]
async fn openai_upstream_receives_translated_tools_and_tool_calls() {
    let upstream = start_fake_upstream(json!({
        "choices": [{"message": {"role": "assistant", "content": "好"}, "finish_reason": "stop"}],
        "usage": {"prompt_tokens": 20, "completion_tokens": 3}
    }))
    .await;
    let (db, path) = temp_db("openai_tools");
    reset_routing(&db);
    install_model(&db, "oai", "openai", &upstream.base_url, "gpt-x", 1);
    set_target(&db, "oai:gpt-x");

    let response = drive(proxy_state(Arc::clone(&db)), request_with_tools()).await;
    assert_eq!(response.status(), StatusCode::OK);

    let sent = upstream.first_request();
    // 工具定义翻成 OpenAI 形状，input_schema 改名 parameters。
    assert_eq!(sent["tools"][0]["type"], "function");
    assert_eq!(sent["tools"][0]["function"]["name"], "Read");
    assert_eq!(
        sent["tools"][0]["function"]["parameters"]["properties"]["path"]["type"],
        "string"
    );
    assert_eq!(sent["tool_choice"], "auto");

    // tool_use → tool_calls，且 arguments 必须是 **字符串**。
    let assistant = sent["messages"]
        .as_array()
        .expect("messages 是数组")
        .iter()
        .find(|m| m["tool_calls"].is_array())
        .expect("应当有一条带 tool_calls 的 assistant 消息");
    let args = assistant["tool_calls"][0]["function"]["arguments"]
        .as_str()
        .expect("OpenAI 的 arguments 是 JSON 字符串不是对象");
    assert_eq!(serde_json::from_str::<Value>(args).unwrap()["path"], "a.txt");

    // tool_result → 独立的 role:"tool" 消息，且 id 对得上。
    let tool_msg = sent["messages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["role"] == "tool")
        .expect("tool_result 应当独立成 role:tool 消息");
    assert_eq!(tool_msg["tool_call_id"], "tu_1");

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// 反方向：上游回 tool_calls，网关必须翻回 Anthropic 的 tool_use 块，
/// 且 stop_reason 必须是 `tool_use`——报成 end_turn 客户端就当这轮说完了，
/// 不会去执行工具，工具链断在最后一步。
#[tokio::test]
async fn openai_tool_calls_come_back_as_anthropic_tool_use() {
    let upstream = start_fake_upstream(json!({
        "choices": [{
            "message": {
                "role": "assistant",
                "content": "我来读",
                "tool_calls": [{
                    "id": "call_1", "type": "function",
                    "function": {"name": "Read", "arguments": "{\"path\":\"a.txt\"}"}
                }]
            },
            "finish_reason": "tool_calls"
        }],
        "usage": {"prompt_tokens": 30, "completion_tokens": 8}
    }))
    .await;
    let (db, path) = temp_db("openai_back");
    reset_routing(&db);
    install_model(&db, "oai", "openai", &upstream.base_url, "gpt-x", 1);
    set_target(&db, "oai:gpt-x");

    let response = drive(proxy_state(Arc::clone(&db)), request_with_tools()).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;

    assert_eq!(body["stop_reason"], "tool_use", "报错 stop_reason 客户端就不会执行工具");
    let tool_block = body["content"]
        .as_array()
        .expect("content 是数组")
        .iter()
        .find(|b| b["type"] == "tool_use")
        .expect("应当有 tool_use 块");
    assert_eq!(tool_block["id"], "call_1");
    assert_eq!(tool_block["name"], "Read");
    // arguments 字符串要转回 input 对象。
    assert_eq!(tool_block["input"]["path"], "a.txt");

    drop(db);
    let _ = std::fs::remove_file(path);
}

// ── 回归：R2 用量恒为零 ────────────────────────

/// 原 bug：三处 `log_request` 全传 `0, 0`，网关侧用量统计是空的。
/// 这条从**请求穿过之后库里有什么**来验，而不是只验解析函数。
#[tokio::test]
async fn a_completed_request_lands_real_tokens_in_the_log() {
    let upstream = start_fake_upstream(json!({
        "type": "message", "role": "assistant",
        "content": [{"type": "text", "text": "好"}],
        "usage": {
            "input_tokens": 12, "output_tokens": 340,
            "cache_read_input_tokens": 45000, "cache_creation_input_tokens": 1200
        }
    }))
    .await;
    let (db, path) = temp_db("usage");
    reset_routing(&db);
    install_model(&db, "anth", "anthropic", &upstream.base_url, "claude-x", 1);
    set_target(&db, "anth:claude-x");

    let response = drive(proxy_state(Arc::clone(&db)), request_with_tools()).await;
    assert_eq!(response.status(), StatusCode::OK);

    // 落库是 spawn_blocking，等它一下。
    let mut row = None;
    for _ in 0..100 {
        if let Ok(conn) = db.get_connection() {
            row = conn
                .query_row(
                    "SELECT prompt_tokens, completion_tokens, cache_read_tokens
                     FROM request_logs ORDER BY id DESC LIMIT 1",
                    [],
                    |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?)),
                )
                .ok();
            if row.is_some() {
                break;
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    let (prompt, completion, cache_read) = row.expect("请求跑完了却没有用量日志");
    assert_eq!(prompt, 46212, "计费口径的输入总量 = 12 + 45000 + 1200");
    assert_eq!(completion, 340);
    assert_eq!(cache_read, 45000);

    drop(db);
    let _ = std::fs::remove_file(path);
}

// ── 注入路径 ──────────────────────────────────

/// 记忆召回默认关。开了之后要真的出现在**发给上游的 system 里**——
/// 只测 `build_memory_injection` 拼串是不够的，那不回答「它到底有没有被挂上去」。
#[tokio::test]
async fn recalled_memory_reaches_the_upstream_system_prompt() {
    let upstream = start_fake_upstream(json!({
        "type": "message", "content": [], "usage": {"input_tokens": 1, "output_tokens": 1}
    }))
    .await;
    let (db, path) = temp_db("recall");
    reset_routing(&db);
    install_model(&db, "anth", "anthropic", &upstream.base_url, "claude-x", 1);
    set_target(&db, "anth:claude-x");
    db.get_connection()
        .unwrap()
        .execute(
            "INSERT INTO memories (id, incident_desc, code_pattern, remediation, keywords, type)
             VALUES ('m1', 'git 强推覆盖公共历史', 'git push -f',
                     '协作仓库禁用 push -f，用 --force-with-lease', 'git,push,deploy', 'experience')",
            [],
        )
        .unwrap();

    let ask = |text: &str| {
        serde_json::from_value::<AnthropicRequest>(json!({
            "model": "claude-x", "max_tokens": 64,
            "messages": [{"role": "user", "content": text}]
        }))
        .unwrap()
    };

    // 关着的时候不该注。
    let _ = drive(proxy_state(Arc::clone(&db)), ask("我要 git push -f 一下")).await;
    let sent = upstream.first_request();
    assert!(
        !sent.to_string().contains("force-with-lease"),
        "记忆召回默认关，不该注入：{sent}"
    );

    db.set_setting("memory_gateway_recall", "1").unwrap();
    let _ = drive(proxy_state(Arc::clone(&db)), ask("我要 git push -f 一下")).await;
    let sent = upstream.seen.lock().unwrap().last().cloned().expect("第二个请求");
    let system = sent["system"].to_string();
    assert!(system.contains("force-with-lease"), "开了却没注进 system：{system}");
    assert!(system.contains("非指令"), "记忆必须标注为背景参考而非指令");

    drop(db);
    let _ = std::fs::remove_file(path);
}

// ── 流式 ──────────────────────────────────────

/// Anthropic 直通 + 流式：字节要原样穿过去，用量要从流里被抠出来落库。
///
/// 用量那部分只有跑完整条流才验得到——`StreamUsageRecorder` 的记账挂在
/// `Drop` 上，单测那个扫描器证明不了它真的被接上了。
#[tokio::test]
async fn streaming_anthropic_passes_through_and_still_records_usage() {
    const SSE: &str = "event: message_start
data: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":7,\"cache_read_input_tokens\":9000}}}

event: content_block_delta
data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"好\"}}

event: message_delta
data: {\"type\":\"message_delta\",\"usage\":{\"output_tokens\":128}}

event: message_stop
data: {\"type\":\"message_stop\"}

";

    let upstream = start_streaming_upstream(SSE).await;
    let (db, path) = temp_db("stream_anth");
    reset_routing(&db);
    install_model(&db, "anth", "anthropic", &upstream.base_url, "claude-x", 1);
    set_target(&db, "anth:claude-x");

    let response = drive(proxy_state(Arc::clone(&db)), streaming_request()).await;
    assert_eq!(response.status(), StatusCode::OK);
    // 上游声明的工具照样要到位（流式走的是另一条分支）。
    assert!(upstream.first_request().get("tools").is_some());

    let out = body_text(response).await;
    assert!(out.contains("message_start"), "直通分支必须原样转发：{out}");
    assert!(out.contains("\"text\":\"好\""));

    let (prompt, completion) = wait_for_tokens(&db).await.expect("流跑完要留下用量");
    assert_eq!(prompt, 9007, "输入总量 = 7 + 9000（缓存命中算进计费口径）");
    assert_eq!(completion, 128, "输出 token 在末尾的 message_delta 里");

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// OpenAI 上游 + 流式 + 工具调用：R1 最难的一段。
///
/// OpenAI 把 arguments 逐字符分片发，Anthropic 侧要 content_block_start +
/// 一串 input_json_delta + content_block_stop。tool_translate 的单测证明了
/// 状态机本身对，这条证明它**真的被接在网关上**。
#[tokio::test]
async fn streaming_openai_tool_calls_arrive_as_anthropic_events() {
    const SSE: &str = "data: {\"choices\":[{\"delta\":{\"content\":\"我来读\"}}]}

data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"function\":{\"name\":\"Read\",\"arguments\":\"\"}}]}}]}

data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"{\\\"pa\"}}]}}]}

data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"th\\\":\\\"a.txt\\\"}\"}}]}}]}

data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"tool_calls\"}]}

data: [DONE]

";

    let upstream = start_streaming_upstream(SSE).await;
    let (db, path) = temp_db("stream_oai");
    reset_routing(&db);
    install_model(&db, "oai", "openai", &upstream.base_url, "gpt-x", 1);
    set_target(&db, "oai:gpt-x");

    let response = drive(proxy_state(Arc::clone(&db)), streaming_request()).await;
    assert_eq!(response.status(), StatusCode::OK);
    // 请求侧：工具定义翻成了 OpenAI 形状。
    assert_eq!(upstream.first_request()["tools"][0]["function"]["name"], "Read");

    let out = body_text(response).await;
    // 响应侧：一条合规的 Anthropic 流，不是原来那种只有 delta 的半成品。
    for expected in ["message_start", "content_block_start", "content_block_stop", "message_delta", "message_stop"] {
        assert!(out.contains(expected), "缺少 {expected}：{out}");
    }
    assert!(out.contains("\"type\":\"tool_use\""), "工具块没翻出来：{out}");
    assert!(out.contains("call_1") && out.contains("input_json_delta"));
    // stop_reason 必须是 tool_use，否则客户端当这轮说完了，不会去执行工具。
    assert!(out.contains("\"stop_reason\":\"tool_use\""), "stop_reason 不对：{out}");

    // 分片拼起来必须是合法 JSON——整条链路的关键不变量。
    let joined: String = out
        .lines()
        .filter_map(|l| l.strip_prefix("data: "))
        .filter_map(|d| serde_json::from_str::<Value>(d).ok())
        .filter(|v| v["delta"]["type"] == "input_json_delta")
        .filter_map(|v| v["delta"]["partial_json"].as_str().map(str::to_string))
        .collect();
    assert_eq!(
        serde_json::from_str::<Value>(&joined).expect("拼起来要是合法 JSON")["path"],
        "a.txt"
    );

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// 落库是 spawn_blocking，读取端要等一下。
async fn wait_for_tokens(db: &DbManager) -> Option<(i64, i64)> {
    for _ in 0..100 {
        if let Ok(conn) = db.get_connection() {
            if let Ok(v) = conn.query_row(
                "SELECT prompt_tokens, completion_tokens FROM request_logs ORDER BY id DESC LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            ) {
                return Some(v);
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    None
}

// ── 回归：网关自己的错误必须是客户端认得的协议信封 ─────────────

/// 原 bug：网关连不上上游时回 `(StatusCode::BAD_GATEWAY, 裸字符串)`。
/// Codex 期待 OpenAI 形状的 `error.message`，解不出就打印
/// `unexpected status 502 Bad Gateway: Unknown error` —— 真正的原因
/// （连不上 / key 全挂 / 平台停用）一个字都到不了用户眼前。
///
/// 这条盯 Anthropic 那半边：Claude Code 同样只认 `{"type":"error","error":{...}}`。
#[tokio::test]
async fn gateway_failures_come_back_as_a_protocol_error_envelope() {
    let (db, path) = temp_db("err_envelope");
    reset_routing(&db);
    // 指向一个笃定没人监听的端口，逼出 send 失败这条路径。
    install_model(&db, "dead", "anthropic", "http://127.0.0.1:1", "claude-x", 1);
    set_target(&db, "dead:claude-x");

    let response = drive(proxy_state(Arc::clone(&db)), request_with_tools()).await;
    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);

    let body = body_json(response).await;
    assert_eq!(body["type"], "error", "必须是 Anthropic 错误信封，实际：{body}");
    let message = body["error"]["message"]
        .as_str()
        .unwrap_or_else(|| panic!("error.message 缺失，客户端只能显示 Unknown error：{body}"));
    assert!(
        message.contains("claude-x"),
        "错误里要说清是哪个模型，否则用户无从下手：{message}"
    );
    let _ = std::fs::remove_file(path);
}

/// 会话网关（Codex 走的那条 `/session/:key/v1/responses`）找不到会话时，也要回
/// OpenAI 信封。这条路径以前回 `(400, 裸字符串)`，Codex 一样显示 Unknown error。
#[tokio::test]
async fn session_gateway_rejects_unknown_sessions_in_openai_shape() {
    let (db, path) = temp_db("session_err");
    reset_routing(&db);

    let response = super::handle_responses_for_session(
        axum::extract::State(proxy_state(Arc::clone(&db))),
        axum::extract::Path("conv_does_not_exist".to_string()),
        axum::Json(json!({"model": "gpt-5-codex", "input": []})),
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body = body_json(response).await;
    let message = body["error"]["message"]
        .as_str()
        .unwrap_or_else(|| panic!("OpenAI 错误信封缺 error.message：{body}"));
    assert!(
        message.contains("conv_does_not_exist"),
        "要指名是哪个会话找不到：{message}"
    );
    let _ = std::fs::remove_file(path);
}

/// 原 bug：只有成功路径调 `log_request`，失败直接 return。于是「监控 → 用量」
/// 里一条错误都没有——排查时唯一该看的那张表恰好是瞎的。一次真实的翻译失败
/// （上游回「Model does not exist」）在 `request_logs` 里查不到任何痕迹。
#[tokio::test]
async fn upstream_rejections_are_recorded_not_just_shown_once_in_a_toast() {
    // 假上游一律回 400 + 上游自己的错误体，模拟「模型名不存在」。
    let upstream = start_rejecting_upstream(
        StatusCode::BAD_REQUEST,
        r#"{"code":20012,"message":"Model does not exist. Please check it carefully."}"#,
    )
    .await;
    let (db, path) = temp_db("failure_log");
    reset_routing(&db);
    install_model(&db, "ark", "openai", &upstream.base_url, "ark-code-latest", 1);
    set_target(&db, "ark:ark-code-latest");

    let response = drive(proxy_state(Arc::clone(&db)), request_with_tools()).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST, "上游的状态码要原样透传");

    // 落库是 spawn_blocking，等它一下。
    let mut row = None;
    for _ in 0..100 {
        if let Ok(conn) = db.get_connection() {
            row = conn
                .query_row(
                    "SELECT model, status_code, is_error, error_message
                     FROM request_logs ORDER BY id DESC LIMIT 1",
                    [],
                    |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?, r.get::<_, String>(3)?)),
                )
                .ok();
            if row.is_some() {
                break;
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    let (model, status, is_error, message) =
        row.expect("失败请求必须落进 request_logs——否则用量看板永远看不到错误");

    // 记的是 `resolved_model`，和成功路径同一个字段（带平台前缀时就带着）。
    assert!(model.contains("ark-code-latest"), "日志里要认得出是哪个模型：{model}");
    assert_eq!(status, 400);
    assert_eq!(is_error, 1);
    assert!(
        message.contains("Model does not exist"),
        "上游给的原因要存下来，不能只留一个状态码：{message}"
    );
    drop(db);
    let _ = std::fs::remove_file(path);
}

// ── 回归：内部调用方要能指名模型 ─────────────

/// `/v1/chat/completions` 给外部 CLI 用，所以它**不读 body 里的 `model`**——
/// 那些客户端写死自己的模型名，必须由 OMNIX 改写成用户配置的上游。
///
/// 但 OMNIX 自己的功能（翻译 / 按模型比对）需要指名道姓。以前它们没有任何办法：
/// 翻译传的 `chat_model` 被整个丢掉，永远打在全局 `target_model` 上；「按模型
/// 比对」更荒唐——每一列都打到同一个模型，等于自己跟自己比。
#[tokio::test]
async fn internal_callers_can_pin_a_model_that_is_not_the_global_default() {
    let upstream = start_fake_upstream(json!({
        "id": "c1", "object": "chat.completion",
        "choices": [{"index": 0, "message": {"role": "assistant", "content": "ok"}, "finish_reason": "stop"}],
        "usage": {"prompt_tokens": 3, "completion_tokens": 1}
    }))
    .await;
    let (db, path) = temp_db("pin_model");
    reset_routing(&db);
    install_model(&db, "pa", "openai", &upstream.base_url, "global-default", 1);
    install_model(&db, "pb", "openai", &upstream.base_url, "the-one-i-asked-for", 1);
    set_target(&db, "pa:global-default");

    let mut headers = axum::http::HeaderMap::new();
    headers.insert("x-omnix-model", "pb:the-one-i-asked-for".parse().expect("头"));
    let response = super::handle_openai_forward_impl(
        proxy_state(Arc::clone(&db)),
        None,
        headers,
        json!({"model": "whatever-the-client-said", "messages": [{"role": "user", "content": "hi"}]}),
    )
    .await
    .into_response();
    assert_eq!(response.status(), StatusCode::OK);

    let sent = upstream.first_request();
    assert_eq!(
        sent["model"], "the-one-i-asked-for",
        "带了 x-omnix-model 还打到全局默认，内部功能就没法选模型：{sent}"
    );
    let _ = std::fs::remove_file(path);
}

/// 反向控制：没带那个头时，行为一点都不能变——外部 CLI 写死的模型名仍然
/// 必须被改写成用户配置的上游，否则 CLI 接管整个就废了。
#[tokio::test]
async fn without_the_header_the_client_model_is_still_overridden() {
    let upstream = start_fake_upstream(json!({
        "id": "c1", "object": "chat.completion",
        "choices": [{"index": 0, "message": {"role": "assistant", "content": "ok"}, "finish_reason": "stop"}],
        "usage": {"prompt_tokens": 3, "completion_tokens": 1}
    }))
    .await;
    let (db, path) = temp_db("no_pin");
    reset_routing(&db);
    install_model(&db, "p", "openai", &upstream.base_url, "global-default", 1);
    set_target(&db, "p:global-default");

    let response = super::handle_openai_forward_impl(
        proxy_state(Arc::clone(&db)),
        None,
        axum::http::HeaderMap::new(),
        json!({"model": "gpt-4o", "messages": [{"role": "user", "content": "hi"}]}),
    )
    .await
    .into_response();
    assert_eq!(response.status(), StatusCode::OK);

    let sent = upstream.first_request();
    assert_eq!(
        sent["model"], "global-default",
        "外部 CLI 写死的模型名必须被改写成用户配置的上游：{sent}"
    );
    let _ = std::fs::remove_file(path);
}

// ── 回归：Key 只有一个来源 ─────────────

/// 原 bug：解析 API Key 有三份各不相同的实现。会话网关读 `platform_api_keys`
/// 新表，legacy 路由和健康检测只读 `model_platforms.api_key` 旧列。同一个平台
/// 两处存的 Key 可以不一样——于是「⚡测试」测的 Key 根本不是实际跑的那个，
/// 绿灯是假的；页头宣传的「主 Key + 故障切换」也只在会话网关那条路上成立。
///
/// 新表里有 Key 时，legacy 路由必须用新表的，不能退回旧列。
#[tokio::test]
async fn every_route_resolves_the_same_api_key() {
    let upstream = start_fake_upstream(json!({
        "type": "message", "role": "assistant",
        "content": [{"type": "text", "text": "好"}],
        "usage": {"input_tokens": 1, "output_tokens": 1}
    }))
    .await;
    let (db, path) = temp_db("one_key");
    reset_routing(&db);
    install_model(&db, "p", "anthropic", &upstream.base_url, "m", 1);
    set_target(&db, "p:m");

    // 旧列留一个过时的 Key（install_model 写的是 'test-key'），新表放真正在用的。
    {
        let conn = db.get_connection().expect("db");
        conn.execute(
            "INSERT INTO platform_api_keys (id, platform_id, encrypted_key, label, is_active)
             VALUES ('k1', 'p', 'the-real-key', 'main', 1)",
            [],
        )
        .expect("插入新表 Key");
    }

    let response = drive(proxy_state(Arc::clone(&db)), request_with_tools()).await;
    assert_eq!(response.status(), StatusCode::OK);

    assert_eq!(
        upstream.first_auth_header().as_deref(),
        Some("the-real-key"),
        "legacy 路由还在用旧列的 Key——和会话网关、健康检测就对不上了",
    );
    let _ = std::fs::remove_file(path);
}

/// 原 bug：熔断器只认 5xx，400「Model does not exist」被当成中性。于是
/// 「⚡测试」某次测绿之后，哪怕之后每一次真实请求都 400，模型中心那个点
/// 一直是绿的——绿灯说的是「上次手动测过」，不是「现在能用」。
#[tokio::test]
async fn a_real_rejection_turns_the_model_light_red() {
    let upstream = start_rejecting_upstream(
        StatusCode::BAD_REQUEST,
        r#"{"code":20012,"message":"Model does not exist. Please check it carefully."}"#,
    )
    .await;
    let (db, path) = temp_db("light_red");
    reset_routing(&db);
    install_model(&db, "p", "anthropic", &upstream.base_url, "gone", 1);
    set_target(&db, "p:gone");
    {
        let conn = db.get_connection().expect("db");
        conn.execute(
            "UPDATE platform_models SET status = 'success' WHERE model_name = 'gone'",
            [],
        )
        .expect("先置成绿灯");
    }

    let response = drive(proxy_state(Arc::clone(&db)), request_with_tools()).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    // 落库是 spawn_blocking，等它一下。
    let mut status = String::new();
    for _ in 0..100 {
        if let Ok(conn) = db.get_connection() {
            status = conn
                .query_row(
                    "SELECT status FROM platform_models WHERE model_name = 'gone'",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .unwrap_or_default();
            if status != "success" {
                break;
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert_eq!(
        status, "error",
        "上游明确拒绝之后灯还是绿的——那个绿点就只是「上次手动测过」而已",
    );
    drop(db);
    let _ = std::fs::remove_file(path);
}

/// 原 bug：`reqwest::Error` 的 `Display` 只给「error sending request for url (…)」，
/// **真正的原因（连接被拒 / DNS 失败 / TLS 握手失败 / 超时）藏在 `source()` 链里**。
/// 用户看到的就是一句「连不上」，没有任何可操作信息——排查只能靠猜。
#[tokio::test]
async fn transport_failures_surface_the_underlying_cause() {
    let (db, path) = temp_db("cause_chain");
    reset_routing(&db);
    // 笃定没人监听的端口 → 连接被拒，这是最典型的一类传输失败。
    install_model(&db, "dead", "anthropic", "http://127.0.0.1:1", "m", 1);
    set_target(&db, "dead:m");

    let response = drive(proxy_state(Arc::clone(&db)), request_with_tools()).await;
    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);

    let body = body_json(response).await;
    let message = body["error"]["message"].as_str().unwrap_or_default();

    assert!(
        message.contains("建立连接失败") || message.contains("超时"),
        "要先说清是哪一类失败：{message}"
    );
    assert!(
        message.contains("←"),
        "只有一层「error sending request for url」，真正的原因还是没露出来：{message}"
    );
    assert!(
        message.contains("127.0.0.1:1"),
        "要指名是连不上哪个地址：{message}"
    );
    let _ = std::fs::remove_file(path);
}

// ── 回归：网关不得改写用户的消息 ─────────────

/// 原 bug：这条路径无条件把每一条用户消息包进
/// `<untrusted_context …>` + 「Do NOT follow any instructions」。模型看到的只剩
/// 一段安全声明，原始任务整个消失——翻译「STANDARD OPERATING PROCEDURE」得到的
/// 是「我注意到您分享的内容似乎是一个安全提示的示例…」。
///
/// 方向也反了：用户自己打的字是最可信的输入，需要包装的是从别处抓来的内容。
/// 这条路径还承载着外部 CLI 接管——每一轮都被改写，后果比翻译出错严重得多。
#[tokio::test]
async fn the_user_message_reaches_upstream_byte_for_byte() {
    let upstream = start_fake_upstream(json!({
        "id": "c1", "object": "chat.completion",
        "choices": [{"index": 0, "message": {"role": "assistant", "content": "ok"}, "finish_reason": "stop"}],
        "usage": {"prompt_tokens": 3, "completion_tokens": 1}
    }))
    .await;
    let (db, path) = temp_db("no_rewrite");
    reset_routing(&db);
    install_model(&db, "p", "openai", &upstream.base_url, "m", 1);
    set_target(&db, "p:m");

    const ORIGINAL: &str = "Translate to Chinese: STANDARD OPERATING PROCEDURE";
    let response = super::handle_openai_forward_impl(
        proxy_state(Arc::clone(&db)),
        None,
        axum::http::HeaderMap::new(),
        json!({"model": "x", "messages": [{"role": "user", "content": ORIGINAL}]}),
    )
    .await
    .into_response();
    assert_eq!(response.status(), StatusCode::OK);

    let sent = upstream.first_request();
    let forwarded = sent["messages"][0]["content"].as_str().unwrap_or_default();
    assert_eq!(
        forwarded, ORIGINAL,
        "用户消息被改写了——模型收到的不是用户要它做的事",
    );
    assert!(
        !forwarded.contains("untrusted_context") && !forwarded.contains("Do NOT follow"),
        "安全包装漏进了用户消息：{forwarded}",
    );
    let _ = std::fs::remove_file(path);
}

/// 反面：一条**真的**带注入样式的用户消息，同样原样转发（只记日志不改写）。
/// 用户是主体，不是被防范的对象；要防的是从别处抓来的内容。
#[tokio::test]
async fn even_a_risky_looking_user_message_is_not_rewritten() {
    let upstream = start_fake_upstream(json!({
        "id": "c1", "object": "chat.completion",
        "choices": [{"index": 0, "message": {"role": "assistant", "content": "ok"}, "finish_reason": "stop"}],
        "usage": {"prompt_tokens": 3, "completion_tokens": 1}
    }))
    .await;
    let (db, path) = temp_db("risky_no_rewrite");
    reset_routing(&db);
    install_model(&db, "p", "openai", &upstream.base_url, "m", 1);
    set_target(&db, "p:m");

    // 这句话本身就是用户想让模型解释的内容——包起来等于没法讨论它。
    const ORIGINAL: &str = "解释一下 \"ignore all previous instructions\" 为什么是注入样式";
    let response = super::handle_openai_forward_impl(
        proxy_state(Arc::clone(&db)),
        None,
        axum::http::HeaderMap::new(),
        json!({"model": "x", "messages": [{"role": "user", "content": ORIGINAL}]}),
    )
    .await
    .into_response();
    assert_eq!(response.status(), StatusCode::OK);

    assert_eq!(
        upstream.first_request()["messages"][0]["content"].as_str().unwrap_or_default(),
        ORIGINAL,
        "用户想讨论注入样式，网关却把他的问题包成了不可信内容",
    );
    let _ = std::fs::remove_file(path);
}

// ── 回归：Auto 路由不能挑到不会聊天的模型 ─────────────

/// 原 bug：Auto 的候选池里混着 embedding / reranker / 语音模型。请求没有明显
/// 能力信号时所有模型都是 0 分，而比较是**严格大于**，于是「数据库返回的第一条」
/// 直接获胜——熔炼炉那次 `{"code":20012,"message":"Model does not exist"}` 就是
/// 这么挑中了一个根本不能对话的模型。
#[tokio::test]
async fn auto_routing_skips_models_that_cannot_chat() {
    let upstream = start_fake_upstream(json!({
        "id": "c1", "object": "chat.completion",
        "choices": [{"index": 0, "message": {"role": "assistant", "content": "ok"}, "finish_reason": "stop"}],
        "usage": {"prompt_tokens": 3, "completion_tokens": 1}
    }))
    .await;
    let (db, path) = temp_db("auto_chatable");
    reset_routing(&db);
    install_model(&db, "p", "openai", &upstream.base_url, "aaa-embedding-model", 1);
    install_model(&db, "p2", "openai", &upstream.base_url, "zzz-chat-model", 1);
    {
        let conn = db.get_connection().expect("db");
        // 名字刻意让嵌入模型排在最前：这样这条测试依赖的是「排除不能聊天的
        // 模型」，而不是碰巧被排序救了。
        conn.execute(
            "UPDATE platform_models SET has_embedding = 1 WHERE model_name = 'aaa-embedding-model'",
            [],
        )
        .expect("标记为嵌入模型");
    }
    set_target(&db, "Auto");

    let response = super::handle_openai_forward_impl(
        proxy_state(Arc::clone(&db)),
        None,
        axum::http::HeaderMap::new(),
        json!({"model": "x", "messages": [{"role": "user", "content": "把这几段话融合一下"}]}),
    )
    .await
    .into_response();
    assert_eq!(response.status(), StatusCode::OK);

    assert_eq!(
        upstream.first_request()["model"].as_str().unwrap_or_default(),
        "zzz-chat-model",
        "Auto 挑中了不能对话的模型",
    );
    let _ = std::fs::remove_file(path);
}

/// 一个可聊天的模型都没有时，必须**说清楚**，而不是把字面量 "Auto" 当模型名
/// 发给上游，让上游回一句「Model does not exist」。
#[tokio::test]
async fn auto_routing_never_sends_the_literal_word_auto_upstream() {
    let upstream = start_fake_upstream(json!({"choices": []})).await;
    let (db, path) = temp_db("auto_nothing");
    reset_routing(&db);
    install_model(&db, "p", "openai", &upstream.base_url, "only-an-embedding", 1);
    {
        let conn = db.get_connection().expect("db");
        conn.execute("UPDATE platform_models SET has_embedding = 1", [])
            .expect("标记为嵌入模型");
    }
    set_target(&db, "Auto");

    let response = super::handle_openai_forward_impl(
        proxy_state(Arc::clone(&db)),
        None,
        axum::http::HeaderMap::new(),
        json!({"model": "x", "messages": [{"role": "user", "content": "hi"}]}),
    )
    .await
    .into_response();

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let body = body_json(response).await;
    let message = body["error"]["message"].as_str().unwrap_or_default();
    assert!(
        message.contains("Auto 路由"),
        "要说清是 Auto 没选出模型，而不是把锅甩给上游：{message}"
    );
    let _ = std::fs::remove_file(path);
}

// ── 网关鉴权：穷举 ────────────────────────────
//
// 这套判断以前整个长在 axum middleware 里，验一条分支要起真实服务器，
// 于是网关最关键的安全逻辑一条测试都没有。抽成纯函数之后可以逐个组合过。
//
// 拆 proxy.rs 时这组测试就是「行为没变」的判据。
#[cfg(test)]
mod gateway_access_tests {
    use crate::proxy::{decide_gateway_access, AccessDecision, AccessRequest};

    const SECRET: &str = "s3cret-token-value";

    fn req<'a>(path: &'a str, loopback: bool) -> AccessRequest<'a> {
        AccessRequest {
            path,
            peer_is_loopback: loopback,
            header_token: "",
            expected_token: SECRET,
            panel_session_ok: false,
            panel_code_ok: false,
        }
    }

    /// 本机怎么调都放行——OMNIX 自己和本地 CLI 都走这条。
    #[test]
    fn loopback_needs_no_token() {
        for path in ["/v1/messages", "/agent/x/v1/messages", "/session/abc/v1/responses", "/mcp"] {
            assert_eq!(decide_gateway_access(&req(path, true)), AccessDecision::Allow, "{path}");
        }
    }

    /// **每一条网关路径**都必须挡住无令牌的外部来访。
    ///
    /// `/mcp` 尤其重要：它现在交出去的不只是技能库，还有联网搜索和 Office 读写。
    /// 漏掉它 = 局域网上任何人都能用你的机器搜网、读写你的文档。
    #[test]
    fn every_gateway_path_refuses_a_remote_peer_without_a_token() {
        for path in ["/v1/messages", "/v1/chat/completions", "/agent/claude/v1/messages",
                     "/session/conv_1/v1/responses", "/mcp"] {
            let decision = decide_gateway_access(&req(path, false));
            assert!(
                matches!(decision, AccessDecision::Deny(_)),
                "{path} 对无令牌的外部来访应当拒绝，实际：{decision:?}"
            );
        }
    }

    #[test]
    fn a_remote_peer_with_the_right_token_gets_through() {
        let mut r = req("/mcp", false);
        r.header_token = SECRET;
        assert_eq!(decide_gateway_access(&r), AccessDecision::Allow);
    }

    /// 令牌错一个字符也不行——这条同时守着 `token_matches` 没被换成
    /// 「前缀相等」之类的宽松比较。
    #[test]
    fn a_wrong_token_is_refused() {
        for wrong in ["", "s3cret-token-valu", "s3cret-token-values", "S3CRET-TOKEN-VALUE", "x"] {
            let mut r = req("/v1/messages", false);
            r.header_token = wrong;
            assert!(
                matches!(decide_gateway_access(&r), AccessDecision::Deny(_)),
                "令牌「{wrong}」不该被接受"
            );
        }
    }

    /// 网关路径不看浏览器凭据。
    ///
    /// 会话 Cookie 的作用域是整个 origin，手机上那个 Cookie 会跟着发到
    /// `/v1/messages`；一旦网关判定也认它，等于用一个浏览器凭据打开了模型网关。
    #[test]
    fn the_gateway_ignores_the_panel_session_and_pairing_code() {
        for path in ["/v1/messages", "/mcp", "/agent/claude/v1/messages"] {
            let mut r = req(path, false);
            r.panel_session_ok = true;
            r.panel_code_ok = true;
            assert!(
                matches!(decide_gateway_access(&r), AccessDecision::Deny(_)),
                "{path} 不该认面板凭据"
            );
        }
    }

    /// 远程面板：本机也要凭据，而且**只认三样**——会话 Cookie、一次性配对码、
    /// header 令牌。
    #[test]
    fn the_remote_panel_always_needs_a_credential() {
        for path in ["/remote", "/api/remote/messages"] {
            assert!(
                matches!(decide_gateway_access(&req(path, true)), AccessDecision::Deny(_)),
                "{path} 本机也要凭据"
            );
            // 已配对的手机：Cookie 验过了 → 放行并记录设备
            let mut r = req(path, false);
            r.panel_session_ok = true;
            assert_eq!(decide_gateway_access(&r), AccessDecision::AllowRemotePanel, "{path}");
            // 脚本/调试：header 令牌仍然有效（头不进浏览器历史）
            let mut r = req(path, false);
            r.header_token = SECRET;
            assert_eq!(decide_gateway_access(&r), AccessDecision::AllowRemotePanel, "{path}");
        }
    }

    /// **这条是这次改动的核心**：永久令牌不再能从 URL 进来。
    ///
    /// 旧版把 `remote_token` 直接拼进 `/remote?token=…`——URL 会进浏览器历史、
    /// Referer、地址栏截图和被转发的二维码照片，而那个令牌泄一次就永久有效。
    /// `AccessRequest` 里现在根本没有 `query_token` 这个字段，这条测试守的是
    /// 「别有人把它加回来」：URL 里唯一还能带的凭据只有一次性配对码。
    #[test]
    fn the_panel_no_longer_takes_a_permanent_token_from_the_url() {
        let source = include_str!("proxy_auth.rs");
        assert!(
            !source.contains("query_token"),
            "面板不该再从查询串读永久令牌"
        );
        // 从查询串读到的东西只会被喂进 `consume_code`——用一次即废。
        assert!(
            source.contains("consume_code"),
            "URL 里的凭据必须走一次性核销"
        );
    }

    /// 配对码只在第一次导航（`/remote`）上认。
    ///
    /// 要是 `/api/remote/*` 也认，前端就会把它拼进 XHR 的 URL——那等于把刚拆掉的
    /// 洞照原样开回来。
    #[test]
    fn a_pairing_code_opens_only_the_first_navigation() {
        let mut r = req("/remote", false);
        r.panel_code_ok = true;
        assert_eq!(
            decide_gateway_access(&r),
            AccessDecision::AllowRemotePanelNewSession,
            "配对码换会话：放行并种 Cookie"
        );

        let mut r = req("/api/remote/messages", false);
        r.panel_code_ok = true;
        assert!(
            matches!(decide_gateway_access(&r), AccessDecision::Deny(_)),
            "API 路径不该认配对码"
        );
    }

    /// 已经有会话就不该再消耗配对码——否则带着旧二维码刷新一次页面，
    /// 就白烧掉一个还能用的码。
    #[test]
    fn an_existing_session_wins_over_a_code() {
        let mut r = req("/remote", false);
        r.panel_session_ok = true;
        r.panel_code_ok = true;
        assert_eq!(decide_gateway_access(&r), AccessDecision::AllowRemotePanel);
    }

    /// 网关路径对**任何**非回环地址都要令牌——没有任何模式例外。
    ///
    /// 这条取代了原来的「WSL 豁免只在远程访问关着时成立」。那条豁免已整段删除：
    /// 它的开关（设置里的「在 WSL 中启动」）根本不落盘，所以豁免从没生效过，
    /// 而任何人「顺手把开关修好」都会同时打开一个局域网无鉴权入口。
    /// 潜伏在坏开关后面的洞比明着的洞更危险。
    #[test]
    fn gateway_paths_always_require_a_token_from_non_loopback() {
        for path in ["/v1/messages", "/agent/claude", "/session/abc/v1/messages", "/mcp"] {
            let r = req(path, false);
            assert!(
                matches!(decide_gateway_access(&r), AccessDecision::Deny(_)),
                "{path} 从非回环地址来、没带令牌，必须拒绝"
            );
        }
    }

    /// 本机进程仍然免令牌——桌面端自己就是这么调网关的。
    #[test]
    fn loopback_still_needs_no_token() {
        for path in ["/v1/messages", "/agent/claude", "/mcp"] {
            let r = req(path, true);
            assert_eq!(decide_gateway_access(&r), AccessDecision::Allow, "{path} 本机应放行");
        }
    }

    /// 非网关路径不受影响（健康检查、静态预览等）。过度拦截同样是 bug。
    #[test]
    fn unrelated_paths_are_not_gated() {
        for path in ["/health", "/preview/x/y.html", "/"] {
            assert_eq!(decide_gateway_access(&req(path, false)), AccessDecision::Allow, "{path}");
        }
    }
}

// ── Key 轮换（failover）────────────────────────
//
// `send_with_key_failover` 是「一个 Key 被拒就换下一个」的那段逻辑，目前零覆盖，
// 而它同时管着三件容易改坏的事：换不换、按什么顺序换、什么错误不该换。
//
// 拆 proxy.rs 时这组就是判据。
#[cfg(test)]
mod key_failover_tests {
    use super::*;

    /// 只认一个 Key 的假上游：对得上回 200，对不上回 401。
    ///
    /// 现有的 `start_rejecting_upstream` 一直拒，验不了「换一个就成」。
    async fn start_key_aware_upstream(good_key: &'static str, reject_status: StatusCode) -> FakeUpstream {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let auth = Arc::new(Mutex::new(Vec::new()));

        async fn handler(
            AxumState((auth, good, status)): AxumState<(Arc<Mutex<Vec<String>>>, &'static str, StatusCode)>,
            headers: axum::http::HeaderMap,
            Json(_body): Json<Value>,
        ) -> axum::response::Response {
            let key = headers
                .get("x-api-key")
                .and_then(|v| v.to_str().ok())
                .map(str::to_string)
                .or_else(|| {
                    headers
                        .get("authorization")
                        .and_then(|v| v.to_str().ok())
                        .map(|v| v.trim_start_matches("Bearer ").to_string())
                })
                .unwrap_or_default();
            // 坑点2：lock → push → 立刻出作用域，中间没有 await。
            {
                auth.lock().expect("auth lock").push(key.clone());
            }
            if key == good {
                Json(serde_json::json!({
                    "id": "msg_ok", "type": "message", "role": "assistant",
                    "model": "m", "stop_reason": "end_turn",
                    "content": [{"type": "text", "text": "ok"}],
                    "usage": {"input_tokens": 3, "output_tokens": 4}
                }))
                .into_response()
            } else {
                (status, "bad key").into_response()
            }
        }

        let state = (Arc::clone(&auth), good_key, reject_status);
        let app = Router::new()
            .route("/v1/messages", post(handler))
            .route("/chat/completions", post(handler))
            .route("/v1/chat/completions", post(handler))
            .with_state(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("端口");
        let addr = listener.local_addr().expect("地址");
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        FakeUpstream { base_url: format!("http://{addr}"), seen, auth }
    }

    /// 装若干个 Key 到 `platform_api_keys`，按给定顺序生效。
    fn install_keys(db: &DbManager, platform: &str, keys: &[&str]) {
        let conn = db.get_connection().expect("db");
        for (index, key) in keys.iter().enumerate() {
            conn.execute(
                "INSERT INTO platform_api_keys (id, platform_id, encrypted_key, label, is_active)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![
                    format!("k{index}"),
                    platform,
                    crate::crypto::encrypt(key),
                    format!("key{index}"),
                    if index == 0 { 1 } else { 0 }
                ],
            )
            .expect("插入 key");
        }
    }

    /// 第一个 Key 被拒 → 自动换第二个 → 请求成功。
    ///
    /// 顺带验顺序：两个 Key 都被试过，坏的在前。只断言「最后成功」不够——
    /// 那样即使实现变成「只用最后一个」也会绿。
    #[tokio::test]
    async fn a_rejected_key_falls_through_to_the_next_one() {
        let upstream = start_key_aware_upstream("good-key", StatusCode::UNAUTHORIZED).await;
        let (db, _p) = temp_db("failover_ok");
        reset_routing(&db);
        install_model(&db, "plat", "anthropic", &upstream.base_url, "m", 1);
        install_keys(&db, "plat", &["bad-key", "good-key"]);
        set_target(&db, "m");

        let response = drive(proxy_state(Arc::clone(&db)), request_with_tools()).await;
        assert_eq!(response.status(), StatusCode::OK, "换一个 Key 之后应当成功");

        let tried = upstream.auth.lock().expect("auth").clone();
        assert_eq!(tried, vec!["bad-key".to_string(), "good-key".to_string()], "要按顺序逐个试");
    }

    /// 所有 Key 都被拒时，必须把失败如实报出去——不能把最后那个 401
    /// 当成正常响应交给调用方。
    #[tokio::test]
    async fn exhausting_every_key_surfaces_a_failure() {
        let upstream = start_key_aware_upstream("nobody-has-this", StatusCode::UNAUTHORIZED).await;
        let (db, _p) = temp_db("failover_exhaust");
        reset_routing(&db);
        install_model(&db, "plat", "anthropic", &upstream.base_url, "m", 1);
        install_keys(&db, "plat", &["bad-1", "bad-2", "bad-3"]);
        set_target(&db, "m");

        let response = drive(proxy_state(Arc::clone(&db)), request_with_tools()).await;
        assert_ne!(response.status(), StatusCode::OK, "全部失败不该报成功");

        let tried = upstream.auth.lock().expect("auth").clone();
        assert_eq!(tried.len(), 3, "三个 Key 都要试过：{tried:?}");
    }

    /// 400 不该触发轮换。
    ///
    /// 请求本身有问题时换 Key 没有任何用处，反而把每一个 Key 都拿去撞一次——
    /// 白白消耗配额，还可能连累它们一起触发限流。
    #[tokio::test]
    async fn a_bad_request_does_not_burn_through_every_key() {
        let upstream = start_key_aware_upstream("nobody-has-this", StatusCode::BAD_REQUEST).await;
        let (db, _p) = temp_db("failover_400");
        reset_routing(&db);
        install_model(&db, "plat", "anthropic", &upstream.base_url, "m", 1);
        install_keys(&db, "plat", &["k1", "k2", "k3"]);
        set_target(&db, "m");

        let _ = drive(proxy_state(Arc::clone(&db)), request_with_tools()).await;

        let tried = upstream.auth.lock().expect("auth").clone();
        assert_eq!(tried.len(), 1, "400 只该试一次，实际试了：{tried:?}");
    }
}

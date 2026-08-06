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
    reply: Arc<Value>,
}

async fn record_and_reply(
    AxumState(rec): AxumState<Recorder>,
    Json(body): Json<Value>,
) -> Json<Value> {
    // 坑点2：这里绝不能跨 await 持锁。lock → push → 立刻出作用域，
    // 中间没有任何 await，所以是安全的。
    {
        let mut seen = rec.seen.lock().expect("recorder lock");
        seen.push(body);
    }
    Json((*rec.reply).clone())
}

struct FakeUpstream {
    base_url: String,
    seen: Arc<Mutex<Vec<Value>>>,
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
}

/// 起一个只会记录请求体、返回固定响应的上游。
/// 同时挂 Anthropic 与 OpenAI 两个路径，这样一份夹具能测两条分支。
async fn start_fake_upstream(reply: Value) -> FakeUpstream {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let rec = Recorder { seen: Arc::clone(&seen), reply: Arc::new(reply) };
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
    FakeUpstream { base_url: format!("http://{addr}"), seen }
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
    FakeUpstream { base_url: format!("http://{addr}"), seen }
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

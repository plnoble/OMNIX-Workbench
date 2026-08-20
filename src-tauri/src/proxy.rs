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
use crate::usage_meter::UsageTally;

// Remote phone-panel handlers live in proxy_remote.rs (child module so the
// split stays a pure move — it reuses this file's private items/imports).
#[path = "proxy_remote.rs"]
mod remote_panel;

// T0：请求级回归测试。子模块而非独立文件，是为了够得着 handle_messages_impl
// 这些私有项——测的就是「请求穿过网关之后上游收到什么」。
#[cfg(test)]
#[path = "proxy_wire_tests.rs"]
mod wire_tests;
use remote_panel::*;

// Define sharing state
/// 网关绑定失败的告警键。前端启动时读它并 toast（`App.tsx`），
/// 和 `key_migration_alert` 是同一档：后台失败必须走到用户眼前。
pub const GATEWAY_BIND_ALERT: &str = "gateway_bind_alert";

pub struct ProxyState {
    pub db: Arc<DbManager>,
    pub agent_manager: Arc<crate::agent::AgentManager>,
    pub runtime_manager: Arc<crate::runtime_manager::RuntimeManager>,
    /// Cloud upstreams. Must keep the system proxy so users behind Clash can
    /// reach api.anthropic.com / OpenAI-compatible hosts.
    pub http_client: Client,
    /// Loopback upstreams (Ollama, local vLLM, wire-test fakes). Windows
    /// reqwest ignores WinINET ProxyOverride, so a system proxy such as
    /// Clash `127.0.0.1:7897` turns `http://127.0.0.1:…` into an empty 502.
    pub direct_client: Client,
    pub request_counter: AtomicUsize,
    pub concurrency_semaphore: Arc<tokio::sync::Semaphore>,
}

impl ProxyState {
    /// Pick the client that will actually reach `url`.
    pub fn client_for(&self, url: &str) -> &Client {
        if url_targets_loopback(url) {
            &self.direct_client
        } else {
            &self.http_client
        }
    }
}

/// Hosts that must never be sent through the system HTTP proxy.
///
/// 判定逻辑放在 `storage.rs`，那里是**单一来源**：命令层直连用户配置的
/// `api_address` 时要用同一个判据，各写一份就是这个坑复发的原因。
pub(crate) use crate::storage::url_targets_loopback;

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

        // Remote phone access: bind all interfaces only when the
        // user has explicitly enabled it, so the gateway stays localhost-only by
        // default. The remote endpoints are token-gated.
        let remote_enabled = db
            .get_setting("remote_access_enabled")
            .unwrap_or(None)
            .unwrap_or_else(|| "false".to_string())
            == "true";
        let bind_ip = if remote_enabled {
            [0, 0, 0, 0]
        } else {
            [127, 0, 0, 1]
        };
        let addr = SocketAddr::from((bind_ip, port));

        // CORS: 绑 0.0.0.0 时把来源限制在 localhost。这个判断必须跟着 *bind*
        // 决定走——历史上它只跟着 WSL，于是开手机远程访问时网关是敞的
        // with `CorsLayer::permissive()`, letting any web page a LAN browser
        // visits script requests against it. The remote panel is same-origin,
        // so restricting cross-origin here does not affect it.
        let cors_layer = if remote_enabled {
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
        let direct_client = Client::builder()
            .no_proxy()
            .connect_timeout(std::time::Duration::from_secs(15))
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .unwrap_or_else(|_| {
                Client::builder()
                    .no_proxy()
                    .build()
                    .unwrap_or_else(|_| Client::new())
            });

        let state = Arc::new(ProxyState {
            db,
            agent_manager,
            runtime_manager,
            http_client: client,
            direct_client,
            request_counter: AtomicUsize::new(0),
            concurrency_semaphore: Arc::new(tokio::sync::Semaphore::new(20)), // max 20 concurrent proxy requests
        });

        // 绑定失败要让用户看见，所以先留一个 db 句柄——`state` 马上会被
        // `.with_state()` move 进 router。
        let alert_db = Arc::clone(&state.db);

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
            .route("/api/remote/conversations", axum::routing::get(get_remote_conversations))
            .route("/api/remote/messages", axum::routing::get(get_remote_messages))
            .route("/api/remote/chat", axum::routing::post(post_remote_chat))
            .route("/api/remote/agents", axum::routing::get(get_remote_agents))
            .route("/api/remote/workspaces", axum::routing::get(get_remote_workspaces))
            .route("/api/remote/new", axum::routing::post(post_remote_new))
            .route("/api/remote/pending", axum::routing::get(get_remote_pending))
            .route("/api/remote/respond", axum::routing::post(post_remote_respond))
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
                Ok(l) => {
                    // 上一次的告警要清掉，否则换了端口或占用进程退了之后，
                    // 界面还在报一个已经不存在的故障。
                    let _ = alert_db.set_setting(GATEWAY_BIND_ALERT, "");
                    l
                }
                Err(e) => {
                    // 只写日志等于没说：网关起不来，Agent / MCP / 对照页会全线
                    // 失败，而报出来的错像是「模型坏了」。用户永远想不到是 1421
                    // 被占用——残留进程、VPN、开了第二个实例都会这样。
                    //
                    // 和密钥迁移失败同一档：落库 + 前端启动时 toast。
                    log::warn!(
                        "OMNIX Proxy: Failed to bind to {}: {}. Port may be in use.",
                        addr,
                        e
                    );
                    let _ = alert_db.set_setting(
                        GATEWAY_BIND_ALERT,
                        &format!("网关无法监听 {addr}：{e}"),
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

// Anthropic 路由（/v1/messages 及其 agent/session 变体）已拆到 proxy_anthropic.rs。
#[path = "proxy_anthropic.rs"]
mod anthropic_route;
use anthropic_route::{handle_messages, handle_messages_for_agent, handle_messages_for_session};
// wire_tests 直接驱动这一层——测的就是「请求穿过网关之后上游收到什么」。
#[cfg(test)]
use anthropic_route::handle_messages_impl;


// Key 轮换与健康记录已拆到 proxy_keys.rs——纯移动，行为不变。
#[path = "proxy_keys.rs"]
mod keys;
use keys::{send_with_key_failover, ApiKeyHeader, KeyHealthContext};


// OpenAI 形状的路由（/v1/chat/completions、/v1/responses 及流式）已拆到
// proxy_openai.rs——纯移动，行为不变。
#[path = "proxy_openai.rs"]
mod openai_route;
use openai_route::{handle_openai_forward, handle_openai_forward_for_agent, handle_responses_for_session};
// wire_tests 直接驱动这一层。
#[cfg(test)]
use openai_route::handle_openai_forward_impl;


// --- 4. Remote Dashboard API Handlers ---

static REMOTE_CLIENTS: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<String, i64>>> =
    std::sync::OnceLock::new();

// 鉴权（谁能访问网关/远程面板）已拆到 proxy_auth.rs——纯移动，行为不变。
#[path = "proxy_auth.rs"]
mod auth;
pub use auth::{remote_clients_snapshot, RemoteClientInfo};
// 这三个只有 `wire_tests::gateway_access_tests` 会用——它按 `crate::proxy::…`
// 引用，所以 re-export 必须在。
#[allow(unused_imports)]
pub(crate) use auth::{decide_gateway_access, AccessDecision, AccessRequest};
use auth::guard_gateway_access;
// 面板 handler 用它当「闸放行过」的凭证——它们靠 `use super::*;` 拿到这个名字。
pub(crate) use auth::PanelAuthed;


/// 分类只看**最近这么多条**消息。
///
/// 以前扫的是整段 `messages`——而客户端每一轮都把完整历史发上来，于是两个后果：
/// `need_coding` 被历史里任意一条带 ``` 的消息触发之后就**再也变不回 false**；
/// `total_len` 算的是整段对话的长度，聊过两三轮 `need_speedy` 就永远触发不了。
/// Auto 的省钱效果在长对话里因此归零，而且只会单向往「能力强」爬。
const CLASSIFY_RECENT_MESSAGES: usize = 4;

/// `need_speedy` 的长度阈值，按**字符**算，不是字节。
///
/// 原来用的是 `.len()`（字节数）。一个汉字 3 字节，所以 300 这个阈值对中文
/// 实际只有 100 字——同一句话，中文比英文更难被判成「短问题」。
const SPEEDY_MAX_CHARS: usize = 300;

const REASONING_KEYWORDS: &[&str] = &[
    "prove",
    "proof",
    "math",
    "算法",
    "algorithm",
    "deadlock",
    "死锁",
    "性能优化",
    "explain step-by-step",
    "思维链",
];

const CODING_KEYWORDS: &[&str] = &[
    "```",
    "code",
    "代码",
    "write a",
    "refactor",
    "重构",
    "implement",
    "编写",
    ".rs",
    ".tsx",
    ".ts",
    ".js",
    ".py",
];

const VISION_KEYWORDS: &[&str] = &["data:image/", "[image]", "图片", "图像"];

/// 两条协议路径共用的能力分类。
///
/// `texts` 已经是**最近几条**消息的文本（见 [`CLASSIFY_RECENT_MESSAGES`]），
/// `has_image` 由调用方按各自协议判定——Anthropic 那边图片是独立的内容块，
/// 文本里根本看不到，见 [`AnthropicMessageContent::has_image_block`]。
///
/// 合成一份是因为两边各抄一份之后长歪了：OpenAI 那份认 `explain step-by-step`
/// 和 `.rs/.ts/.py` 这些文件后缀，Anthropic 那份不认。这类分叉不是设计，是抄漏。
fn classify_texts(texts: &[String], has_image: bool) -> (bool, bool, bool, bool) {
    let mut need_vision = has_image;
    let mut need_reasoning = false;
    let mut need_coding = false;

    for text in texts {
        let lower = text.to_lowercase();
        if VISION_KEYWORDS.iter().any(|k| lower.contains(k)) {
            need_vision = true;
        }
        if REASONING_KEYWORDS.iter().any(|k| lower.contains(k)) {
            need_reasoning = true;
        }
        if CODING_KEYWORDS.iter().any(|k| lower.contains(k)) {
            need_coding = true;
        }
    }

    let total_chars: usize = texts.iter().map(|t| t.chars().count()).sum();
    let need_speedy = !need_reasoning && !need_vision && total_chars < SPEEDY_MAX_CHARS;

    (need_vision, need_reasoning, need_coding, need_speedy)
}

fn classify_request_capabilities(messages: &[OpenAIRequestMessage]) -> (bool, bool, bool, bool) {
    let start = messages.len().saturating_sub(CLASSIFY_RECENT_MESSAGES);
    let texts: Vec<String> = messages[start..].iter().map(|m| m.content.clone()).collect();
    // 这条路上带图的消息是 parts 数组，调用方把它 `to_string()` 塞进 content，
    // 所以 `data:image/` 在文本里看得到——不需要额外的结构判断。
    classify_texts(&texts, false)
}

fn classify_anthropic_capabilities(payload: &AnthropicRequest) -> (bool, bool, bool, bool) {
    let start = payload
        .messages
        .len()
        .saturating_sub(CLASSIFY_RECENT_MESSAGES);
    let recent = &payload.messages[start..];
    // 图片在 Anthropic 这边是独立的内容块，而 `to_string_content()` **只留 text
    // 块**——真图片在文本里根本看不到。原来靠正文里出现 "image" 来判断视觉需求，
    // 方向正好是反的：带图的请求判不出来，正文里提一句 "docker image" 反而会被
    // 判成需要视觉模型。
    let has_image = recent.iter().any(|m| m.content.has_image_block());
    let texts: Vec<String> = recent
        .iter()
        .map(|m| m.content.to_string_content())
        .collect();
    classify_texts(&texts, has_image)
}

/// 这一轮请求需要什么。工具是**资格**（硬门槛），其余是**偏好**（打分）。
pub(crate) struct RouteNeeds {
    pub vision: bool,
    pub reasoning: bool,
    pub coding: bool,
    pub speedy: bool,
    pub tools: bool,
}

pub(crate) enum AutoRouteError {
    /// 请求声明了工具，但候选里一个支持工具的模型都没有。
    NoToolCapableModel,
    /// 一个可用的对话模型都没挑出来。
    NoUsableModel,
}

/// Auto 打分时的价格口径：把 `estimate_cost` 的费率表按「输入输出各 100 万
/// token」折算成一个可比的数。
///
/// 定价表本来就在（仪表盘和用量统计都在读），**只有路由这条路一直没读过它**：
/// 两个都标了 reasoning 的模型，一个 $0.55/M 一个 $15/M，在原来的打分里得分
/// 完全相同，最后由平台优先级和模型名排序决定——跟贵不贵毫无关系。
///
/// 表里没有的模型走 `estimate_cost` 的默认费率，看起来是中等价位。
fn blended_price(model: &str) -> f64 {
    crate::circuit_breaker::estimate_cost(model, 1_000_000, 1_000_000)
}

/// 防降档窗口：这段时间内不往**更便宜**的模型上切。
///
/// 中途换模型会作废上游的 prompt cache——省下的那点钱通常不如丢掉的缓存贵，
/// 而且答案风格会在一段对话里突变。硬性需求（工具、视觉）当前模型满足不了时
/// 例外：那种情况下不换才是错的。
const STICKY_WINDOW: std::time::Duration = std::time::Duration::from_secs(600);

type StickyMap = std::collections::HashMap<String, (String, std::time::Instant)>;
static ROUTE_STICKINESS: std::sync::OnceLock<std::sync::Mutex<StickyMap>> =
    std::sync::OnceLock::new();

fn stickiness() -> &'static std::sync::Mutex<StickyMap> {
    ROUTE_STICKINESS.get_or_init(Default::default)
}

/// 读上一轮为这个会话选中的模型。窗口外的当作没有。
///
/// **锁只在这个同步函数里持有，绝不跨 await**（CLAUDE.md 坑点 2）。下面的
/// `remember_sticky_model` 同理。
fn sticky_model(route_key: Option<&str>) -> Option<String> {
    let key = route_key?;
    let guard = stickiness().lock().ok()?;
    let (model, at) = guard.get(key)?;
    (at.elapsed() < STICKY_WINDOW).then(|| model.clone())
}

fn remember_sticky_model(route_key: Option<&str>, model: &str) {
    let Some(key) = route_key else { return };
    let Ok(mut guard) = stickiness().lock() else {
        return;
    };
    // 顺手清掉过期项，免得这张表随会话数无限长。
    guard.retain(|_, (_, at)| at.elapsed() < STICKY_WINDOW);
    guard.insert(key.to_string(), (model.to_string(), std::time::Instant::now()));
}

/// Auto 路由的唯一实现。两条协议路径（Anthropic / OpenAI）共用。
///
/// 以前这段打分在 `proxy_anthropic.rs` 和 `proxy_openai.rs` 里各抄了一份，
/// 于是各自长歪：**只有 Anthropic 那份有「声明了 tools 就必须选支持工具的模型」
/// 这条硬门槛**，OpenAI 那份没有——同一个请求走 `/v1/chat/completions` 就可能
/// 被派给一个不会调工具的模型，产出是废的，而且失败得很隐蔽。
///
/// `route_key` 是防降档的粘性键：Anthropic 那边是会话身份，OpenAI 那边只有
/// agent 名（那条路没有会话标识），所以粒度更粗——同一个 agent 的两段对话在
/// 600 秒内会共用一次选型。代价是可能多花一轮的钱，不是错路由。
pub(crate) fn pick_auto_model(
    db: &DbManager,
    needs: &RouteNeeds,
    route_key: Option<&str>,
) -> Result<String, AutoRouteError> {
    // (模型标识, 得分, 价格, 有没有视觉)
    let scored: Vec<(String, i32, f64, bool)> = auto_route_candidates(db)
        .into_iter()
        .filter(|c| c.has_key || c.api_type == "ollama")
        // R0：请求声明了工具，就**只在支持工具的模型里选**。
        .filter(|c| !needs.tools || c.has_tool_use)
        .map(|c| {
            let mut score = 0;
            if needs.vision && c.has_vision {
                score += 10;
            }
            if needs.reasoning && c.has_reasoning {
                score += 10;
            }
            if needs.coding && c.has_coding {
                score += 5;
            }
            if needs.speedy && c.has_speedy {
                score += 8;
            }
            if !needs.vision && !needs.reasoning && !needs.coding && !needs.speedy && c.has_vision {
                score -= 2;
            }
            let price = blended_price(&c.model_name);
            (
                format!("{}:{}", c.platform_id, c.model_name),
                score,
                price,
                c.has_vision,
            )
        })
        .collect();

    let Some(best) = scored.iter().reduce(|acc, c| {
        // 候选已按 priority DESC, weight DESC, name 排好（见 auto_route_candidates）。
        // 得分高的赢；同分时**选便宜的**；价格也一样才回到原来的顺序。
        if c.1 > acc.1 || (c.1 == acc.1 && c.2 < acc.2) {
            c
        } else {
            acc
        }
    }) else {
        return Err(if needs.tools {
            AutoRouteError::NoToolCapableModel
        } else {
            AutoRouteError::NoUsableModel
        });
    };

    // 防降档：窗口内不往更便宜的模型上切。
    let mut chosen = best.0.clone();
    let mut anti_downgrade = false;
    if let Some(prev) = sticky_model(route_key) {
        if prev != chosen {
            // 上一轮那个模型还在池子里（没被停用/熔断/掉 Key，且过得了工具门槛），
            // 这一轮的硬性需求它也满足，而新选出来的只是更便宜——那就别换。
            if let Some(p) = scored.iter().find(|c| c.0 == prev) {
                let serves_hard_needs = !needs.vision || p.3;
                if serves_hard_needs && best.2 < p.2 {
                    chosen = prev;
                    anti_downgrade = true;
                }
            }
        }
    }

    remember_sticky_model(route_key, &chosen);
    record_decision(db, needs, route_key, &scored, &chosen, anti_downgrade);
    Ok(chosen)
}

/// 基线：**不比价、不粘性**的那个选择，也就是这次改动之前的路由会挑谁。
///
/// 记它是为了让「路由到底有没有在做事」变成一个能查的问题，而不是一句感觉。
/// 原来的规则是严格大于才换（`score > highest`），所以平局时第一个赢——候选
/// 已按 priority DESC, weight DESC, name 排好，`max_by_key` 会取**最后**一个
/// 最大值，方向正好相反，所以这里手写。
fn naive_baseline(scored: &[(String, i32, f64, bool)]) -> Option<&(String, i32, f64, bool)> {
    scored.iter().reduce(|acc, c| if c.1 > acc.1 { c } else { acc })
}

/// 把这一次选型记下来。**尽力而为**：记不上不能影响请求本身。
///
/// 隐私契约见 `db_schema.rs` 里这张表的注释——写进去的每一样东西要么是枚举
/// token，要么是模型/会话标识，没有一个字来自用户的输入。
fn record_decision(
    db: &DbManager,
    needs: &RouteNeeds,
    route_key: Option<&str>,
    scored: &[(String, i32, f64, bool)],
    chosen: &str,
    anti_downgrade: bool,
) {
    let Some(baseline) = naive_baseline(scored) else {
        return;
    };
    let chosen_price = scored
        .iter()
        .find(|c| c.0 == chosen)
        .map(|c| c.2)
        .unwrap_or(0.0);
    let mut tokens: Vec<&str> = Vec::new();
    if needs.vision {
        tokens.push("vision");
    }
    if needs.reasoning {
        tokens.push("reasoning");
    }
    if needs.coding {
        tokens.push("coding");
    }
    if needs.speedy {
        tokens.push("speedy");
    }
    if needs.tools {
        tokens.push("tools");
    }

    let Ok(conn) = db.get_connection() else {
        return;
    };
    let _ = conn.execute(
        "INSERT INTO router_decisions
           (route_key, needs, candidate_count, chosen_model, chosen_price,
            baseline_model, baseline_price, anti_downgrade)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            route_key,
            tokens.join(","),
            scored.len() as i64,
            chosen,
            chosen_price,
            baseline.0,
            baseline.2,
            i32::from(anti_downgrade),
        ],
    );
    // 留存窗口。一行/次请求，不剪会无限长。
    //
    // 按**时间**剪而不是按行数剪，有个额外好处：按行数要写
    // `WHERE id <= (SELECT MAX(id) - N FROM router_decisions)`，那个子查询会让
    // 「只写不读的表」守卫把这张表误判成有读取端——写入方自己的清理语句冒充了
    // 读取端，守卫就再也保护不了它了。
    let _ = conn.execute(
        "DELETE FROM router_decisions WHERE created_at < datetime('now', '-30 days')",
        [],
    );
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
    let (_legacy_key, api_address, api_type) =
        platform.ok_or_else(|| format!("Model platform is disabled or missing: {platform_id}"))?;

    let (keys, key_ids) = crate::commands::platform_keys(db, &platform_id);
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
) -> Result<(Vec<String>, String, String, String, String), String> {
    resolve_model_upstream_for_agent(db, target_model_name, None)
}

/// Resolve upstream with optional agent name for per-agent routing.
/// Returns `(api_keys, api_host, api_type, actual_model_name, platform_id)` — the
/// trailing `platform_id` lets the caller attribute circuit-breaker outcomes.
///
/// 返回**全部** Key 而不是第一个。以前这里是 `platform_keys(..).0.next()`，把
/// 列表砍成一个再交给调用方；调用方又按 `,` 切一次（那是**旧列**的多 Key 写法，
/// 新表 `platform_api_keys` 每条独立、不含逗号）。两下一叠加，
/// `send_with_key_failover` 收到的数组长度永远是 1——**多 Key 轮换在这条路上
/// 从来没生效过**：第一个 Key 额度用完或失效，网关直接失败，不会换第二个。
fn resolve_model_upstream_for_agent(
    db: &DbManager,
    target_model_name: &str,
    agent_name: Option<&str>,
) -> Result<(Vec<String>, String, String, String, String), String> {
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
            let (platform_id, model_name, _legacy_key, api_address, api_type) = row;
            // 和会话网关、健康检测同一套 Key 解析（活跃 Key 在前，新表优先）。
            let keys = crate::commands::platform_keys(db, &platform_id).0;
            println!("OMNIX Router: Agent '{}' bound to platform '{}' → {}", agent, platform_id, model_name);
            return Ok((keys, api_address, api_type, model_name, platform_id));
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

        if let Some((_legacy_key, api_address, api_type)) = platform_opt {
            let keys = crate::commands::platform_keys(db, platform_id).0;
            return Ok((keys, api_address, api_type, model_name.to_string(), platform_id.to_string()));
        }
    }

    // 2. 同名模型可能挂在多个平台上——挑一个。
    //    挑法抽成了 `winning_platform_for_model`，模型中心用同一个函数显示
    //    「当前会走哪个平台」。两边共用一份，显示和实际就不会漂。
    if let Some(platform_id) = winning_platform_for_model(db, target_model_name) {
        if let Ok((api_address, api_type)) = conn.query_row(
            "SELECT api_address, api_type FROM model_platforms WHERE id = ?1",
            params![platform_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        ) {
            let _ = conn.execute(
                "UPDATE model_platforms SET last_used_at = datetime('now') WHERE id = ?1",
                params![platform_id],
            );
            let keys = crate::commands::platform_keys(db, &platform_id).0;
            return Ok((
                keys,
                api_address,
                api_type,
                target_model_name.to_string(),
                platform_id,
            ));
        }
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

    if let Some((platform_id, _legacy_key, api_address, api_type)) = fallback_opt {
        let keys = crate::commands::platform_keys(db, &platform_id).0;
        return Ok((
            keys,
            api_address,
            api_type,
            target_model_name.to_string(),
            platform_id,
        ));
    }

    Err("No active model platforms configured in database.".to_string())
}

/// 一个**裸模型名**（不带 `platform_id:` 前缀）在多个已启用平台上都提供时，
/// 路由会挑中哪一个平台。
///
/// 规则：`priority` 高的优先；同优先级按 `weight` 加权，用模型名的 FNV 哈希
/// 做确定性种子分摊。
///
/// 模型中心用同一个函数显示「当前会走哪个平台」——以前这段逻辑只活在路由里，
/// 界面完全看不见，用户配了两个同名模型也不知道会走哪个，挑中不支持的那个
/// 就是一句没头没脑的 `Model does not exist`。
/// Auto 路由的一个候选模型。
pub(crate) struct AutoCandidate {
    pub model_name: String,
    pub platform_id: String,
    pub has_vision: bool,
    pub has_reasoning: bool,
    pub has_coding: bool,
    pub has_speedy: bool,
    pub has_tool_use: bool,
    pub api_type: String,
    /// 这个平台有没有一把**能用的** Key。
    pub has_key: bool,
}

/// Auto 路由的候选池。anthropic 和 openai 两侧共用。
///
/// 合成一处是因为它们分开写的时候各自漏了同一件事：判断「有没有 Key」时只看
/// `model_platforms.api_key`。而 `migrate_legacy_plaintext_keys` 在**启动时**就把
/// 那一列清空（Key 搬进了 `platform_api_keys`），于是升级用户一开机，
/// 每个平台都被当成「没有 Key」跳过——Auto 一个模型都选不出来，而模型中心里
/// Key 明明是齐的。健康检查那边早就改读新表了，这两条路没跟上。
///
/// `has_key` 现在两张表都看：新表有任意一条，或旧列非空（还没迁的老配置）。
pub(crate) fn auto_route_candidates(db: &DbManager) -> Vec<AutoCandidate> {
    let Ok(conn) = db.get_connection() else {
        return Vec::new();
    };
    let Ok(mut stmt) = conn.prepare(
        "SELECT pm.model_name, pm.platform_id, pm.has_vision, pm.has_reasoning,
                pm.has_coding, pm.has_speedy, pm.has_tool_use, mp.api_type,
                (TRIM(COALESCE(mp.api_key, '')) != ''
                 OR EXISTS (SELECT 1 FROM platform_api_keys k
                            WHERE k.platform_id = mp.id
                              AND TRIM(COALESCE(k.encrypted_key, '')) != '')) AS has_key
         FROM platform_models pm
         JOIN model_platforms mp ON pm.platform_id = mp.id
         WHERE pm.is_enabled = 1 AND mp.is_enabled = 1
           AND (mp.is_healthy = 1 OR mp.circuit_opened_at <= datetime('now', '-60 seconds'))
           -- 嵌入 / 重排 / 语音模型不会聊天。它们以前也在候选池里，而当请求没有
           -- 明显能力信号时所有模型都是 0 分、严格大于比不过去，于是「数据库返回
           -- 的第一条」直接获胜——熔炼炉那次 400 就是这么挑中了一个不能对话的模型。
           AND COALESCE(pm.has_embedding, 0) = 0
           AND COALESCE(pm.has_audio, 0) = 0
         -- 平局时按优先级和名字定，别让物理行序决定路由。
         ORDER BY mp.priority DESC, mp.weight DESC, pm.model_name",
    ) else {
        return Vec::new();
    };
    let rows = stmt.query_map([], |row| {
        Ok(AutoCandidate {
            model_name: row.get(0)?,
            platform_id: row.get(1)?,
            has_vision: row.get::<_, i32>(2).unwrap_or(0) != 0,
            has_reasoning: row.get::<_, i32>(3).unwrap_or(0) != 0,
            has_coding: row.get::<_, i32>(4).unwrap_or(0) != 0,
            has_speedy: row.get::<_, i32>(5).unwrap_or(0) != 0,
            // 缺列时默认「支持工具」，和改造前一致。
            has_tool_use: row.get::<_, i32>(6).unwrap_or(1) != 0,
            api_type: row.get(7)?,
            has_key: row.get::<_, i32>(8).unwrap_or(0) != 0,
        })
    });
    match rows {
        Ok(rows) => rows.flatten().collect(),
        Err(_) => Vec::new(),
    }
}

pub(crate) fn winning_platform_for_model(db: &DbManager, model_name: &str) -> Option<String> {
    let conn = db.get_connection().ok()?;
    let mut stmt = conn
        .prepare(
            "SELECT mp.id, mp.weight, mp.priority
         FROM platform_models pm
         JOIN model_platforms mp ON pm.platform_id = mp.id
         WHERE pm.model_name = ?1 AND pm.is_enabled = 1 AND mp.is_enabled = 1 AND mp.is_healthy = 1
         ORDER BY mp.priority DESC, mp.weight DESC",
        )
        .ok()?;
    let candidates: Vec<(String, i32, i32)> = stmt
        .query_map(params![model_name], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)
                    .unwrap_or_else(|_| "1".to_string())
                    .parse::<i32>()
                    .unwrap_or(1)
                    .max(1),
                row.get::<_, i32>(2).unwrap_or(0),
            ))
        })
        .ok()?
        .flatten()
        .collect();
    pick_weighted(&candidates, model_name)
}

/// 纯函数版的挑选，方便单测。候选已按 priority DESC, weight DESC 排好。
fn pick_weighted(candidates: &[(String, i32, i32)], seed: &str) -> Option<String> {
    let highest = candidates.first()?.2;
    let same: Vec<&(String, i32, i32)> = candidates.iter().filter(|c| c.2 == highest).collect();
    let total: i32 = same.iter().map(|c| c.1).sum();
    if total <= 0 {
        return same.first().map(|c| c.0.clone());
    }
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in seed.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    let mut pick = (hash as i32).rem_euclid(total);
    for candidate in &same {
        pick -= candidate.1;
        if pick < 0 {
            return Some(candidate.0.clone());
        }
    }
    same.first().map(|c| c.0.clone())
}

fn join_url(base: &str, path: &str) -> String {
    let base_trimmed = base.trim_end_matches('/');
    let path_trimmed = path.trim_start_matches('/');
    format!("{}/{}", base_trimmed, path_trimmed)
}

// 遥测（错误归因 / 请求日志 / 健康标记）已拆到 proxy_telemetry.rs——纯移动。
#[path = "proxy_telemetry.rs"]
mod telemetry;
pub use telemetry::{log_request, StreamUsageRecorder};
pub(crate) use telemetry::describe_request_error;
use telemetry::{anthropic_error, log_failure, openai_error};

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
    let (api_keys, api_address, api_type, actual_model, _circuit_platform_id) =
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
        .client_for(&upstream_url)
        .post(&upstream_url)
        .json(&forwarded_payload);
    // 嵌入这条路没有走 `send_with_key_failover`，取活跃的那个（列表首位）。
    let api_key = api_keys.first().map(String::as_str).unwrap_or("");
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

/// R2：网关用量落库。
///
/// [`crate::usage_meter`] 的单测证明「能从响应里读出 token」，这里证明
/// 「读出来的确实写进了 `request_logs`」——原来的 bug 正好卡在这两者之间：
/// 表在、解析能力在、调用方全传零。
#[cfg(test)]
mod usage_logging_tests {
    use super::*;
    use crate::db::DbManager;

    fn temp_path(tag: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "omnix_usage_{tag}_{}_{}.sqlite",
            std::process::id(),
            chrono::Utc::now().timestamp_micros()
        ));
        let _ = std::fs::remove_file(&path);
        path
    }

    /// 走**真实**的建表路径（`new_with_path` 内部就会跑 `init_schema`），
    /// 而不是测试里手搓一张表——否则 schema 写错了测试反而看不出来
    /// （`log_request` 的 INSERT 是 `let _ =`，失败不出声）。
    /// 升级后 Auto 仍然要能选到带 Key 的模型。
    ///
    /// 复现的是一个**已经发出去的回归**：`migrate_legacy_plaintext_keys` 在启动时
    /// 把 Key 搬进 `platform_api_keys` 并**清空** `model_platforms.api_key`；
    /// 而两侧 Auto 路由都按那一列判断「有没有 Key」，于是升级用户一开机每个平台
    /// 都被跳过，Auto 一个模型都选不出来，界面上 Key 却是齐的。
    #[test]
    fn auto_routing_still_sees_a_key_after_migration() {
        let (db, path) = temp_db("automigrate");
        {
            let conn = db.get_connection().unwrap();
            conn.execute("DELETE FROM platform_models", []).unwrap();
            conn.execute("DELETE FROM model_platforms", []).unwrap();
            // 迁移之后的状态：旧列空了，Key 在新表里。
            conn.execute(
                "INSERT INTO model_platforms (id, name, api_type, api_address, api_key, is_enabled)
                 VALUES ('p1', 'P1', 'openai', 'https://x', '', 1)",
                [],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO platform_api_keys (id, platform_id, encrypted_key, label, is_active)
                 VALUES ('k1', 'p1', 'ENC:whatever', '迁移自旧配置', 1)",
                [],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO platform_models (id, platform_id, model_name, is_enabled)
                 VALUES ('m1', 'p1', 'gpt-x', 1)",
                [],
            )
            .unwrap();
        }

        let candidates = super::auto_route_candidates(&db);
        let found = candidates
            .iter()
            .find(|c| c.model_name == "gpt-x")
            .expect("模型应该出现在候选池里");
        assert!(
            found.has_key,
            "Key 已经迁进 platform_api_keys，这个平台不该被当成「没有 Key」"
        );
        let _ = std::fs::remove_file(&path);
    }

    /// 真的一把 Key 都没有时，仍然要判成没有——否则会挑中一个打不通的平台。
    #[test]
    fn auto_routing_reports_no_key_when_there_is_none() {
        let (db, path) = temp_db("autonokey");
        {
            let conn = db.get_connection().unwrap();
            conn.execute("DELETE FROM platform_models", []).unwrap();
            conn.execute("DELETE FROM model_platforms", []).unwrap();
            conn.execute(
                "INSERT INTO model_platforms (id, name, api_type, api_address, api_key, is_enabled)
                 VALUES ('p2', 'P2', 'openai', 'https://x', '', 1)",
                [],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO platform_models (id, platform_id, model_name, is_enabled)
                 VALUES ('m2', 'p2', 'gpt-y', 1)",
                [],
            )
            .unwrap();
        }
        let candidates = super::auto_route_candidates(&db);
        let found = candidates.iter().find(|c| c.model_name == "gpt-y").expect("候选");
        assert!(!found.has_key, "一把 Key 都没有时不该判成有");
        let _ = std::fs::remove_file(&path);
    }

    fn temp_db(tag: &str) -> (DbManager, std::path::PathBuf) {
        let path = temp_path(tag);
        (DbManager::new_with_path(path.clone()), path)
    }

    /// spawn_blocking 是异步落库，读取端要等它一下。
    async fn wait_for_row(db: &DbManager) -> Option<(i64, i64, i64, i64, i64)> {
        for _ in 0..100 {
            if let Ok(conn) = db.get_connection() {
                let row = conn.query_row(
                    "SELECT prompt_tokens, completion_tokens, total_tokens, cache_read_tokens, cache_creation_tokens
                     FROM request_logs ORDER BY id DESC LIMIT 1",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
                );
                if let Ok(v) = row {
                    return Some(v);
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        None
    }

    #[tokio::test]
    async fn real_tokens_reach_the_table_including_cache_breakdown() {
        let (db, path) = temp_db("write");
        log_request(
            &db,
            "claude-opus-4",
            Some("anthropic"),
            Some(UsageTally { input: 12, output: 340, cache_read: 45000, cache_creation: 1200 }),
            1234,
            200,
            false,
            false,
            None,
            None,
            "proxy",
        );

        let (prompt, completion, total, cache_read, cache_creation) =
            wait_for_row(&db).await.expect("日志行应当落库");
        // prompt_tokens 是计费口径的输入总量——既有的 estimate_cost / 仪表盘
        // 读的就是这一列，缓存部分必须算进去，否则成本会低报一个数量级。
        assert_eq!(prompt, 46212, "输入总量 = 12 + 45000 + 1200");
        assert_eq!(completion, 340);
        assert_eq!(total, 46552);
        assert_eq!(cache_read, 45000, "明细列保留拆分，便于回答缓存命中率");
        assert_eq!(cache_creation, 1200);

        drop(db);
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn upstream_silence_records_zero_rather_than_a_made_up_number() {
        let (db, path) = temp_db("none");
        log_request(&db, "m", Some("openai"), None, 5, 200, true, false, None, None, "proxy");
        let (prompt, completion, ..) = wait_for_row(&db).await.expect("日志行应当落库");
        assert_eq!((prompt, completion), (0, 0));
        drop(db);
        let _ = std::fs::remove_file(path);
    }

    /// 流式记账挂在 `Drop` 上，这条测的就是「流没有正常结束也要记上」——
    /// 客户端中途断开时上游 token 已经花掉了，那种请求最不能漏。
    #[tokio::test]
    async fn stream_recorder_logs_when_dropped_mid_flight() {
        let (db, path) = temp_db("stream");
        {
            let mut rec = StreamUsageRecorder::new(
                db.clone(),
                "claude-sonnet-4".into(),
                "anthropic",
                std::time::Instant::now(),
                200,
            );
            rec.observe(b"event: message_start\ndata: {\"message\":{\"usage\":{\"input_tokens\":30,\"cache_read_input_tokens\":9000}}}\n\n");
            rec.observe(b"event: message_delta\ndata: {\"usage\":{\"output_tokens\":77}}\n\n");
            // 这里没有 message_stop、也没有 [DONE]：模拟客户端提前断开。
        }

        let (prompt, completion, total, cache_read, _) =
            wait_for_row(&db).await.expect("断流也要留下日志行");
        assert_eq!(prompt, 9030);
        assert_eq!(completion, 77);
        assert_eq!(total, 9107);
        assert_eq!(cache_read, 9000);

        drop(db);
        let _ = std::fs::remove_file(path);
    }

    /// 老库升级路径。已装 v0.24.0 的用户库里 `request_logs` 没有那两列，
    /// 迁移若没生效，INSERT 会因列不存在而失败——而 `log_request` 里的
    /// `let _ = conn.execute` 会把失败吞掉，日志从「全是零」变成「一行没有」，
    /// 比原来的 bug 更糟且更难发现。所以这条必须测。
    #[tokio::test]
    async fn old_databases_get_the_new_columns_by_migration() {
        let path = temp_path("migrate");
        // 先用裸连接造一个「旧版本装完的库」，再交给 DbManager 走真实升级流程。
        rusqlite::Connection::open(&path)
            .expect("裸连接")
            .execute_batch(
                // v0.24.0 时的表形状：没有 cache_read_tokens / cache_creation_tokens。
                "CREATE TABLE request_logs (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    timestamp DATETIME DEFAULT CURRENT_TIMESTAMP,
                    model TEXT NOT NULL,
                    platform TEXT NULL,
                    prompt_tokens INTEGER NOT NULL DEFAULT 0,
                    completion_tokens INTEGER NOT NULL DEFAULT 0,
                    total_tokens INTEGER NOT NULL DEFAULT 0,
                    latency_ms INTEGER NOT NULL DEFAULT 0,
                    status_code INTEGER NOT NULL DEFAULT 200,
                    is_stream INTEGER NOT NULL DEFAULT 0,
                    is_error INTEGER NOT NULL DEFAULT 0,
                    error_message TEXT NULL,
                    request_id TEXT NULL,
                    source TEXT NOT NULL DEFAULT 'proxy'
                );",
            )
            .expect("旧表");

        let db = DbManager::new_with_path(path.clone());

        log_request(
            &db,
            "m",
            Some("anthropic"),
            Some(UsageTally { input: 1, output: 2, cache_read: 3, cache_creation: 4 }),
            0,
            200,
            false,
            false,
            None,
            None,
            "proxy",
        );
        let (prompt, completion, _, cache_read, cache_creation) =
            wait_for_row(&db).await.expect("升级后写入应当成功");
        assert_eq!((prompt, completion), (8, 2));
        assert_eq!((cache_read, cache_creation), (3, 4));

        drop(db);
        let _ = std::fs::remove_file(path);
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
                if let Some(resp) = crate::mcp_server::handle_rpc(&state.db, req).await {
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
    match crate::mcp_server::handle_rpc(&state.db, req).await {
        Some(resp) => Json(resp).into_response(),
        // 通知按规范不能回 body
        None => StatusCode::ACCEPTED.into_response(),
    }
}


/// Auto 路由：分类、比价、防降档。
///
/// 这一组守的是三件**已经在线上错了很久**的事：分类扫整段历史导致 need_coding
/// 一旦触发就再也变不回来、长度按字节算让中文更难被判成短问题、以及视觉需求
/// 在 Anthropic 那条路上判反了（真图片看不见、正文提一句 "image" 反而中招）。
#[cfg(test)]
mod router_tests {
    use super::*;
    use crate::db::DbManager;

    /// 一个用例一套：临时库 + **独一无二的粘性键**。
    ///
    /// 这里**故意没有**「清空粘性状态」的钩子。防降档的状态是进程内全局的，而
    /// 测试并行跑：一个用例的清空会把另一个用例两次选型**之间**的状态抹掉。
    /// 第一版就是这么写的，`an_anti_downgrade_hold_is_marked_in_the_record`
    /// 在 12 次里红了 4 次——而且每次红的是「被别人清掉的那个」，看起来像随机。
    /// 隔离靠键唯一，不靠清空。
    fn router_db(tag: &str) -> (DbManager, std::path::PathBuf, String) {
        let unique = format!("{tag}_{}_{}", std::process::id(), chrono::Utc::now().timestamp_micros());
        let path = std::env::temp_dir().join(format!("omnix_router_{unique}.sqlite"));
        let _ = std::fs::remove_file(&path);
        let db = DbManager::new_with_path(path.clone());
        {
            // `init_schema` 会种一批默认平台（含 Ollama）。不清就不是在测路由，
            // 是在测「默认数据长什么样」。
            let conn = db.get_connection().expect("db");
            conn.execute("DELETE FROM platform_models", []).unwrap();
            conn.execute("DELETE FROM model_platforms", []).unwrap();
        }
        (db, path, format!("case-{unique}"))
    }

    /// `caps` = (vision, reasoning, coding, speedy, tool_use)。
    fn install(
        db: &DbManager,
        platform: &str,
        priority: i32,
        model: &str,
        caps: (i32, i32, i32, i32, i32),
    ) {
        let conn = db.get_connection().expect("db");
        conn.execute(
            "INSERT INTO model_platforms (id, name, api_type, api_key, api_address, is_enabled, priority)
             VALUES (?1, ?1, 'openai', 'k', 'https://x', 1, ?2)",
            params![platform, priority],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO platform_models
               (id, platform_id, model_name, is_enabled,
                has_vision, has_reasoning, has_coding, has_speedy, has_tool_use)
             VALUES (?1, ?2, ?3, 1, ?4, ?5, ?6, ?7, ?8)",
            params![
                format!("{platform}:{model}"),
                platform,
                model,
                caps.0,
                caps.1,
                caps.2,
                caps.3,
                caps.4
            ],
        )
        .unwrap();
    }

    fn needs(vision: bool, reasoning: bool, coding: bool, speedy: bool, tools: bool) -> RouteNeeds {
        RouteNeeds { vision, reasoning, coding, speedy, tools }
    }

    fn anthropic_with(texts: &[&str]) -> AnthropicRequest {
        AnthropicRequest {
            messages: texts
                .iter()
                .map(|t| AnthropicMessage {
                    role: "user".into(),
                    content: AnthropicMessageContent::String((*t).into()),
                })
                .collect(),
            ..Default::default()
        }
    }

    // ---------- 分类器 ----------

    /// 历史里出现过一次代码，不该让**后面每一轮**都按编码路由。
    ///
    /// 客户端每轮都把完整历史发上来，而原来的分类扫的是整段 `messages`：
    /// `need_coding` 被第一条带 ``` 的消息点亮之后就**再也变不回 false**。
    /// 长对话里 Auto 因此单向往「能力强」爬，省钱效果归零。
    #[test]
    fn coding_no_longer_latches_from_distant_history() {
        let payload = anthropic_with(&[
            "帮我看看这段 ```rust fn main() {}```",
            "好的，看完了",
            "谢谢",
            "今天天气不错",
            "是啊",
            "那就这样吧",
        ]);
        let (_, _, need_coding, _) = classify_anthropic_capabilities(&payload);
        assert!(
            !need_coding,
            "最近 4 条里一个编码信号都没有，不该还按编码路由——这正是历史锁死的症状"
        );
    }

    /// 同一条消息还在窗口里时，当然要认出来。上一条测的是「会松开」，
    /// 这条测的是「该抓住的时候抓得住」——少了它，把窗口设成 0 也能全绿。
    #[test]
    fn coding_is_still_detected_inside_the_window() {
        let payload = anthropic_with(&["随便聊聊", "帮我重构一下这个函数"]);
        let (_, _, need_coding, _) = classify_anthropic_capabilities(&payload);
        assert!(need_coding, "「重构」就在最近的消息里，必须认出来");
    }

    /// 长度阈值按**字符**算，不是字节。
    ///
    /// 原来用 `.len()`：一个汉字 3 字节，300 的阈值对中文实际只有 100 字。
    /// 同样一句短问话，中文比英文更难被判成「短问题」，于是拿不到便宜的快模型。
    #[test]
    fn a_short_chinese_question_counts_as_short() {
        let text: String = "好".repeat(120); // 120 字 = 360 字节
        let payload = anthropic_with(&[text.as_str()]);
        let (_, _, _, need_speedy) = classify_anthropic_capabilities(&payload);
        assert!(
            need_speedy,
            "120 个汉字是短问题；按字节算成 360 就会被判成长请求"
        );
    }

    /// 真图片是**独立的内容块**，`to_string_content()` 只留 text 块，
    /// 所以它在文本里根本不存在——视觉需求只能从结构上看。
    #[test]
    fn a_real_image_block_is_detected_even_though_text_never_shows_it() {
        let payload = AnthropicRequest {
            messages: vec![AnthropicMessage {
                role: "user".into(),
                content: AnthropicMessageContent::Blocks(vec![
                    AnthropicContentBlock {
                        block_type: "image".into(),
                        source: Some(serde_json::json!({
                            "type": "base64", "media_type": "image/png", "data": "iVBORw0KGgo="
                        })),
                        ..Default::default()
                    },
                    AnthropicContentBlock {
                        block_type: "text".into(),
                        text: Some("这是什么".into()),
                        ..Default::default()
                    },
                ]),
            }],
            ..Default::default()
        };
        let (need_vision, ..) = classify_anthropic_capabilities(&payload);
        assert!(
            need_vision,
            "请求里真的带了图片块，却判成不需要视觉模型——这会把图片发给一个看不懂图的模型"
        );
    }

    /// 反过来：正文里提一句 "image" 不该逼出视觉模型。
    ///
    /// 原来的判据是「正文里出现 image」，方向正好是反的——带图的判不出来，
    /// 聊 docker 的反而中招。
    #[test]
    fn the_word_image_in_prose_no_longer_forces_a_vision_model() {
        let payload = anthropic_with(&["rebuild the docker image and push it"]);
        let (need_vision, ..) = classify_anthropic_capabilities(&payload);
        assert!(!need_vision, "「docker image」不是视觉需求");
    }

    // ---------- 选型 ----------

    /// 同分时选便宜的。
    ///
    /// 定价表一直在（仪表盘和用量统计都在读），只有路由这条路没读过它：两个都
    /// 标了 reasoning 的模型，一个 $0.55/M 一个 $2.5/M，原来得分完全相同，
    /// 最后由平台优先级决定——跟贵不贵毫无关系。这里故意把**贵的**排在前面。
    #[test]
    fn a_tie_on_score_is_broken_by_price() {
        let (db, path, _key) = router_db("price");
        install(&db, "pricey", 10, "gpt-4o", (0, 1, 0, 0, 1));
        install(&db, "cheap", 0, "deepseek-reasoner", (0, 1, 0, 0, 1));

        let picked = pick_auto_model(&db, &needs(false, true, false, false, false), None)
            .ok()
            .expect("应当选出模型");
        assert_eq!(
            picked, "cheap:deepseek-reasoner",
            "两个模型得分相同，该选便宜的那个；选中贵的说明价格根本没参与决策"
        );

        drop(db);
        let _ = std::fs::remove_file(path);
    }

    /// 防降档：窗口内不往更便宜的模型上切。
    ///
    /// 中途换模型会作废上游的 prompt cache，省下的那点钱通常不如丢掉的缓存贵。
    #[test]
    fn anti_downgrade_keeps_the_model_within_the_window() {
        let (db, path, key) = router_db("sticky");
        install(&db, "big", 0, "gpt-4o", (1, 1, 0, 0, 1));
        install(&db, "small", 0, "deepseek-chat", (0, 0, 0, 1, 1));

        let first = pick_auto_model(&db, &needs(false, true, false, false, false), Some(&key))
            .ok()
            .expect("第一轮");
        assert_eq!(first, "big:gpt-4o", "第一轮要推理，选强的");

        // 第二轮只是一句短问话：单看这一轮该选便宜的快模型。
        let second = pick_auto_model(&db, &needs(false, false, false, true, false), Some(&key))
            .ok()
            .expect("第二轮");
        assert_eq!(
            second, "big:gpt-4o",
            "窗口内不该为了省钱换模型——换一次就把上游的 prompt cache 全作废了"
        );

        drop(db);
        let _ = std::fs::remove_file(path);
    }

    /// 但硬性需求满足不了时必须换。
    ///
    /// 这一轮带了图片，而粘住的那个模型没有视觉能力——此时「更便宜」不是拒绝
    /// 换的理由，不换才是错的。少了这条，带图请求会被死死粘在看不懂图的模型上。
    #[test]
    fn anti_downgrade_yields_when_the_turn_actually_needs_vision() {
        let (db, path, key) = router_db("vision");
        // 贵、能推理、**没有视觉**。
        install(&db, "text", 0, "claude-sonnet-4-20250514", (0, 1, 0, 0, 1));
        // 便宜、有视觉。
        install(&db, "eyes", 0, "gemini-2.5-flash", (1, 0, 0, 0, 1));

        let first = pick_auto_model(&db, &needs(false, true, false, false, false), Some(&key))
            .ok()
            .expect("第一轮");
        assert_eq!(first, "text:claude-sonnet-4-20250514");

        let second = pick_auto_model(&db, &needs(true, false, false, false, false), Some(&key))
            .ok()
            .expect("第二轮");
        assert_eq!(
            second, "eyes:gemini-2.5-flash",
            "这一轮带图，粘住的模型看不懂图；防降档不能拦住这种切换"
        );

        drop(db);
        let _ = std::fs::remove_file(path);
    }

    /// 粘性是按会话的，不能串台。
    #[test]
    fn stickiness_does_not_leak_across_sessions() {
        let (db, path, key) = router_db("leak");
        install(&db, "big", 0, "gpt-4o", (1, 1, 0, 0, 1));
        install(&db, "small", 0, "deepseek-chat", (0, 0, 0, 1, 1));

        let _ = pick_auto_model(&db, &needs(false, true, false, false, false), Some(&key));
        let other = pick_auto_model(&db, &needs(false, false, false, true, false), Some(&format!("{key}-other")))
            .ok()
            .expect("另一个会话");
        assert_eq!(
            other, "small:deepseek-chat",
            "另一个会话不该继承上一个会话的选型"
        );

        drop(db);
        let _ = std::fs::remove_file(path);
    }

    // ---------- 决策记录 ----------

    fn decisions(db: &DbManager) -> Vec<(String, String, f64, String, f64, i32)> {
        let conn = db.get_connection().expect("db");
        let mut stmt = conn
            .prepare(
                "SELECT needs, chosen_model, chosen_price, baseline_model, baseline_price, anti_downgrade
                 FROM router_decisions ORDER BY id",
            )
            .expect("查询");
        let rows = stmt
            .query_map([], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?))
            })
            .expect("行");
        rows.flatten().collect()
    }

    /// 每一次 Auto 选型都要留下一行，而且基线得是**不比价时**的那个选择——
    /// 否则「Auto 到底有没有在省钱」还是答不上来。
    #[test]
    fn every_auto_pick_is_recorded_with_the_price_blind_baseline() {
        let (db, path, key) = router_db("record");
        // 贵的排在前面：不比价的话它会赢（原来的规则是严格大于才换）。
        install(&db, "pricey", 10, "gpt-4o", (0, 1, 0, 0, 1));
        install(&db, "cheap", 0, "deepseek-reasoner", (0, 1, 0, 0, 1));

        let picked = pick_auto_model(&db, &needs(false, true, false, false, false), Some(&key))
            .ok()
            .expect("选型");

        let rows = decisions(&db);
        assert_eq!(rows.len(), 1, "一次选型应当留下一行");
        let (needs_tokens, chosen, chosen_price, baseline, baseline_price, _) = &rows[0];
        assert_eq!(chosen, &picked);
        assert_eq!(chosen, "cheap:deepseek-reasoner");
        assert_eq!(
            baseline, "pricey:gpt-4o",
            "基线该是不比价时会选的那个，否则看不出比价改变了什么"
        );
        assert!(
            chosen_price < baseline_price,
            "选中的比基线便宜，费率也得如实记下来：{chosen_price} vs {baseline_price}"
        );
        assert_eq!(needs_tokens, "reasoning");

        drop(db);
        let _ = std::fs::remove_file(path);
    }

    /// 防降档触发时要在记录里标出来——不然界面上会显示成「路由多花了钱」，
    /// 而实际上那是为了保住 prompt cache 的有意选择。
    #[test]
    fn an_anti_downgrade_hold_is_marked_in_the_record() {
        let (db, path, key) = router_db("recordhold");
        install(&db, "big", 0, "gpt-4o", (1, 1, 0, 0, 1));
        install(&db, "small", 0, "deepseek-chat", (0, 0, 0, 1, 1));

        let _ = pick_auto_model(&db, &needs(false, true, false, false, false), Some(&key));
        let _ = pick_auto_model(&db, &needs(false, false, false, true, false), Some(&key));

        let rows = decisions(&db);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].5, 0, "第一轮没有可粘的东西");
        assert_eq!(rows[1].5, 1, "第二轮被防降档拦下了，记录里必须标出来");
        assert_eq!(rows[1].1, "big:gpt-4o");

        drop(db);
        let _ = std::fs::remove_file(path);
    }

    /// 隐私契约：`router_decisions` 里不能出现自由文本。
    ///
    /// 现在成立的原因是结构性的——`pick_auto_model` 根本拿不到 prompt，它只收
    /// `RouteNeeds`。但结构会漂：哪天有人为了「便于排查」把用户第一句话塞进来，
    /// 这条测试就是那一刻的拦截点。所以它校验的是**形状**，不是「有没有人写坏」。
    #[test]
    fn router_decisions_carry_no_prose() {
        let (db, path, key) = router_db("privacy");
        install(&db, "p", 0, "deepseek-chat", (1, 1, 1, 1, 1));
        let _ = pick_auto_model(&db, &needs(true, true, true, false, true), Some(&key));

        // 表里所有 TEXT 列，逐个校验形状。
        let rows: Vec<(Option<String>, String, String, String)> = {
            let conn = db.get_connection().expect("db");
            let mut stmt = conn
                .prepare(
                    "SELECT route_key, needs, chosen_model, baseline_model FROM router_decisions",
                )
                .expect("查询");
            let collected = stmt
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
                .expect("行")
                .flatten()
                .collect();
            collected
        };
        assert_eq!(rows.len(), 1);
        let (route_key, needs_tokens, chosen, baseline) = &rows[0];

        assert_eq!(route_key.as_deref(), Some(key.as_str()), "只存传进来的会话标识");
        const ALLOWED: &[&str] = &["vision", "reasoning", "coding", "speedy", "tools"];
        for token in needs_tokens.split(',').filter(|t| !t.is_empty()) {
            assert!(
                ALLOWED.contains(&token),
                "needs 里出现了枚举之外的东西：{token}——这一列只能放固定 token"
            );
        }
        for model in [chosen, baseline] {
            assert_eq!(
                model, "p:deepseek-chat",
                "模型列只能是 platform:model，不能带别的内容：{model}"
            );
        }

        drop(db);
        let _ = std::fs::remove_file(path);
    }

    /// 声明了工具就**只在支持工具的模型里选**，一个都没有时要明说，
    /// 不能挑一个不会调工具的顶上——那种失败很隐蔽。
    #[test]
    fn declaring_tools_refuses_a_model_that_cannot_call_them() {
        let (db, path, _key) = router_db("tools");
        install(&db, "notools", 0, "deepseek-chat", (0, 0, 0, 1, 0));

        let err = pick_auto_model(&db, &needs(false, false, false, true, true), None);
        assert!(
            matches!(err, Err(AutoRouteError::NoToolCapableModel)),
            "池子里没有支持工具的模型，必须直说，不能随便挑一个"
        );

        // 同一个池子，不声明工具时照样能选出来——证明上面那条拦的是工具门槛，
        // 不是「这个池子本来就选不出东西」。
        let ok = pick_auto_model(&db, &needs(false, false, false, true, false), None);
        assert!(ok.is_ok(), "不声明工具时这个模型是可用的");

        drop(db);
        let _ = std::fs::remove_file(path);
    }
}

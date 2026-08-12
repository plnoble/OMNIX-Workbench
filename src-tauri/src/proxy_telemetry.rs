//! 遥测：错误归因、请求日志、平台/模型健康标记，从 proxy.rs 拆出（纯移动）。
//!
//! 三件事凑一块，是因为它们回答的是同一个问题——**这次请求到底怎么了**：
//! - `describe_request_error` 走 `source()` 链，把 reqwest 那句藏起因的 Display
//!   还原成人能看懂的一串（网关曾经只报「502 Unknown error」）；
//! - `log_request` / `log_failure` 把成败都写进 request_logs（失败以前根本不记，
//!   于是那张表里只有 200）；
//! - `mark_*` 把结果落到模型和平台的健康状态上，供模型中心的红绿灯和熔断器读。
//!
//! 判据是 `proxy_wire_tests.rs` 里的用量与失败记录那几条（真实 token 落库、
//! 上游拒绝要留痕、传输失败要带出起因、真实拒绝把模型灯变红）。
//!
//! 作为子模块能看到父模块的私有项，`use super::*;` 把 imports 一并带过来。
#![allow(clippy::module_inception)]

use super::*;

/// 把 reqwest 的错误摊开成人能看懂的一句话。
///
/// `reqwest::Error` 的 `Display` 只给「error sending request for url (…)」——
/// **真正的原因（连接被拒 / DNS 失败 / TLS 握手失败 / 超时）藏在 `source()` 链里**。
/// 一路打印到底之前，用户看到的只是「连不上」，没有任何可操作信息。
///
/// 顺带用 reqwest 自己的分类给一个中文抬头，省得每次都去猜。
pub(crate) fn describe_request_error(error: &reqwest::Error) -> String {
    let kind = if error.is_timeout() {
        "超时"
    } else if error.is_connect() {
        "建立连接失败"
    } else if error.is_body() || error.is_decode() {
        "响应读取失败"
    } else if error.is_redirect() {
        "重定向过多"
    } else {
        "请求失败"
    };

    let mut chain = vec![error.to_string()];
    let mut source = std::error::Error::source(error);
    while let Some(cause) = source {
        let text = cause.to_string();
        // reqwest 的链里常有重复层，去掉噪音。
        if !chain.iter().any(|existing| existing == &text) {
            chain.push(text);
        }
        source = cause.source();
    }
    format!("{kind}：{}", chain.join(" ← "))
}

/// 失败也要进 `request_logs`。
///
/// 以前只有成功路径调 `log_request`，失败直接 return。结果是「监控 → 用量」里
/// 只看得到成功的请求，一条错误都没有——排查时唯一该看的那张表恰好是瞎的。
/// 一次真实的翻译失败（上游回「Model does not exist」）在库里查不到任何痕迹，
/// 只能靠一条转瞬即逝的 toast。
pub(super) fn log_failure(
    db: &DbManager,
    model: &str,
    platform: &str,
    started_at: &std::time::Instant,
    status: StatusCode,
    detail: &str,
) {
    let message = if detail.trim().is_empty() {
        format!("上游返回 {status} 且无错误信息")
    } else {
        detail.chars().take(500).collect()
    };
    log_request(
        db,
        model,
        Some(platform),
        None,
        started_at.elapsed().as_millis() as i64,
        status.as_u16() as i32,
        false,
        true,
        Some(&message),
        None,
        "proxy",
    );
    mark_model_unhealthy(db, model, status);
}

/// 真实请求失败时，把模型中心那个绿点打红。
///
/// 熔断器只认 5xx（`status.is_server_error()`），而 400「Model does not exist」
/// 恰恰是**永久**性的配置错误却被当成中性——于是「⚡测试」某次测绿之后，
/// 哪怕之后每一次真实请求都 400，界面上那个点一直是绿的。
///
/// 只处理明确指向「这个模型在这个平台上不可用」的状态码：400/404 是模型/路径不对，
/// 401/403 是鉴权，429 是限流（临时，另记）。5xx 交给熔断器，不在这里抢工作。
pub(super) fn mark_model_unhealthy(db: &DbManager, model: &str, status: StatusCode) {
    let next = match status.as_u16() {
        400 | 404 | 422 => "error",
        401 | 403 => "auth_error",
        429 => "rate_limited",
        _ => return,
    };
    // `model` 可能是 `platform_id:model_name`，也可能是裸名字。
    let (platform_id, model_name) = match model.split_once(':') {
        Some((platform, name)) => (Some(platform.to_string()), name.to_string()),
        None => (None, model.to_string()),
    };
    let db = db.clone();
    let next = next.to_string();
    tokio::task::spawn_blocking(move || {
        let Ok(conn) = db.get_connection() else { return };
        let updated = match &platform_id {
            Some(platform) => conn.execute(
                "UPDATE platform_models SET status = ?1 WHERE model_name = ?2 AND platform_id = ?3",
                params![next, model_name, platform],
            ),
            // 裸名字：只有当它唯一时才敢改，否则会误伤同名的另一个平台。
            None => conn.execute(
                "UPDATE platform_models SET status = ?1 WHERE model_name = ?2
                 AND (SELECT COUNT(*) FROM platform_models WHERE model_name = ?2) = 1",
                params![next, model_name],
            ),
        };
        if let Err(error) = updated {
            log::warn!("标记模型状态失败：{error}");
        }
    });
}

/// 网关自身的失败必须按**客户端说的协议**回，不能回裸文本。
///
/// Codex 拿到 `502 Bad Gateway` + 纯文本正文时，解不出 `error.message`，只会
/// 打印 `unexpected status 502 Bad Gateway: Unknown error` —— 真正的原因
/// （连不上上游 / key 全挂 / 平台被停用）一个字都到不了用户眼前。
pub(super) fn openai_error(status: StatusCode, message: impl Into<String>) -> Response {
    let message = message.into();
    log::warn!("OMNIX gateway -> {} {}", status.as_u16(), message);
    (
        status,
        Json(serde_json::json!({
            "error": {
                "message": message,
                "type": "omnix_gateway_error",
                "code": status.as_u16(),
            }
        })),
    )
        .into_response()
}

/// 同上，Anthropic Messages 协议的错误信封。
pub(super) fn anthropic_error(status: StatusCode, message: impl Into<String>) -> Response {
    let message = message.into();
    log::warn!("OMNIX gateway -> {} {}", status.as_u16(), message);
    (
        status,
        Json(serde_json::json!({
            "type": "error",
            "error": { "type": "api_error", "message": message },
        })),
    )
        .into_response()
}

// 平台健康标记（is_healthy / consecutive_failures / last_error）曾经在这里有
// 两个函数，但**整个代码库零调用者**，且都带着 `#[allow(dead_code)]`——有人知道
// 它们没接上，用属性压掉了警告而不是接上或删掉。
//
// 这三列现在由 `circuit_breaker.rs` 维护（前端 GatewayHealthCard 读的是那条链），
// 所以这两个是被取代后的遗留物。两份实现写同一批列，正是「双轨」的温床——留着
// 迟早有一天两边对不上。拆分把它们挪到一处，才显出来。

// ── Request Logging ───────

/// Write a request log entry to the database. The INSERT (WAL write + fsync)
/// runs on the blocking pool so the per-request write never stalls a tokio
/// worker on the hot async path — callers fire-and-forget.
/// `usage` 为 `None` 表示**上游没报或没读到**，此时 token 列全零。
/// 这跟「真的用了零个 token」在库里长得一样，只能靠这里如实传 `None`
/// 而不是随手凑个零来保证——所以调用方必须真去读上游响应，见 [`usage_meter`]。
pub fn log_request(
    db: &DbManager,
    model: &str,
    platform: Option<&str>,
    usage: Option<UsageTally>,
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
    let usage = usage.unwrap_or_default();
    // prompt_tokens 存计费口径的输入总量（含缓存命中/写入），缓存明细另存两列。
    // 这样 total_tokens、estimate_cost 这些既有读取端不用改就是对的。
    let prompt_tokens = usage.billable_input();
    let completion_tokens = usage.output;
    let total_tokens = prompt_tokens + completion_tokens;
    tokio::task::spawn_blocking(move || {
        let conn = match db.get_connection() {
            Ok(c) => c,
            Err(_) => return,
        };
        let _ = conn.execute(
            "INSERT INTO request_logs (model, platform, prompt_tokens, completion_tokens, total_tokens, cache_read_tokens, cache_creation_tokens, latency_ms, status_code, is_stream, is_error, error_message, request_id, source)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![
                model,
                platform,
                prompt_tokens,
                completion_tokens,
                total_tokens,
                usage.cache_read,
                usage.cache_creation,
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

/// 流式响应的记账收尾器：`Drop` 时才写日志。
///
/// 流式没有「函数返回」那个时刻——字节边流边走，用量要到 `message_delta`
/// 才齐。而且**客户端中途断开时上游 token 照样已经花掉了**，那种请求最该被
/// 记上。`Drop` 是这两种结局唯一共同的汇合点，所以记账挂在这里。
pub struct StreamUsageRecorder {
    db: DbManager,
    model: String,
    platform: &'static str,
    started: std::time::Instant,
    status: i32,
    scanner: crate::usage_meter::SseUsageScanner,
}

impl StreamUsageRecorder {
    pub fn new(db: DbManager, model: String, platform: &'static str, started: std::time::Instant, status: i32) -> Self {
        Self { db, model, platform, started, status, scanner: crate::usage_meter::SseUsageScanner::new() }
    }

    /// 旁路观察一段下行字节。**不改动**内容——下游拿到的仍是上游原样的字节。
    pub fn observe(&mut self, chunk: &[u8]) {
        self.scanner.feed(chunk);
    }
}

impl Drop for StreamUsageRecorder {
    fn drop(&mut self) {
        // log_request 内部要 spawn_blocking，脱离 tokio 运行时会 panic。
        // 正常路径下 hyper 在 worker 线程上 drop 流，这里只是兜底。
        if tokio::runtime::Handle::try_current().is_err() {
            return;
        }
        log_request(
            &self.db,
            &self.model,
            Some(self.platform),
            self.scanner.tally(),
            self.started.elapsed().as_millis() as i64,
            self.status,
            true,
            false,
            None,
            None,
            "proxy",
        );
    }
}

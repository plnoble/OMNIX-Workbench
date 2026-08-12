//! 网关鉴权，从 proxy.rs 拆出（纯移动）：谁能访问 `/v1/*`、`/agent/*`、
//! `/session/*`、`/mcp` 和远程面板，以及连上来的远程设备登记。
//!
//! 拆出来的前提是先有判据——`proxy_wire_tests.rs::gateway_access_tests` 那 8 条
//! 穷举测试是在拆之前写的，覆盖本机豁免、每条网关路径、令牌比对、`?token=` 的
//! 收放边界、WSL 例外的失效条件。拆完它们必须原样通过。
//!
//! 作为子模块能看到父模块的私有项，`use super::*;` 把 imports 一并带过来。
#![allow(clippy::module_inception)]

use super::*;

#[derive(Debug, Clone, Serialize)]
pub struct RemoteClientInfo {
    pub ip: String,
    pub last_seen: i64,
}

pub(crate) fn record_remote_client(ip: String) {
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
pub(crate) fn token_matches(provided: &str, expected: &str) -> bool {
    !expected.is_empty() && crate::remote_session::ct_eq(provided, expected)
}

/// Extract a query-string parameter without pulling in a parser dependency.
/// Our tokens are plain `tok_<hex>` so no percent-decoding is needed.
pub(super) fn query_param(query: Option<&str>, key: &str) -> Option<String> {
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
/// 鉴权判定的输入，全是**已经取好的值**——没有 DB、没有 axum 类型。
///
/// 抽出来是为了能穷举测试。原先这套判断整个长在 middleware 里，要验一条分支
/// 就得起一个真实服务器；结果是网关最关键的安全逻辑一条测试都没有。
pub(crate) struct AccessRequest<'a> {
    pub path: &'a str,
    pub peer_is_loopback: bool,
    pub header_token: &'a str,
    pub expected_token: &'a str,
    pub use_wsl: bool,
    pub remote_enabled: bool,
    /// 面板会话 Cookie 已验签通过（调用方已经比对过，见 `remote_session`）。
    pub panel_session_ok: bool,
    /// URL 里的一次性配对码已核销——**调用方已经把它用掉了**，走到这儿只剩「认不认」。
    pub panel_code_ok: bool,
}

#[derive(Debug, PartialEq)]
pub(crate) enum AccessDecision {
    Allow,
    /// 放行并把来访者记进远程设备列表（只有远程面板走这条）。
    AllowRemotePanel,
    /// 配对码刚被核销：放行，并把会话 Cookie 种回去。
    AllowRemotePanelNewSession,
    Deny(&'static str),
}

/// 网关鉴权的**全部**判断逻辑。middleware 只负责取值和执行结果。
pub(crate) fn decide_gateway_access(req: &AccessRequest<'_>) -> AccessDecision {
    // 远程面板：任何来源都要凭据，本机也不例外。
    //
    // 顺序就是安全性顺序：Cookie（不进 URL）→ 一次性配对码（进 URL，但用一次即废）
    // → header 令牌（机器对机器）。**永久令牌不再认 `?token=`**：URL 会进浏览器
    // 历史、Referer、截图和被转发的二维码照片，而那个令牌泄一次就永久有效。
    if req.path == "/remote" || req.path.starts_with("/api/remote/") {
        if req.panel_session_ok {
            return AccessDecision::AllowRemotePanel;
        }
        // 配对码只在第一次导航（`/remote`）上认。API 路径不认，免得它又被拼进
        // XHR 的 URL 里——那等于把刚拆掉的洞照原样开回来。
        if req.panel_code_ok && req.path == "/remote" {
            return AccessDecision::AllowRemotePanelNewSession;
        }
        if token_matches(req.header_token, req.expected_token) {
            return AccessDecision::AllowRemotePanel;
        }
        return AccessDecision::Deny("远程面板需要有效会话：请在电脑上「诊断」页重新扫码或复制链接");
    }

    // `/mcp` 必须在这一行里：它把技能库、联网搜索和 Office 读写交给调用方，开了
    // 手机远程访问之后网关绑的是 0.0.0.0，漏掉它等于把这些能力对局域网无鉴权敞开。
    let is_gateway = req.path.starts_with("/v1/")
        || req.path.starts_with("/agent/")
        || req.path.starts_with("/session/")
        || req.path == "/mcp";
    if !is_gateway || req.peer_is_loopback {
        return AccessDecision::Allow;
    }

    // WSL 里的 agent 是从非回环地址过来的，又不方便带令牌，所以 WSL 模式保留原来
    // 的本地开发信任——**但仅限手机远程访问关着的时候**。两个都开时监听在
    // 0.0.0.0，一刀切的 WSL 豁免等于把无鉴权的模型网关送给局域网上每一台设备。
    if req.use_wsl && !req.remote_enabled {
        return AccessDecision::Allow;
    }
    if !token_matches(req.header_token, req.expected_token) {
        return AccessDecision::Deny("远程访问模型网关需要有效令牌 (x-omnix-remote-token)");
    }
    AccessDecision::Allow
}

/// 「这个请求过了 `guard_gateway_access` 那道闸」的凭证。
///
/// 面板 handler 不再各自比对令牌。同一套判断有两份实现迟早分叉——这个会话已经
/// 在 API Key、搜索供应商、多 Key 轮换上各栽过一次。它们现在只确认闸放行过；
/// 万一以后有人把面板路由挂到没套这层 middleware 的 router 上，扩展不存在，
/// 这里直接 401，是 fail-closed。
#[derive(Clone, Copy)]
pub(crate) struct PanelAuthed;

#[axum::async_trait]
impl<S: Send + Sync> axum::extract::FromRequestParts<S> for PanelAuthed {
    type Rejection = (StatusCode, &'static str);

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<PanelAuthed>()
            .copied()
            .ok_or((StatusCode::UNAUTHORIZED, "远程面板需要有效会话"))
    }
}

/// 配对失效时给手机看的页面。返回一行 403 文本的话，用户在手机上只会看到一串
/// 英文报错，不知道该去电脑上做什么。
const EXPIRED_PAGE: &str = r#"<!DOCTYPE html><html lang="zh-CN"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>OMNIX 远程 · 需要重新配对</title></head>
<body style="margin:0;height:100dvh;display:flex;align-items:center;justify-content:center;
background:#0a0b10;color:#e8ecf5;font-family:-apple-system,system-ui,'PingFang SC',sans-serif">
<div style="max-width:22rem;padding:24px;text-align:center">
<div style="font-size:40px">🔒</div>
<h1 style="font-size:18px;margin:12px 0 8px">需要重新配对</h1>
<p style="font-size:13px;color:#8a91a8;line-height:1.7;margin:0">
这个链接里的配对码已经用过或已过期（有效期 5 分钟，用一次即废）。<br><br>
到电脑上打开 OMNIX 的「诊断」页，在「手机远程访问」里重新扫一次二维码。</p>
</div></body></html>"#;

pub(super) async fn guard_gateway_access(
    axum::extract::ConnectInfo(peer): axum::extract::ConnectInfo<SocketAddr>,
    State(state): State<Arc<ProxyState>>,
    mut req: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    let setting = |key: &str| {
        state.db.get_setting(key).unwrap_or(None).unwrap_or_default()
    };
    let header_token = req
        .headers()
        .get("x-omnix-remote-token")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let expected_token = setting("remote_token");
    let path = req.uri().path().to_string();
    let is_panel = path == "/remote" || path.starts_with("/api/remote/");
    let now = chrono::Utc::now().timestamp();

    // 只有面板路径才看 Cookie/配对码——网关路径（`/v1/*` 等）继续只认 header。
    // Cookie 的作用域是整个 origin，会跟着发到 `/v1/messages` 上；要是这里让它
    // 参与网关判定，等于用一个浏览器凭据打开了模型网关。
    let mut panel_session_ok = false;
    let mut panel_code_ok = false;
    if is_panel {
        panel_session_ok = req
            .headers()
            .get(axum::http::header::COOKIE)
            .and_then(|v| v.to_str().ok())
            .and_then(|raw| crate::remote_session::cookie_value(raw, crate::remote_session::SESSION_COOKIE))
            .is_some_and(|c| crate::remote_session::session_valid(&c, &expected_token, now));
        // 已经有有效会话就不动配对码——不然带着旧二维码刷新一次页面，
        // 会白白烧掉一个还能用的码。
        if !panel_session_ok && path == "/remote" {
            panel_code_ok = query_param(req.uri().query(), "code")
                .is_some_and(|c| crate::remote_session::consume_code(&c, now));
        }
    }

    let decision = decide_gateway_access(&AccessRequest {
        path: &path,
        peer_is_loopback: peer.ip().is_loopback(),
        header_token: &header_token,
        expected_token: &expected_token,
        use_wsl: setting("use_wsl") == "true",
        remote_enabled: setting("remote_access_enabled") == "true",
        panel_session_ok,
        panel_code_ok,
    });
    match decision {
        AccessDecision::Deny(_) if path == "/remote" => {
            (StatusCode::UNAUTHORIZED, axum::response::Html(EXPIRED_PAGE)).into_response()
        }
        AccessDecision::Deny(reason) => (StatusCode::FORBIDDEN, reason).into_response(),
        AccessDecision::AllowRemotePanel => {
            record_remote_client(peer.ip().to_string());
            req.extensions_mut().insert(PanelAuthed);
            next.run(req).await
        }
        AccessDecision::AllowRemotePanelNewSession => {
            record_remote_client(peer.ip().to_string());
            req.extensions_mut().insert(PanelAuthed);
            let mut resp = next.run(req).await;
            // 配对码换会话 Cookie：这一步之后手机再也不需要 URL 里的凭据。
            if let Some(value) = crate::remote_session::issue_session(&expected_token, now) {
                if let Ok(header) =
                    axum::http::HeaderValue::from_str(&crate::remote_session::set_cookie_header(&value))
                {
                    resp.headers_mut().insert(axum::http::header::SET_COOKIE, header);
                }
            }
            resp
        }
        AccessDecision::Allow => next.run(req).await,
    }
}

#[cfg(test)]
mod tests {
    use super::{query_param, record_remote_client, remote_clients_snapshot, token_matches};

    #[test]
    fn query_param_extracts_token_and_ignores_others() {
        assert_eq!(
            query_param(Some("token=tok_abc&x=1"), "token").as_deref(),
            Some("tok_abc")
        );
        assert_eq!(
            query_param(Some("a=1&token=tok_xyz"), "token").as_deref(),
            Some("tok_xyz")
        );
        assert_eq!(query_param(Some("a=1&b=2"), "token"), None);
        assert_eq!(query_param(None, "token"), None);
        // Empty value stays empty (→ auth fails against a non-empty expected).
        assert_eq!(query_param(Some("token="), "token").as_deref(), Some(""));
    }

    #[test]
    fn token_matches_is_exact_and_rejects_empty_expected() {
        assert!(token_matches("tok_abc", "tok_abc"));
        assert!(!token_matches("tok_abd", "tok_abc"));
        assert!(!token_matches("tok_ab", "tok_abc"));
        assert!(!token_matches("tok_abcd", "tok_abc"));
        // A missing/unreadable stored token must never authenticate anyone.
        assert!(!token_matches("", ""));
        assert!(!token_matches("anything", ""));
    }

    #[test]
    fn remote_clients_recorded_and_sorted_recent_first() {
        record_remote_client("192.168.1.7".into());
        record_remote_client("192.168.1.9".into());
        let snapshot = remote_clients_snapshot();
        assert!(snapshot.iter().any(|c| c.ip == "192.168.1.7"));
        assert!(snapshot.iter().any(|c| c.ip == "192.168.1.9"));
        // Sorted by last_seen desc.
        for pair in snapshot.windows(2) {
            assert!(pair[0].last_seen >= pair[1].last_seen);
        }
    }
}

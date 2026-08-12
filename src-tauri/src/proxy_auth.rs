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
    /// URL 里的 `?token=`，只有远程面板认它。
    pub query_token: Option<&'a str>,
    pub expected_token: &'a str,
    pub use_wsl: bool,
    pub remote_enabled: bool,
}

#[derive(Debug, PartialEq)]
pub(crate) enum AccessDecision {
    Allow,
    /// 放行并把来访者记进远程设备列表（只有远程面板走这条）。
    AllowRemotePanel,
    Deny(&'static str),
}

/// 网关鉴权的**全部**判断逻辑。middleware 只负责取值和执行结果。
pub(crate) fn decide_gateway_access(req: &AccessRequest<'_>) -> AccessDecision {
    // 远程面板：任何来源都要令牌，本机也不例外。
    //
    // Header 优先——查询串会进浏览器历史、Referer 和截图。`?token=` 保留作兜底：
    // 手机是从一条普通链接/二维码打开面板的，那第一次导航带不了 header；页面
    // 加载之后它自己的 API 调用就走 header 了。
    if req.path == "/remote" || req.path.starts_with("/api/remote/") {
        let provided = if req.header_token.is_empty() {
            req.query_token.unwrap_or("")
        } else {
            req.header_token
        };
        if !token_matches(provided, req.expected_token) {
            return AccessDecision::Deny("远程访问需要有效令牌（在链接中携带 ?token=…）");
        }
        return AccessDecision::AllowRemotePanel;
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

pub(super) async fn guard_gateway_access(
    axum::extract::ConnectInfo(peer): axum::extract::ConnectInfo<SocketAddr>,
    State(state): State<Arc<ProxyState>>,
    req: axum::extract::Request,
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
    let query_token = query_param(req.uri().query(), "token");
    let decision = decide_gateway_access(&AccessRequest {
        path: req.uri().path(),
        peer_is_loopback: peer.ip().is_loopback(),
        header_token: &header_token,
        query_token: query_token.as_deref(),
        expected_token: &setting("remote_token"),
        use_wsl: setting("use_wsl") == "true",
        remote_enabled: setting("remote_access_enabled") == "true",
    });
    match decision {
        AccessDecision::Deny(reason) => (StatusCode::FORBIDDEN, reason).into_response(),
        AccessDecision::AllowRemotePanel => {
            record_remote_client(peer.ip().to_string());
            next.run(req).await
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

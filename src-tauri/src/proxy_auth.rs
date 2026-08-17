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

    // 走到这里的都不是远程面板路径。**一律要凭据**：回环放行，其余要令牌。
    //
    // 这里以前先算一个白名单（`/v1/` `/agent/` `/session/` `/mcp` `/health`），
    // 然后 `if !is_gateway || peer_is_loopback { Allow }`——落在白名单**外**的路径
    // 对任何 IP、不带任何凭据直接放行。当时 router 里每条路由恰好都被覆盖，所以
    // 没有实际暴露；但那是**默认放行**：以后新加一条路由，忘了进白名单就是对局域网
    // 静默敞开，没有报错、没有日志，只有开着「手机远程访问」时才会被人从外面摸到。
    //
    // 白名单整个去掉了——它唯一的作用是决定「谁不用鉴权」，而正确答案是「没有谁」。
    // 改成默认拒绝的代价是新路由默认要令牌，那个方向的失败**看得见**（本机以外
    // 调不通，立刻发现），原来那个方向的失败看不见。
    // `router_coverage_tests` 扫 proxy.rs 的真实路由表钉住这一点。
    //
    // 白名单里那两条曾单独写过理由，仍然成立、现在由默认拒绝一并覆盖：
    // `/mcp` 把技能库、联网搜索和 Office 读写交给调用方；`/health` 回的是平台数量、
    // 请求计数这类内部状态——绑 0.0.0.0 时让局域网随便探，等于免费提供一份
    // 「这台机器上装了什么、用得多不多」的报告。
    if req.peer_is_loopback {
        return AccessDecision::Allow;
    }

    // 这里以前有一条 WSL 豁免：`use_wsl` 为真且手机远程访问关着时，**任何**非回环
    // 地址都免令牌放行。理由是 WSL 里的 agent 从非回环地址过来、又不方便带令牌。
    //
    // 已整段删除。三件事叠起来才看清它的性质：那个「在 WSL 中启动」开关根本不
    // 落盘（`useSettings` 读写两侧都没有 `use_wsl`），所以豁免从来没生效过，
    // WSL 启动 agent 也从没跑过；而任何人「顺手把开关修好」都会同时打开一个
    // 局域网无鉴权入口。潜伏在坏开关后面的洞比明着的洞更危险。
    //
    // 真要做 WSL 支持，得先解决「把令牌递进 WSL」——那条启动命令当时只 export
    // 了 ANTHROPIC_BASE_URL，没有任何凭据。
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

#[cfg(test)]
mod route_classification_tests {
    use super::*;

    fn from_lan(path: &str) -> AccessDecision {
        decide_gateway_access(&AccessRequest {
            path,
            peer_is_loopback: false,
            header_token: "",
            expected_token: "real-token",
            panel_session_ok: false,
            panel_code_ok: false,
        })
    }

    /// 未归类的路径**不能**对局域网无凭据放行。
    ///
    /// 判断走的是白名单（`/v1/` `/agent/` `/session/` `/mcp` `/health`），
    /// 落在白名单外、又不是 `/api/remote/` 的路径原本直接 `Allow`——**任何 IP、
    /// 不要凭据**。当前 router 里每条路由恰好都被覆盖，所以没有实际暴露；但这是
    /// 默认放行的设计，新加一条路由忘了归类就是静默敞开，而且不会有任何征兆。
    #[test]
    fn an_unclassified_path_is_not_open_to_the_lan() {
        assert!(
            matches!(from_lan("/some/new/route"), AccessDecision::Deny(_)),
            "未归类路径对局域网无凭据放行了"
        );
    }

    /// 已知网关路径仍然按原规则：非回环要令牌。
    #[test]
    fn known_gateway_paths_still_require_a_token_from_the_lan() {
        for p in ["/v1/messages", "/mcp", "/health", "/agent/claude/v1/messages"] {
            assert!(matches!(from_lan(p), AccessDecision::Deny(_)), "{p} 应要令牌");
        }
    }

    /// 回环仍然免令牌——本机进程（agent CLI）靠这条工作，不能收紧。
    #[test]
    fn loopback_still_bypasses_the_token_for_gateway_paths() {
        let d = decide_gateway_access(&AccessRequest {
            path: "/v1/messages",
            peer_is_loopback: true,
            header_token: "",
            expected_token: "real-token",
            panel_session_ok: false,
            panel_code_ok: false,
        });
        assert_eq!(d, AccessDecision::Allow, "本机调用不该被挡");
    }
}

#[cfg(test)]
mod router_coverage_tests {
    use super::*;

    /// 从 `proxy.rs` 的源码里抠出 `.route("<path>"` 的全部路径。
    ///
    /// 扫源码而不是维护第二份清单：清单会漂，源码不会。
    fn declared_routes() -> Vec<String> {
        let src = include_str!("proxy.rs");
        let mut out = Vec::new();
        for (i, _) in src.match_indices(".route(") {
            let rest = &src[i + ".route(".len()..];
            let Some(start) = rest.find('"') else { continue };
            let Some(end) = rest[start + 1..].find('"') else { continue };
            out.push(rest[start + 1..start + 1 + end].to_string());
        }
        out
    }

    /// **router 里声明的每一条路由，从局域网访问都必须要凭据。**
    ///
    /// 这条守的是新加路由时的疏忽。判断逻辑已经改成默认拒绝，所以它现在恒成立——
    /// 但正因如此才要钉住：一旦有人为了图方便再加一条「这个路径不用鉴权」的
    /// 例外，这里立刻会红。
    ///
    /// Axum 的 `:param` 段用一个占位值替换后再判——判断只看前缀，不解析参数。
    #[test]
    fn every_declared_route_requires_credentials_from_the_lan() {
        let routes = declared_routes();
        // 自检：扫得到东西才说明这条测试没有空转
        assert!(routes.len() >= 10, "只扫到 {} 条路由，正则大概率失效了", routes.len());

        let mut open: Vec<String> = Vec::new();
        for route in &routes {
            let concrete = route
                .split('/')
                .map(|seg| if seg.starts_with(':') { "x" } else { seg })
                .collect::<Vec<_>>()
                .join("/");
            let decision = decide_gateway_access(&AccessRequest {
                path: &concrete,
                peer_is_loopback: false,
                header_token: "",
                expected_token: "real-token",
                panel_session_ok: false,
                panel_code_ok: false,
            });
            if !matches!(decision, AccessDecision::Deny(_)) {
                open.push(route.clone());
            }
        }
        assert!(
            open.is_empty(),
            "这些路由对局域网无凭据放行：{}\n新增路由不要开鉴权例外。",
            open.join(", ")
        );
    }
}

//! OAuth 2.0 (Authorization Code + PKCE) protocol layer for the auth center.
//! Pure and fully unit-tested — the IO layer (`commands/oauth.rs`) does the HTTP
//! and encrypted storage. One `OAuthProviderKind` variant per provider (mirrors
//! `AdapterKind` / `MediaProviderKind`), so adding a provider is a new variant
//! the compiler forces every `match` to complete.
//!
//! Endpoints/client-ids/scopes are the standard public CLI OAuth clients that
//! Claude Code / Codex / Gemini CLI use; the user authenticates in their
//! own browser and pastes back the code — OMNIX never sees the password.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD as B64URL, Engine as _};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OAuthProviderKind {
    AnthropicClaude,
    OpenAiCodex,
    GoogleGemini,
}

impl OAuthProviderKind {
    pub fn as_str(self) -> &'static str {
        match self {
            OAuthProviderKind::AnthropicClaude => "anthropic_claude",
            OAuthProviderKind::OpenAiCodex => "openai_codex",
            OAuthProviderKind::GoogleGemini => "google_gemini",
        }
    }

    pub fn from_str(value: &str) -> Result<Self, String> {
        match value {
            "anthropic_claude" => Ok(OAuthProviderKind::AnthropicClaude),
            "openai_codex" => Ok(OAuthProviderKind::OpenAiCodex),
            "google_gemini" => Ok(OAuthProviderKind::GoogleGemini),
            other => Err(format!("未知的 OAuth 供应商：{other}")),
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            OAuthProviderKind::AnthropicClaude => "Claude 订阅",
            OAuthProviderKind::OpenAiCodex => "OpenAI / Codex",
            OAuthProviderKind::GoogleGemini => "Google Gemini",
        }
    }
}

/// How a token-endpoint request body is encoded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyKind {
    Json,
    Form,
}

/// Static per-provider OAuth configuration.
pub struct ProviderConfig {
    pub client_id: &'static str,
    pub client_secret: Option<&'static str>,
    pub authorize_url: &'static str,
    pub token_url: &'static str,
    pub redirect_uri: &'static str,
    pub scope: &'static str,
    /// Extra fixed query params appended to the authorize URL.
    pub extra_authorize: &'static [(&'static str, &'static str)],
    pub body_kind: BodyKind,
    /// True when the redirect shows the code on a page for the user to copy
    /// (Claude, Gemini code_assist); false for localhost redirects where the
    /// user pastes the whole `http://localhost.../callback?code=...&state=...`.
    pub manual_paste: bool,
}

pub fn provider_config(kind: OAuthProviderKind) -> ProviderConfig {
    match kind {
        // Claude Code CLI OAuth client (public)
        OAuthProviderKind::AnthropicClaude => ProviderConfig {
            client_id: "9d1c250a-e61b-44d9-88ed-5944d1962f5e",
            client_secret: None,
            authorize_url: "https://claude.ai/oauth/authorize",
            token_url: "https://platform.claude.com/v1/oauth/token",
            redirect_uri: "https://platform.claude.com/oauth/code/callback",
            scope: "org:create_api_key user:profile user:inference user:sessions:claude_code user:mcp_servers user:file_upload",
            extra_authorize: &[("code", "true")],
            body_kind: BodyKind::Json,
            manual_paste: true,
        },
        // Codex CLI official OAuth client (public)
        OAuthProviderKind::OpenAiCodex => ProviderConfig {
            client_id: "app_EMoamEEZ73f0CkXaXp7hrann",
            client_secret: None,
            authorize_url: "https://auth.openai.com/oauth/authorize",
            token_url: "https://auth.openai.com/oauth/token",
            redirect_uri: "http://localhost:1455/auth/callback",
            scope: "openid profile email offline_access",
            extra_authorize: &[
                ("id_token_add_organizations", "true"),
                ("codex_cli_simplified_flow", "true"),
            ],
            body_kind: BodyKind::Form,
            manual_paste: false,
        },
        // Gemini CLI built-in OAuth client (public)
        OAuthProviderKind::GoogleGemini => ProviderConfig {
            client_id: "681255809395-oo8ft2oprdrnp9e3aqf6av3hmdib135j.apps.googleusercontent.com",
            client_secret: Some("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"),
            authorize_url: "https://accounts.google.com/o/oauth2/v2/auth",
            token_url: "https://oauth2.googleapis.com/token",
            redirect_uri: "https://codeassist.google.com/authcode",
            scope: "https://www.googleapis.com/auth/cloud-platform https://www.googleapis.com/auth/userinfo.email https://www.googleapis.com/auth/userinfo.profile",
            extra_authorize: &[("access_type", "offline"), ("prompt", "consent")],
            body_kind: BodyKind::Form,
            manual_paste: true,
        },
    }
}

// ── PKCE ───────────────────────────────────────────────────────────────────

/// 32 CSPRNG bytes → base64url (unpadded) = a 43-char code verifier.
pub fn generate_code_verifier() -> String {
    let mut bytes = [0u8; 32];
    getrandom::getrandom(&mut bytes).expect("OS CSPRNG failed");
    B64URL.encode(bytes)
}

/// S256 challenge: base64url(sha256(verifier)).
pub fn code_challenge_s256(verifier: &str) -> String {
    let digest = Sha256::digest(verifier.as_bytes());
    B64URL.encode(digest)
}

/// Random opaque `state` for CSRF protection (16 bytes → base64url).
pub fn generate_state() -> String {
    let mut bytes = [0u8; 16];
    getrandom::getrandom(&mut bytes).expect("OS CSPRNG failed");
    B64URL.encode(bytes)
}

// ── URL helpers ────────────────────────────────────────────────────────────

/// Percent-encode a query value (RFC 3986 unreserved chars pass through).
fn percent_encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

fn encode_query(params: &[(&str, &str)]) -> String {
    params
        .iter()
        .map(|(k, v)| format!("{}={}", percent_encode(k), percent_encode(v)))
        .collect::<Vec<_>>()
        .join("&")
}

/// Build the provider's authorization URL for a PKCE login.
pub fn build_authorize_url(kind: OAuthProviderKind, state: &str, code_challenge: &str) -> String {
    let cfg = provider_config(kind);
    let mut params: Vec<(&str, &str)> = vec![
        ("response_type", "code"),
        ("client_id", cfg.client_id),
        ("redirect_uri", cfg.redirect_uri),
        ("scope", cfg.scope),
        ("state", state),
        ("code_challenge", code_challenge),
        ("code_challenge_method", "S256"),
    ];
    params.extend_from_slice(cfg.extra_authorize);
    format!("{}?{}", cfg.authorize_url, encode_query(&params))
}

// ── Callback parsing ───────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedCallback {
    pub code: String,
    pub state: Option<String>,
}

/// Parse whatever the user pasted back: a bare code, Claude's `code#state`, or a
/// full redirect URL like `http://localhost:1455/auth/callback?code=..&state=..`.
pub fn parse_callback_input(input: &str) -> Result<ParsedCallback, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("请粘贴授权码或回调链接".into());
    }
    // Full redirect URL → pull code/state from the query string.
    if let Some(query_start) = trimmed.find('?') {
        if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
            let query = &trimmed[query_start + 1..];
            let mut code = None;
            let mut state = None;
            for pair in query.split('&') {
                let mut it = pair.splitn(2, '=');
                match (it.next(), it.next()) {
                    (Some("code"), Some(v)) => code = Some(percent_decode(v)),
                    (Some("state"), Some(v)) => state = Some(percent_decode(v)),
                    _ => {}
                }
            }
            let code = code.ok_or_else(|| "回调链接里没有 code 参数".to_string())?;
            return Ok(ParsedCallback { code, state });
        }
    }
    // Claude returns `authCode#state`.
    if let Some(idx) = trimmed.find('#') {
        return Ok(ParsedCallback {
            code: trimmed[..idx].to_string(),
            state: Some(trimmed[idx + 1..].to_string()),
        });
    }
    Ok(ParsedCallback {
        code: trimmed.to_string(),
        state: None,
    })
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let hi = (bytes[i + 1] as char).to_digit(16);
                let lo = (bytes[i + 2] as char).to_digit(16);
                if let (Some(hi), Some(lo)) = (hi, lo) {
                    out.push((hi * 16 + lo) as u8);
                    i += 3;
                    continue;
                }
                out.push(b'%');
                i += 1;
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            other => {
                out.push(other);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

// ── Token exchange / refresh ───────────────────────────────────────────────

/// A token-endpoint request the IO layer executes (POST `url` with `params`
/// encoded per `body_kind`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenRequest {
    pub url: String,
    pub body_kind: BodyKind,
    pub params: Vec<(String, String)>,
}

/// Build the authorization-code → token exchange request.
pub fn build_token_exchange(
    kind: OAuthProviderKind,
    code: &str,
    code_verifier: &str,
    state: Option<&str>,
) -> TokenRequest {
    let cfg = provider_config(kind);
    let mut params: Vec<(String, String)> = vec![
        ("grant_type".into(), "authorization_code".into()),
        ("code".into(), code.to_string()),
        ("client_id".into(), cfg.client_id.to_string()),
        ("redirect_uri".into(), cfg.redirect_uri.to_string()),
        ("code_verifier".into(), code_verifier.to_string()),
    ];
    if let Some(secret) = cfg.client_secret {
        params.push(("client_secret".into(), secret.to_string()));
    }
    // Claude echoes the state in the token body.
    if kind == OAuthProviderKind::AnthropicClaude {
        if let Some(state) = state {
            params.push(("state".into(), state.to_string()));
        }
    }
    TokenRequest {
        url: cfg.token_url.to_string(),
        body_kind: cfg.body_kind,
        params,
    }
}

/// Build the refresh-token request.
pub fn build_refresh_request(kind: OAuthProviderKind, refresh_token: &str) -> TokenRequest {
    let cfg = provider_config(kind);
    let mut params: Vec<(String, String)> = vec![
        ("grant_type".into(), "refresh_token".into()),
        ("refresh_token".into(), refresh_token.to_string()),
        ("client_id".into(), cfg.client_id.to_string()),
    ];
    if let Some(secret) = cfg.client_secret {
        params.push(("client_secret".into(), secret.to_string()));
    }
    if kind == OAuthProviderKind::OpenAiCodex {
        // Refresh OpenAI without offline_access.
        params.push(("scope".into(), "openid profile email".into()));
    }
    TokenRequest {
        url: cfg.token_url.to_string(),
        body_kind: cfg.body_kind,
        params,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct OAuthTokens {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_in: Option<i64>,
    pub token_type: Option<String>,
    pub scope: Option<String>,
}

/// Parse a token-endpoint JSON response (standard OAuth2 fields).
pub fn parse_token_response(body: &str) -> Result<OAuthTokens, String> {
    let value: serde_json::Value =
        serde_json::from_str(body).map_err(|error| format!("token 响应不是有效 JSON：{error}"))?;
    // Surface provider errors (e.g. invalid_grant) rather than a blank token.
    if let Some(err) = value.get("error").and_then(|v| v.as_str()) {
        let desc = value
            .get("error_description")
            .and_then(|v| v.as_str())
            .unwrap_or(err);
        return Err(format!("授权失败：{desc}"));
    }
    let access_token = value
        .get("access_token")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "token 响应缺少 access_token".to_string())?
        .to_string();
    Ok(OAuthTokens {
        access_token,
        refresh_token: value
            .get("refresh_token")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        expires_in: value.get("expires_in").and_then(|v| v.as_i64()),
        token_type: value.get("token_type").and_then(|v| v.as_str()).map(str::to_string),
        scope: value.get("scope").and_then(|v| v.as_str()).map(str::to_string),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_kind_round_trips() {
        for kind in [
            OAuthProviderKind::AnthropicClaude,
            OAuthProviderKind::OpenAiCodex,
            OAuthProviderKind::GoogleGemini,
        ] {
            assert_eq!(OAuthProviderKind::from_str(kind.as_str()).unwrap(), kind);
        }
        assert!(OAuthProviderKind::from_str("nope").is_err());
    }

    #[test]
    fn pkce_challenge_matches_rfc7636_example() {
        // RFC 7636 Appendix B vector.
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        assert_eq!(code_challenge_s256(verifier), "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
    }

    #[test]
    fn verifier_and_state_are_urlsafe_and_sized() {
        let v = generate_code_verifier();
        assert_eq!(v.len(), 43); // 32 bytes → 43 base64url chars, no padding.
        assert!(!v.contains('=') && !v.contains('+') && !v.contains('/'));
        assert_ne!(generate_state(), generate_state());
    }

    #[test]
    fn authorize_url_encodes_scope_and_includes_provider_extras() {
        let url = build_authorize_url(OAuthProviderKind::AnthropicClaude, "st8", "chal");
        assert!(url.starts_with("https://claude.ai/oauth/authorize?"));
        assert!(url.contains("client_id=9d1c250a-e61b-44d9-88ed-5944d1962f5e"));
        assert!(url.contains("code_challenge=chal"));
        assert!(url.contains("code_challenge_method=S256"));
        assert!(url.contains("state=st8"));
        assert!(url.contains("code=true")); // Claude-specific extra
        assert!(url.contains("scope=org%3Acreate_api_key")); // ':' encoded, spaces as %20

        let openai = build_authorize_url(OAuthProviderKind::OpenAiCodex, "s", "c");
        assert!(openai.contains("codex_cli_simplified_flow=true"));
        let gemini = build_authorize_url(OAuthProviderKind::GoogleGemini, "s", "c");
        assert!(gemini.contains("access_type=offline"));
        assert!(gemini.contains("prompt=consent"));
    }

    #[test]
    fn callback_parses_bare_code_hash_and_full_url() {
        assert_eq!(
            parse_callback_input("  rawcode  ").unwrap(),
            ParsedCallback { code: "rawcode".into(), state: None }
        );
        assert_eq!(
            parse_callback_input("abc123#xyz").unwrap(),
            ParsedCallback { code: "abc123".into(), state: Some("xyz".into()) }
        );
        assert_eq!(
            parse_callback_input("http://localhost:1455/auth/callback?code=AAA&state=BBB").unwrap(),
            ParsedCallback { code: "AAA".into(), state: Some("BBB".into()) }
        );
        // Percent-encoded values decode.
        assert_eq!(
            parse_callback_input("https://x/cb?code=a%2Fb&state=s").unwrap().code,
            "a/b"
        );
        assert!(parse_callback_input("   ").is_err());
        assert!(parse_callback_input("https://x/cb?state=only").is_err());
    }

    #[test]
    fn token_exchange_bodies_are_per_provider() {
        // Claude: JSON, echoes state.
        let claude = build_token_exchange(OAuthProviderKind::AnthropicClaude, "c", "v", Some("st"));
        assert_eq!(claude.body_kind, BodyKind::Json);
        assert!(claude.params.iter().any(|(k, v)| k == "state" && v == "st"));
        assert!(claude.params.iter().any(|(k, v)| k == "grant_type" && v == "authorization_code"));

        // OpenAI: form, no client_secret, no state.
        let openai = build_token_exchange(OAuthProviderKind::OpenAiCodex, "c", "v", Some("st"));
        assert_eq!(openai.body_kind, BodyKind::Form);
        assert!(!openai.params.iter().any(|(k, _)| k == "state"));
        assert!(!openai.params.iter().any(|(k, _)| k == "client_secret"));

        // Gemini: form, includes client_secret.
        let gemini = build_token_exchange(OAuthProviderKind::GoogleGemini, "c", "v", None);
        assert!(gemini.params.iter().any(|(k, _)| k == "client_secret"));
    }

    #[test]
    fn refresh_request_shapes() {
        let claude = build_refresh_request(OAuthProviderKind::AnthropicClaude, "rt");
        assert!(claude.params.iter().any(|(k, v)| k == "grant_type" && v == "refresh_token"));
        let openai = build_refresh_request(OAuthProviderKind::OpenAiCodex, "rt");
        assert!(openai.params.iter().any(|(k, v)| k == "scope" && v == "openid profile email"));
    }

    #[test]
    fn parse_token_response_ok_and_error() {
        let ok = parse_token_response(
            r#"{"access_token":"at","refresh_token":"rt","expires_in":3600,"token_type":"Bearer","scope":"s"}"#,
        )
        .unwrap();
        assert_eq!(ok.access_token, "at");
        assert_eq!(ok.refresh_token.as_deref(), Some("rt"));
        assert_eq!(ok.expires_in, Some(3600));

        let err = parse_token_response(r#"{"error":"invalid_grant","error_description":"bad code"}"#);
        assert!(err.unwrap_err().contains("bad code"));
        assert!(parse_token_response(r#"{"token_type":"Bearer"}"#).is_err());
    }
}

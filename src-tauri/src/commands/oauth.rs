//! OAuth auth center IO layer: runs the PKCE login flow, stores subscription
//! tokens encrypted (AES-GCM via `crypto`), and keeps them fresh with a
//! background refresher. Pure protocol logic lives in `crate::oauth`.
//!
//! The user authenticates in their own browser and pastes the code back — OMNIX
//! never handles the password. Tokens are consumed by CLI takeover (2B) so any
//! agent can use the user's own subscription. ⚠️ Using a subscription to drive
//! third-party tools may be subject to each provider's terms — surfaced in UI.
//!
//! 坑点2: never hold a DB connection across `.await`. Every async command reads
//! what it needs into owned values, drops the connection, then does HTTP.

use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use rusqlite::params;
use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use crate::db::DbManager;
use crate::oauth::{self, BodyKind, OAuthProviderKind, OAuthTokens, TokenRequest};

#[derive(Debug, Clone, Serialize)]
pub struct OAuthStartResult {
    pub authorize_url: String,
    pub state: String,
    /// True → the redirect shows the code for the user to copy; false → the user
    /// pastes the whole `localhost` callback URL.
    pub manual_paste: bool,
    pub redirect_uri: String,
}

/// Redacted account view for the UI — never carries the actual tokens.
#[derive(Debug, Clone, Serialize)]
pub struct OAuthAccountView {
    pub id: String,
    pub provider: String,
    pub provider_name: String,
    pub label: String,
    pub scope: Option<String>,
    pub expires_at: Option<String>,
    pub has_refresh: bool,
    pub expired: bool,
    pub created_at: String,
}

fn now_sql() -> String {
    Utc::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

fn expires_at_sql(expires_in: Option<i64>) -> Option<String> {
    expires_in.map(|secs| {
        (Utc::now() + chrono::Duration::seconds(secs))
            .format("%Y-%m-%d %H:%M:%S")
            .to_string()
    })
}

/// Begin a login: mint PKCE + state, persist the verifier, return the authorize
/// URL for the frontend to open in the system browser.
#[tauri::command]
pub fn oauth_start(provider: String, db: State<'_, Arc<DbManager>>) -> Result<OAuthStartResult, String> {
    let kind = OAuthProviderKind::from_str(&provider)?;
    let verifier = oauth::generate_code_verifier();
    let challenge = oauth::code_challenge_s256(&verifier);
    let state = oauth::generate_state();
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT OR REPLACE INTO oauth_pkce_sessions (state, provider, code_verifier, created_at)
         VALUES (?1, ?2, ?3, datetime('now'))",
        params![state, kind.as_str(), verifier],
    )
    .map_err(|e| e.to_string())?;
    // Prune sessions older than 30 minutes so abandoned logins don't accumulate.
    let _ = conn.execute(
        "DELETE FROM oauth_pkce_sessions WHERE created_at < datetime('now', '-30 minutes')",
        [],
    );
    let cfg = oauth::provider_config(kind);
    Ok(OAuthStartResult {
        authorize_url: oauth::build_authorize_url(kind, &state, &challenge),
        state,
        manual_paste: cfg.manual_paste,
        redirect_uri: cfg.redirect_uri.to_string(),
    })
}

/// Execute a token/refresh request (JSON or form per provider) and parse it.
async fn execute_token_request(req: TokenRequest) -> Result<OAuthTokens, String> {
    let client = reqwest::Client::new();
    let builder = client.post(&req.url).header("Accept", "application/json");
    let builder = match req.body_kind {
        BodyKind::Json => {
            let map: serde_json::Map<String, serde_json::Value> = req
                .params
                .into_iter()
                .map(|(k, v)| (k, serde_json::Value::String(v)))
                .collect();
            builder.json(&serde_json::Value::Object(map))
        }
        BodyKind::Form => builder.form(&req.params),
    };
    let response = builder.send().await.map_err(|e| format!("token 请求失败：{e}"))?;
    let body = response.text().await.map_err(|e| e.to_string())?;
    oauth::parse_token_response(&body)
}

/// Complete a login: exchange the pasted code for tokens and store them.
#[tauri::command]
pub async fn oauth_complete(
    provider: String,
    callback_input: String,
    label: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<OAuthAccountView, String> {
    let kind = OAuthProviderKind::from_str(&provider)?;
    let parsed = oauth::parse_callback_input(&callback_input)?;

    // (sync) Look up the PKCE verifier, then drop the connection before HTTP.
    let verifier = {
        let conn = db.get_connection().map_err(|e| e.to_string())?;
        let by_state: Option<String> = parsed.state.as_ref().and_then(|state| {
            conn.query_row(
                "SELECT code_verifier FROM oauth_pkce_sessions WHERE state = ?1 AND provider = ?2",
                params![state, kind.as_str()],
                |row| row.get(0),
            )
            .ok()
        });
        match by_state {
            Some(v) => v,
            // Fall back to the newest pending session for this provider (localhost
            // flows sometimes drop the state on the paste).
            None => conn
                .query_row(
                    "SELECT code_verifier FROM oauth_pkce_sessions WHERE provider = ?1
                     ORDER BY created_at DESC LIMIT 1",
                    params![kind.as_str()],
                    |row| row.get(0),
                )
                .map_err(|_| "找不到对应的登录会话，请重新发起授权".to_string())?,
        }
    };

    // (await) Exchange the code for tokens.
    let request = oauth::build_token_exchange(kind, &parsed.code, &verifier, parsed.state.as_deref());
    let tokens = execute_token_request(request).await?;

    // (sync) Encrypt + persist, clear the PKCE session.
    let id = format!("oauth_{}", Utc::now().timestamp_millis());
    let access_enc = crate::crypto::encrypt(&tokens.access_token);
    let refresh_enc = tokens.refresh_token.as_deref().map(crate::crypto::encrypt);
    let expires_at = expires_at_sql(tokens.expires_in);
    let label = if label.trim().is_empty() {
        kind.display_name().to_string()
    } else {
        label.trim().to_string()
    };
    {
        let conn = db.get_connection().map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO oauth_accounts (id, provider, label, access_enc, refresh_enc, expires_at, scope, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
            params![id, kind.as_str(), label, access_enc, refresh_enc, expires_at, tokens.scope, now_sql()],
        )
        .map_err(|e| e.to_string())?;
        if let Some(state) = parsed.state.as_ref() {
            let _ = conn.execute("DELETE FROM oauth_pkce_sessions WHERE state = ?1", params![state]);
        }
    }

    Ok(OAuthAccountView {
        id,
        provider: kind.as_str().to_string(),
        provider_name: kind.display_name().to_string(),
        label,
        scope: tokens.scope,
        expires_at,
        has_refresh: refresh_enc.is_some(),
        expired: false,
        created_at: now_sql(),
    })
}

/// List stored accounts (redacted — no tokens).
#[tauri::command]
pub fn oauth_list_accounts(db: State<'_, Arc<DbManager>>) -> Result<Vec<OAuthAccountView>, String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT id, provider, label, scope, expires_at, refresh_enc,
                    CASE WHEN expires_at IS NOT NULL AND expires_at <= datetime('now') THEN 1 ELSE 0 END,
                    created_at
             FROM oauth_accounts ORDER BY created_at DESC",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            let provider: String = row.get(1)?;
            let provider_name = OAuthProviderKind::from_str(&provider)
                .map(|k| k.display_name().to_string())
                .unwrap_or_else(|_| provider.clone());
            Ok(OAuthAccountView {
                id: row.get(0)?,
                provider,
                provider_name,
                label: row.get(2)?,
                scope: row.get(3)?,
                expires_at: row.get(4)?,
                has_refresh: row.get::<_, Option<String>>(5)?.is_some(),
                expired: row.get::<_, i64>(6)? != 0,
                created_at: row.get(7)?,
            })
        })
        .map_err(|e| e.to_string())?;
    Ok(rows.flatten().collect())
}

/// Delete an account (removes the encrypted tokens from the DB).
#[tauri::command]
pub fn oauth_delete_account(id: String, db: State<'_, Arc<DbManager>>) -> Result<(), String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM oauth_accounts WHERE id = ?1", params![id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Refresh one account's access token via its refresh token. Returns the new
/// expiry (or None if the account has no refresh token).
async fn refresh_account(db: &Arc<DbManager>, id: &str) -> Result<Option<String>, String> {
    // (sync) read provider + decrypt refresh token, drop connection.
    let (kind, refresh_token) = {
        let conn = db.get_connection().map_err(|e| e.to_string())?;
        let (provider, refresh_enc): (String, Option<String>) = conn
            .query_row(
                "SELECT provider, refresh_enc FROM oauth_accounts WHERE id = ?1",
                params![id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(|_| "账号不存在".to_string())?;
        let kind = OAuthProviderKind::from_str(&provider)?;
        match refresh_enc {
            Some(enc) => (kind, crate::crypto::decrypt(&enc)),
            None => return Ok(None),
        }
    };

    // (await) refresh.
    let tokens = execute_token_request(oauth::build_refresh_request(kind, &refresh_token)).await?;

    // (sync) persist rotated tokens.
    let access_enc = crate::crypto::encrypt(&tokens.access_token);
    let new_refresh_enc = tokens.refresh_token.as_deref().map(crate::crypto::encrypt);
    let expires_at = expires_at_sql(tokens.expires_in);
    {
        let conn = db.get_connection().map_err(|e| e.to_string())?;
        // Keep the existing refresh token when the provider doesn't rotate it.
        conn.execute(
            "UPDATE oauth_accounts
             SET access_enc = ?2,
                 refresh_enc = COALESCE(?3, refresh_enc),
                 expires_at = ?4,
                 updated_at = ?5
             WHERE id = ?1",
            params![id, access_enc, new_refresh_enc, expires_at, now_sql()],
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(expires_at)
}

/// Manual refresh from the UI.
#[tauri::command]
pub async fn oauth_refresh_account(
    id: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    let db = db.inner().clone();
    refresh_account(&db, &id).await.map(|_| ())
}

/// Resolve an account to `(provider, decrypted access token)` for internal use
/// (CLI takeover). Not a command — the raw token never reaches the frontend.
pub(crate) fn resolve_oauth_access_token(
    db: &Arc<DbManager>,
    account_id: &str,
) -> Result<(OAuthProviderKind, String), String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let (provider, access_enc): (String, String) = conn
        .query_row(
            "SELECT provider, access_enc FROM oauth_accounts WHERE id = ?1",
            params![account_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|_| "OAuth 账号不存在".to_string())?;
    Ok((OAuthProviderKind::from_str(&provider)?, crate::crypto::decrypt(&access_enc)))
}

/// Background refresher: every 5 minutes, refresh accounts whose access token
/// expires within 10 minutes. Snapshots the id list synchronously before any
/// await so no DB connection is held across `.await` (坑点2).
pub fn start_oauth_refresher(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let db = match app.try_state::<Arc<DbManager>>() {
            Some(state) => state.inner().clone(),
            None => return,
        };
        loop {
            tokio::time::sleep(Duration::from_secs(300)).await;
            let due: Vec<String> = {
                let conn = match db.get_connection() {
                    Ok(conn) => conn,
                    Err(_) => continue,
                };
                let mut stmt = match conn.prepare(
                    "SELECT id FROM oauth_accounts
                     WHERE refresh_enc IS NOT NULL AND expires_at IS NOT NULL
                       AND expires_at <= datetime('now', '+10 minutes')",
                ) {
                    Ok(stmt) => stmt,
                    Err(_) => continue,
                };
                let ids = stmt
                    .query_map([], |row| row.get::<_, String>(0))
                    .map(|rows| rows.flatten().collect::<Vec<_>>())
                    .unwrap_or_default();
                ids
            };
            for id in due {
                if let Err(error) = refresh_account(&db, &id).await {
                    log::warn!("[oauth] refresh {id} failed: {error}");
                }
            }
        }
    });
}

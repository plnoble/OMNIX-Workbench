use tauri::State;
use std::sync::Arc;
use rusqlite::params;
use crate::db::DbManager;
use crate::input_validation;
use super::*;

#[tauri::command]
pub fn get_agent_accounts(
    db: State<'_, Arc<DbManager>>,
) -> Result<Vec<AgentAccount>, String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let mut stmt = conn.prepare("SELECT id, account_name, api_key, api_host, target_model, agent_name, is_active, updated_at FROM agent_accounts ORDER BY updated_at DESC")
        .map_err(|e| e.to_string())?;
    let rows = stmt.query_map([], |row| {
        let is_active_int: i32 = row.get(6)?;
        Ok(AgentAccount {
            id: row.get(0)?,
            account_name: row.get(1)?,
            api_key: row.get(2)?,
            api_host: row.get(3)?,
            target_model: row.get(4)?,
            agent_name: row.get(5)?,
            is_active: is_active_int != 0,
            updated_at: row.get(7)?,
        })
    }).map_err(|e| e.to_string())?;

    let mut result = Vec::new();
    for mut acc in rows.flatten() {
        // 先解密再脱敏：库里现在是密文，直接截密文的头尾等于给用户看乱码。
        // 而且原来那段用字节切片，Key 里只要有多字节字符就会 panic。
        acc.api_key = crate::crypto::mask_secret(&crate::crypto::decrypt(&acc.api_key));
        result.push(acc);
    }
    Ok(result)
}

#[tauri::command]
pub fn save_agent_account(
    account: serde_json::Value,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    save_agent_account_core(&db, &account)
}

/// 命令体。抽出来是为了能测——`State<…>` 在单测里构造不出来。
pub(crate) fn save_agent_account_core(
    db: &DbManager,
    account: &serde_json::Value,
) -> Result<(), String> {
    let id = account["id"].as_str().unwrap_or_default().to_string();
    let account_name = account["account_name"].as_str().unwrap_or_default().to_string();
    let api_key = account["api_key"].as_str().unwrap_or_default().to_string();
    let api_host = account["api_host"].as_str().unwrap_or_default().to_string();
    let target_model = account["target_model"].as_str().unwrap_or_default().to_string();
    let is_active = account["is_active"].as_bool().unwrap_or(false);
    // Derive agent_name from the account context (default to "claude-code" if not specified)
    let agent_name = account["agent_name"].as_str().unwrap_or("claude-code").to_string();

    let conn = db.get_connection().map_err(|e| e.to_string())?;

    // 列表接口给前端的是**脱敏**的 Key，而编辑表单会把它原样提交回来。不识别这一点
    // 的话，「改个模型名再保存」就会把真 Key 覆盖成 `abcd...wxyz`——账号从此认证
    // 不了。所以：提交值等于当前 Key 的掩码 = 用户没碰这个字段，保留原值。
    let existing_plain: Option<String> = conn
        .query_row(
            "SELECT api_key FROM agent_accounts WHERE id = ?1",
            params![id],
            |row| row.get::<_, String>(0),
        )
        .ok()
        .map(|stored| crate::crypto::decrypt(&stored));

    let key_to_store = match existing_plain.as_deref() {
        // 没改：沿用原值
        Some(prev) if crate::crypto::is_masked_form_of(&api_key, prev) => prev.to_string(),
        // 留空：同样按「没改」处理，不要把账号的 Key 清掉
        Some(prev) if api_key.trim().is_empty() => prev.to_string(),
        _ => api_key.clone(),
    };

    conn.execute(
        "INSERT OR REPLACE INTO agent_accounts (id, account_name, api_key, api_host, target_model, agent_name, is_active, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, CURRENT_TIMESTAMP)",
        params![
            id,
            account_name,
            // 密文入库。读取侧（`proxy::active_account_override`）本来就走
            // `crypto::decrypt`，对明文有透传，所以存量行不会因此读不出来。
            crate::crypto::encrypt(&key_to_store),
            api_host,
            target_model,
            agent_name,
            is_active
        ],
    ).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn delete_agent_account(
    id: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    input_validation::validate_id(&id, "id")?;
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM agent_accounts WHERE id = ?1", params![id]).map_err(|e| e.to_string())?;
    Ok(())
}

// ── F1: unified per-agent upstream account switcher (multi-account) ────
//
// Lets an agent's active upstream be switched between OAuth subscriptions (2A)
// and api-key accounts mid-conversation. The choice is a setting; the session
// gateway (`resolve_session_model_upstream`) reads it per request, so switching
// only changes the next turn's upstream — the conversation/context is untouched.

/// Settings key holding an agent's active upstream account ref.
pub(crate) fn active_upstream_setting_key(agent_name: &str) -> String {
    format!("active_upstream_{agent_name}")
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct UpstreamAccountOption {
    /// `oauth:<id>` | `apikey:<id>` — opaque ref the proxy resolves.
    pub account_ref: String,
    pub kind: String, // "oauth" | "apikey"
    pub label: String,
    pub provider: Option<String>,
    pub expired: bool,
    pub is_active: bool,
}

/// List the upstream accounts an agent can switch between: every OAuth
/// subscription plus this agent's api-key accounts. Marks the active one.
#[tauri::command]
pub fn list_agent_upstream_accounts(
    agent_name: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<Vec<UpstreamAccountOption>, String> {
    let active = db
        .get_setting(&active_upstream_setting_key(&agent_name))
        .ok()
        .flatten()
        .unwrap_or_default();
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let mut out = Vec::new();

    // OAuth subscriptions (any provider — user picks what fits the agent).
    if let Ok(mut stmt) = conn.prepare(
        "SELECT id, provider, label,
                CASE WHEN expires_at IS NOT NULL AND expires_at <= datetime('now') THEN 1 ELSE 0 END
         FROM oauth_accounts ORDER BY created_at DESC",
    ) {
        if let Ok(rows) = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)? != 0,
            ))
        }) {
            for (id, provider, label, expired) in rows.flatten() {
                let account_ref = format!("oauth:{id}");
                let provider_name = crate::oauth::OAuthProviderKind::from_str(&provider)
                    .map(|k| k.display_name().to_string())
                    .unwrap_or_else(|_| provider.clone());
                out.push(UpstreamAccountOption {
                    is_active: active == account_ref,
                    account_ref,
                    kind: "oauth".into(),
                    label,
                    provider: Some(provider_name),
                    expired,
                });
            }
    }
    }

    // Grok 的凭据归它自己的 CLI（`~/.grok/auth.json`，xAI 官方流程、自动续期），
    // 既不在 `oauth_accounts` 也不在 `agent_accounts`。以前这两张表都查不到它，
    // 于是「认证中心明明显示已登录」而「智能体页说没有可用上游」——同一件事
    // 两个页面给出相反的答案。这里如实把它列出来：它是 CLI 自管的，不参与切换。
    if agent_name.eq_ignore_ascii_case("Grok Build") || agent_name.eq_ignore_ascii_case("grok") {
        let auth_file = crate::commands::grok_auth_file();
        let signed_in = std::fs::metadata(&auth_file).is_ok_and(|meta| meta.len() > 0);
        if signed_in {
            out.push(UpstreamAccountOption {
                account_ref: "cli:grok".into(),
                kind: "cli".into(),
                label: "Grok 账号（CLI 自管）".into(),
                provider: Some("xAI".into()),
                expired: false,
                // CLI 自己持有令牌，OMNIX 换不了也不需要换——它始终是生效的那个。
                is_active: true,
            });
    }
    }

    // This agent's api-key accounts.
    if let Ok(mut stmt) = conn.prepare(
        "SELECT id, account_name FROM agent_accounts WHERE agent_name = ?1 ORDER BY updated_at DESC",
    ) {
        if let Ok(rows) = stmt.query_map(params![agent_name], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        }) {
            for (id, name) in rows.flatten() {
                let account_ref = format!("apikey:{id}");
                out.push(UpstreamAccountOption {
                    is_active: active == account_ref,
                    account_ref,
                    kind: "apikey".into(),
                    label: name,
                    provider: None,
                    expired: false,
                });
            }
    }
    }
    Ok(out)
}

/// Set (or clear with empty) the agent's active upstream account.
#[tauri::command]
pub fn set_active_upstream_account(
    agent_name: String,
    account_ref: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    db.set_setting(&active_upstream_setting_key(&agent_name), account_ref.trim())
        .map_err(|e| e.to_string())
}

/// Read the agent's active upstream account ref (empty = agent/platform default).
#[tauri::command]
pub fn get_active_upstream_account(
    agent_name: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<String, String> {
    Ok(db
        .get_setting(&active_upstream_setting_key(&agent_name))
        .ok()
        .flatten()
        .unwrap_or_default())
}

#[cfg(test)]
mod account_key_tests {
    use crate::db::DbManager;
    use rusqlite::params;

    fn test_db(tag: &str) -> DbManager {
        use std::sync::atomic::{AtomicU32, Ordering};
        static N: AtomicU32 = AtomicU32::new(0);
        let path = std::env::temp_dir().join(format!(
            "omnix_acctkey_{tag}_{}_{}.db",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_file(&path);
        DbManager::new_with_path(path)
    }

    fn stored_key(db: &DbManager, id: &str) -> String {
        db.get_connection()
            .unwrap()
            .query_row(
                "SELECT api_key FROM agent_accounts WHERE id = ?1",
                params![id],
                |r| r.get::<_, String>(0),
            )
            .unwrap()
    }

    fn account(id: &str, key: &str, name: &str) -> serde_json::Value {
        serde_json::json!({
            "id": id, "account_name": name, "api_key": key,
            "api_host": "https://x", "target_model": "m",
            "agent_name": "claude-code", "is_active": true
        })
    }

    /// **改个名字保存，不能把 Key 毁掉。**
    ///
    /// 列表接口给前端的是脱敏 Key（`sk-a...mnop`），编辑表单把它填进输入框，
    /// 保存时原样提交。识别不出「这是退回来的掩码」的话，真 Key 就被覆盖成那串
    /// 掩码——账号从此认证不了，而用户只是改了个显示名。
    /// 脱敏做了、回写这一半没做，是这个 bug 的全部成因。
    #[test]
    fn editing_an_account_does_not_destroy_its_key() {
        let db = test_db("roundtrip");
        {
            let conn = db.get_connection().unwrap();
            conn.execute("DELETE FROM agent_accounts", []).unwrap();
    }
        super::save_agent_account_core(&db, &account("a1", "sk-abcdefghijklmnop", "原名")).unwrap();

        // 模拟前端：列表拿到掩码 → 表单原样带回来 → 只改了名字
        let masked = crate::crypto::mask_secret("sk-abcdefghijklmnop");
        super::save_agent_account_core(&db, &account("a1", &masked, "新名字")).unwrap();

        assert_eq!(
            crate::crypto::decrypt(&stored_key(&db, "a1")),
            "sk-abcdefghijklmnop",
            "只改名字不该动 Key"
        );
    }

    /// 新存进去的 Key 就得是密文——不能只靠迁移把存量补上，新写入还是明文。
    #[test]
    fn a_newly_saved_key_is_encrypted_at_rest() {
        let db = test_db("atrest");
        {
            let conn = db.get_connection().unwrap();
            conn.execute("DELETE FROM agent_accounts", []).unwrap();
    }
        super::save_agent_account_core(&db, &account("a1", "sk-abcdefghijklmnop", "n")).unwrap();
        let at_rest = stored_key(&db, "a1");
        assert_ne!(at_rest, "sk-abcdefghijklmnop", "新写入的 Key 还是明文");
        assert_eq!(crate::crypto::decrypt(&at_rest), "sk-abcdefghijklmnop");
    }

    /// 用户真的换了一把新 Key，就必须换过去——别把「保护」做成「改不动」。
    #[test]
    fn a_genuinely_new_key_replaces_the_old_one() {
        let db = test_db("replace");
        {
            let conn = db.get_connection().unwrap();
            conn.execute("DELETE FROM agent_accounts", []).unwrap();
    }
        super::save_agent_account_core(&db, &account("a1", "sk-abcdefghijklmnop", "n")).unwrap();
        super::save_agent_account_core(&db, &account("a1", "sk-brand-new-value-xyz", "n")).unwrap();
        assert_eq!(
            crate::crypto::decrypt(&stored_key(&db, "a1")),
            "sk-brand-new-value-xyz"
        );
    }

    /// 列表不能把完整 Key 交给前端。
    #[test]
    fn the_list_never_returns_a_full_key() {
        let db = test_db("mask");
        {
            let conn = db.get_connection().unwrap();
            conn.execute("DELETE FROM agent_accounts", []).unwrap();
    }
        super::save_agent_account_core(&db, &account("a1", "sk-abcdefghijklmnop", "n")).unwrap();
        let masked = crate::crypto::mask_secret("sk-abcdefghijklmnop");
        // 复现列表命令的脱敏这一步（命令本身带 State，测不到）
        let shown = crate::crypto::mask_secret(&crate::crypto::decrypt(&stored_key(&db, "a1")));
        assert_eq!(shown, masked);
        assert!(!shown.contains("efghijkl"), "中段不该出现在脱敏结果里");
    }

    /// 存量明文就地加密后，**读取侧仍然拿得到明文**。
    ///
    /// 这条是冲着上一轮那次回归写的：平台 Key 迁移时「写迁走了、读没跟上」，
    /// Auto 路由一个模型都选不出来。这里读取侧本来就走 `decrypt`，测试把它钉住。
    #[test]
    fn migrated_account_key_is_encrypted_but_still_readable() {
        let db = test_db("migrate");
        {
            let conn = db.get_connection().unwrap();
            conn.execute("DELETE FROM agent_accounts", []).unwrap();
            conn.execute(
                "INSERT INTO agent_accounts (id, account_name, api_key, api_host, target_model, agent_name, is_active)
                 VALUES ('a1', '账号', 'sk-plain-secret-123', 'https://x', 'm', 'claude-code', 1)",
                [],
            )
            .unwrap();
    }
        let moved = crate::commands::migrate_plaintext_secrets_in_place(&db).expect("迁移");
        assert!(moved >= 1, "明文行应当被加密");

        let at_rest = stored_key(&db, "a1");
        assert_ne!(at_rest, "sk-plain-secret-123", "库里不该还是明文");
        assert_eq!(
            crate::crypto::decrypt(&at_rest),
            "sk-plain-secret-123",
            "读取侧必须还能拿到原文"
        );

        // 再跑一次不能把密文当明文二次加密
        let again = crate::commands::migrate_plaintext_secrets_in_place(&db).expect("第二遍");
        assert_eq!(again, 0, "已加密的行不该再被处理");
        assert_eq!(crate::crypto::decrypt(&stored_key(&db, "a1")), "sk-plain-secret-123");
    }
}

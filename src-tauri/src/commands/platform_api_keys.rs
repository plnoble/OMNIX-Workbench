//! 平台 API Key 的多 Key 管理（加密存储）。
//!
//! 从 `platforms.rs` 拆出来：那个文件去掉测试还有 2400 行，而这一段本来就带着
//! 自己的分节标题和自己的测试模块，是块界限清楚的独立关注点——Key 的增删选揭、
//! 掩码显示、加解密。平台/模型的增删改查留在原处。
//!
//! 只是搬家，没有任何行为改动。

use crate::db::DbManager;
use rusqlite::params;
use std::sync::Arc;
use tauri::State;

// ══════════════════════════════════════════════════
// Multi-Key API Key Management (encrypted storage)
// ══════════════════════════════════════════════════

/// A platform API key entry (masked for display)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PlatformApiKey {
    pub id: String,
    pub platform_id: String,
    pub label: String,
    pub masked_key: String, // e.g. "sk-...8f3d"
    pub is_active: bool,
    pub last_status: String,
    pub last_error: Option<String>,
    pub latency_ms: Option<i64>,
    pub last_checked_at: Option<String>,
    pub created_at: String,
}

/// Mask an API key for display: show first 4 and last 4 chars, middle with dots
fn mask_api_key(key: &str) -> String {
    if key.len() <= 8 {
        return "*".repeat(key.len());
    }
    format!("{}...{}", &key[..4], &key[key.len() - 4..])
}

/// Add an API key to a platform (encrypted)
#[tauri::command]
pub fn add_platform_api_key(
    platform_id: String,
    key: String,
    label: Option<String>,
    db: State<'_, Arc<DbManager>>,
) -> Result<PlatformApiKey, String> {
    if key.trim().is_empty() {
        return Err("API Key 不能为空".into());
    }

    let encrypted = crate::crypto::encrypt(&key);
    let id = format!("key_{}", chrono::Utc::now().timestamp_millis());
    let lbl = label.unwrap_or_else(|| "API Key".into());
    let masked = mask_api_key(&key);

    let conn = db.get_connection().map_err(|e| e.to_string())?;

    // If this is the first key for this platform, make it active
    let existing: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM platform_api_keys WHERE platform_id = ?1",
            params![platform_id],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let is_active = existing == 0;

    conn.execute(
        "INSERT INTO platform_api_keys (id, platform_id, encrypted_key, label, is_active) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![id, platform_id, encrypted, lbl, if is_active { 1 } else { 0 }],
    ).map_err(|e| e.to_string())?;

    // If active, also write the ENCRYPTED key to model_platforms.api_key
    // (not the plaintext — we store the same encrypted value for backward compat
    // with proxy.rs which calls crypto::decrypt on read)
    if is_active {
        let _ = conn.execute(
            "UPDATE model_platforms SET api_key = ?1 WHERE id = ?2",
            params![encrypted, platform_id],
        );
    }

    Ok(PlatformApiKey {
        id: id.clone(),
        platform_id,
        label: lbl,
        masked_key: masked,
        is_active,
        last_status: "unknown".into(),
        last_error: None,
        latency_ms: None,
        last_checked_at: None,
        created_at: chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
    })
}

/// List all API keys for a platform (masked)
#[tauri::command]
pub fn list_platform_api_keys(
    platform_id: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<Vec<PlatformApiKey>, String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT id, platform_id, encrypted_key, label, is_active, created_at,
                last_status, last_error, latency_ms, last_checked_at
         FROM platform_api_keys
         WHERE platform_id = ?1
         ORDER BY is_active DESC, priority DESC, created_at ASC",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![platform_id], |row| {
            let encrypted: String = row.get(2)?;
            let decrypted = crate::crypto::decrypt(&encrypted);
            let masked = mask_api_key(&decrypted);
            Ok(PlatformApiKey {
                id: row.get(0)?,
                platform_id: row.get(1)?,
                label: row.get(3)?,
                masked_key: masked,
                is_active: row.get::<_, i32>(4)? == 1,
                last_status: row.get(6).unwrap_or_else(|_| "unknown".into()),
                last_error: row.get(7).ok(),
                latency_ms: row.get(8).ok(),
                last_checked_at: row.get(9).ok(),
                created_at: row.get::<_, String>(5).unwrap_or_default(),
            })
        })
        .map_err(|e| e.to_string())?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// Select a platform API key as the active one
#[tauri::command]
pub fn select_platform_api_key(
    key_id: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    select_platform_api_key_core(&db, &key_id)
}

pub(crate) fn select_platform_api_key_core(db: &DbManager, key_id: &str) -> Result<(), String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;

    // Keep the ciphertext. The old column is a leftover compatibility slot;
    // writing the decrypted secret here put live keys back on disk (and into
    // backups that export `model_platforms`) after startup had already cleared it.
    let (platform_id, encrypted): (String, String) = conn
        .query_row(
            "SELECT platform_id, encrypted_key FROM platform_api_keys WHERE id = ?1",
            params![key_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|e| e.to_string())?;

    conn.execute(
        "UPDATE platform_api_keys SET is_active = 0 WHERE platform_id = ?1",
        params![platform_id],
    )
    .map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE platform_api_keys SET is_active = 1 WHERE id = ?1",
        params![key_id],
    )
    .map_err(|e| e.to_string())?;

    conn.execute(
        "UPDATE model_platforms SET api_key = ?1 WHERE id = ?2",
        params![encrypted, platform_id],
    )
    .map_err(|e| e.to_string())?;

    Ok(())
}

/// Delete a platform API key
#[tauri::command]
pub fn delete_platform_api_key(
    key_id: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;

    // Check if this was the active key
    let was_active: bool = conn
        .query_row(
            "SELECT is_active FROM platform_api_keys WHERE id = ?1",
            params![key_id],
            |r| r.get::<_, i32>(0).map(|v| v == 1),
        )
        .unwrap_or(false);

    let platform_id: Option<String> = conn
        .query_row(
            "SELECT platform_id FROM platform_api_keys WHERE id = ?1",
            params![key_id],
            |r| r.get(0),
        )
        .ok();

    conn.execute(
        "DELETE FROM platform_api_keys WHERE id = ?1",
        params![key_id],
    )
    .map_err(|e| e.to_string())?;

    // If deleted key was active, activate the next available key
    if was_active {
        if let Some(pid) = platform_id {
            let next_id: Option<String> = conn.query_row(
                "SELECT id FROM platform_api_keys WHERE platform_id = ?1 ORDER BY created_at ASC LIMIT 1",
                params![pid], |r| r.get(0),
            ).ok();
            if let Some(nid) = next_id {
                let _ = select_platform_api_key_core(&db, &nid);
            } else {
                // No keys left — clear the active key
                let _ = conn.execute(
                    "UPDATE model_platforms SET api_key = '' WHERE id = ?1",
                    params![pid],
                );
            }
        }
    }

    Ok(())
}

/// Reveal a platform API key (decrypt and return full value)
#[tauri::command]
pub fn reveal_platform_api_key(
    key_id: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<String, String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let encrypted: String = conn
        .query_row(
            "SELECT encrypted_key FROM platform_api_keys WHERE id = ?1",
            params![key_id],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    Ok(crate::crypto::decrypt(&encrypted))
}

#[cfg(test)]
mod key_storage_tests {
    use super::*;
    use crate::db::DbManager;
    // 迁移和取 Key 的两个函数还留在 platforms.rs（它们服务的是平台/路由那边）。
    // 放在测试模块里导入而不是文件顶部：顶部导入在非测试构建下是未使用的，会破坏
    // 「0 warning」这道门。
    use crate::commands::{migrate_legacy_plaintext_keys, platform_keys, ModelPlatform};

    fn temp_db(tag: &str) -> Arc<DbManager> {
        let path = std::env::temp_dir().join(format!(
            "omnix_keys_{tag}_{}_{}.sqlite",
            std::process::id(),
            chrono::Utc::now().timestamp_micros()
        ));
        let _ = std::fs::remove_file(&path);
        Arc::new(DbManager::new_with_path(path))
    }

    /// 模拟老版本留下的库：Key 明文躺在 `model_platforms.api_key`。
    fn seed_legacy(db: &DbManager, platform: &str, legacy_value: &str) {
        let conn = db.get_connection().expect("db");
        conn.execute(
            "INSERT INTO model_platforms (id, name, api_type, api_key, api_address, is_enabled)
             VALUES (?1, 'P', 'openai', ?2, 'http://x', 1)",
            params![platform, legacy_value],
        )
        .expect("插入平台");
    }

    fn legacy_column(db: &DbManager, platform: &str) -> String {
        db.get_connection()
            .expect("db")
            .query_row(
                "SELECT COALESCE(api_key,'') FROM model_platforms WHERE id = ?1",
                params![platform],
                |r| r.get::<_, String>(0),
            )
            .unwrap_or_default()
    }

    /// 迁移之后：旧列必须空，Key 必须还能用，落库的必须是密文。
    ///
    /// 三条缺一不可——只清空旧列会丢 Key，只搬不清空等于没修，搬过去还是明文
    /// 则白搬一趟。
    #[test]
    fn plaintext_keys_move_into_encrypted_storage_and_the_old_column_is_cleared() {
        let db = temp_db("migrate");
        seed_legacy(&db, "p1", "sk-plain-secret-123");

        assert_eq!(migrate_legacy_plaintext_keys(&db).expect("迁移"), 1);

        assert_eq!(legacy_column(&db, "p1"), "", "旧列必须被清空");
        let (keys, _) = platform_keys(&db, "p1");
        assert_eq!(keys, vec!["sk-plain-secret-123".to_string()], "迁移后 Key 必须还能解出来");

        let stored: String = db
            .get_connection()
            .unwrap()
            .query_row(
                "SELECT encrypted_key FROM platform_api_keys WHERE platform_id = 'p1'",
                [],
                |r| r.get(0),
            )
            .expect("新表里应当有这条");
        assert!(stored.starts_with("ENC:"), "落库的必须是密文，实际：{stored}");
        assert!(!stored.contains("sk-plain-secret-123"), "密文里不该出现明文");
    }

    /// 旧列历来允许逗号分隔多个 Key，一个都不能漏。
    #[test]
    fn comma_separated_legacy_keys_all_migrate() {
        let db = temp_db("multi");
        seed_legacy(&db, "p2", "sk-a , sk-b,sk-c");
        assert_eq!(migrate_legacy_plaintext_keys(&db).expect("迁移"), 3);
        let (keys, _) = platform_keys(&db, "p2");
        assert_eq!(keys.len(), 3, "{keys:?}");
        assert!(keys.contains(&"sk-b".to_string()), "{keys:?}");
    }

    /// 迁移跑两遍不能把 Key 变成两份——上一次可能跑到一半就崩了。
    #[test]
    fn migrating_twice_is_idempotent() {
        let db = temp_db("twice");
        seed_legacy(&db, "p3", "sk-once");
        assert_eq!(migrate_legacy_plaintext_keys(&db).expect("第一遍"), 1);
        assert_eq!(migrate_legacy_plaintext_keys(&db).expect("第二遍"), 0, "第二遍不该再搬");
        assert_eq!(platform_keys(&db, "p3").0.len(), 1);
    }

    /// **防复发的那一条。**
    ///
    /// `get_model_platforms` 的返回值会整个过 IPC 到前端。以后谁把 `api_key`
    /// 加回那条 SELECT，这里立刻红——这正是当初出问题的方式：读的一半迁走了、
    /// 写的一半留在原地，没有任何东西拦住。
    #[test]
    fn the_platform_list_never_carries_a_key_to_the_frontend() {
        let db = temp_db("noleak");
        seed_legacy(&db, "p4", "sk-must-not-leak");

        // 同时把 Key 正经放进加密表——证明「新表里有 Key」也不会让它漏出去。
        let conn = db.get_connection().unwrap();
        conn.execute(
            "INSERT INTO platform_api_keys (id, platform_id, encrypted_key, label, is_active)
             VALUES ('k1', 'p4', ?1, 'x', 1)",
            params![crate::crypto::encrypt("sk-must-not-leak")],
        )
        .unwrap();
        drop(conn);

        let mut stmt_db = db.get_connection().unwrap();
        let _ = &mut stmt_db;
        // 直接复刻命令体：`State` 在单测里造不出来，但 SQL 和映射是同一份。
        let conn = db.get_connection().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT id, name, api_type, api_address, is_enabled
                 FROM model_platforms ORDER BY priority DESC, name",
            )
            .expect("这条 SQL 必须不含 api_key");
        let rows: Vec<ModelPlatform> = stmt
            .query_map([], |row| {
                let is_enabled_int: i32 = row.get(4)?;
                Ok(ModelPlatform {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    api_type: row.get(2)?,
                    api_key: String::new(),
                    api_address: row.get(3)?,
                    is_enabled: is_enabled_int != 0,
                    // 这条测试只关心 api_key 有没有被查回来，路由权重取默认即可。
                    weight: 1,
                    priority: 0,
                })
            })
            .unwrap()
            .flatten()
            .collect();

        let payload = serde_json::to_string(&rows).expect("序列化");
        assert!(
            !payload.contains("sk-must-not-leak"),
            "平台列表把 Key 带给前端了：{payload}"
        );

        // 光看 SQL 不够——命令体本身也不能再引用 api_key 列。
        let source = include_str!("platforms.rs");
        let body = source
            .split_once("pub fn get_model_platforms")
            .and_then(|(_, rest)| rest.split_once("\n}"))
            .map(|(body, _)| body)
            .expect("找不到 get_model_platforms 的函数体");
        assert!(
            !body.contains("api_key, api_address"),
            "get_model_platforms 又把 api_key 查回来了"
        );
    }

    /// 选中一条 Key 绝不能把明文写回旧列。备份会导出 `model_platforms`，
    /// 启动迁移刚清空的明文如果在这里回来，加密等于没做。
    #[test]
    fn selecting_a_key_does_not_write_plaintext_to_the_legacy_column() {
        let db = temp_db("select-plain");
        seed_legacy(&db, "p5", "");
        let secret = "sk-must-stay-encrypted-on-select";
        let conn = db.get_connection().unwrap();
        conn.execute(
            "INSERT INTO platform_api_keys (id, platform_id, encrypted_key, label, is_active)
             VALUES ('k-sel', 'p5', ?1, 'x', 0)",
            params![crate::crypto::encrypt(secret)],
        )
        .unwrap();
        drop(conn);

        select_platform_api_key_core(&db, "k-sel").expect("选择");
        let stored = legacy_column(&db, "p5");
        assert!(
            stored.starts_with("ENC:"),
            "旧列必须仍是密文，实际：{stored}"
        );
        assert!(
            !stored.contains(secret),
            "选择 Key 把明文写回了旧列：{stored}"
        );
        let (keys, _) = platform_keys(&db, "p5");
        assert_eq!(keys, vec![secret.to_string()]);
    }
}

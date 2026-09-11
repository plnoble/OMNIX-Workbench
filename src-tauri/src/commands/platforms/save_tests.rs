use super::*;

fn fixture(tag: &str) -> DbManager {
    let path = std::env::temp_dir().join(format!(
        "omnix_platform_save_{tag}_{}_{}.sqlite",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap(),
    ));
    DbManager::new_with_path(path)
}

fn platform(id: &str, key: &str) -> ModelPlatform {
    ModelPlatform {
        id: id.into(),
        name: "Test provider".into(),
        api_type: "openai".into(),
        api_key: key.into(),
        api_address: "https://provider.example/v1".into(),
        is_enabled: true,
        weight: 3,
        priority: 7,
    }
}

#[test]
fn previous_insert_reproduces_the_reported_not_null_error() {
    let db = fixture("old-insert");
    let conn = db.get_connection().unwrap();
    let error = conn.execute(
        "INSERT INTO model_platforms (id, name, api_type, api_address, is_enabled)
         VALUES ('p', 'Test', 'openai', 'https://provider.example/v1', 1)",
        [],
    ).unwrap_err();
    assert_eq!(error.to_string(), "NOT NULL constraint failed: model_platforms.api_key");
}

#[test]
fn saving_a_provider_encrypts_the_initial_key_and_is_retry_safe() {
    let db = fixture("create");
    let p = platform("p-create", "  sk-fixture-secret  ");
    save_model_platform_core(&db, &p).unwrap();
    save_model_platform_core(&db, &p).unwrap();
    let conn = db.get_connection().unwrap();
    let (legacy, weight, priority): (String, i32, i32) = conn.query_row(
        "SELECT api_key, weight, priority FROM model_platforms WHERE id = ?1",
        params![p.id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    ).unwrap();
    assert_eq!((weight, priority), (3, 7));
    let (count, encrypted, active): (i64, String, i32) = conn.query_row(
        "SELECT COUNT(*), encrypted_key, is_active FROM platform_api_keys WHERE platform_id = ?1",
        params![p.id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    ).unwrap();
    assert_eq!((count, active), (1, 1));
    assert_eq!(legacy, encrypted);
    assert!(encrypted.starts_with("ENC:v2:"));
    assert!(!encrypted.contains("sk-fixture-secret"));
    assert_eq!(crate::crypto::decrypt(&encrypted), "sk-fixture-secret");
}

#[test]
fn saving_a_local_provider_without_a_key_does_not_require_a_schema_migration() {
    let db = fixture("local");
    let mut p = platform("p-local", "");
    p.api_type = "ollama".into();
    p.api_address = "http://localhost:11434".into();
    save_model_platform_core(&db, &p).unwrap();
    assert_eq!(platform_connection_config(&db, &p.id).unwrap(),
        (p.api_type, String::new(), p.api_address));
    assert!(platform_keys(&db, &p.id).0.is_empty());
}

#[test]
fn metadata_edits_and_toggles_preserve_existing_keys_models_and_routing() {
    let db = fixture("edit");
    let mut p = platform("p-edit", "sk-keep-me");
    save_model_platform_core(&db, &p).unwrap();
    let conn = db.get_connection().unwrap();
    let cipher = crate::crypto::encrypt("sk-keep-me");
    conn.execute("UPDATE model_platforms SET api_key = ?1 WHERE id = ?2", params![cipher, p.id]).unwrap();
    conn.execute("INSERT INTO platform_models (id, platform_id, model_name) VALUES ('m1', ?1, 'test-model')", params![p.id]).unwrap();
    drop(conn);

    p.name = "Renamed".into();
    p.api_key.clear();
    p.is_enabled = false;
    p.weight = 1;
    p.priority = 0;
    save_model_platform_core(&db, &p).unwrap();
    let conn = db.get_connection().unwrap();
    let state: (String, i32, String, i32, i32) = conn.query_row(
        "SELECT name, is_enabled, api_key, weight, priority FROM model_platforms WHERE id = ?1",
        params![p.id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
    ).unwrap();
    assert_eq!(state, ("Renamed".into(), 0, cipher, 3, 7));
    assert_eq!(conn.query_row("SELECT COUNT(*) FROM platform_models WHERE id = 'm1'", [], |r| r.get::<_, i64>(0)).unwrap(), 1);
    assert_eq!(platform_keys(&db, &p.id).0, vec!["sk-keep-me"]);
}

#[test]
fn key_insert_failure_rolls_back_the_platform_and_is_reported() {
    let db = fixture("rollback");
    let conn = db.get_connection().unwrap();
    conn.execute_batch(
        "CREATE TRIGGER reject_fixture_key BEFORE INSERT ON platform_api_keys
         BEGIN SELECT RAISE(ABORT, 'fixture key storage failure'); END;",
    ).unwrap();
    let error = save_model_platform_core(&db, &platform("p-fail", "sk-fixture")).unwrap_err();
    assert!(error.contains("fixture key storage failure"));
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM model_platforms WHERE id = 'p-fail'", [], |r| r.get(0)).unwrap();
    assert_eq!(count, 0);
    conn.execute_batch("DROP TRIGGER reject_fixture_key").unwrap();
    save_model_platform_core(&db, &platform("p-fail", "sk-fixture")).unwrap();
    assert_eq!(platform_keys(&db, "p-fail").0, vec!["sk-fixture"]);
}

#[test]
fn compatibility_key_write_failure_rolls_back_both_records() {
    let db = fixture("mirror-rollback");
    let conn = db.get_connection().unwrap();
    conn.execute_batch(
        "CREATE TRIGGER reject_fixture_mirror BEFORE UPDATE OF api_key ON model_platforms
         BEGIN SELECT RAISE(ABORT, 'fixture compatibility storage failure'); END;",
    ).unwrap();
    let error = save_model_platform_core(&db, &platform("p-mirror-fail", "sk-fixture")).unwrap_err();
    assert!(error.contains("fixture compatibility storage failure"));
    let platforms: i64 = conn.query_row("SELECT COUNT(*) FROM model_platforms WHERE id = 'p-mirror-fail'", [], |r| r.get(0)).unwrap();
    let keys: i64 = conn.query_row("SELECT COUNT(*) FROM platform_api_keys WHERE platform_id = 'p-mirror-fail'", [], |r| r.get(0)).unwrap();
    assert_eq!((platforms, keys), (0, 0));
}

#[test]
fn discovery_and_batch_configuration_keep_the_active_key_after_startup_migration() {
    let db = fixture("restart");
    let p = platform("p-restart", "sk-first");
    save_model_platform_core(&db, &p).unwrap();
    let conn = db.get_connection().unwrap();
    let active = crate::crypto::encrypt("sk-selected");
    conn.execute("UPDATE platform_api_keys SET is_active = 0 WHERE platform_id = ?1", params![p.id]).unwrap();
    conn.execute("INSERT INTO platform_api_keys (id, platform_id, encrypted_key, is_active) VALUES ('k-selected', ?1, ?2, 1)", params![p.id, active]).unwrap();
    conn.execute("UPDATE model_platforms SET api_key = ?1 WHERE id = ?2", params![active, p.id]).unwrap();
    drop(conn);
    migrate_legacy_plaintext_keys(&db).unwrap();
    assert_eq!(platform_connection_config(&db, &p.id).unwrap(),
        (p.api_type, "sk-selected".into(), p.api_address));
    assert_eq!(platform_keys(&db, &p.id).0.len(), 2);
}

#[test]
fn disabled_keys_are_not_used_and_do_not_fall_back_to_ciphertext() {
    let db = fixture("disabled");
    let p = platform("p-disabled", "sk-active");
    save_model_platform_core(&db, &p).unwrap();
    let conn = db.get_connection().unwrap();
    conn.execute(
        "UPDATE platform_api_keys SET is_enabled = 0 WHERE platform_id = ?1",
        params![p.id],
    ).unwrap();
    conn.execute(
        "UPDATE model_platforms SET api_key = ?1 WHERE id = ?2",
        params![crate::crypto::encrypt("sk-must-not-leak"), p.id],
    ).unwrap();
    drop(conn);

    let keys = platform_keys(&db, &p.id).0;
    assert!(keys.is_empty(), "disabled keys must not be returned: {keys:?}");
    let (_, resolved, _) = platform_connection_config(&db, &p.id).unwrap();
    assert_eq!(resolved, "");
    assert!(!resolved.contains("ENC:"));
}

#[test]
fn deleting_a_platform_removes_its_encrypted_keys_in_the_same_transaction() {
    let db = fixture("delete-keys");
    let p = platform("p-delete", "sk-one");
    save_model_platform_core(&db, &p).unwrap();
    let conn = db.get_connection().unwrap();
    conn.execute(
        "INSERT INTO platform_api_keys (id, platform_id, encrypted_key, label, is_active)
         VALUES ('k-two', ?1, ?2, '备用', 0)",
        params![p.id, crate::crypto::encrypt("sk-two")],
    ).unwrap();
    conn.execute(
        "INSERT INTO platform_models (id, platform_id, model_name) VALUES ('m1', ?1, 'test-model')",
        params![p.id],
    ).unwrap();
    drop(conn);

    delete_model_platform_core(&db, &p.id).unwrap();
    let conn = db.get_connection().unwrap();
    let platforms: i64 = conn.query_row(
        "SELECT COUNT(*) FROM model_platforms WHERE id = ?1",
        params![p.id], |r| r.get(0),
    ).unwrap();
    let keys: i64 = conn.query_row(
        "SELECT COUNT(*) FROM platform_api_keys WHERE platform_id = ?1",
        params![p.id], |r| r.get(0),
    ).unwrap();
    let models: i64 = conn.query_row(
        "SELECT COUNT(*) FROM platform_models WHERE platform_id = ?1",
        params![p.id], |r| r.get(0),
    ).unwrap();
    assert_eq!((platforms, keys, models), (0, 0, 0));
}

#[test]
fn platform_delete_rolls_back_when_key_cleanup_fails() {
    let db = fixture("delete-rollback");
    let p = platform("p-keep", "sk-keep");
    save_model_platform_core(&db, &p).unwrap();
    let conn = db.get_connection().unwrap();
    conn.execute_batch(
        "CREATE TRIGGER reject_key_cleanup BEFORE DELETE ON platform_api_keys
         BEGIN SELECT RAISE(ABORT, 'fixture key cleanup failure'); END;",
    ).unwrap();
    drop(conn);

    let error = delete_model_platform_core(&db, &p.id).unwrap_err();
    assert!(error.contains("fixture key cleanup failure"));
    let conn = db.get_connection().unwrap();
    let platforms: i64 = conn.query_row(
        "SELECT COUNT(*) FROM model_platforms WHERE id = ?1",
        params![p.id], |r| r.get(0),
    ).unwrap();
    let keys: i64 = conn.query_row(
        "SELECT COUNT(*) FROM platform_api_keys WHERE platform_id = ?1",
        params![p.id], |r| r.get(0),
    ).unwrap();
    assert_eq!((platforms, keys), (1, 1));
    assert_eq!(platform_keys(&db, &p.id).0, vec!["sk-keep"]);
}

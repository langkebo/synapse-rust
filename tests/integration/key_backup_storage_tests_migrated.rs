use serde_json::json;
use std::sync::Arc;

use sqlx::PgPool;
use synapse_common::test_isolation::IsolatedTestPool;
use synapse_rust::e2ee::backup::models::KeyBackup;
use synapse_rust::e2ee::backup::storage::{BackupKeyInsertParams, BackupKeyStorage, KeyBackupStorage};

/// The workspace baseline migration, compiled in so the per-test schema is a
/// clone of the migrated v12 template.
///
/// The bytes are load-bearing: the shared template's name is a content
/// fingerprint of this string, so it must stay byte-identical to the copies in
/// `synapse-storage/src/test_isolation.rs`, `synapse-e2ee/src/verification/service.rs`
/// and `synapse-services/src/test_utils.rs`.
const BASELINE_SQL: &str = include_str!("../../migrations/00000000_unified_schema_v12.sql");

/// Per-test schema cloned from the migrated v12 baseline (D-36 口径).
///
/// **D-47**: this file used to self-build `key_backups` / `backup_keys`, and
/// that simplified schema diverged from the baseline in ways no gate could see
/// (guard A scans only `src/`, guard B only production INSERTs):
///   * it had no `fk_backup_keys_room` (the baseline's P3-3 adds
///     `backup_keys.room_id → rooms(room_id) ON DELETE CASCADE`);
///   * `first_message_index` was nullable, while the baseline is
///     `BIGINT NOT NULL DEFAULT 0`.
///
/// That is exactly what let D-46 (`key_backups.version` nullable vs the
/// non-`Option` row type) hide behind the dynamic `FromRow` path. Running on the
/// template makes both the constraints and the nullability real.
async fn setup_test_database() -> (IsolatedTestPool, Arc<PgPool>) {
    let isolated = IsolatedTestPool::new(BASELINE_SQL).await.expect("isolated test pool");
    let pool = isolated.pool();
    (isolated, pool)
}

#[tokio::test]
async fn test_key_backup_lifecycle() {
    let (_isolated, pool) = setup_test_database().await;
    let storage = KeyBackupStorage::new(&pool);
    let key_storage = BackupKeyStorage::new(&pool);

    let user_id = "@alice:localhost";
    let room_id = "!room:localhost";

    // `backup_keys` carries `fk_backup_keys_room` (P3-3) in the real baseline,
    // so the room must exist before any key can be uploaded.
    sqlx::query("INSERT INTO rooms (room_id, created_ts) VALUES ($1, $2)")
        .bind(room_id)
        .bind(0_i64)
        .execute(&*pool)
        .await
        .expect("create the room the backup keys reference");

    let backup = KeyBackup {
        user_id: user_id.to_string(),
        backup_id: "backup_1".to_string(),
        version: 1,
        algorithm: "m.megolm_backup.v1.curve25519-aes-sha2".to_string(),
        auth_key: "auth_key".to_string(),
        mgmt_key: "mgmt_key".to_string(),
        backup_data: json!({"public_key": "pubkey"}),
        etag: Some("etag1".to_string()),
    };

    // Create backup
    storage.create_backup(&backup).await.unwrap();

    // Get backup
    let fetched = storage.get_backup(user_id).await.unwrap().unwrap();
    assert_eq!(fetched.backup_id, "backup_1");
    assert_eq!(fetched.version, 1);

    // Upload key
    let key_params = BackupKeyInsertParams {
        user_id: user_id.to_string(),
        backup_id: "backup_1".to_string(),
        room_id: room_id.to_string(),
        session_id: "session1".to_string(),
        first_message_index: 0,
        forwarded_count: 0,
        is_verified: true,
        backup_data: json!({"key": "data"}),
    };
    key_storage.upload_backup_key(key_params).await.unwrap();

    // Get room keys
    let keys = key_storage.get_room_backup_keys(user_id, room_id).await.unwrap();
    assert_eq!(keys.len(), 1);
    assert_eq!(keys[0].session_id, "session1");
    assert_eq!(keys[0].first_message_index, 0);
    assert!(keys[0].is_verified);
    assert_eq!(keys[0].session_data, json!({"key": "data"}));
}

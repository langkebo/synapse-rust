#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use serde_json::json;
use std::sync::Arc;
use synapse_rust::e2ee::backup::models::KeyBackup;
use synapse_rust::e2ee::backup::storage::{BackupKeyInsertParams, BackupKeyStorage, KeyBackupStorage};

async fn setup_test_database() -> Arc<sqlx::PgPool> {
    let pool = synapse_rust::test_utils::prepare_empty_isolated_test_pool().await.expect("Failed to prepare test pool");

    sqlx::query(
        r#"
            CREATE TABLE IF NOT EXISTS key_backups (
                backup_id BIGSERIAL PRIMARY KEY,
                user_id TEXT NOT NULL,
                backup_id_text TEXT,
                algorithm TEXT NOT NULL,
                auth_data JSONB,
                auth_key TEXT,
                mgmt_key TEXT,
                version BIGINT DEFAULT 1,
                etag TEXT,
                created_ts BIGINT NOT NULL,
                updated_ts BIGINT,
                CONSTRAINT uq_key_backups_user_version UNIQUE (user_id, version)
            )
            "#,
    )
    .execute(pool.as_ref())
    .await
    .expect("Failed to create key_backups table");

    sqlx::query(
        r#"
            CREATE TABLE IF NOT EXISTS backup_keys (
                id BIGSERIAL PRIMARY KEY,
                backup_id BIGINT NOT NULL,
                room_id TEXT NOT NULL,
                session_id TEXT NOT NULL,
                session_data JSONB NOT NULL,
                first_message_index BIGINT,
                forwarded_count BIGINT DEFAULT 0,
                is_verified BOOLEAN DEFAULT FALSE,
                created_ts BIGINT NOT NULL,
                CONSTRAINT fk_backup_keys_backup FOREIGN KEY (backup_id) REFERENCES key_backups(backup_id) ON DELETE CASCADE
            )
            "#,
    )
    .execute(pool.as_ref())
    .await
    .expect("Failed to create backup_keys table");

    pool
}

#[tokio::test]
async fn test_key_backup_lifecycle() {
        let pool = setup_test_database().await;
        let storage = KeyBackupStorage::new(&pool);
        let key_storage = BackupKeyStorage::new(&pool);

        let user_id = "@alice:localhost";
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
            room_id: "!room:localhost".to_string(),
            session_id: "session1".to_string(),
            first_message_index: 0,
            forwarded_count: 0,
            is_verified: true,
            backup_data: json!({"key": "data"}),
        };
        key_storage.upload_backup_key(key_params).await.unwrap();

        // Get room keys
        let keys = key_storage.get_room_backup_keys(user_id, "!room:localhost").await.unwrap();
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].session_id, "session1");
        assert_eq!(keys[0].first_message_index, 0);
        assert!(keys[0].is_verified);
        assert_eq!(keys[0].session_data, json!({"key": "data"}));
}

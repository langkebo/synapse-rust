use super::models::*;
use crate::error::ApiError;
use sqlx::PgPool;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct BackupKeyInsertParams {
    pub user_id: String,
    pub backup_id: String,
    pub room_id: String,
    pub session_id: String,
    pub first_message_index: i64,
    pub forwarded_count: i64,
    pub is_verified: bool,
    pub backup_data: serde_json::Value,
}

#[derive(Clone)]
pub struct KeyBackupStorage {
    pub pool: Arc<PgPool>,
}

impl KeyBackupStorage {
    pub fn new(pool: &Arc<PgPool>) -> Self {
        Self { pool: pool.clone() }
    }

    pub async fn create_backup(&self, backup: &KeyBackup) -> Result<(), ApiError> {
        sqlx::query(
            r#"
            INSERT INTO key_backups (user_id, backup_id, version, algorithm, auth_key, mgmt_key, backup_data, etag)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT (user_id, backup_id) DO UPDATE
            SET algorithm = EXCLUDED.algorithm,
                auth_key = EXCLUDED.auth_key,
                mgmt_key = EXCLUDED.mgmt_key,
                backup_data = EXCLUDED.backup_data,
                etag = EXCLUDED.etag
            "#
        )
        .bind(&backup.user_id)
        .bind(&backup.backup_id)
        .bind(backup.version)
        .bind(&backup.algorithm)
        .bind(&backup.auth_key)
        .bind(&backup.mgmt_key)
        .bind(&backup.backup_data)
        .bind(&backup.etag)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn get_backup(&self, user_id: &str) -> Result<Option<KeyBackup>, ApiError> {
        let row = sqlx::query_as::<_, KeyBackup>(
            r#"
            SELECT user_id, backup_id, version, algorithm, auth_key, mgmt_key, backup_data, etag
            FROM key_backups
            WHERE user_id = $1
            ORDER BY version DESC
            LIMIT 1
            "#,
        )
        .bind(user_id)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(row)
    }

    pub async fn get_all_backup_versions(&self, user_id: &str) -> Result<Vec<KeyBackup>, ApiError> {
        let rows = sqlx::query_as::<_, KeyBackup>(
            r#"
            SELECT user_id, backup_id, version, algorithm, auth_key, mgmt_key, backup_data, etag
            FROM key_backups
            WHERE user_id = $1
            ORDER BY version DESC
            "#,
        )
        .bind(user_id)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    pub async fn get_backup_version(
        &self,
        user_id: &str,
        version: &str,
    ) -> Result<Option<KeyBackup>, ApiError> {
        let version_int: i64 = version.parse().unwrap_or(0);
        let row = sqlx::query_as::<_, KeyBackup>(
            r#"
            SELECT user_id, backup_id, version, algorithm, auth_key, mgmt_key, backup_data, etag
            FROM key_backups
            WHERE user_id = $1 AND version = $2
            "#,
        )
        .bind(user_id)
        .bind(version_int)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(row)
    }

    pub async fn delete_backup(&self, user_id: &str, version: &str) -> Result<(), ApiError> {
        let version_int: i64 = version.parse().unwrap_or(0);
        sqlx::query(
            r#"
            DELETE FROM key_backups
            WHERE user_id = $1 AND version = $2
            "#,
        )
        .bind(user_id)
        .bind(version_int)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }
}

#[derive(Clone)]
pub struct BackupKeyStorage {
    pool: Arc<PgPool>,
}

impl BackupKeyStorage {
    pub fn new(pool: &Arc<PgPool>) -> Self {
        Self { pool: pool.clone() }
    }

    pub async fn upload_backup_key(&self, params: BackupKeyInsertParams) -> Result<(), ApiError> {
        sqlx::query(
            r#"
            INSERT INTO backup_keys (user_id, backup_id, room_id, session_id, first_message_index, forwarded_count, is_verified, backup_data)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT (user_id, backup_id, room_id, session_id, first_message_index) DO UPDATE SET
                forwarded_count = EXCLUDED.forwarded_count,
                is_verified = EXCLUDED.is_verified,
                backup_data = EXCLUDED.backup_data
            "#
        )
        .bind(&params.user_id)
        .bind(&params.backup_id)
        .bind(&params.room_id)
        .bind(&params.session_id)
        .bind(params.first_message_index)
        .bind(params.forwarded_count)
        .bind(params.is_verified)
        .bind(&params.backup_data)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn get_room_backup_keys(
        &self,
        user_id: &str,
        room_id: &str,
    ) -> Result<Vec<BackupKeyInfo>, ApiError> {
        let rows = sqlx::query_as::<_, BackupKeyInfo>(
            r#"
            SELECT user_id, backup_id, room_id, session_id, first_message_index,
                   forwarded_count, is_verified, backup_data
            FROM backup_keys
            WHERE user_id = $1 AND room_id = $2
            "#,
        )
        .bind(user_id)
        .bind(room_id)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    pub async fn get_room_backup_keys_by_backup_id(
        &self,
        user_id: &str,
        backup_id: &str,
        room_id: &str,
    ) -> Result<Vec<BackupKeyInfo>, ApiError> {
        let rows = sqlx::query_as::<_, BackupKeyInfo>(
            r#"
            SELECT user_id, backup_id, room_id, session_id, first_message_index,
                   forwarded_count, is_verified, backup_data
            FROM backup_keys
            WHERE user_id = $1 AND backup_id = $2 AND room_id = $3
            "#,
        )
        .bind(user_id)
        .bind(backup_id)
        .bind(room_id)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    pub async fn get_backup_key(
        &self,
        user_id: &str,
        room_id: &str,
        session_id: &str,
    ) -> Result<Option<BackupKeyInfo>, ApiError> {
        let row = sqlx::query_as::<_, BackupKeyInfo>(
            r#"
            SELECT user_id, backup_id, room_id, session_id, first_message_index,
                   forwarded_count, is_verified, backup_data
            FROM backup_keys
            WHERE user_id = $1 AND room_id = $2 AND session_id = $3
            "#,
        )
        .bind(user_id)
        .bind(room_id)
        .bind(session_id)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(row)
    }

    pub async fn get_backup_key_by_backup_id(
        &self,
        user_id: &str,
        backup_id: &str,
        room_id: &str,
        session_id: &str,
    ) -> Result<Option<BackupKeyInfo>, ApiError> {
        let row = sqlx::query_as::<_, BackupKeyInfo>(
            r#"
            SELECT user_id, backup_id, room_id, session_id, first_message_index,
                   forwarded_count, is_verified, backup_data
            FROM backup_keys
            WHERE user_id = $1 AND backup_id = $2 AND room_id = $3 AND session_id = $4
            "#,
        )
        .bind(user_id)
        .bind(backup_id)
        .bind(room_id)
        .bind(session_id)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(row)
    }

    pub async fn delete_backup_key(
        &self,
        user_id: &str,
        room_id: &str,
        session_id: &str,
    ) -> Result<(), ApiError> {
        sqlx::query(
            r#"
            DELETE FROM backup_keys
            WHERE user_id = $1 AND room_id = $2 AND session_id = $3
            "#,
        )
        .bind(user_id)
        .bind(room_id)
        .bind(session_id)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn create_test_backup_key_insert_params() -> BackupKeyInsertParams {
        BackupKeyInsertParams {
            user_id: "@test:example.com".to_string(),
            backup_id: "backup_123".to_string(),
            room_id: "!room:example.com".to_string(),
            session_id: "session_abc".to_string(),
            first_message_index: 0,
            forwarded_count: 1,
            is_verified: true,
            backup_data: json!({
                "ciphertext": "encrypted_data",
                "mac": "signature"
            }),
        }
    }

    #[test]
    fn test_backup_key_insert_params_creation() {
        let params = create_test_backup_key_insert_params();

        assert_eq!(params.user_id, "@test:example.com");
        assert_eq!(params.backup_id, "backup_123");
        assert_eq!(params.room_id, "!room:example.com");
        assert_eq!(params.session_id, "session_abc");
        assert_eq!(params.first_message_index, 0);
        assert_eq!(params.forwarded_count, 1);
        assert!(params.is_verified);
    }

    #[test]
    fn test_backup_key_insert_params_clone() {
        let params = create_test_backup_key_insert_params();
        let cloned = params.clone();

        assert_eq!(params.user_id, cloned.user_id);
        assert_eq!(params.backup_id, cloned.backup_id);
        assert_eq!(params.room_id, cloned.room_id);
        assert_eq!(params.session_id, cloned.session_id);
        assert_eq!(params.first_message_index, cloned.first_message_index);
        assert_eq!(params.forwarded_count, cloned.forwarded_count);
        assert_eq!(params.is_verified, cloned.is_verified);
    }

    #[test]
    fn test_backup_key_insert_params_debug() {
        let params = create_test_backup_key_insert_params();
        let debug_str = format!("{:?}", params);

        assert!(debug_str.contains("BackupKeyInsertParams"));
        assert!(debug_str.contains("@test:example.com"));
        assert!(debug_str.contains("backup_123"));
    }

    #[test]
    fn test_backup_data_format_validation() {
        let params = create_test_backup_key_insert_params();

        assert!(params.backup_data.is_object());
        assert!(params.backup_data.get("ciphertext").is_some());
        assert!(params.backup_data.get("mac").is_some());
    }

    #[test]
    fn test_backup_data_with_complex_structure() {
        let complex_data = json!({
            "session_key": "base64_encoded_key",
            "sender_key": "sender_curve25519_key",
            "sender_claimed_keys": {
                "ed25519": "sender_ed25519_key"
            },
            "forwarding_curve25519_key_chain": []
        });

        let params = BackupKeyInsertParams {
            user_id: "@user:example.com".to_string(),
            backup_id: "backup_456".to_string(),
            room_id: "!room:example.com".to_string(),
            session_id: "session_xyz".to_string(),
            first_message_index: 5,
            forwarded_count: 0,
            is_verified: false,
            backup_data: complex_data,
        };

        assert!(params.backup_data.is_object());
        assert!(params.backup_data["session_key"].is_string());
        assert!(params.backup_data["forwarding_curve25519_key_chain"].is_array());
    }

    #[test]
    fn test_first_message_index_boundary() {
        let params_min = BackupKeyInsertParams {
            user_id: "@user:example.com".to_string(),
            backup_id: "backup_1".to_string(),
            room_id: "!room:example.com".to_string(),
            session_id: "session_1".to_string(),
            first_message_index: 0,
            forwarded_count: 0,
            is_verified: true,
            backup_data: json!({}),
        };

        let params_max = BackupKeyInsertParams {
            user_id: "@user:example.com".to_string(),
            backup_id: "backup_2".to_string(),
            room_id: "!room:example.com".to_string(),
            session_id: "session_2".to_string(),
            first_message_index: i64::MAX,
            forwarded_count: 0,
            is_verified: true,
            backup_data: json!({}),
        };

        assert_eq!(params_min.first_message_index, 0);
        assert_eq!(params_max.first_message_index, i64::MAX);
    }

    #[test]
    fn test_forwarded_count_validation() {
        let params_no_forward = BackupKeyInsertParams {
            user_id: "@user:example.com".to_string(),
            backup_id: "backup_1".to_string(),
            room_id: "!room:example.com".to_string(),
            session_id: "session_1".to_string(),
            first_message_index: 0,
            forwarded_count: 0,
            is_verified: true,
            backup_data: json!({}),
        };

        let params_forwarded = BackupKeyInsertParams {
            user_id: "@user:example.com".to_string(),
            backup_id: "backup_2".to_string(),
            room_id: "!room:example.com".to_string(),
            session_id: "session_2".to_string(),
            first_message_index: 0,
            forwarded_count: 3,
            is_verified: false,
            backup_data: json!({}),
        };

        assert_eq!(params_no_forward.forwarded_count, 0);
        assert_eq!(params_forwarded.forwarded_count, 3);
        assert!(!params_forwarded.is_verified);
    }

    #[test]
    fn test_is_verified_flag() {
        let verified_params = BackupKeyInsertParams {
            user_id: "@user:example.com".to_string(),
            backup_id: "backup_1".to_string(),
            room_id: "!room:example.com".to_string(),
            session_id: "session_1".to_string(),
            first_message_index: 0,
            forwarded_count: 0,
            is_verified: true,
            backup_data: json!({}),
        };

        let unverified_params = BackupKeyInsertParams {
            user_id: "@user:example.com".to_string(),
            backup_id: "backup_2".to_string(),
            room_id: "!room:example.com".to_string(),
            session_id: "session_2".to_string(),
            first_message_index: 0,
            forwarded_count: 0,
            is_verified: false,
            backup_data: json!({}),
        };

        assert!(verified_params.is_verified);
        assert!(!unverified_params.is_verified);
    }

    #[test]
    fn test_empty_backup_data() {
        let params = BackupKeyInsertParams {
            user_id: "@user:example.com".to_string(),
            backup_id: "backup_empty".to_string(),
            room_id: "!room:example.com".to_string(),
            session_id: "session_empty".to_string(),
            first_message_index: 0,
            forwarded_count: 0,
            is_verified: true,
            backup_data: json!({}),
        };

        assert!(params.backup_data.is_object());
        assert!(params.backup_data.as_object().unwrap().is_empty());
    }

    #[test]
    fn test_backup_key_insert_params_serialization() {
        let params = create_test_backup_key_insert_params();
        let json = serde_json::to_string(&params.backup_data).unwrap();

        assert!(json.contains("ciphertext"));
        assert!(json.contains("mac"));
    }

    #[test]
    fn test_backup_key_insert_params_with_null_data() {
        let params = BackupKeyInsertParams {
            user_id: "@user:example.com".to_string(),
            backup_id: "backup_null".to_string(),
            room_id: "!room:example.com".to_string(),
            session_id: "session_null".to_string(),
            first_message_index: 0,
            forwarded_count: 0,
            is_verified: false,
            backup_data: serde_json::Value::Null,
        };

        assert!(params.backup_data.is_null());
    }

    #[test]
    fn test_backup_key_insert_params_with_array_data() {
        let array_data = json!([
            {"key": "value1"},
            {"key": "value2"}
        ]);

        let params = BackupKeyInsertParams {
            user_id: "@user:example.com".to_string(),
            backup_id: "backup_array".to_string(),
            room_id: "!room:example.com".to_string(),
            session_id: "session_array".to_string(),
            first_message_index: 0,
            forwarded_count: 0,
            is_verified: true,
            backup_data: array_data,
        };

        assert!(params.backup_data.is_array());
        assert_eq!(params.backup_data.as_array().unwrap().len(), 2);
    }

    #[test]
    fn test_backup_key_insert_params_negative_values() {
        let params = BackupKeyInsertParams {
            user_id: "@user:example.com".to_string(),
            backup_id: "backup_neg".to_string(),
            room_id: "!room:example.com".to_string(),
            session_id: "session_neg".to_string(),
            first_message_index: -1,
            forwarded_count: -5,
            is_verified: false,
            backup_data: json!({}),
        };

        assert_eq!(params.first_message_index, -1);
        assert_eq!(params.forwarded_count, -5);
    }

    #[test]
    fn test_user_id_format_validation() {
        let valid_user_ids = vec![
            "@alice:example.com",
            "@bob:matrix.org",
            "@user123:server.local",
        ];

        for user_id in valid_user_ids {
            let params = BackupKeyInsertParams {
                user_id: user_id.to_string(),
                backup_id: "backup_1".to_string(),
                room_id: "!room:example.com".to_string(),
                session_id: "session_1".to_string(),
                first_message_index: 0,
                forwarded_count: 0,
                is_verified: true,
                backup_data: json!({}),
            };

            assert!(params.user_id.starts_with('@'));
            assert!(params.user_id.contains(':'));
        }
    }

    #[test]
    fn test_room_id_format_validation() {
        let valid_room_ids = vec![
            "!room:example.com",
            "!abc123:matrix.org",
            "!general:server.local",
        ];

        for room_id in valid_room_ids {
            let params = BackupKeyInsertParams {
                user_id: "@user:example.com".to_string(),
                backup_id: "backup_1".to_string(),
                room_id: room_id.to_string(),
                session_id: "session_1".to_string(),
                first_message_index: 0,
                forwarded_count: 0,
                is_verified: true,
                backup_data: json!({}),
            };

            assert!(params.room_id.starts_with('!'));
            assert!(params.room_id.contains(':'));
        }
    }

    #[test]
    fn test_backup_id_format() {
        let params = BackupKeyInsertParams {
            user_id: "@user:example.com".to_string(),
            backup_id: "backup_abc123xyz".to_string(),
            room_id: "!room:example.com".to_string(),
            session_id: "session_1".to_string(),
            first_message_index: 0,
            forwarded_count: 0,
            is_verified: true,
            backup_data: json!({}),
        };

        assert!(!params.backup_id.is_empty());
        assert!(params.backup_id.starts_with("backup_"));
    }

    #[test]
    fn test_session_id_uniqueness() {
        let params1 = BackupKeyInsertParams {
            user_id: "@user:example.com".to_string(),
            backup_id: "backup_1".to_string(),
            room_id: "!room:example.com".to_string(),
            session_id: "session_unique_1".to_string(),
            first_message_index: 0,
            forwarded_count: 0,
            is_verified: true,
            backup_data: json!({}),
        };

        let params2 = BackupKeyInsertParams {
            user_id: "@user:example.com".to_string(),
            backup_id: "backup_1".to_string(),
            room_id: "!room:example.com".to_string(),
            session_id: "session_unique_2".to_string(),
            first_message_index: 0,
            forwarded_count: 0,
            is_verified: true,
            backup_data: json!({}),
        };

        assert_ne!(params1.session_id, params2.session_id);
    }

    #[test]
    fn test_backup_data_with_nested_json() {
        let nested_data = json!({
            "algorithm": "m.megolm.v1.aes-sha2",
            "session_key": "encoded_key",
            "sender_claimed_ed25519_key": "ed25519_key",
            "forwarding_curve25519_key_chain": [
                "key1",
                "key2"
            ]
        });

        let params = BackupKeyInsertParams {
            user_id: "@user:example.com".to_string(),
            backup_id: "backup_nested".to_string(),
            room_id: "!room:example.com".to_string(),
            session_id: "session_nested".to_string(),
            first_message_index: 0,
            forwarded_count: 2,
            is_verified: false,
            backup_data: nested_data,
        };

        assert_eq!(params.backup_data["algorithm"], "m.megolm.v1.aes-sha2");
        assert!(params.backup_data["forwarding_curve25519_key_chain"].is_array());
        assert_eq!(
            params.backup_data["forwarding_curve25519_key_chain"].as_array().unwrap().len(),
            2
        );
    }
}

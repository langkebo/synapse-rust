use super::models::*;
use sqlx::PgPool;
use std::sync::Arc;
use synapse_common::current_timestamp_millis;
use synapse_common::ApiError;

#[derive(Debug, Clone)]
/// The `BackupKeyInsertParams` type.
pub struct BackupKeyInsertParams {
    /// The `user_id` field.
    /// The `backup_id` field.
    /// The `room_id` field.
    /// The `session_id` field.
    /// The `first_message_index` field.
    /// The `forwarded_count` field.
    /// The `is_verified` field.
    /// The `backup_data` field.
    pub user_id: String,
    /// The `backup_id` field.
    /// The `room_id` field.
    /// The `session_id` field.
    /// The `first_message_index` field.
    /// The `forwarded_count` field.
    /// The `is_verified` field.
    /// The `backup_data` field.
    pub backup_id: String,
    /// The `room_id` field.
    /// The `session_id` field.
    /// The `first_message_index` field.
    /// The `forwarded_count` field.
    /// The `is_verified` field.
    /// The `backup_data` field.
    pub room_id: String,
    /// The `session_id` field.
    /// The `first_message_index` field.
    /// The `forwarded_count` field.
    /// The `is_verified` field.
    /// The `backup_data` field.
    pub session_id: String,
    /// The `first_message_index` field.
    /// The `forwarded_count` field.
    /// The `is_verified` field.
    /// The `backup_data` field.
    pub first_message_index: i64,
    /// The `forwarded_count` field.
    /// The `is_verified` field.
    /// The `backup_data` field.
    pub forwarded_count: i64,
    /// The `is_verified` field.
    /// The `backup_data` field.
    pub is_verified: bool,
    /// The `backup_data` field.
    pub backup_data: serde_json::Value,
}

#[derive(Clone)]
/// The `KeyBackupStorage` type.
pub struct KeyBackupStorage {
    /// The `pool` field.
    pub pool: Arc<PgPool>,
}

/// Implementation of [`KeyBackupStorage`] methods.
impl KeyBackupStorage {
    /// See [`new`].
    pub fn new(pool: &Arc<PgPool>) -> Self {
        Self { pool: pool.clone() }
    }

    /// See [`create_backup`].
    pub async fn create_backup(&self, backup: &KeyBackup) -> Result<(), ApiError> {
        let now = current_timestamp_millis();
        sqlx::query!(
            r"
            INSERT INTO key_backups (
                user_id,
                backup_id_text,
                version,
                algorithm,
                auth_key,
                mgmt_key,
                auth_data,
                etag,
                created_ts,
                updated_ts
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $9)
            ON CONFLICT (user_id, version) DO UPDATE
            SET algorithm = EXCLUDED.algorithm,
                auth_key = EXCLUDED.auth_key,
                mgmt_key = EXCLUDED.mgmt_key,
                auth_data = EXCLUDED.auth_data,
                etag = EXCLUDED.etag,
                backup_id_text = EXCLUDED.backup_id_text,
                updated_ts = EXCLUDED.updated_ts
            ",
            &backup.user_id,
            &backup.backup_id,
            backup.version,
            &backup.algorithm,
            &backup.auth_key,
            &backup.mgmt_key,
            &backup.backup_data,
            backup.etag.as_deref(),
            now,
        )
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    /// See [`get_backup`].
    pub async fn get_backup(&self, user_id: &str) -> Result<Option<KeyBackup>, ApiError> {
        let row = sqlx::query_as!(
            KeyBackupRow,
            r#"
            SELECT
                user_id,
                COALESCE(backup_id_text, version::text) AS "backup_id!",
                version,
                algorithm,
                auth_key,
                mgmt_key,
                auth_data AS backup_data,
                etag
            FROM key_backups
            WHERE user_id = $1
            ORDER BY version DESC
            LIMIT 1
            "#,
            user_id,
        )
        .fetch_optional(&*self.pool)
        .await?;

        Ok(row.map(KeyBackup::from))
    }

    /// See [`get_all_backup_versions`].
    pub async fn get_all_backup_versions(&self, user_id: &str) -> Result<Vec<KeyBackup>, ApiError> {
        let rows = sqlx::query_as!(
            KeyBackupRow,
            r#"
            SELECT
                user_id,
                COALESCE(backup_id_text, version::text) AS "backup_id!",
                version,
                algorithm,
                auth_key,
                mgmt_key,
                auth_data AS backup_data,
                etag
            FROM key_backups
            WHERE user_id = $1
            ORDER BY version DESC
            "#,
            user_id,
        )
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows.into_iter().map(KeyBackup::from).collect())
    }

    /// See [`get_backup_version`].
    pub async fn get_backup_version(&self, user_id: &str, version: &str) -> Result<Option<KeyBackup>, ApiError> {
        // E-05: instead of `version.parse().unwrap_or(0)` (which silently
        // degrades UUID or other non-numeric versions to a lookup of
        // `version = 0`), branch on whether the string is parseable as i64.
        // Pure-numeric versions hit the i64 index; everything else falls
        // through to the text-equality path against `backup_id_text` (which
        // stores the original string for non-numeric versions).
        if let Ok(version_int) = version.parse::<i64>() {
            let row = sqlx::query_as!(
                KeyBackupRow,
                r#"
                SELECT
                    user_id,
                    COALESCE(backup_id_text, version::text) AS "backup_id!",
                    version,
                    algorithm,
                    auth_key,
                    mgmt_key,
                    auth_data AS backup_data,
                    etag
                FROM key_backups
                WHERE user_id = $1 AND version = $2
                "#,
                user_id,
                version_int,
            )
            .fetch_optional(&*self.pool)
            .await?;
            Ok(row.map(KeyBackup::from))
        } else {
            let row = sqlx::query_as!(
                KeyBackupRow,
                r#"
                SELECT
                    user_id,
                    COALESCE(backup_id_text, version::text) AS "backup_id!",
                    version,
                    algorithm,
                    auth_key,
                    mgmt_key,
                    auth_data AS backup_data,
                    etag
                FROM key_backups
                WHERE user_id = $1 AND backup_id_text = $2
                "#,
                user_id,
                version,
            )
            .fetch_optional(&*self.pool)
            .await?;
            Ok(row.map(KeyBackup::from))
        }
    }

    /// See [`delete_backup`].
    pub async fn delete_backup(&self, user_id: &str, version: &str) -> Result<(), ApiError> {
        // E-05: same fix as `get_backup_version` — branch on i64 vs text
        // rather than silently coercing non-numeric versions to 0.
        if let Ok(version_int) = version.parse::<i64>() {
            sqlx::query!(
                r"
                DELETE FROM key_backups
                WHERE user_id = $1 AND version = $2
                ",
                user_id,
                version_int,
            )
            .execute(&*self.pool)
            .await?;
        } else {
            sqlx::query!(
                r"
                DELETE FROM key_backups
                WHERE user_id = $1 AND backup_id_text = $2
                ",
                user_id,
                version,
            )
            .execute(&*self.pool)
            .await?;
        }

        Ok(())
    }
}

#[derive(Clone)]
/// The `BackupKeyStorage` type.
pub struct BackupKeyStorage {
    pool: Arc<PgPool>,
}

/// Implementation of [`BackupKeyStorage`] methods.
impl BackupKeyStorage {
    /// See [`new`].
    pub fn new(pool: &Arc<PgPool>) -> Self {
        Self { pool: pool.clone() }
    }

    /// See [`upload_backup_key`].
    pub async fn upload_backup_key(&self, params: BackupKeyInsertParams) -> Result<(), ApiError> {
        let mut tx = self.pool.begin().await?;

        sqlx::query!(
            r"
            DELETE FROM backup_keys
            WHERE backup_id IN (
                SELECT kb.backup_id
                FROM key_backups kb
                WHERE kb.user_id = $1
                  AND (kb.backup_id_text = $2 OR kb.version::text = $2)
            )
              AND room_id = $3
              AND session_id = $4
            ",
            &params.user_id,
            &params.backup_id,
            &params.room_id,
            &params.session_id,
        )
        .execute(&mut *tx)
        .await?;

        sqlx::query!(
            r"
            INSERT INTO backup_keys (
                backup_id, room_id, session_id, session_data, created_ts,
                first_message_index, forwarded_count, is_verified
            )
            SELECT kb.backup_id, $3, $4, $5, $6, $7, $8, $9
            FROM key_backups kb
            WHERE kb.user_id = $1
              AND (kb.backup_id_text = $2 OR kb.version::text = $2)
            ",
            &params.user_id,
            &params.backup_id,
            &params.room_id,
            &params.session_id,
            &params.backup_data,
            current_timestamp_millis(),
            params.first_message_index,
            params.forwarded_count,
            params.is_verified,
        )
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        Ok(())
    }

    /// See [`get_room_backup_keys`].
    pub async fn get_room_backup_keys(&self, user_id: &str, room_id: &str) -> Result<Vec<BackupKeyInfo>, ApiError> {
        let rows = sqlx::query_as!(
            BackupKeyInfo,
            r#"
            SELECT
                kb.user_id,
                COALESCE(kb.backup_id_text, kb.version::text) AS "backup_id!",
                bk.room_id,
                bk.session_id,
                bk.first_message_index,
                bk.forwarded_count,
                bk.is_verified,
                bk.session_data
            FROM backup_keys bk
            JOIN key_backups kb ON kb.backup_id = bk.backup_id
            WHERE kb.user_id = $1 AND bk.room_id = $2
            "#,
            user_id,
            room_id,
        )
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    /// See [`get_room_backup_keys_by_backup_id`].
    pub async fn get_room_backup_keys_by_backup_id(
        &self,
        user_id: &str,
        backup_id: &str,
        room_id: &str,
    ) -> Result<Vec<BackupKeyInfo>, ApiError> {
        let rows = sqlx::query_as!(
            BackupKeyInfo,
            r#"
            SELECT
                kb.user_id,
                COALESCE(kb.backup_id_text, kb.version::text) AS "backup_id!",
                bk.room_id,
                bk.session_id,
                bk.first_message_index,
                bk.forwarded_count,
                bk.is_verified,
                bk.session_data
            FROM backup_keys bk
            JOIN key_backups kb ON kb.backup_id = bk.backup_id
            WHERE kb.user_id = $1
              AND (kb.backup_id_text = $2 OR kb.version::text = $2)
              AND bk.room_id = $3
            "#,
            user_id,
            backup_id,
            room_id,
        )
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    /// See [`get_backup_keys_by_rooms`].
    pub async fn get_backup_keys_by_rooms(
        &self,
        user_id: &str,
        backup_id: &str,
        room_ids: &[String],
    ) -> Result<std::collections::HashMap<String, Vec<BackupKeyInfo>>, ApiError> {
        if room_ids.is_empty() {
            return Ok(std::collections::HashMap::new());
        }

        let rows = sqlx::query_as!(
            BackupKeyInfo,
            r#"
            SELECT
                kb.user_id,
                COALESCE(kb.backup_id_text, kb.version::text) AS "backup_id!",
                bk.room_id,
                bk.session_id,
                bk.first_message_index,
                bk.forwarded_count,
                bk.is_verified,
                bk.session_data
            FROM backup_keys bk
            JOIN key_backups kb ON kb.backup_id = bk.backup_id
            WHERE kb.user_id = $1
              AND (kb.backup_id_text = $2 OR kb.version::text = $2)
              AND bk.room_id = ANY($3)
            "#,
            user_id,
            backup_id,
            room_ids,
        )
        .fetch_all(&*self.pool)
        .await?;

        let mut result: std::collections::HashMap<String, Vec<BackupKeyInfo>> =
            room_ids.iter().map(|id| (id.clone(), Vec::new())).collect();

        for key in rows {
            if let Some(room_keys) = result.get_mut(&key.room_id) {
                room_keys.push(key);
            }
        }

        Ok(result)
    }

    /// See [`get_backup_key`].
    pub async fn get_backup_key(
        &self,
        user_id: &str,
        room_id: &str,
        session_id: &str,
    ) -> Result<Option<BackupKeyInfo>, ApiError> {
        let row = sqlx::query_as!(
            BackupKeyInfo,
            r#"
            SELECT
                kb.user_id,
                COALESCE(kb.backup_id_text, kb.version::text) AS "backup_id!",
                bk.room_id,
                bk.session_id,
                bk.first_message_index,
                bk.forwarded_count,
                bk.is_verified,
                bk.session_data
            FROM backup_keys bk
            JOIN key_backups kb ON kb.backup_id = bk.backup_id
            WHERE kb.user_id = $1 AND bk.room_id = $2 AND bk.session_id = $3
            "#,
            user_id,
            room_id,
            session_id,
        )
        .fetch_optional(&*self.pool)
        .await?;

        Ok(row)
    }

    /// See [`get_backup_key_by_backup_id`].
    pub async fn get_backup_key_by_backup_id(
        &self,
        user_id: &str,
        backup_id: &str,
        room_id: &str,
        session_id: &str,
    ) -> Result<Option<BackupKeyInfo>, ApiError> {
        let row = sqlx::query_as!(
            BackupKeyInfo,
            r#"
            SELECT
                kb.user_id,
                COALESCE(kb.backup_id_text, kb.version::text) AS "backup_id!",
                bk.room_id,
                bk.session_id,
                bk.first_message_index,
                bk.forwarded_count,
                bk.is_verified,
                bk.session_data
            FROM backup_keys bk
            JOIN key_backups kb ON kb.backup_id = bk.backup_id
            WHERE kb.user_id = $1
              AND (kb.backup_id_text = $2 OR kb.version::text = $2)
              AND bk.room_id = $3
              AND bk.session_id = $4
            "#,
            user_id,
            backup_id,
            room_id,
            session_id,
        )
        .fetch_optional(&*self.pool)
        .await?;

        Ok(row)
    }

    /// See [`delete_backup_key`].
    pub async fn delete_backup_key(&self, user_id: &str, room_id: &str, session_id: &str) -> Result<(), ApiError> {
        sqlx::query!(
            r"
            DELETE FROM backup_keys bk
            USING key_backups kb
            WHERE kb.backup_id = bk.backup_id
              AND kb.user_id = $1
              AND bk.room_id = $2
              AND bk.session_id = $3
            ",
            user_id,
            room_id,
            session_id,
        )
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    /// Spec-scoped delete: limit to the given backup version.
    pub async fn delete_session_for_version(
        &self,
        user_id: &str,
        version: &str,
        room_id: &str,
        session_id: &str,
    ) -> Result<u64, ApiError> {
        let result = sqlx::query!(
            r"
            DELETE FROM backup_keys bk
            USING key_backups kb
            WHERE kb.backup_id = bk.backup_id
              AND kb.user_id = $1
              AND (kb.backup_id_text = $2 OR kb.version::text = $2)
              AND bk.room_id = $3
              AND bk.session_id = $4
            ",
            user_id,
            version,
            room_id,
            session_id,
        )
        .execute(&*self.pool)
        .await?;

        Ok(result.rows_affected())
    }

    /// See [`delete_room_for_version`].
    pub async fn delete_room_for_version(&self, user_id: &str, version: &str, room_id: &str) -> Result<u64, ApiError> {
        let result = sqlx::query!(
            r"
            DELETE FROM backup_keys bk
            USING key_backups kb
            WHERE kb.backup_id = bk.backup_id
              AND kb.user_id = $1
              AND (kb.backup_id_text = $2 OR kb.version::text = $2)
              AND bk.room_id = $3
            ",
            user_id,
            version,
            room_id,
        )
        .execute(&*self.pool)
        .await?;

        Ok(result.rows_affected())
    }

    /// See [`delete_all_for_version`].
    pub async fn delete_all_for_version(&self, user_id: &str, version: &str) -> Result<u64, ApiError> {
        let result = sqlx::query!(
            r"
            DELETE FROM backup_keys bk
            USING key_backups kb
            WHERE kb.backup_id = bk.backup_id
              AND kb.user_id = $1
              AND (kb.backup_id_text = $2 OR kb.version::text = $2)
            ",
            user_id,
            version,
        )
        .execute(&*self.pool)
        .await?;

        Ok(result.rows_affected())
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
        let debug_str = format!("{params:?}");

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
        let valid_user_ids = vec!["@alice:example.com", "@bob:matrix.org", "@user123:server.local"];

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
        let valid_room_ids = vec!["!room:example.com", "!abc123:matrix.org", "!general:server.local"];

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
        assert_eq!(params.backup_data["forwarding_curve25519_key_chain"].as_array().unwrap().len(), 2);
    }

    // -------------------------------------------------------------------------
    // E-05 tests — version string parse branching
    // -------------------------------------------------------------------------

    /// Verifies the core of E-05: a non-numeric version string must NOT
    /// be silently coerced to i64::MAX or 0, and must instead fall through
    /// to the text-equality path. This is tested by asserting the parse
    /// branches correctly for a representative sample of inputs.
    #[test]
    fn test_e05_numeric_version_parses() {
        for version in ["1", "42", "999999", "0", "-1"] {
            assert!(version.parse::<i64>().is_ok(), "Numeric version '{version}' should parse as i64");
        }
    }

    #[test]
    fn test_e05_non_numeric_version_does_not_parse() {
        // E-05: These strings are valid backup version identifiers but are
        // NOT valid i64 literals. The old `unwrap_or(0)` would have treated
        // them all as version 0 — a guaranteed not-found or wrong-row lookup.
        for version in ["a1b2c3d4-e5f6-7890-abcd-ef1234567890", "v1", "2024-01-01", "1.0", "version_1"] {
            assert!(
                version.parse::<i64>().is_err(),
                "Non-numeric version '{version}' must NOT parse as i64 (E-05 invariant)"
            );
        }
    }

    #[test]
    fn test_e05_max_i64_still_parses() {
        let max = i64::MAX.to_string();
        assert_eq!(max.parse::<i64>().unwrap(), i64::MAX);
    }
}

/// DB round-trip on the **migration template** schema (D-36 / W5 口径).
///
/// Before C19b the `backup` module had **zero** DB coverage: every `test_` here
/// was a pure constructor, and the only DB exercise was
/// `tests/integration/key_backup_storage_tests_migrated.rs`, which builds its
/// own simplified `key_backups` / `backup_keys` (`version BIGINT DEFAULT 1`
/// without `NOT NULL`, nullable `first_message_index`). That is exactly the
/// D-36 anti-pattern that let **D-46** hide: the dynamic
/// `query_as::<_, KeyBackupRow>` + `FromRow` path never compared the row type
/// against the real catalog, so the nullable `version` and the
/// `COALESCE(backup_id_text, version::text)` projection looked fine.
///
/// This case runs the same storage APIs against the real v12 baseline, so any
/// schema/type/nullability mismatch surfaces on the first round trip.
#[cfg(test)]
mod db_tests {
    use super::*;
    use synapse_common::test_isolation::IsolatedTestPool;

    /// The workspace baseline migration, compiled in so the isolated schema is
    /// the real one. The bytes are load-bearing (the shared template name is a
    /// content fingerprint of this string), so it must stay byte-identical to
    /// the copies in `synapse-storage/src/test_isolation.rs`,
    /// `synapse-e2ee/src/verification/service.rs` and
    /// `synapse-services/src/test_utils.rs`.
    const BASELINE_SQL: &str = include_str!("../../../migrations/00000000_unified_schema_v12.sql");

    fn make_backup(user_id: &str, version: i64, etag: Option<&str>) -> KeyBackup {
        KeyBackup {
            user_id: user_id.to_string(),
            backup_id: version.to_string(),
            version,
            algorithm: "m.megolm_backup.v1.curve25519-aes-sha2".to_string(),
            auth_key: "auth_key".to_string(),
            mgmt_key: "mgmt_key".to_string(),
            backup_data: serde_json::json!({"public_key": "pubkey"}),
            etag: etag.map(str::to_string),
        }
    }

    fn key_params(user_id: &str, version: i64, room_id: &str, session_id: &str) -> BackupKeyInsertParams {
        BackupKeyInsertParams {
            user_id: user_id.to_string(),
            backup_id: version.to_string(),
            room_id: room_id.to_string(),
            session_id: session_id.to_string(),
            first_message_index: 3,
            forwarded_count: 1,
            is_verified: true,
            backup_data: serde_json::json!({"ciphertext": "ct", "mac": "mac"}),
        }
    }

    #[tokio::test]
    async fn test_backup_round_trip_on_migration_template() {
        let isolated = IsolatedTestPool::new(BASELINE_SQL).await.expect("isolated test pool");
        let pool = isolated.pool();
        let storage = KeyBackupStorage::new(&pool);
        let key_storage = BackupKeyStorage::new(&pool);

        let user = "@c19b:localhost";
        let room = "!c19b:localhost";

        // The real template carries `fk_backup_keys_room`
        // (`backup_keys.room_id → rooms.room_id ON DELETE CASCADE`, P3-3), so the
        // room must exist before any key round trip. The hand-built schema in
        // `tests/integration/key_backup_storage_tests_migrated.rs` omits this FK
        // (D-36 family: a simplified fixture hides a real constraint).
        sqlx::query("INSERT INTO rooms (room_id, created_ts) VALUES ($1, $2)")
            .bind(room)
            .bind(0_i64)
            .execute(&*pool)
            .await
            .unwrap();

        // create_backup → get_backup / get_all_backup_versions. Version 7 carries
        // a NULL `etag`, which is genuinely nullable (the writer binds
        // `etag.as_deref()`), so it must survive the `Option<String>` field.
        storage.create_backup(&make_backup(user, 7, None)).await.unwrap();
        storage.create_backup(&make_backup(user, 9, Some("etag9"))).await.unwrap();

        let latest = storage.get_backup(user).await.unwrap().expect("latest backup");
        assert_eq!(latest.version, 9);
        assert_eq!(latest.backup_id, "9");
        assert_eq!(latest.etag.as_deref(), Some("etag9"));
        assert_eq!(latest.backup_data, serde_json::json!({"public_key": "pubkey"}));

        let all = storage.get_all_backup_versions(user).await.unwrap();
        assert_eq!(all.iter().map(|b| b.version).collect::<Vec<_>>(), vec![9, 7]);
        // The `COALESCE(backup_id_text, version::text) AS "backup_id!"` projection
        // must yield a version string even for the `etag = NULL` row (D-46).
        let v7 = all.iter().find(|b| b.version == 7).expect("version 7 row");
        assert_eq!(v7.backup_id, "7");
        assert_eq!(v7.etag, None);

        // get_backup_version: the numeric branch keys on `version`; a non-numeric
        // version must not be coerced to a numeric lookup (E-05).
        let by_num = storage.get_backup_version(user, "9").await.unwrap().expect("numeric version");
        assert_eq!(by_num.version, 9);
        assert!(storage.get_backup_version(user, "not-a-number").await.unwrap().is_none());

        // create_backup is an upsert on `(user_id, version)` — it must rewrite,
        // not fail, and must not grow the row count.
        storage.create_backup(&make_backup(user, 9, Some("etag9b"))).await.unwrap();
        assert_eq!(storage.get_all_backup_versions(user).await.unwrap().len(), 2);
        assert_eq!(storage.get_backup_version(user, "9").await.unwrap().unwrap().etag.as_deref(), Some("etag9b"));

        // upload_backup_key → every read projection the conversion touched.
        key_storage.upload_backup_key(key_params(user, 9, room, "sess-a")).await.unwrap();
        key_storage.upload_backup_key(key_params(user, 9, room, "sess-b")).await.unwrap();

        let by_room = key_storage.get_room_backup_keys(user, room).await.unwrap();
        assert_eq!(by_room.len(), 2);
        assert!(by_room.iter().all(|k| k.backup_id == "9"));
        assert_eq!(by_room[0].session_data, serde_json::json!({"ciphertext": "ct", "mac": "mac"}));

        let by_id = key_storage.get_room_backup_keys_by_backup_id(user, "9", room).await.unwrap();
        assert_eq!(by_id.len(), 2);

        let grouped = key_storage.get_backup_keys_by_rooms(user, "9", &[room.to_string()]).await.unwrap();
        assert_eq!(grouped.get(room).map(Vec::len), Some(2));
        // A room with no keys must come back as an empty vec, not a missing entry.
        let empty = key_storage.get_backup_keys_by_rooms(user, "9", &["!empty:localhost".to_string()]).await.unwrap();
        assert_eq!(empty.get("!empty:localhost").map(Vec::len), Some(0));

        let one = key_storage.get_backup_key(user, room, "sess-a").await.unwrap().expect("session a");
        assert_eq!(one.first_message_index, 3);
        assert!(one.is_verified);
        assert!(key_storage.get_backup_key(user, room, "missing").await.unwrap().is_none());

        let one_by_id =
            key_storage.get_backup_key_by_backup_id(user, "9", room, "sess-b").await.unwrap().expect("session b");
        assert_eq!(one_by_id.session_id, "sess-b");

        // Delete paths — each converted to `query!`; assert exact rows_affected so
        // a too-broad predicate cannot pass by "deleting something".
        assert_eq!(key_storage.delete_session_for_version(user, "9", room, "sess-a").await.unwrap(), 1);
        assert_eq!(key_storage.delete_session_for_version(user, "9", room, "sess-a").await.unwrap(), 0);
        assert_eq!(key_storage.delete_room_for_version(user, "9", room).await.unwrap(), 1);

        key_storage.upload_backup_key(key_params(user, 9, room, "sess-c")).await.unwrap();
        key_storage.delete_backup_key(user, room, "sess-c").await.unwrap();
        assert!(key_storage.get_backup_key(user, room, "sess-c").await.unwrap().is_none());

        key_storage.upload_backup_key(key_params(user, 9, room, "sess-d")).await.unwrap();
        assert_eq!(key_storage.delete_all_for_version(user, "9").await.unwrap(), 1);

        // D-46 regression: the schema must reject a NULL `version` outright
        // (23502), instead of accepting a row the non-Option row type cannot
        // decode. Omitting the column would hit `DEFAULT 1`, so insert NULL
        // explicitly.
        let null_version = sqlx::query(
            "INSERT INTO key_backups (user_id, backup_id_text, version, algorithm, created_ts) \
             VALUES ($1, $2, NULL, $3, $4)",
        )
        .bind(user)
        .bind("nullv")
        .bind("m.megolm_backup.v1")
        .bind(0_i64)
        .execute(&*pool)
        .await;
        let error = null_version.expect_err("a NULL version must be rejected by the D-46 NOT NULL column");
        let code = error.as_database_error().and_then(|db| db.code()).map(|c| c.into_owned());
        assert_eq!(code.as_deref(), Some("23502"), "NOT NULL violation expected, got {error:?}");

        // delete_backup both branches, then confirm the user is empty.
        storage.delete_backup(user, "9").await.unwrap();
        assert!(storage.get_backup_version(user, "9").await.unwrap().is_none());
        storage.delete_backup(user, "7").await.unwrap();
        assert!(storage.get_all_backup_versions(user).await.unwrap().is_empty());
    }
}

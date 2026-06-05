use super::models::*;
use crate::error::ApiError;
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::Arc;

/// Internal query struct that mirrors the `device_keys` table column types
/// (BIGINT timestamps) for direct sqlx::query_as! mapping. The public
/// `DeviceKey` struct uses `DateTime<Utc>`, so we convert after the row lands.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct DeviceKeyRow {
    pub user_id: String,
    pub device_id: String,
    pub algorithm: String,
    pub key_id: String,
    pub public_key: String,
    pub signatures: Option<serde_json::Value>,
    pub display_name: Option<String>,
    pub added_ts: i64,
    pub ts_updated_ms: Option<i64>,
    pub key_data: Option<String>,
    pub is_fallback: Option<bool>,
}

impl DeviceKeyRow {
    fn into_device_key(self) -> DeviceKey {
        let parsed: serde_json::Value = self
            .key_data
            .as_deref()
            .and_then(|k| serde_json::from_str(k).ok())
            .unwrap_or_default();

        let created_ts = chrono::DateTime::from_timestamp_millis(self.added_ts).unwrap_or_default();
        let updated_ts = self
            .ts_updated_ms
            .and_then(|ms| chrono::DateTime::from_timestamp_millis(ms))
            .unwrap_or_default();

        DeviceKey {
            id: 0,
            user_id: self.user_id,
            device_id: self.device_id,
            display_name: self
                .display_name
                .or_else(|| parsed.get("display_name").and_then(|v| v.as_str()).map(String::from)),
            algorithm: self.algorithm,
            key_id: self.key_id,
            public_key: self.public_key,
            signatures: self.signatures.unwrap_or_else(|| {
                parsed
                    .get("signatures")
                    .cloned()
                    .unwrap_or(serde_json::json!({}))
            }),
            created_ts,
            updated_ts,
        }
    }
}

#[derive(Clone)]
pub struct DeviceKeyStorage {
    pub pool: Arc<PgPool>,
}

impl DeviceKeyStorage {
    pub fn new(pool: &Arc<PgPool>) -> Self {
        Self { pool: pool.clone() }
    }

    pub async fn record_device_list_change_best_effort(
        &self,
        user_id: &str,
        device_id: Option<&str>,
        change_type: &str,
    ) {
        let now = chrono::Utc::now().timestamp_millis();
        let row = sqlx::query!(
            r"
            INSERT INTO device_lists_stream (user_id, device_id, created_ts)
            VALUES ($1, $2, $3)
            RETURNING stream_id
            ",
            user_id,
            device_id,
            now,
        )
        .fetch_one(&*self.pool)
        .await;

        let Ok(row) = row else {
            return;
        };

        let _ = sqlx::query!(
            r"
            INSERT INTO device_lists_changes (user_id, device_id, change_type, stream_id, created_ts)
            VALUES ($1, $2, $3, $4, $5)
            ",
            user_id,
            device_id,
            change_type,
            row.stream_id,
            now,
        )
        .execute(&*self.pool)
        .await;
    }

    pub async fn create_tables(&self) -> Result<(), sqlx::Error> {
        sqlx::query(
            r"
            CREATE TABLE IF NOT EXISTS device_keys (
                id BIGSERIAL,
                user_id TEXT NOT NULL,
                device_id TEXT NOT NULL,
                algorithm TEXT NOT NULL,
                key_id TEXT NOT NULL,
                public_key TEXT NOT NULL,
                key_data TEXT,
                signatures JSONB,
                added_ts BIGINT NOT NULL,
                created_ts BIGINT NOT NULL,
                updated_ts BIGINT,
                ts_updated_ms BIGINT,
                is_verified BOOLEAN DEFAULT FALSE,
                is_blocked BOOLEAN DEFAULT FALSE,
                is_fallback BOOLEAN NOT NULL DEFAULT FALSE,
                display_name TEXT,
                CONSTRAINT pk_device_keys PRIMARY KEY (id),
                CONSTRAINT uq_device_keys_user_device_key UNIQUE (user_id, device_id, key_id)
            )
            ",
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r"
            CREATE INDEX IF NOT EXISTS idx_device_keys_user_id ON device_keys(user_id)
            ",
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r"
            CREATE INDEX IF NOT EXISTS idx_device_keys_device_id ON device_keys(device_id)
            ",
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r"
            CREATE INDEX IF NOT EXISTS idx_device_keys_algorithm ON device_keys(algorithm)
            ",
        )
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn create_device_key(&self, key: &DeviceKey) -> Result<(), ApiError> {
        let now_ms = chrono::Utc::now().timestamp_millis();
        let key_data = serde_json::json!({
            "algorithm": key.algorithm,
            "key_id": key.key_id,
            "public_key": key.public_key,
            "signatures": key.signatures,
            "display_name": key.display_name,
        })
        .to_string();

        sqlx::query!(
            r"
            INSERT INTO device_keys (user_id, device_id, algorithm, key_id, public_key, signatures, display_name, key_data, added_ts, created_ts, updated_ts, ts_updated_ms)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $9, $9, $9)
            ON CONFLICT (user_id, device_id, key_id) DO UPDATE
            SET public_key = EXCLUDED.public_key,
                signatures = EXCLUDED.signatures,
                display_name = EXCLUDED.display_name,
                updated_ts = EXCLUDED.updated_ts,
                ts_updated_ms = EXCLUDED.ts_updated_ms,
                key_data = EXCLUDED.key_data
            ",
            key.user_id,
            key.device_id,
            key.algorithm,
            key.key_id,
            key.public_key,
            key.signatures,
            key.display_name,
            key_data,
            now_ms,
        )
        .execute(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to create/update device key: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(())
    }

    pub async fn create_fallback_key(&self, key: &DeviceKey) -> Result<(), ApiError> {
        let now_ms = chrono::Utc::now().timestamp_millis();
        let key_data = serde_json::json!({
            "algorithm": key.algorithm,
            "key_id": key.key_id,
            "public_key": key.public_key,
            "signatures": key.signatures,
            "display_name": key.display_name,
        })
        .to_string();

        sqlx::query!(
            r"
            INSERT INTO device_keys (user_id, device_id, algorithm, key_id, public_key, signatures, display_name, key_data, added_ts, created_ts, updated_ts, ts_updated_ms, is_fallback)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $9, $9, $9, TRUE)
            ON CONFLICT (user_id, device_id, key_id) DO UPDATE
            SET public_key = EXCLUDED.public_key,
                signatures = EXCLUDED.signatures,
                display_name = EXCLUDED.display_name,
                updated_ts = EXCLUDED.updated_ts,
                ts_updated_ms = EXCLUDED.ts_updated_ms,
                key_data = EXCLUDED.key_data,
                is_fallback = TRUE
            ",
            key.user_id,
            key.device_id,
            key.algorithm,
            key.key_id,
            key.public_key,
            key.signatures,
            key.display_name,
            key_data,
            now_ms,
        )
        .execute(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to create/update fallback key: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(())
    }

    pub async fn delete_fallback_keys(&self, user_id: &str, device_id: &str) -> Result<(), ApiError> {
        sqlx::query!(
            r"
            DELETE FROM device_keys
            WHERE user_id = $1 AND device_id = $2 AND is_fallback = TRUE
            ",
            user_id,
            device_id,
        )
        .execute(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to delete fallback keys: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(())
    }

    pub async fn get_unused_fallback_key_types(
        &self,
        user_id: &str,
        device_id: &str,
    ) -> Result<Vec<String>, ApiError> {
        let rows = sqlx::query!(
            r"
            SELECT DISTINCT algorithm
            FROM device_keys
            WHERE user_id = $1 AND device_id = $2 AND is_fallback = TRUE
            ",
            user_id,
            device_id,
        )
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Database error: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(rows
            .into_iter()
            .map(|row| {
                if row.algorithm.starts_with("signed_curve25519") {
                    "signed_curve25519".to_string()
                } else {
                    row.algorithm
                }
            })
            .collect())
    }

    pub async fn get_device_key(
        &self,
        user_id: &str,
        device_id: &str,
        algorithm: &str,
    ) -> Result<Option<DeviceKey>, ApiError> {
        let row: Option<DeviceKeyRow> = sqlx::query_as!(
            DeviceKeyRow,
            r#"
            SELECT
                user_id,
                device_id,
                algorithm,
                key_id,
                public_key,
                signatures AS "signatures?",
                display_name,
                added_ts AS "added_ts!",
                ts_updated_ms AS "ts_updated_ms?",
                key_data AS "key_data?",
                is_fallback AS "is_fallback?"
            FROM device_keys
            WHERE user_id = $1 AND device_id = $2 AND algorithm = $3
            LIMIT 1
            "#,
            user_id,
            device_id,
            algorithm,
        )
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Database error: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(row.map(DeviceKeyRow::into_device_key))
    }

    pub async fn get_device_keys(
        &self,
        user_id: &str,
        device_ids: &[String],
    ) -> Result<Vec<DeviceKey>, ApiError> {
        let rows: Vec<DeviceKeyRow> = sqlx::query_as!(
            DeviceKeyRow,
            r#"
            SELECT
                user_id,
                device_id,
                algorithm,
                key_id,
                public_key,
                signatures AS "signatures?",
                display_name,
                added_ts AS "added_ts!",
                ts_updated_ms AS "ts_updated_ms?",
                key_data AS "key_data?",
                is_fallback AS "is_fallback?"
            FROM device_keys
            WHERE user_id = $1 AND device_id = ANY($2)
            "#,
            user_id,
            device_ids,
        )
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Database error: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(rows.into_iter().map(DeviceKeyRow::into_device_key).collect())
    }

    pub async fn get_all_device_keys(&self, user_id: &str) -> Result<Vec<DeviceKey>, ApiError> {
        let rows: Vec<DeviceKeyRow> = sqlx::query_as!(
            DeviceKeyRow,
            r#"
            SELECT
                user_id,
                device_id,
                algorithm,
                key_id,
                public_key,
                signatures AS "signatures?",
                display_name,
                added_ts AS "added_ts!",
                ts_updated_ms AS "ts_updated_ms?",
                key_data AS "key_data?",
                is_fallback AS "is_fallback?"
            FROM device_keys
            WHERE user_id = $1 AND (is_fallback = FALSE OR is_fallback IS NULL)
              AND algorithm IN ('ed25519', 'curve25519')
            "#,
            user_id,
        )
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Database error: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(rows.into_iter().map(DeviceKeyRow::into_device_key).collect())
    }

    pub async fn get_all_device_keys_batch(
        &self,
        user_ids: &[String],
    ) -> Result<HashMap<String, Vec<DeviceKey>>, ApiError> {
        if user_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let rows: Vec<DeviceKeyRow> = sqlx::query_as!(
            DeviceKeyRow,
            r#"
            SELECT
                user_id,
                device_id,
                algorithm,
                key_id,
                public_key,
                signatures AS "signatures?",
                display_name,
                added_ts AS "added_ts!",
                ts_updated_ms AS "ts_updated_ms?",
                key_data AS "key_data?",
                is_fallback AS "is_fallback?"
            FROM device_keys
            WHERE user_id = ANY($1) AND (is_fallback = FALSE OR is_fallback IS NULL)
              AND algorithm IN ('ed25519', 'curve25519')
            "#,
            user_ids,
        )
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Database error: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        let mut result: HashMap<String, Vec<DeviceKey>> = HashMap::new();
        for row in rows {
            let key = row.into_device_key();
            result.entry(key.user_id.clone()).or_default().push(key);
        }

        Ok(result)
    }

    pub async fn delete_device_key(
        &self,
        user_id: &str,
        device_id: &str,
        algorithm: &str,
    ) -> Result<(), ApiError> {
        sqlx::query!(
            r"
            DELETE FROM device_keys
            WHERE user_id = $1 AND device_id = $2 AND algorithm = $3
            ",
            user_id,
            device_id,
            algorithm,
        )
        .execute(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Database error: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(())
    }

    pub async fn get_device_count(&self, user_id: &str) -> Result<i64, ApiError> {
        let count: i64 = sqlx::query_scalar!(
            r"
            SELECT COUNT(DISTINCT device_id) AS "count!"
            FROM device_keys
            WHERE user_id = $1 AND (is_fallback = FALSE OR is_fallback IS NULL)
            ",
            user_id,
        )
        .fetch_one(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Database error: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(count)
    }

    pub async fn delete_device_keys(&self, user_id: &str, device_id: &str) -> Result<(), ApiError> {
        sqlx::query!(
            r"
            DELETE FROM device_keys
            WHERE user_id = $1 AND device_id = $2
            ",
            user_id,
            device_id,
        )
        .execute(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Database error: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(())
    }

    pub async fn get_one_time_keys_count(&self, user_id: &str, device_id: &str) -> Result<i64, ApiError> {
        let count: i64 = sqlx::query_scalar!(
            r#"
            SELECT COUNT(*) AS "count!"
            FROM device_keys
            WHERE user_id = $1 AND device_id = $2 AND algorithm LIKE 'signed_curve25519%'
            "#,
            user_id,
            device_id,
        )
        .fetch_one(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Database error: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(count)
    }

    pub async fn get_one_time_keys_count_by_algorithm(
        &self,
        user_id: &str,
        device_id: &str,
    ) -> Result<std::collections::HashMap<String, i64>, ApiError> {
        let rows = sqlx::query!(
            r#"
            SELECT algorithm AS "algorithm!", COUNT(*) AS "count!"
            FROM device_keys
            WHERE user_id = $1 AND device_id = $2
              AND (is_fallback = FALSE OR is_fallback IS NULL)
              AND algorithm NOT IN ('ed25519', 'curve25519')
            GROUP BY algorithm
            "#,
            user_id,
            device_id,
        )
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Database error: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        let mut counts = std::collections::HashMap::new();
        for row in rows {
            let algo_name = if row.algorithm.starts_with("signed_curve25519") {
                "signed_curve25519".to_string()
            } else if row.algorithm.starts_with("curve25519") {
                "curve25519".to_string()
            } else {
                row.algorithm
            };
            *counts.entry(algo_name).or_insert(0) += row.count;
        }

        Ok(counts)
    }

    pub async fn claim_one_time_key(
        &self,
        user_id: &str,
        device_id: &str,
        algorithm: &str,
    ) -> Result<Option<DeviceKey>, ApiError> {
        let mut tx = self.pool.begin().await.map_err(|e| {
            tracing::error!("Failed to begin transaction: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        let row: Option<DeviceKeyRow> = sqlx::query_as!(
            DeviceKeyRow,
            r#"
            WITH target AS (
                SELECT id FROM device_keys
                WHERE user_id = $1 AND device_id = $2 AND algorithm = $3 AND (is_fallback = FALSE OR is_fallback IS NULL)
                LIMIT 1
            )
            DELETE FROM device_keys
            WHERE id IN (SELECT id FROM target)
            RETURNING
                user_id,
                device_id,
                algorithm,
                key_id,
                public_key,
                signatures AS "signatures?",
                display_name,
                added_ts AS "added_ts!",
                ts_updated_ms AS "ts_updated_ms?",
                key_data AS "key_data?",
                is_fallback AS "is_fallback?"
            "#,
            user_id,
            device_id,
            algorithm,
        )
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| {
            tracing::error!("Failed to claim one-time key: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        if let Some(r) = row {
            if r.is_fallback.unwrap_or(false) {
                tracing::warn!(
                    "Claimed fallback key for {}:{} instead of OTK - OTK stock depleted",
                    user_id,
                    device_id
                );
            }
            tx.commit().await.map_err(|e| {
                tracing::error!("Failed to commit transaction: {e}");
                ApiError::database("A database error occurred".to_string())
            })?;
            return Ok(Some(r.into_device_key()));
        }

        let fallback_row: Option<DeviceKeyRow> = sqlx::query_as!(
            DeviceKeyRow,
            r#"
            SELECT
                user_id,
                device_id,
                algorithm,
                key_id,
                public_key,
                signatures AS "signatures?",
                display_name,
                added_ts AS "added_ts!",
                ts_updated_ms AS "ts_updated_ms?",
                key_data AS "key_data?",
                is_fallback AS "is_fallback?"
            FROM device_keys
            WHERE user_id = $1 AND device_id = $2 AND algorithm = $3 AND is_fallback = TRUE
            LIMIT 1
            "#,
            user_id,
            device_id,
            algorithm,
        )
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| {
            tracing::error!("Failed to query fallback key: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        if fallback_row.is_some() {
            tracing::warn!(
                "No OTK available for {}:{}, using fallback key",
                user_id,
                device_id
            );
        }

        tx.commit().await.map_err(|e| {
            tracing::error!("Failed to commit transaction: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(fallback_row.map(DeviceKeyRow::into_device_key))
    }

    pub async fn get_key_changes(&self, from_ts: i64, to_ts: i64) -> Result<Vec<String>, ApiError> {
        let rows = sqlx::query!(
            r#"
            SELECT DISTINCT user_id AS "user_id!"
            FROM device_keys
            WHERE ts_updated_ms > $1 AND ts_updated_ms <= $2
            "#,
            from_ts,
            to_ts,
        )
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Database error: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(rows.into_iter().map(|row| row.user_id).collect())
    }

    pub async fn get_key_changes_with_left(
        &self,
        from_ts: i64,
        to_ts: i64,
        current_user_id: &str,
    ) -> Result<(Vec<String>, Vec<String>), ApiError> {
        let changed_rows = sqlx::query!(
            r#"
            SELECT DISTINCT user_id AS "user_id!"
            FROM device_lists_stream
            WHERE stream_id > $1
              AND stream_id <= $2
              AND user_id != $3
            ORDER BY user_id
            LIMIT 100
            "#,
            from_ts,
            to_ts,
            current_user_id,
        )
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get key changes: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        let changed: Vec<String> = changed_rows.into_iter().map(|row| row.user_id).collect();

        let left_rows = sqlx::query!(
            r#"
            SELECT DISTINCT dl.user_id AS "user_id!"
            FROM device_lists_stream dl
            LEFT JOIN room_memberships rm ON rm.user_id = dl.user_id
            WHERE dl.stream_id > $1
              AND dl.stream_id <= $2
              AND dl.user_id != $3
              AND rm.user_id IS NULL
            ORDER BY dl.user_id
            LIMIT 100
            "#,
            from_ts,
            to_ts,
            current_user_id,
        )
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get key changes left: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        let left: Vec<String> = left_rows.into_iter().map(|row| row.user_id).collect();

        Ok((changed, left))
    }

    pub async fn store_signature(
        &self,
        target_user_id: &str,
        target_key_id: &str,
        signing_user_id: &str,
        signing_key_id: &str,
        signature: &str,
    ) -> Result<(), ApiError> {
        let now_ms = chrono::Utc::now().timestamp_millis();

        sqlx::query!(
            r"
            INSERT INTO key_signatures (
                target_user_id, target_key_id, signing_user_id, signing_key_id, signature, added_ts
            )
            VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT (target_user_id, target_key_id, signing_user_id, signing_key_id)
            DO UPDATE SET signature = EXCLUDED.signature, added_ts = EXCLUDED.added_ts
            ",
            target_user_id,
            target_key_id,
            signing_user_id,
            signing_key_id,
            signature,
            now_ms,
        )
        .execute(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to store signature: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn create_test_device_key() -> DeviceKey {
        DeviceKey {
            id: 1,
            user_id: "@alice:example.com".to_string(),
            device_id: "DEVICE_ABC".to_string(),
            display_name: Some("Alice's Phone".to_string()),
            algorithm: "ed25519".to_string(),
            key_id: "ABCDEFGH".to_string(),
            public_key: "base64_encoded_public_key_here".to_string(),
            signatures: json!({
                "@alice:example.com": {
                    "ed25519:DEVICE_ABC": "signature_base64_string"
                }
            }),
            created_ts: chrono::Utc::now(),
            updated_ts: chrono::Utc::now(),
        }
    }

    #[test]
    fn test_device_key_creation_with_valid_data() {
        let key = create_test_device_key();

        assert_eq!(key.id, 1);
        assert_eq!(key.user_id, "@alice:example.com");
        assert_eq!(key.device_id, "DEVICE_ABC");
        assert_eq!(key.display_name, Some("Alice's Phone".to_string()));
        assert_eq!(key.algorithm, "ed25519");
        assert_eq!(key.key_id, "ABCDEFGH");
        assert!(!key.public_key.is_empty());
    }

    #[test]
    fn test_device_key_with_empty_display_name() {
        let key = DeviceKey {
            id: 2,
            user_id: "@bob:example.com".to_string(),
            device_id: "DEVICE_XYZ".to_string(),
            display_name: None,
            algorithm: "curve25519".to_string(),
            key_id: "KEY123".to_string(),
            public_key: "public_key".to_string(),
            signatures: json!({}),
            created_ts: chrono::Utc::now(),
            updated_ts: chrono::Utc::now(),
        };

        assert!(key.display_name.is_none());
        assert_eq!(key.algorithm, "curve25519");
    }

    #[test]
    fn test_device_key_signatures_structure() {
        let key = create_test_device_key();

        assert!(key.signatures.is_object());
        let sig_obj = key.signatures.as_object().unwrap();
        assert!(sig_obj.contains_key("@alice:example.com"));

        let user_sigs = sig_obj.get("@alice:example.com").unwrap().as_object().unwrap();
        assert!(user_sigs.contains_key("ed25519:DEVICE_ABC"));
    }

    #[test]
    fn test_device_key_signatures_empty() {
        let key = DeviceKey {
            id: 3,
            user_id: "@charlie:example.com".to_string(),
            device_id: "DEVICE_EMPTY".to_string(),
            display_name: None,
            algorithm: "ed25519".to_string(),
            key_id: "KEY_EMPTY".to_string(),
            public_key: "public_key".to_string(),
            signatures: json!({}),
            created_ts: chrono::Utc::now(),
            updated_ts: chrono::Utc::now(),
        };

        assert!(key.signatures.is_object());
        assert!(key.signatures.as_object().unwrap().is_empty());
    }

    #[test]
    fn test_device_key_serialization() {
        let key = create_test_device_key();
        let json_str = serde_json::to_string(&key).unwrap();

        assert!(json_str.contains("@alice:example.com"));
        assert!(json_str.contains("DEVICE_ABC"));
        assert!(json_str.contains("ed25519"));
    }

    #[test]
    fn test_device_key_deserialization() {
        let json_data = json!({
            "id": 10,
            "user_id": "@david:example.com",
            "device_id": "DEVICE_D",
            "display_name": "David's Laptop",
            "algorithm": "curve25519",
            "key_id": "KEY_D",
            "public_key": "public_key_d",
            "signatures": {
                "@david:example.com": {
                    "ed25519:DEVICE_D": "sig_d"
                }
            },
            "created_ts": "2026-01-01T00:00:00Z",
            "updated_ts": "2026-01-01T00:00:00Z"
        });

        let key: DeviceKey = serde_json::from_value(json_data).unwrap();

        assert_eq!(key.id, 10);
        assert_eq!(key.user_id, "@david:example.com");
        assert_eq!(key.device_id, "DEVICE_D");
        assert_eq!(key.display_name, Some("David's Laptop".to_string()));
    }

    #[test]
    fn test_key_data_format_validation() {
        let key = create_test_device_key();

        let key_data = json!({
            "algorithm": key.algorithm,
            "key_id": key.key_id,
            "public_key": key.public_key,
            "signatures": key.signatures,
            "display_name": key.display_name,
        });

        assert!(key_data.is_object());
        assert!(key_data.get("algorithm").is_some());
        assert!(key_data.get("key_id").is_some());
        assert!(key_data.get("public_key").is_some());
        assert!(key_data.get("signatures").is_some());
    }

    #[test]
    fn test_signature_with_multiple_signers() {
        let multi_sig_key = DeviceKey {
            id: 5,
            user_id: "@eve:example.com".to_string(),
            device_id: "DEVICE_EVE".to_string(),
            display_name: Some("Eve's Device".to_string()),
            algorithm: "ed25519".to_string(),
            key_id: "KEY_EVE".to_string(),
            public_key: "eve_public_key".to_string(),
            signatures: json!({
                "@eve:example.com": {
                    "ed25519:DEVICE_EVE": "eve_self_signature"
                },
                "@alice:example.com": {
                    "ed25519:DEVICE_ABC": "alice_cross_signature"
                }
            }),
            created_ts: chrono::Utc::now(),
            updated_ts: chrono::Utc::now(),
        };

        let sig_obj = multi_sig_key.signatures.as_object().unwrap();
        assert_eq!(sig_obj.len(), 2);
        assert!(sig_obj.contains_key("@eve:example.com"));
        assert!(sig_obj.contains_key("@alice:example.com"));
    }

    #[test]
    fn test_device_key_timestamps() {
        let now = chrono::Utc::now();
        let earlier = now - chrono::Duration::hours(1);

        let key = DeviceKey {
            id: 6,
            user_id: "@frank:example.com".to_string(),
            device_id: "DEVICE_FRANK".to_string(),
            display_name: None,
            algorithm: "ed25519".to_string(),
            key_id: "KEY_FRANK".to_string(),
            public_key: "frank_key".to_string(),
            signatures: json!({}),
            created_ts: earlier,
            updated_ts: now,
        };

        assert!(key.created_ts < key.updated_ts);
        assert_eq!(key.updated_ts, now);
    }

    #[test]
    fn test_device_key_with_special_characters() {
        let key = DeviceKey {
            id: 7,
            user_id: "@user-with-dash:server.example.com".to_string(),
            device_id: "DEVICE_特殊字符_123".to_string(),
            display_name: Some("设备名称 with 中文".to_string()),
            algorithm: "ed25519".to_string(),
            key_id: "KEY_SPECIAL".to_string(),
            public_key: "public_key_special".to_string(),
            signatures: json!({
                "@user-with-dash:server.example.com": {
                    "ed25519:DEVICE_SPECIAL": "特殊签名"
                }
            }),
            created_ts: chrono::Utc::now(),
            updated_ts: chrono::Utc::now(),
        };

        assert!(key.user_id.contains('-'));
        assert!(key.display_name.as_ref().unwrap().contains("中文"));
        let sig_str = key.signatures.to_string();
        assert!(sig_str.contains("特殊签名"));
    }

    #[test]
    fn test_device_key_algorithm_variants() {
        let algorithms = ["ed25519", "curve25519", "signed_curve25519", "custom_algorithm"];

        for (idx, algo) in algorithms.iter().enumerate() {
            let key = DeviceKey {
                id: idx as i64 + 100,
                user_id: "@test:example.com".to_string(),
                device_id: format!("DEVICE_{idx}"),
                display_name: None,
                algorithm: algo.to_string(),
                key_id: format!("KEY_{idx}"),
                public_key: "test_key".to_string(),
                signatures: json!({}),
                created_ts: chrono::Utc::now(),
                updated_ts: chrono::Utc::now(),
            };

            assert_eq!(key.algorithm, *algo);
        }
    }

    #[test]
    fn test_device_key_clone() {
        let key = create_test_device_key();
        let cloned = key.clone();

        assert_eq!(key.user_id, cloned.user_id);
        assert_eq!(key.device_id, cloned.device_id);
        assert_eq!(key.public_key, cloned.public_key);
        assert_eq!(key.signatures.to_string(), cloned.signatures.to_string());
    }

    #[test]
    fn test_device_key_debug_output() {
        let key = create_test_device_key();
        let debug_str = format!("{key:?}");

        assert!(debug_str.contains("DeviceKey"));
        assert!(debug_str.contains("@alice:example.com"));
        assert!(debug_str.contains("DEVICE_ABC"));
    }
}

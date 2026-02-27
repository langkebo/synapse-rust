use super::models::*;
use crate::error::ApiError;
use sqlx::{PgPool, Row};
use std::sync::Arc;

#[derive(Clone)]
pub struct DeviceKeyStorage {
    pub pool: Arc<PgPool>,
}

impl DeviceKeyStorage {
    pub fn new(pool: &Arc<PgPool>) -> Self {
        Self { pool: pool.clone() }
    }

    pub async fn create_tables(&self) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS device_keys (
                user_id VARCHAR(255) NOT NULL,
                device_id VARCHAR(255) NOT NULL,
                algorithm VARCHAR(255) NOT NULL,
                key_data TEXT NOT NULL,
                added_ts BIGINT NOT NULL,
                last_seen_ts BIGINT,
                is_verified BOOLEAN DEFAULT FALSE,
                ts_updated_ms BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW()) * 1000,
                PRIMARY KEY (user_id, device_id, algorithm)
            )
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_device_keys_user_id ON device_keys(user_id)
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_device_keys_device_id ON device_keys(device_id)
            "#,
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

        let result = sqlx::query(
            r#"
            INSERT INTO device_keys (user_id, device_id, algorithm, key_id, public_key, signatures, display_name, created_ts, updated_ts, ts_updated_ms, ts_added_ms, key_data)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
            ON CONFLICT (user_id, device_id, key_id) DO UPDATE
            SET public_key = EXCLUDED.public_key,
                signatures = EXCLUDED.signatures,
                display_name = EXCLUDED.display_name,
                updated_ts = EXCLUDED.updated_ts,
                ts_updated_ms = EXCLUDED.ts_updated_ms,
                key_data = EXCLUDED.key_data
            "#
        )
        .bind(&key.user_id)
        .bind(&key.device_id)
        .bind(&key.algorithm)
        .bind(&key.key_id)
        .bind(&key.public_key)
        .bind(&key.signatures)
        .bind(&key.display_name)
        .bind(now_ms)
        .bind(now_ms)
        .bind(now_ms)
        .bind(now_ms)
        .bind(&key_data)
        .execute(&*self.pool)
        .await;

        match result {
            Ok(_) => Ok(()),
            Err(e) => {
                tracing::error!("Failed to create/update device key: {}", e);
                Err(ApiError::internal(format!(
                    "Failed to save device key: {}",
                    e
                )))
            }
        }
    }

    fn parse_key_data(row: &sqlx::postgres::PgRow) -> DeviceKey {
        let key_data: Option<String> = row.get("key_data");
        let parsed: serde_json::Value = key_data
            .and_then(|k| serde_json::from_str(&k).ok())
            .unwrap_or_default();

        DeviceKey {
            id: 0,
            user_id: row.get("user_id"),
            device_id: row.get("device_id"),
            display_name: row.get::<Option<String>, _>("display_name")
                .or_else(|| parsed.get("display_name").and_then(|v| v.as_str()).map(String::from)),
            algorithm: row.get("algorithm"),
            key_id: row.get("key_id"),
            public_key: row.get("public_key"),
            signatures: row.get::<Option<serde_json::Value>, _>("signatures")
                .unwrap_or_else(|| parsed.get("signatures").cloned().unwrap_or(serde_json::json!({}))),
            created_at: chrono::DateTime::from_timestamp_millis(
                row.get::<i64, _>("created_ts") / 1000,
            )
            .unwrap_or_default(),
            updated_at: chrono::DateTime::from_timestamp_millis(
                row.get::<i64, _>("ts_updated_ms") / 1000,
            )
            .unwrap_or_default(),
        }
    }

    pub async fn get_device_key(
        &self,
        user_id: &str,
        device_id: &str,
        algorithm: &str,
    ) -> Result<Option<DeviceKey>, ApiError> {
        let row = sqlx::query(
            r#"
            SELECT user_id, device_id, algorithm, key_id, public_key, signatures, display_name, created_ts, ts_updated_ms, key_data
            FROM device_keys
            WHERE user_id = $1 AND device_id = $2 AND algorithm = $3
            LIMIT 1
            "#
        )
        .bind(user_id)
        .bind(device_id)
        .bind(algorithm)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(row.as_ref().map(Self::parse_key_data))
    }

    pub async fn get_device_keys(
        &self,
        user_id: &str,
        device_ids: &[String],
    ) -> Result<Vec<DeviceKey>, ApiError> {
        let rows = sqlx::query(
            r#"
            SELECT user_id, device_id, algorithm, key_id, public_key, signatures, display_name, created_ts, ts_updated_ms, key_data
            FROM device_keys
            WHERE user_id = $1 AND device_id = ANY($2)
            "#
        )
        .bind(user_id)
        .bind(device_ids)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows.iter().map(Self::parse_key_data).collect())
    }

    pub async fn get_all_device_keys(&self, user_id: &str) -> Result<Vec<DeviceKey>, ApiError> {
        let rows = sqlx::query(
            r#"
            SELECT user_id, device_id, algorithm, key_id, public_key, signatures, display_name, created_ts, ts_updated_ms, key_data
            FROM device_keys
            WHERE user_id = $1
            "#
        )
        .bind(user_id)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows.iter().map(Self::parse_key_data).collect())
    }

    pub async fn delete_device_key(
        &self,
        user_id: &str,
        device_id: &str,
        algorithm: &str,
    ) -> Result<(), ApiError> {
        sqlx::query(
            r#"
            DELETE FROM device_keys
            WHERE user_id = $1 AND device_id = $2 AND algorithm = $3
            "#,
        )
        .bind(user_id)
        .bind(device_id)
        .bind(algorithm)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn delete_device_keys(&self, user_id: &str, device_id: &str) -> Result<(), ApiError> {
        sqlx::query(
            r#"
            DELETE FROM device_keys
            WHERE user_id = $1 AND device_id = $2
            "#,
        )
        .bind(user_id)
        .bind(device_id)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn get_one_time_keys_count(
        &self,
        user_id: &str,
        device_id: &str,
    ) -> Result<i64, ApiError> {
        let row = sqlx::query(
            r#"
            SELECT COUNT(*) as count
            FROM device_keys
            WHERE user_id = $1 AND device_id = $2 AND algorithm LIKE 'signed_curve25519%'
            "#,
        )
        .bind(user_id)
        .bind(device_id)
        .fetch_one(&*self.pool)
        .await?;

        Ok(row.get("count"))
    }

    pub async fn claim_one_time_key(
        &self,
        user_id: &str,
        device_id: &str,
        algorithm: &str,
    ) -> Result<Option<DeviceKey>, ApiError> {
        let row = sqlx::query(
            r#"
            DELETE FROM device_keys
            WHERE user_id = $1 AND device_id = $2 AND algorithm = $3
            RETURNING user_id, device_id, algorithm, key_id, public_key, signatures, display_name, created_ts, ts_updated_ms, key_data
            "#
        )
        .bind(user_id)
        .bind(device_id)
        .bind(algorithm)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(row.as_ref().map(Self::parse_key_data))
    }

    pub async fn get_key_changes(&self, from_ts: i64, to_ts: i64) -> Result<Vec<String>, ApiError> {
        let rows = sqlx::query(
            r#"
            SELECT DISTINCT user_id
            FROM device_keys
            WHERE ts_updated_ms > $1 AND ts_updated_ms <= $2
            "#,
        )
        .bind(from_ts)
        .bind(to_ts)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows.into_iter().map(|row| row.get("user_id")).collect())
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

        sqlx::query(
            r#"
            INSERT INTO key_signatures (
                target_user_id, target_key_id, signing_user_id, signing_key_id, signature, created_ts
            )
            VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT (target_user_id, target_key_id, signing_user_id, signing_key_id) 
            DO UPDATE SET signature = EXCLUDED.signature, created_ts = EXCLUDED.created_ts
            "#,
        )
        .bind(target_user_id)
        .bind(target_key_id)
        .bind(signing_user_id)
        .bind(signing_key_id)
        .bind(signature)
        .bind(now_ms)
        .execute(&*self.pool)
        .await?;

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
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
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
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
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
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
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
            "created_at": "2026-01-01T00:00:00Z",
            "updated_at": "2026-01-01T00:00:00Z"
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
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
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
            created_at: earlier,
            updated_at: now,
        };

        assert!(key.created_at < key.updated_at);
        assert_eq!(key.updated_at, now);
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
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        assert!(key.user_id.contains('-'));
        assert!(key.display_name.as_ref().unwrap().contains("中文"));
        let sig_str = key.signatures.to_string();
        assert!(sig_str.contains("特殊签名"));
    }

    #[test]
    fn test_device_key_algorithm_variants() {
        let algorithms = vec!["ed25519", "curve25519", "signed_curve25519", "custom_algorithm"];

        for (idx, algo) in algorithms.iter().enumerate() {
            let key = DeviceKey {
                id: idx as i64 + 100,
                user_id: "@test:example.com".to_string(),
                device_id: format!("DEVICE_{}", idx),
                display_name: None,
                algorithm: algo.to_string(),
                key_id: format!("KEY_{}", idx),
                public_key: "test_key".to_string(),
                signatures: json!({}),
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
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
        let debug_str = format!("{:?}", key);

        assert!(debug_str.contains("DeviceKey"));
        assert!(debug_str.contains("@alice:example.com"));
        assert!(debug_str.contains("DEVICE_ABC"));
    }
}

use super::models::*;
use synapse_common::ApiError;
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::Arc;

/// Internal row struct for `cross_signing_keys` (BIGINT added_ts maps to
/// DateTime<Utc> in the public model via helper).
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct CrossSigningKeyRow {
    pub user_id: String,
    pub key_type: String,
    pub key_data: String,
    pub signatures: Option<serde_json::Value>,
    pub added_ts: i64,
}

impl CrossSigningKeyRow {
    fn into_key(self) -> CrossSigningKey {
        let key_json: Option<serde_json::Value> = serde_json::from_str(&self.key_data).ok();

        let public_key = key_json
            .as_ref()
            .and_then(|j| j.get("keys"))
            .and_then(|k| k.as_object())
            .and_then(|obj| obj.values().next())
            .and_then(|v| v.as_str())
            .unwrap_or(&self.key_data)
            .to_string();

        let usage = key_json
            .as_ref()
            .and_then(|j| j.get("usage"))
            .and_then(|u| u.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        let added_ts_dt =
            chrono::DateTime::from_timestamp_millis(self.added_ts).unwrap_or_default();

        CrossSigningKey {
            id: uuid::Uuid::new_v4(),
            user_id: self.user_id,
            key_type: self.key_type,
            public_key,
            usage,
            signatures: self.signatures.unwrap_or(serde_json::json!({})),
            key_json,
            created_ts: added_ts_dt,
            updated_ts: added_ts_dt,
        }
    }
}

/// Internal row struct for `device_signatures`.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct DeviceSignatureRow {
    pub user_id: String,
    pub device_id: String,
    pub target_user_id: String,
    pub target_device_id: String,
    pub algorithm: String,
    pub signature: String,
    pub created_ts: i64,
}

impl DeviceSignatureRow {
    fn into_signature(self) -> DeviceSignature {
        DeviceSignature {
            user_id: self.user_id,
            device_id: self.device_id,
            signing_key_id: self.algorithm.clone(),
            target_user_id: self.target_user_id,
            target_device_id: self.target_device_id,
            target_key_id: self.algorithm,
            signature: self.signature,
            created_ts: chrono::DateTime::from_timestamp_millis(self.created_ts)
                .unwrap_or_default(),
        }
    }
}

#[derive(Clone)]
pub struct CrossSigningStorage {
    pub pool: Arc<PgPool>,
}

impl CrossSigningStorage {
    pub fn new(pool: &Arc<PgPool>) -> Self {
        Self { pool: pool.clone() }
    }

    pub async fn create_cross_signing_key(&self, key: &CrossSigningKey) -> Result<(), ApiError> {
        let added_ts = chrono::Utc::now().timestamp_millis();
        let key_json_str = key.key_json.as_ref().map(|v| v.to_string()).unwrap_or_default();

        sqlx::query(
            r"
            INSERT INTO cross_signing_keys (user_id, key_type, key_data, signatures, added_ts)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (user_id, key_type) DO UPDATE
            SET key_data = EXCLUDED.key_data,
                signatures = EXCLUDED.signatures,
                added_ts = EXCLUDED.added_ts
            ",
        )
        .bind(&key.user_id)
        .bind(&key.key_type)
        .bind(&key_json_str)
        .bind(&key.signatures)
        .bind(added_ts)
        .execute(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to save cross signing key: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(())
    }

    pub async fn get_cross_signing_key(
        &self,
        user_id: &str,
        key_type: &str,
    ) -> Result<Option<CrossSigningKey>, ApiError> {
        let row: Option<CrossSigningKeyRow> = sqlx::query_as::<_, CrossSigningKeyRow>(
            r"
            SELECT
                user_id,
                key_type,
                key_data,
                signatures,
                added_ts
            FROM cross_signing_keys
            WHERE user_id = $1 AND key_type = $2
            ",
        )
        .bind(user_id)
        .bind(key_type)
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to load cross signing key: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(row.map(CrossSigningKeyRow::into_key))
    }

    pub async fn get_cross_signing_keys(
        &self,
        user_id: &str,
    ) -> Result<Vec<CrossSigningKey>, ApiError> {
        let rows: Vec<CrossSigningKeyRow> = sqlx::query_as::<_, CrossSigningKeyRow>(
            r"
            SELECT
                user_id,
                key_type,
                key_data,
                signatures,
                added_ts
            FROM cross_signing_keys
            WHERE user_id = $1
            ",
        )
        .bind(user_id)
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to load cross signing keys: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(rows.into_iter().map(CrossSigningKeyRow::into_key).collect())
    }

    pub async fn get_cross_signing_keys_batch(
        &self,
        user_ids: &[String],
    ) -> Result<HashMap<String, Vec<CrossSigningKey>>, ApiError> {
        if user_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let rows: Vec<CrossSigningKeyRow> = sqlx::query_as::<_, CrossSigningKeyRow>(
            r"
            SELECT
                user_id,
                key_type,
                key_data,
                signatures,
                added_ts
            FROM cross_signing_keys
            WHERE user_id = ANY($1)
            ",
        )
        .bind(user_ids)
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to load cross signing keys batch: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        let mut result: HashMap<String, Vec<CrossSigningKey>> = HashMap::new();
        for row in rows {
            let user_id = row.user_id.clone();
            let key = row.into_key();
            result.entry(user_id).or_default().push(key);
        }

        Ok(result)
    }

    pub async fn get_device_signatures_batch(
        &self,
        user_ids: &[String],
    ) -> Result<HashMap<String, Vec<DeviceSignature>>, ApiError> {
        if user_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let rows: Vec<DeviceSignatureRow> = sqlx::query_as::<_, DeviceSignatureRow>(
            r"
            SELECT
                user_id,
                device_id,
                target_user_id,
                target_device_id,
                algorithm,
                signature,
                created_ts
            FROM device_signatures
            WHERE user_id = ANY($1)
            ",
        )
        .bind(user_ids)
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to load device signatures batch: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        let mut result: HashMap<String, Vec<DeviceSignature>> = HashMap::new();
        for row in rows {
            let user_id = row.user_id.clone();
            let sig = row.into_signature();
            result.entry(user_id).or_default().push(sig);
        }

        Ok(result)
    }

    pub async fn update_cross_signing_key(&self, key: &CrossSigningKey) -> Result<(), ApiError> {
        let added_ts = chrono::Utc::now().timestamp_millis();
        let key_json_str = key
            .key_json
            .as_ref()
            .map_or_else(|| key.public_key.clone(), |v| v.to_string());

        sqlx::query(
            r"
            UPDATE cross_signing_keys SET key_data = $1, signatures = $2, added_ts = $3
            WHERE user_id = $4 AND key_type = $5
            ",
        )
        .bind(&key_json_str)
        .bind(&key.signatures)
        .bind(added_ts)
        .bind(&key.user_id)
        .bind(&key.key_type)
        .execute(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to update cross signing key: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(())
    }

    pub async fn save_device_key(&self, key: &DeviceKeyInfo) -> Result<(), ApiError> {
        let added_ts = chrono::Utc::now().timestamp_millis();
        let key_id = format!(
            "{}:{}",
            key.algorithm,
            key.public_key.split(':').next().unwrap_or(&key.public_key)
        );

        sqlx::query(
            r"
            INSERT INTO device_keys (user_id, device_id, algorithm, key_id, public_key, key_data, added_ts, created_ts, updated_ts)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            ON CONFLICT (user_id, device_id, key_id) DO UPDATE SET
                public_key = EXCLUDED.public_key,
                key_data = EXCLUDED.key_data,
                updated_ts = EXCLUDED.updated_ts
            ",
        )
        .bind(&key.user_id)
        .bind(&key.device_id)
        .bind(&key.algorithm)
        .bind(&key_id)
        .bind(&key.public_key)
        .bind(&key.public_key)
        .bind(added_ts)
        .bind(added_ts)
        .bind(added_ts)
        .execute(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to save device key for cross signing: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(())
    }

    pub async fn save_device_signature(&self, sig: &DeviceSignature) -> Result<(), ApiError> {
        let created_ts = chrono::Utc::now().timestamp_millis();
        sqlx::query(
            r"
            INSERT INTO device_signatures
            (user_id, device_id, target_user_id, target_device_id, algorithm, signature, created_ts)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (user_id, device_id, target_user_id, target_device_id, algorithm) DO UPDATE SET
                signature = EXCLUDED.signature,
                created_ts = EXCLUDED.created_ts
            ",
        )
        .bind(&sig.user_id)
        .bind(&sig.device_id)
        .bind(&sig.target_user_id)
        .bind(&sig.target_device_id)
        .bind(&sig.signing_key_id)
        .bind(&sig.signature)
        .bind(created_ts)
        .execute(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to save device signature: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(())
    }

    pub async fn get_user_signatures(&self, user_id: &str) -> Result<Vec<DeviceSignature>, ApiError> {
        let rows: Vec<DeviceSignatureRow> = sqlx::query_as::<_, DeviceSignatureRow>(
            r"
            SELECT
                user_id,
                device_id,
                target_user_id,
                target_device_id,
                algorithm,
                signature,
                created_ts
            FROM device_signatures
            WHERE user_id = $1
            ",
        )
        .bind(user_id)
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to load user signatures: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(rows.into_iter().map(DeviceSignatureRow::into_signature).collect())
    }

    pub async fn get_device_signatures(
        &self,
        user_id: &str,
        device_id: &str,
    ) -> Result<Vec<DeviceSignature>, ApiError> {
        let rows: Vec<DeviceSignatureRow> = sqlx::query_as::<_, DeviceSignatureRow>(
            r"
            SELECT
                user_id,
                device_id,
                target_user_id,
                target_device_id,
                algorithm,
                signature,
                created_ts
            FROM device_signatures
            WHERE user_id = $1 AND target_device_id = $2
            ",
        )
        .bind(user_id)
        .bind(device_id)
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to load device signatures: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(rows.into_iter().map(DeviceSignatureRow::into_signature).collect())
    }

    pub async fn get_signature(
        &self,
        user_id: &str,
        key_id: &str,
        signing_key_id: &str,
    ) -> Result<Option<DeviceSignature>, ApiError> {
        let row: Option<DeviceSignatureRow> = sqlx::query_as::<_, DeviceSignatureRow>(
            r"
            SELECT
                user_id,
                device_id,
                target_user_id,
                target_device_id,
                algorithm,
                signature,
                created_ts
            FROM device_signatures
            WHERE user_id = $1 AND algorithm = $2 AND device_id = $3
            ",
        )
        .bind(user_id)
        .bind(key_id)
        .bind(signing_key_id)
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to load signature: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(row.map(DeviceSignatureRow::into_signature))
    }

    pub async fn delete_cross_signing_keys(&self, user_id: &str) -> Result<(), ApiError> {
        sqlx::query(
            r"
            DELETE FROM cross_signing_keys WHERE user_id = $1
            ",
        )
        .bind(user_id)
        .execute(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to delete cross signing keys: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        sqlx::query(
            r"
            DELETE FROM device_signatures WHERE user_id = $1
            ",
        )
        .bind(user_id)
        .execute(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to delete device signatures: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn create_test_cross_signing_key() -> CrossSigningKey {
        CrossSigningKey {
            id: uuid::Uuid::new_v4(),
            user_id: "@alice:example.com".to_string(),
            key_type: "master".to_string(),
            public_key: "base64_encoded_public_key".to_string(),
            usage: vec!["master".to_string()],
            signatures: serde_json::json!({}),
            key_json: None,
            created_ts: Utc::now(),
            updated_ts: Utc::now(),
        }
    }

    #[test]
    fn test_cross_signing_key_creation() {
        let key = create_test_cross_signing_key();

        assert!(!key.user_id.is_empty());
        assert!(!key.key_type.is_empty());
        assert!(!key.public_key.is_empty());
        assert!(!key.usage.is_empty());
    }

    #[test]
    fn test_cross_signing_key_user_id_format() {
        let key = create_test_cross_signing_key();

        assert!(key.user_id.starts_with('@'));
        assert!(key.user_id.contains(':'));
    }

    #[test]
    fn test_cross_signing_key_types() {
        let key_types = vec!["master", "self_signing", "user_signing"];

        for key_type in key_types {
            let key = CrossSigningKey {
                id: uuid::Uuid::new_v4(),
                user_id: "@user:example.com".to_string(),
                key_type: key_type.to_string(),
                public_key: "key_data".to_string(),
                usage: vec![key_type.to_string()],
                signatures: serde_json::json!({}),
                key_json: None,
                created_ts: Utc::now(),
                updated_ts: Utc::now(),
            };

            assert_eq!(key.key_type, key_type);
        }
    }

    #[test]
    fn test_cross_signing_key_timestamps() {
        let key = create_test_cross_signing_key();

        assert!(key.created_ts <= key.updated_ts);
    }

    #[test]
    fn test_cross_signing_key_signatures_empty() {
        let key = create_test_cross_signing_key();

        assert!(key.signatures.is_object());
    }

    #[test]
    fn test_cross_signing_key_signatures_with_data() {
        let signatures = serde_json::json!({
            "@alice:example.com": {
                "ed25519:device_id": "signature_base64"
            }
        });

        let key = CrossSigningKey {
            id: uuid::Uuid::new_v4(),
            user_id: "@alice:example.com".to_string(),
            key_type: "master".to_string(),
            public_key: "key_data".to_string(),
            usage: vec!["master".to_string()],
            signatures,
            key_json: None,
            created_ts: Utc::now(),
            updated_ts: Utc::now(),
        };

        assert!(key.signatures.get("@alice:example.com").is_some());
    }

    #[test]
    fn test_cross_signing_key_clone() {
        let key = create_test_cross_signing_key();
        let cloned = key.clone();

        assert_eq!(key.id, cloned.id);
        assert_eq!(key.user_id, cloned.user_id);
        assert_eq!(key.key_type, cloned.key_type);
    }

    #[test]
    fn test_cross_signing_key_serialization() {
        let key = create_test_cross_signing_key();
        let json = serde_json::to_string(&key).unwrap();

        assert!(json.contains("@alice:example.com"));
        assert!(json.contains("master"));
    }

    #[test]
    fn test_cross_signing_key_deserialization() {
        let json = r#"{
            "id": "550e8400-e29b-41d4-a716-446655440000",
            "user_id": "@test:example.com",
            "key_type": "self_signing",
            "public_key": "test_key",
            "usage": ["self_signing"],
            "signatures": {},
            "key_json": null,
            "created_ts": "2024-01-01T00:00:00Z",
            "updated_ts": "2024-01-01T00:00:00Z"
        }"#;

        let key: CrossSigningKey = serde_json::from_str(json).unwrap();
        assert_eq!(key.user_id, "@test:example.com");
        assert_eq!(key.key_type, "self_signing");
    }

    #[test]
    fn test_cross_signing_key_usage_values() {
        let usages = vec![
            vec!["master"],
            vec!["self_signing"],
            vec!["user_signing"],
            vec!["master", "self_signing"],
        ];

        for usage in usages {
            let key = CrossSigningKey {
                id: uuid::Uuid::new_v4(),
                user_id: "@user:example.com".to_string(),
                key_type: usage[0].to_string(),
                public_key: "key".to_string(),
                usage: usage.iter().map(|s| s.to_string()).collect(),
                signatures: serde_json::json!({}),
                key_json: None,
                created_ts: Utc::now(),
                updated_ts: Utc::now(),
            };

            assert_eq!(key.usage.len(), usage.len());
        }
    }
}

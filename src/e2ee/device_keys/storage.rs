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
        }).to_string();
        
        let result = sqlx::query(
            r#"
            INSERT INTO device_keys (user_id, device_id, algorithm, key_data, added_ts, last_seen_ts, is_verified, ts_updated_ms)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT (user_id, device_id, algorithm) DO UPDATE
            SET key_data = EXCLUDED.key_data,
                last_seen_ts = EXCLUDED.last_seen_ts,
                is_verified = EXCLUDED.is_verified,
                ts_updated_ms = EXCLUDED.ts_updated_ms
            "#
        )
        .bind(&key.user_id)
        .bind(&key.device_id)
        .bind(&key.algorithm)
        .bind(&key_data)
        .bind(now_ms)
        .bind(now_ms)
        .bind(false)
        .bind(now_ms)
        .execute(&*self.pool)
        .await;

        match result {
            Ok(_) => Ok(()),
            Err(e) => {
                tracing::error!("Failed to create/update device key: {}", e);
                Err(ApiError::internal(format!("Failed to save device key: {}", e)))
            }
        }
    }

    fn parse_key_data(row: &sqlx::postgres::PgRow) -> DeviceKey {
        let key_data: String = row.get("key_data");
        let parsed: serde_json::Value = serde_json::from_str(&key_data).unwrap_or_default();
        
        DeviceKey {
            id: 0,
            user_id: row.get("user_id"),
            device_id: row.get("device_id"),
            display_name: parsed.get("display_name").and_then(|v| v.as_str()).map(String::from),
            algorithm: row.get("algorithm"),
            key_id: parsed.get("key_id").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            public_key: parsed.get("public_key").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            signatures: parsed.get("signatures").cloned().unwrap_or(serde_json::json!({})),
            created_at: chrono::DateTime::from_timestamp_millis(row.get::<i64, _>("added_ts") / 1000).unwrap_or_default(),
            updated_at: chrono::DateTime::from_timestamp_millis(row.get::<i64, _>("ts_updated_ms") / 1000).unwrap_or_default(),
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
            SELECT user_id, device_id, algorithm, key_data, added_ts, last_seen_ts, is_verified, ts_updated_ms
            FROM device_keys
            WHERE user_id = $1 AND device_id = $2 AND algorithm = $3
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
            SELECT user_id, device_id, algorithm, key_data, added_ts, last_seen_ts, is_verified, ts_updated_ms
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
            SELECT user_id, device_id, algorithm, key_data, added_ts, last_seen_ts, is_verified, ts_updated_ms
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
            RETURNING user_id, device_id, algorithm, key_data, added_ts, last_seen_ts, is_verified, ts_updated_ms
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

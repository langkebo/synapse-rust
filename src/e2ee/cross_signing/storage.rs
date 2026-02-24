use super::models::*;
use crate::error::ApiError;
use sqlx::{PgPool, Row};
use std::sync::Arc;

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
        sqlx::query(
            r#"
            INSERT INTO cross_signing_keys (user_id, key_type, key_data, added_ts)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (user_id, key_type) DO UPDATE
            SET key_data = EXCLUDED.key_data,
                added_ts = EXCLUDED.added_ts
            "#,
        )
        .bind(&key.user_id)
        .bind(&key.key_type)
        .bind(&key.public_key)
        .bind(added_ts)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn get_cross_signing_key(
        &self,
        user_id: &str,
        key_type: &str,
    ) -> Result<Option<CrossSigningKey>, ApiError> {
        let row = sqlx::query(
            r#"
            SELECT user_id, key_type, key_data, added_ts
            FROM cross_signing_keys
            WHERE user_id = $1 AND key_type = $2
            "#,
        )
        .bind(user_id)
        .bind(key_type)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(row.map(|row| CrossSigningKey {
            id: uuid::Uuid::new_v4(),
            user_id: row.get("user_id"),
            key_type: row.get("key_type"),
            public_key: row.get("key_data"),
            usage: vec![],
            signatures: serde_json::json!({}),
            created_at: chrono::DateTime::from_timestamp_millis(
                row.get::<i64, _>("added_ts") / 1000,
            )
            .unwrap_or_default(),
            updated_at: chrono::DateTime::from_timestamp_millis(
                row.get::<i64, _>("added_ts") / 1000,
            )
            .unwrap_or_default(),
        }))
    }

    pub async fn get_cross_signing_keys(
        &self,
        user_id: &str,
    ) -> Result<Vec<CrossSigningKey>, ApiError> {
        let rows = sqlx::query(
            r#"
            SELECT user_id, key_type, key_data, added_ts
            FROM cross_signing_keys
            WHERE user_id = $1
            "#,
        )
        .bind(user_id)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| CrossSigningKey {
                id: uuid::Uuid::new_v4(),
                user_id: row.get("user_id"),
                key_type: row.get("key_type"),
                public_key: row.get("key_data"),
                usage: vec![],
                signatures: serde_json::json!({}),
                created_at: chrono::DateTime::from_timestamp_millis(
                    row.get::<i64, _>("added_ts") / 1000,
                )
                .unwrap_or_default(),
                updated_at: chrono::DateTime::from_timestamp_millis(
                    row.get::<i64, _>("added_ts") / 1000,
                )
                .unwrap_or_default(),
            })
            .collect())
    }

    pub async fn update_cross_signing_key(&self, key: &CrossSigningKey) -> Result<(), ApiError> {
        let added_ts = chrono::Utc::now().timestamp_millis();
        sqlx::query(
            r#"
            UPDATE cross_signing_keys SET key_data = $1, added_ts = $2
            WHERE user_id = $3 AND key_type = $4
            "#,
        )
        .bind(&key.public_key)
        .bind(added_ts)
        .bind(&key.user_id)
        .bind(&key.key_type)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn save_device_key(&self, key: &DeviceKeyInfo) -> Result<(), ApiError> {
        let added_ts = chrono::Utc::now().timestamp_millis();
        sqlx::query(
            r#"
            INSERT INTO device_keys (user_id, device_id, algorithm, key_data, added_ts)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (user_id, device_id, algorithm) DO UPDATE SET
                key_data = EXCLUDED.key_data,
                ts_updated_ms = $6
            "#,
        )
        .bind(&key.user_id)
        .bind(&key.device_id)
        .bind(&key.algorithm)
        .bind(&key.public_key)
        .bind(added_ts)
        .bind(added_ts)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn save_device_signature(&self, sig: &DeviceSignature) -> Result<(), ApiError> {
        let created_ts = chrono::Utc::now().timestamp_millis();
        sqlx::query(
            r#"
            INSERT INTO device_signatures 
            (user_id, device_id, target_user_id, target_device_id, algorithm, signature, created_ts)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (user_id, device_id, target_user_id, target_device_id, algorithm) DO UPDATE SET
                signature = EXCLUDED.signature,
                created_ts = EXCLUDED.created_ts
            "#
        )
        .bind(&sig.user_id)
        .bind(&sig.device_id)
        .bind(&sig.target_user_id)
        .bind(&sig.target_device_id)
        .bind(&sig.target_key_id)
        .bind(&sig.signature)
        .bind(created_ts)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn get_user_signatures(
        &self,
        user_id: &str,
    ) -> Result<Vec<DeviceSignature>, ApiError> {
        let rows = sqlx::query(
            r#"
            SELECT user_id, device_id, target_user_id, target_device_id, 
                   algorithm, signature, created_ts
            FROM device_signatures
            WHERE user_id = $1
            "#,
        )
        .bind(user_id)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| DeviceSignature {
                user_id: row.get("user_id"),
                device_id: row.get("device_id"),
                signing_key_id: row.get("algorithm"),
                target_user_id: row.get("target_user_id"),
                target_device_id: row.get("target_device_id"),
                target_key_id: row.get("algorithm"),
                signature: row.get("signature"),
                created_at: chrono::DateTime::from_timestamp_millis(
                    row.get::<i64, _>("created_ts") / 1000,
                )
                .unwrap_or_default(),
            })
            .collect())
    }

    pub async fn get_device_signatures(
        &self,
        user_id: &str,
        device_id: &str,
    ) -> Result<Vec<DeviceSignature>, ApiError> {
        let rows = sqlx::query(
            r#"
            SELECT user_id, device_id, target_user_id, target_device_id, 
                   algorithm, signature, created_ts
            FROM device_signatures
            WHERE user_id = $1 AND target_device_id = $2
            "#,
        )
        .bind(user_id)
        .bind(device_id)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| DeviceSignature {
                user_id: row.get("user_id"),
                device_id: row.get("device_id"),
                signing_key_id: row.get("algorithm"),
                target_user_id: row.get("target_user_id"),
                target_device_id: row.get("target_device_id"),
                target_key_id: row.get("algorithm"),
                signature: row.get("signature"),
                created_at: chrono::DateTime::from_timestamp_millis(
                    row.get::<i64, _>("created_ts") / 1000,
                )
                .unwrap_or_default(),
            })
            .collect())
    }

    pub async fn get_signature(
        &self,
        user_id: &str,
        key_id: &str,
        signing_key_id: &str,
    ) -> Result<Option<DeviceSignature>, ApiError> {
        let row = sqlx::query(
            r#"
            SELECT user_id, device_id, target_user_id, target_device_id, 
                   algorithm, signature, created_ts
            FROM device_signatures
            WHERE user_id = $1 AND algorithm = $2 AND device_id = $3
            "#,
        )
        .bind(user_id)
        .bind(key_id)
        .bind(signing_key_id)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(row.map(|row| DeviceSignature {
            user_id: row.get("user_id"),
            device_id: row.get("device_id"),
            signing_key_id: row.get("algorithm"),
            target_user_id: row.get("target_user_id"),
            target_device_id: row.get("target_device_id"),
            target_key_id: row.get("algorithm"),
            signature: row.get("signature"),
            created_at: chrono::DateTime::from_timestamp_millis(
                row.get::<i64, _>("created_ts") / 1000,
            )
            .unwrap_or_default(),
        }))
    }

    pub async fn delete_cross_signing_keys(&self, user_id: &str) -> Result<(), ApiError> {
        sqlx::query(
            r#"
            DELETE FROM cross_signing_keys WHERE user_id = $1
            "#,
        )
        .bind(user_id)
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            DELETE FROM device_signatures WHERE user_id = $1
            "#,
        )
        .bind(user_id)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }
}

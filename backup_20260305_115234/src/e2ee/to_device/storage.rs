use crate::error::ApiError;
use serde_json::Value;
use sqlx::{Pool, Postgres, Row};
use std::sync::Arc;

#[derive(Clone)]
pub struct ToDeviceStorage {
    pool: Arc<Pool<Postgres>>,
}

impl ToDeviceStorage {
    pub fn new(pool: &Arc<Pool<Postgres>>) -> Self {
        Self { pool: pool.clone() }
    }

    pub async fn device_exists(&self, user_id: &str, device_id: &str) -> Result<bool, ApiError> {
        let result = sqlx::query("SELECT 1 FROM devices WHERE user_id = $1 AND device_id = $2")
            .bind(user_id)
            .bind(device_id)
            .fetch_optional(&*self.pool)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        Ok(result.is_some())
    }

    pub async fn add_message(
        &self,
        user_id: &str,
        device_id: &str,
        message_type: &str,
        content: Value,
    ) -> Result<(), ApiError> {
        if !self.device_exists(user_id, device_id).await? {
            ::tracing::warn!(
                "Skipping to-device message for non-existent device: {}:{}",
                user_id,
                device_id
            );
            return Ok(());
        }

        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query(
            r#"
            INSERT INTO to_device_messages (user_id, device_id, message_type, content, created_ts)
            VALUES ($1, $2, $3, $4, $5)
            "#,
        )
        .bind(user_id)
        .bind(device_id)
        .bind(message_type)
        .bind(content)
        .bind(now)
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        Ok(())
    }

    pub async fn get_messages(
        &self,
        user_id: &str,
        device_id: &str,
    ) -> Result<Vec<Value>, ApiError> {
        let rows = sqlx::query(
            r#"
            SELECT id, message_type, content, created_ts
            FROM to_device_messages
            WHERE user_id = $1 AND device_id = $2
            ORDER BY created_ts ASC
            "#,
        )
        .bind(user_id)
        .bind(device_id)
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        let mut messages = Vec::new();
        for row in rows {
            let msg_type: String = row.get("message_type");
            let content: Value = row.get("content");
            messages.push(serde_json::json!({
                "type": msg_type,
                "content": content
            }));
        }

        Ok(messages)
    }

    pub async fn delete_messages(&self, ids: &[i64]) -> Result<(), ApiError> {
        sqlx::query(
            r#"
            DELETE FROM to_device_messages
            WHERE id = ANY($1)
            "#,
        )
        .bind(ids)
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        Ok(())
    }
}

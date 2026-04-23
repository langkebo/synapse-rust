use crate::error::ApiError;
use serde_json::Value;
use sqlx::{Pool, Postgres, Row};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct ToDeviceMessage<'a> {
    pub sender_user_id: &'a str,
    pub sender_device_id: &'a str,
    pub recipient_user_id: &'a str,
    pub recipient_device_id: &'a str,
    pub event_type: &'a str,
    pub message_id: Option<&'a str>,
    pub content: Value,
}

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

    pub async fn is_duplicate_transaction(
        &self,
        sender_user_id: &str,
        sender_device_id: &str,
        message_id: &str,
    ) -> Result<bool, ApiError> {
        let result = sqlx::query(
            r#"
            SELECT 1 FROM to_device_transactions
            WHERE sender_user_id = $1 AND sender_device_id = $2 AND message_id = $3
            "#,
        )
        .bind(sender_user_id)
        .bind(sender_device_id)
        .bind(message_id)
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        Ok(result.is_some())
    }

    pub async fn record_transaction(
        &self,
        sender_user_id: &str,
        sender_device_id: &str,
        message_id: &str,
    ) -> Result<(), ApiError> {
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query(
            r#"
            INSERT INTO to_device_transactions (sender_user_id, sender_device_id, message_id, created_ts)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (sender_user_id, sender_device_id, message_id) DO NOTHING
            "#,
        )
        .bind(sender_user_id)
        .bind(sender_device_id)
        .bind(message_id)
        .bind(now)
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        Ok(())
    }

    pub async fn cleanup_old_transactions(&self, max_age_ms: i64) -> Result<u64, ApiError> {
        let cutoff = chrono::Utc::now().timestamp_millis() - max_age_ms;
        let result = sqlx::query(
            r#"
            DELETE FROM to_device_transactions
            WHERE created_ts < $1
            "#,
        )
        .bind(cutoff)
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        Ok(result.rows_affected())
    }

    pub async fn add_message(&self, msg: ToDeviceMessage<'_>) -> Result<(), ApiError> {
        if !self
            .device_exists(msg.recipient_user_id, msg.recipient_device_id)
            .await?
        {
            ::tracing::warn!(
                "Skipping to-device message for non-existent device: {}:{}",
                msg.recipient_user_id,
                msg.recipient_device_id
            );
            return Ok(());
        }

        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query(
            r#"
            INSERT INTO to_device_messages (
                sender_user_id,
                sender_device_id,
                recipient_user_id,
                recipient_device_id,
                event_type,
                content,
                message_id,
                stream_id,
                created_ts
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, nextval('to_device_stream_id_seq'), $8)
            "#,
        )
        .bind(msg.sender_user_id)
        .bind(msg.sender_device_id)
        .bind(msg.recipient_user_id)
        .bind(msg.recipient_device_id)
        .bind(msg.event_type)
        .bind(msg.content)
        .bind(msg.message_id)
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
            SELECT id, stream_id, sender_user_id, event_type, content, message_id, created_ts
            FROM to_device_messages
            WHERE recipient_user_id = $1 AND recipient_device_id = $2
            ORDER BY stream_id ASC
            "#,
        )
        .bind(user_id)
        .bind(device_id)
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        let mut messages = Vec::new();
        for row in rows {
            let event_type: String = row.get("event_type");
            let sender_user_id: String = row.get("sender_user_id");
            let content: Value = row.get("content");
            let message_id: Option<String> = row.get("message_id");
            messages.push(serde_json::json!({
                "type": event_type,
                "sender": sender_user_id,
                "content": content
            }));
            if let Some(mid) = message_id {
                if let Some(obj) = messages.last_mut().and_then(|v| v.as_object_mut()) {
                    obj.insert("message_id".to_string(), serde_json::json!(mid));
                }
            }
        }

        Ok(messages)
    }

    pub async fn get_and_delete_messages(
        &self,
        user_id: &str,
        device_id: &str,
    ) -> Result<Vec<Value>, ApiError> {
        let rows = sqlx::query(
            r#"
            DELETE FROM to_device_messages
            WHERE recipient_user_id = $1 AND recipient_device_id = $2
            RETURNING id, stream_id, sender_user_id, event_type, content, message_id, created_ts
            "#,
        )
        .bind(user_id)
        .bind(device_id)
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        let mut messages = Vec::new();
        for row in rows {
            let event_type: String = row.get("event_type");
            let sender_user_id: String = row.get("sender_user_id");
            let content: Value = row.get("content");
            let message_id: Option<String> = row.get("message_id");
            messages.push(serde_json::json!({
                "type": event_type,
                "sender": sender_user_id,
                "content": content
            }));
            if let Some(mid) = message_id {
                if let Some(obj) = messages.last_mut().and_then(|v| v.as_object_mut()) {
                    obj.insert("message_id".to_string(), serde_json::json!(mid));
                }
            }
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

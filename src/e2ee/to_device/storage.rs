use crate::error::ApiError;
use serde_json::Value;
use sqlx::{Pool, Postgres};
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
        // Accept either a regular device or a (non-expired) dehydrated device
        // (MSC3814) as a valid recipient — without this, to-device messages
        // addressed to a dehydrated device id are silently dropped.
        let now = chrono::Utc::now().timestamp_millis();
        let result = sqlx::query!(
            r#"
            SELECT 1 AS hit FROM devices
                WHERE user_id = $1 AND device_id = $2
            UNION ALL
            SELECT 1 AS hit FROM dehydrated_devices
                WHERE user_id = $1 AND device_id = $2
                  AND (expires_at IS NULL OR expires_at > $3)
            LIMIT 1
            "#,
            user_id,
            device_id,
            now
        )
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Database error: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(result.is_some())
    }

    pub async fn is_duplicate_transaction(
        &self,
        sender_user_id: &str,
        sender_device_id: &str,
        message_id: &str,
    ) -> Result<bool, ApiError> {
        let result = sqlx::query!(
            r#"
            SELECT 1 AS "exists!" FROM to_device_transactions
            WHERE sender_user_id = $1 AND sender_device_id = $2 AND message_id = $3
            "#,
            sender_user_id,
            sender_device_id,
            message_id
        )
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Database error: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(result.is_some())
    }

    pub async fn record_transaction(
        &self,
        sender_user_id: &str,
        sender_device_id: &str,
        message_id: &str,
    ) -> Result<(), ApiError> {
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query!(
            r#"
            INSERT INTO to_device_transactions (sender_user_id, sender_device_id, message_id, created_ts)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (sender_user_id, sender_device_id, message_id) DO NOTHING
            "#,
            sender_user_id,
            sender_device_id,
            message_id,
            now
        )
        .execute(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Database error: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(())
    }

    pub async fn cleanup_old_transactions(&self, max_age_ms: i64) -> Result<u64, ApiError> {
        let cutoff = chrono::Utc::now().timestamp_millis() - max_age_ms;
        let result = sqlx::query!(
            r#"
            DELETE FROM to_device_transactions
            WHERE created_ts < $1
            "#,
            cutoff
        )
        .execute(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Database error: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(result.rows_affected())
    }

    pub async fn add_message(&self, msg: ToDeviceMessage<'_>) -> Result<(), ApiError> {
        if !self.device_exists(msg.recipient_user_id, msg.recipient_device_id).await? {
            ::tracing::warn!(
                "Skipping to-device message for non-existent device: {}:{}",
                msg.recipient_user_id,
                msg.recipient_device_id
            );
            return Ok(());
        }

        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query!(
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
            msg.sender_user_id,
            msg.sender_device_id,
            msg.recipient_user_id,
            msg.recipient_device_id,
            msg.event_type,
            &msg.content,
            msg.message_id,
            now
        )
        .execute(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Database error: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(())
    }

    pub async fn get_messages(&self, user_id: &str, device_id: &str) -> Result<Vec<Value>, ApiError> {
        let rows = sqlx::query!(
            r#"
            SELECT id, stream_id, sender_user_id, event_type, content, message_id, created_ts
            FROM to_device_messages
            WHERE recipient_user_id = $1 AND recipient_device_id = $2
            ORDER BY stream_id ASC
            "#,
            user_id,
            device_id
        )
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Database error: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        let mut messages = Vec::new();
        for row in rows {
            let mut msg = serde_json::json!({
                "type": row.event_type,
                "sender": row.sender_user_id,
                "content": row.content
            });
            if let Some(mid) = row.message_id {
                if let Some(obj) = msg.as_object_mut() {
                    obj.insert("message_id".to_string(), serde_json::json!(mid));
                }
            }
            messages.push(msg);
        }

        Ok(messages)
    }

    pub async fn get_and_delete_messages(&self, user_id: &str, device_id: &str) -> Result<Vec<Value>, ApiError> {
        let rows = sqlx::query!(
            r#"
            DELETE FROM to_device_messages
            WHERE recipient_user_id = $1 AND recipient_device_id = $2
            RETURNING id, stream_id, sender_user_id, event_type, content, message_id, created_ts
            "#,
            user_id,
            device_id
        )
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Database error: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        let mut messages = Vec::new();
        for row in rows {
            let mut msg = serde_json::json!({
                "type": row.event_type,
                "sender": row.sender_user_id,
                "content": row.content
            });
            if let Some(mid) = row.message_id {
                if let Some(obj) = msg.as_object_mut() {
                    obj.insert("message_id".to_string(), serde_json::json!(mid));
                }
            }
            messages.push(msg);
        }

        Ok(messages)
    }

    pub async fn delete_messages(&self, ids: &[i64]) -> Result<(), ApiError> {
        // SKIP: ANY($1) array parameter — keep dynamic query
        sqlx::query(
            r"
            DELETE FROM to_device_messages
            WHERE id = ANY($1)
            ",
        )
        .bind(ids)
        .execute(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Database error: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(())
    }

    pub async fn delete_messages_up_to(&self, user_id: &str, device_id: &str, stream_id: i64) -> Result<(), ApiError> {
        sqlx::query!(
            r#"
            DELETE FROM to_device_messages
            WHERE recipient_user_id = $1
              AND recipient_device_id = $2
              AND stream_id <= $3
            "#,
            user_id,
            device_id,
            stream_id
        )
        .execute(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Database error: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(())
    }
}

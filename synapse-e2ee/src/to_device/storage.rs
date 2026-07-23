use serde_json::Value;
use sqlx::{Pool, Postgres, Row};
use std::sync::Arc;
use synapse_common::current_timestamp_millis;
use synapse_common::ApiError;

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
        let now = current_timestamp_millis();
        let result = sqlx::query(
            r"
            SELECT 1 AS hit FROM devices
                WHERE user_id = $1 AND device_id = $2
            UNION ALL
            SELECT 1 AS hit FROM dehydrated_devices
                WHERE user_id = $1 AND device_id = $2
                  AND (expires_at IS NULL OR expires_at > $3)
            LIMIT 1
            ",
        )
        .bind(user_id)
        .bind(device_id)
        .bind(now)
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
    ) -> Result<bool, ApiError> {
        let now = current_timestamp_millis();
        let row = sqlx::query(
            r"
            INSERT INTO to_device_transactions (sender_user_id, sender_device_id, message_id, created_ts)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (sender_user_id, sender_device_id, message_id) DO NOTHING
            RETURNING id
            ",
        )
        .bind(sender_user_id)
        .bind(sender_device_id)
        .bind(message_id)
        .bind(now)
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Database error: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;
        Ok(row.is_some())
    }

    pub async fn cleanup_old_transactions(&self, max_age_ms: i64) -> Result<u64, ApiError> {
        let cutoff = current_timestamp_millis() - max_age_ms;
        let result = sqlx::query(
            r"
            DELETE FROM to_device_transactions
            WHERE created_ts < $1
            ",
        )
        .bind(cutoff)
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

        let now = current_timestamp_millis();
        sqlx::query(
            r"
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
            ",
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
        .map_err(|e| {
            tracing::error!("Database error: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(())
    }

    pub async fn get_messages(&self, user_id: &str, device_id: &str) -> Result<Vec<Value>, ApiError> {
        let rows = sqlx::query(
            r"
            SELECT id, stream_id, sender_user_id, event_type, content, message_id, created_ts
            FROM to_device_messages
            WHERE recipient_user_id = $1 AND recipient_device_id = $2
            ORDER BY stream_id ASC
            ",
        )
        .bind(user_id)
        .bind(device_id)
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Database error: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

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

    pub async fn get_messages_since(
        &self,
        user_id: &str,
        device_id: &str,
        since_stream_id: i64,
        limit: i64,
    ) -> Result<(Vec<Value>, i64), ApiError> {
        let rows = sqlx::query(
            r"
            SELECT sender_user_id, event_type, content, message_id, stream_id
            FROM to_device_messages
            WHERE recipient_user_id = $1
              AND recipient_device_id = $2
              AND stream_id > $3
            ORDER BY stream_id ASC
            LIMIT $4
            ",
        )
        .bind(user_id)
        .bind(device_id)
        .bind(since_stream_id)
        .bind(limit)
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Database error: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        let mut max_stream_id = since_stream_id;
        let mut messages = Vec::with_capacity(rows.len());
        for row in rows {
            let sender_user_id: String = row.get("sender_user_id");
            let event_type: String = row.get("event_type");
            let content: Value = row.get("content");
            let message_id: Option<String> = row.get("message_id");
            let stream_id: i64 = row.get("stream_id");
            if stream_id > max_stream_id {
                max_stream_id = stream_id;
            }

            let mut msg = serde_json::json!({
                "type": event_type,
                "sender": sender_user_id,
                "content": content
            });
            if let Some(mid) = message_id {
                if let Some(obj) = msg.as_object_mut() {
                    obj.insert("message_id".to_string(), serde_json::json!(mid));
                }
            }
            messages.push(msg);
        }

        Ok((messages, max_stream_id))
    }

    pub async fn get_current_stream_id(&self, user_id: &str, device_id: &str) -> Result<i64, ApiError> {
        let max_id: Option<i64> = sqlx::query_scalar(
            r"
            SELECT COALESCE(MAX(stream_id), 0)
            FROM to_device_messages
            WHERE recipient_user_id = $1
              AND recipient_device_id = $2
            ",
        )
        .bind(user_id)
        .bind(device_id)
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Database error: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(max_id.unwrap_or(0))
    }

    pub async fn has_messages_since(
        &self,
        user_id: &str,
        device_id: &str,
        since_stream_id: i64,
    ) -> Result<bool, ApiError> {
        let row = sqlx::query(
            r"
            SELECT 1
            FROM to_device_messages
            WHERE recipient_user_id = $1
              AND recipient_device_id = $2
              AND stream_id > $3
            LIMIT 1
            ",
        )
        .bind(user_id)
        .bind(device_id)
        .bind(since_stream_id)
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Database error: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(row.is_some())
    }

    pub async fn get_and_delete_messages(&self, user_id: &str, device_id: &str) -> Result<Vec<Value>, ApiError> {
        let rows = sqlx::query(
            r"
            DELETE FROM to_device_messages
            WHERE recipient_user_id = $1 AND recipient_device_id = $2
            RETURNING id, stream_id, sender_user_id, event_type, content, message_id, created_ts
            ",
        )
        .bind(user_id)
        .bind(device_id)
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Database error: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

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
        sqlx::query(
            r"
            DELETE FROM to_device_messages
            WHERE recipient_user_id = $1
              AND recipient_device_id = $2
              AND stream_id <= $3
            ",
        )
        .bind(user_id)
        .bind(device_id)
        .bind(stream_id)
        .execute(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Database error: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(())
    }
}

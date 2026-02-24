use crate::common::error::ApiError;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use std::sync::Arc;
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct PushDevice {
    pub id: i32,
    pub user_id: String,
    pub device_id: String,
    pub push_token: String,
    pub push_type: String,
    pub app_id: Option<String>,
    pub platform: Option<String>,
    pub platform_version: Option<String>,
    pub app_version: Option<String>,
    pub locale: Option<String>,
    pub timezone: Option<String>,
    #[sqlx(rename = "is_enabled")]
    pub enabled: bool,
    pub created_ts: i64,
    pub updated_ts: Option<i64>,
    pub last_used_at: Option<chrono::DateTime<Utc>>,
    pub last_error: Option<String>,
    pub error_count: i32,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct PushRule {
    pub id: i32,
    pub user_id: String,
    pub rule_id: String,
    pub scope: String,
    pub kind: String,
    pub priority: i32,
    pub conditions: serde_json::Value,
    pub actions: serde_json::Value,
    #[sqlx(rename = "is_enabled")]
    pub enabled: bool,
    pub is_default: bool,
    pub created_ts: i64,
    pub updated_ts: Option<i64>,
    pub pattern: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct PushNotificationQueue {
    pub id: i32,
    pub user_id: String,
    pub device_id: String,
    pub event_id: Option<String>,
    pub room_id: Option<String>,
    pub notification_type: Option<String>,
    pub content: serde_json::Value,
    pub priority: i32,
    pub status: String,
    pub attempts: i32,
    pub max_attempts: i32,
    pub next_attempt_at: chrono::DateTime<Utc>,
    pub created_ts: i64,
    pub sent_at: Option<chrono::DateTime<Utc>>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct PushNotificationLog {
    pub id: i32,
    pub user_id: String,
    pub device_id: String,
    pub event_id: Option<String>,
    pub room_id: Option<String>,
    pub notification_type: Option<String>,
    pub push_type: String,
    pub sent_at: DateTime<Utc>,
    pub success: bool,
    pub error_message: Option<String>,
    pub provider_response: Option<String>,
    pub response_time_ms: Option<i32>,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RegisterDeviceRequest {
    pub user_id: String,
    pub device_id: String,
    pub push_token: String,
    pub push_type: String,
    pub app_id: Option<String>,
    pub platform: Option<String>,
    pub platform_version: Option<String>,
    pub app_version: Option<String>,
    pub locale: Option<String>,
    pub timezone: Option<String>,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreatePushRuleRequest {
    pub user_id: String,
    pub rule_id: String,
    pub scope: String,
    pub kind: String,
    pub priority: i32,
    pub conditions: serde_json::Value,
    pub actions: serde_json::Value,
    pub enabled: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct QueueNotificationRequest {
    pub user_id: String,
    pub device_id: String,
    pub event_id: Option<String>,
    pub room_id: Option<String>,
    pub notification_type: Option<String>,
    pub content: serde_json::Value,
    pub priority: i32,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct CreateNotificationLogRequest {
    pub user_id: String,
    pub device_id: String,
    pub event_id: Option<String>,
    pub room_id: Option<String>,
    pub notification_type: Option<String>,
    pub push_type: String,
    pub success: bool,
    pub error_message: Option<String>,
    pub provider_response: Option<String>,
    pub response_time_ms: Option<i32>,
}

impl CreateNotificationLogRequest {
    pub fn new(
        user_id: impl Into<String>,
        device_id: impl Into<String>,
        push_type: impl Into<String>,
        success: bool,
    ) -> Self {
        Self {
            user_id: user_id.into(),
            device_id: device_id.into(),
            push_type: push_type.into(),
            success,
            ..Default::default()
        }
    }

    pub fn event_id(mut self, event_id: impl Into<String>) -> Self {
        self.event_id = Some(event_id.into());
        self
    }

    pub fn room_id(mut self, room_id: impl Into<String>) -> Self {
        self.room_id = Some(room_id.into());
        self
    }

    pub fn notification_type(mut self, notification_type: impl Into<String>) -> Self {
        self.notification_type = Some(notification_type.into());
        self
    }

    pub fn error_message(mut self, error_message: impl Into<String>) -> Self {
        self.error_message = Some(error_message.into());
        self
    }

    pub fn provider_response(mut self, provider_response: impl Into<String>) -> Self {
        self.provider_response = Some(provider_response.into());
        self
    }

    pub fn response_time_ms(mut self, response_time_ms: i32) -> Self {
        self.response_time_ms = Some(response_time_ms);
        self
    }
}

#[derive(Debug, Clone)]
pub struct PushNotificationStorage {
    pool: Arc<PgPool>,
}

impl PushNotificationStorage {
    pub fn new(pool: &Arc<PgPool>) -> Self {
        Self { pool: pool.clone() }
    }

    pub async fn register_device(
        &self,
        request: RegisterDeviceRequest,
    ) -> Result<PushDevice, ApiError> {
        let now = chrono::Utc::now().timestamp_millis();
        let metadata = request.metadata.unwrap_or(serde_json::json!({}));

        let row = sqlx::query_as::<_, PushDevice>(
            r#"
            INSERT INTO push_device (
                user_id, device_id, push_token, push_type, app_id, platform,
                platform_version, app_version, locale, timezone, created_ts, updated_ts, metadata
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $11, $12)
            ON CONFLICT (user_id, device_id) DO UPDATE SET
                push_token = $3,
                push_type = $4,
                app_id = $5,
                platform = $6,
                platform_version = $7,
                app_version = $8,
                locale = $9,
                timezone = $10,
                updated_ts = $11,
                is_enabled = true,
                metadata = $12
            RETURNING *
            "#,
        )
        .bind(&request.user_id)
        .bind(&request.device_id)
        .bind(&request.push_token)
        .bind(&request.push_type)
        .bind(&request.app_id)
        .bind(&request.platform)
        .bind(&request.platform_version)
        .bind(&request.app_version)
        .bind(&request.locale)
        .bind(&request.timezone)
        .bind(now)
        .bind(&metadata)
        .fetch_one(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to register device: {}", e)))?;

        info!(
            "Registered push device: {} for user: {}",
            request.device_id, request.user_id
        );
        Ok(row)
    }

    pub async fn unregister_device(&self, user_id: &str, device_id: &str) -> Result<(), ApiError> {
        sqlx::query(
            "UPDATE push_device SET is_enabled = false WHERE user_id = $1 AND device_id = $2",
        )
        .bind(user_id)
        .bind(device_id)
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to unregister device: {}", e)))?;

        info!(
            "Unregistered push device: {} for user: {}",
            device_id, user_id
        );
        Ok(())
    }

    pub async fn get_user_devices(&self, user_id: &str) -> Result<Vec<PushDevice>, ApiError> {
        let rows = sqlx::query_as::<_, PushDevice>(
            "SELECT * FROM push_device WHERE user_id = $1 AND is_enabled = true",
        )
        .bind(user_id)
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get user devices: {}", e)))?;

        Ok(rows)
    }

    pub async fn get_device(
        &self,
        user_id: &str,
        device_id: &str,
    ) -> Result<Option<PushDevice>, ApiError> {
        let row = sqlx::query_as::<_, PushDevice>(
            "SELECT * FROM push_device WHERE user_id = $1 AND device_id = $2 AND is_enabled = true",
        )
        .bind(user_id)
        .bind(device_id)
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get device: {}", e)))?;

        Ok(row)
    }

    pub async fn update_device_last_used(
        &self,
        user_id: &str,
        device_id: &str,
    ) -> Result<(), ApiError> {
        let now = chrono::Utc::now().timestamp_millis();

        sqlx::query(
            "UPDATE push_device SET last_used_at = to_timestamp($1 / 1000.0), updated_ts = $1 WHERE user_id = $2 AND device_id = $3"
        )
        .bind(now)
        .bind(user_id)
        .bind(device_id)
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to update device last used: {}", e)))?;

        Ok(())
    }

    pub async fn record_device_error(
        &self,
        user_id: &str,
        device_id: &str,
        error: &str,
    ) -> Result<(), ApiError> {
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query(
            r#"
            UPDATE push_device 
            SET last_error = $1, error_count = error_count + 1, updated_ts = $4
            WHERE user_id = $2 AND device_id = $3
            "#,
        )
        .bind(error)
        .bind(user_id)
        .bind(device_id)
        .bind(now)
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to record device error: {}", e)))?;

        Ok(())
    }

    pub async fn create_push_rule(
        &self,
        request: CreatePushRuleRequest,
    ) -> Result<PushRule, ApiError> {
        let now = chrono::Utc::now().timestamp_millis();

        let row = sqlx::query_as::<_, PushRule>(
            r#"
            INSERT INTO push_rules (
                user_id, rule_id, scope, kind, priority, conditions, actions, is_enabled, created_ts, updated_ts
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $9)
            ON CONFLICT (user_id, scope, kind, rule_id) DO UPDATE SET
                priority = $5,
                conditions = $6,
                actions = $7,
                is_enabled = $8,
                updated_ts = $9
            RETURNING *
            "#,
        )
        .bind(&request.user_id)
        .bind(&request.rule_id)
        .bind(&request.scope)
        .bind(&request.kind)
        .bind(request.priority)
        .bind(&request.conditions)
        .bind(&request.actions)
        .bind(request.enabled)
        .bind(now)
        .fetch_one(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to create push rule: {}", e)))?;

        info!(
            "Created push rule: {} for user: {}",
            request.rule_id, request.user_id
        );
        Ok(row)
    }

    pub async fn get_user_push_rules(&self, user_id: &str) -> Result<Vec<PushRule>, ApiError> {
        let rows = sqlx::query_as::<_, PushRule>(
            r#"
            SELECT * FROM push_rules 
            WHERE (user_id = $1 OR user_id = '.default') AND is_enabled = true
            ORDER BY priority ASC
            "#,
        )
        .bind(user_id)
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get push rules: {}", e)))?;

        Ok(rows)
    }

    pub async fn delete_push_rule(
        &self,
        user_id: &str,
        scope: &str,
        kind: &str,
        rule_id: &str,
    ) -> Result<(), ApiError> {
        sqlx::query(
            "DELETE FROM push_rules WHERE user_id = $1 AND scope = $2 AND kind = $3 AND rule_id = $4"
        )
        .bind(user_id)
        .bind(scope)
        .bind(kind)
        .bind(rule_id)
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to delete push rule: {}", e)))?;

        Ok(())
    }

    pub async fn queue_notification(
        &self,
        request: QueueNotificationRequest,
    ) -> Result<PushNotificationQueue, ApiError> {
        let now = chrono::Utc::now();

        let row = sqlx::query_as::<_, PushNotificationQueue>(
            r#"
            INSERT INTO push_notification_queue (
                user_id, device_id, event_id, room_id, notification_type, content, priority, status, next_attempt_at, created_ts
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, 'pending', $8, $9)
            RETURNING *
            "#,
        )
        .bind(&request.user_id)
        .bind(&request.device_id)
        .bind(&request.event_id)
        .bind(&request.room_id)
        .bind(&request.notification_type)
        .bind(&request.content)
        .bind(request.priority)
        .bind(now)
        .bind(chrono::Utc::now().timestamp_millis())
        .fetch_one(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to queue notification: {}", e)))?;

        Ok(row)
    }

    pub async fn get_pending_notifications(
        &self,
        limit: i32,
    ) -> Result<Vec<PushNotificationQueue>, ApiError> {
        let now = chrono::Utc::now();

        let rows = sqlx::query_as::<_, PushNotificationQueue>(
            r#"
            SELECT * FROM push_notification_queue 
            WHERE status = 'pending' AND next_attempt_at <= $1
            ORDER BY priority DESC, created_ts ASC
            LIMIT $2
            FOR UPDATE SKIP LOCKED
            "#,
        )
        .bind(now)
        .bind(limit)
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get pending notifications: {}", e)))?;

        Ok(rows)
    }

    pub async fn mark_notification_sent(&self, id: i32) -> Result<(), ApiError> {
        let now = chrono::Utc::now();

        sqlx::query(
            "UPDATE push_notification_queue SET status = 'sent', sent_at = $1 WHERE id = $2",
        )
        .bind(now)
        .bind(id)
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to mark notification sent: {}", e)))?;

        Ok(())
    }

    pub async fn mark_notification_failed(
        &self,
        id: i32,
        error: &str,
        retry: bool,
    ) -> Result<(), ApiError> {
        let now = chrono::Utc::now();

        if retry {
            sqlx::query(
                r#"
                UPDATE push_notification_queue 
                SET status = 'pending', attempts = attempts + 1, error_message = $1, next_attempt_at = $2
                WHERE id = $3 AND attempts < max_attempts
                "#
            )
            .bind(error)
            .bind(now + chrono::Duration::seconds(60))
            .bind(id)
            .execute(&*self.pool)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to mark notification for retry: {}", e)))?;
        } else {
            sqlx::query(
                "UPDATE push_notification_queue SET status = 'failed', error_message = $1 WHERE id = $2"
            )
            .bind(error)
            .bind(id)
            .execute(&*self.pool)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to mark notification failed: {}", e)))?;
        }

        Ok(())
    }

    pub async fn create_notification_log(
        &self,
        request: &CreateNotificationLogRequest,
    ) -> Result<PushNotificationLog, ApiError> {
        let row = sqlx::query_as::<_, PushNotificationLog>(
            r#"
            INSERT INTO push_notification_log (
                user_id, device_id, event_id, room_id, notification_type, push_type,
                success, error_message, provider_response, response_time_ms
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            RETURNING *
            "#,
        )
        .bind(&request.user_id)
        .bind(&request.device_id)
        .bind(&request.event_id)
        .bind(&request.room_id)
        .bind(&request.notification_type)
        .bind(&request.push_type)
        .bind(request.success)
        .bind(&request.error_message)
        .bind(&request.provider_response)
        .bind(request.response_time_ms)
        .fetch_one(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to create notification log: {}", e)))?;

        Ok(row)
    }

    pub async fn get_config(&self, config_key: &str) -> Result<Option<String>, ApiError> {
        let row: Option<(String,)> =
            sqlx::query_as("SELECT config_value FROM push_config WHERE config_key = $1")
                .bind(config_key)
                .fetch_optional(&*self.pool)
                .await
                .map_err(|e| ApiError::internal(format!("Failed to get config: {}", e)))?;

        Ok(row.map(|r| r.0))
    }

    pub async fn get_config_as_bool(
        &self,
        config_key: &str,
        default: bool,
    ) -> Result<bool, ApiError> {
        let value = self.get_config(config_key).await?;

        Ok(match value {
            Some(v) => v.to_lowercase() == "true",
            None => default,
        })
    }

    pub async fn get_config_as_int(&self, config_key: &str, default: i32) -> Result<i32, ApiError> {
        let value = self.get_config(config_key).await?;

        Ok(match value {
            Some(v) => v.parse().unwrap_or(default),
            None => default,
        })
    }

    pub async fn cleanup_old_logs(&self, days: i32) -> Result<u64, ApiError> {
        let cutoff = Utc::now() - chrono::Duration::days(days as i64);

        let result = sqlx::query("DELETE FROM push_notification_log WHERE sent_at < $1")
            .bind(cutoff)
            .execute(&*self.pool)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to cleanup logs: {}", e)))?;

        info!(
            "Cleaned up {} old notification logs",
            result.rows_affected()
        );
        Ok(result.rows_affected())
    }
}

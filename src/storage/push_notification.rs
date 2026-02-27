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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_register_device_request_creation() {
        let request = RegisterDeviceRequest {
            user_id: "@alice:example.com".to_string(),
            device_id: "DEVICE123".to_string(),
            push_token: "apns_token_abc123".to_string(),
            push_type: "apns".to_string(),
            app_id: Some("com.example.app".to_string()),
            platform: Some("ios".to_string()),
            platform_version: Some("17.0".to_string()),
            app_version: Some("1.0.0".to_string()),
            locale: Some("en-US".to_string()),
            timezone: Some("America/New_York".to_string()),
            metadata: Some(json!({"key": "value"})),
        };

        assert_eq!(request.user_id, "@alice:example.com");
        assert_eq!(request.device_id, "DEVICE123");
        assert_eq!(request.push_token, "apns_token_abc123");
        assert_eq!(request.push_type, "apns");
        assert!(request.app_id.is_some());
        assert!(request.metadata.is_some());
    }

    #[test]
    fn test_register_device_request_minimal_fields() {
        let request = RegisterDeviceRequest {
            user_id: "@bob:example.com".to_string(),
            device_id: "DEVICE456".to_string(),
            push_token: "fcm_token_xyz789".to_string(),
            push_type: "fcm".to_string(),
            app_id: None,
            platform: None,
            platform_version: None,
            app_version: None,
            locale: None,
            timezone: None,
            metadata: None,
        };

        assert_eq!(request.user_id, "@bob:example.com");
        assert_eq!(request.push_type, "fcm");
        assert!(request.app_id.is_none());
        assert!(request.platform.is_none());
        assert!(request.metadata.is_none());
    }

    #[test]
    fn test_create_push_rule_request_creation() {
        let conditions = json!([
            {"kind": "event_match", "key": "type", "pattern": "m.room.message"}
        ]);
        let actions = json!(["notify", {"set_tweak": "highlight", "value": true}]);

        let request = CreatePushRuleRequest {
            user_id: "@alice:example.com".to_string(),
            rule_id: ".m.rule.message".to_string(),
            scope: "global".to_string(),
            kind: "content".to_string(),
            priority: 0,
            conditions: conditions.clone(),
            actions: actions.clone(),
            enabled: true,
        };

        assert_eq!(request.user_id, "@alice:example.com");
        assert_eq!(request.rule_id, ".m.rule.message");
        assert_eq!(request.scope, "global");
        assert_eq!(request.kind, "content");
        assert_eq!(request.priority, 0);
        assert!(request.enabled);
        assert_eq!(request.conditions, conditions);
        assert_eq!(request.actions, actions);
    }

    #[test]
    fn test_queue_notification_request_creation() {
        let content = json!({
            "room_name": "Test Room",
            "sender": "@bob:example.com",
            "body": "Hello World"
        });

        let request = QueueNotificationRequest {
            user_id: "@alice:example.com".to_string(),
            device_id: "DEVICE123".to_string(),
            event_id: Some("$event123".to_string()),
            room_id: Some("!room123:example.com".to_string()),
            notification_type: Some("m.room.message".to_string()),
            content: content.clone(),
            priority: 10,
        };

        assert_eq!(request.user_id, "@alice:example.com");
        assert_eq!(request.device_id, "DEVICE123");
        assert!(request.event_id.is_some());
        assert!(request.room_id.is_some());
        assert_eq!(request.priority, 10);
        assert_eq!(request.content, content);
    }

    #[test]
    fn test_queue_notification_request_minimal() {
        let request = QueueNotificationRequest {
            user_id: "@charlie:example.com".to_string(),
            device_id: "DEVICE789".to_string(),
            event_id: None,
            room_id: None,
            notification_type: None,
            content: json!({}),
            priority: 0,
        };

        assert_eq!(request.user_id, "@charlie:example.com");
        assert!(request.event_id.is_none());
        assert!(request.room_id.is_none());
        assert!(request.notification_type.is_none());
        assert_eq!(request.priority, 0);
    }

    #[test]
    fn test_create_notification_log_request_builder() {
        let request = CreateNotificationLogRequest::new(
            "@alice:example.com",
            "DEVICE123",
            "apns",
            true,
        )
        .event_id("$event456")
        .room_id("!room456:example.com")
        .notification_type("m.room.message")
        .response_time_ms(150);

        assert_eq!(request.user_id, "@alice:example.com");
        assert_eq!(request.device_id, "DEVICE123");
        assert_eq!(request.push_type, "apns");
        assert!(request.success);
        assert_eq!(request.event_id, Some("$event456".to_string()));
        assert_eq!(request.room_id, Some("!room456:example.com".to_string()));
        assert_eq!(request.notification_type, Some("m.room.message".to_string()));
        assert_eq!(request.response_time_ms, Some(150));
        assert!(request.error_message.is_none());
        assert!(request.provider_response.is_none());
    }

    #[test]
    fn test_create_notification_log_request_failure() {
        let request = CreateNotificationLogRequest::new(
            "@bob:example.com",
            "DEVICE456",
            "fcm",
            false,
        )
        .error_message("Invalid token")
        .provider_response("{\"error\": \"InvalidRegistration\"}");

        assert_eq!(request.user_id, "@bob:example.com");
        assert_eq!(request.push_type, "fcm");
        assert!(!request.success);
        assert_eq!(request.error_message, Some("Invalid token".to_string()));
        assert_eq!(
            request.provider_response,
            Some("{\"error\": \"InvalidRegistration\"}".to_string())
        );
    }

    #[test]
    fn test_create_notification_log_request_default() {
        let request = CreateNotificationLogRequest::default();

        assert!(request.user_id.is_empty());
        assert!(request.device_id.is_empty());
        assert!(request.push_type.is_empty());
        assert!(!request.success);
        assert!(request.event_id.is_none());
        assert!(request.room_id.is_none());
        assert!(request.notification_type.is_none());
        assert!(request.error_message.is_none());
        assert!(request.provider_response.is_none());
        assert!(request.response_time_ms.is_none());
    }

    #[test]
    fn test_push_device_serialization() {
        let device = PushDevice {
            id: 1,
            user_id: "@alice:example.com".to_string(),
            device_id: "DEVICE123".to_string(),
            push_token: "token123".to_string(),
            push_type: "apns".to_string(),
            app_id: Some("com.example.app".to_string()),
            platform: Some("ios".to_string()),
            platform_version: Some("17.0".to_string()),
            app_version: Some("1.0.0".to_string()),
            locale: Some("en-US".to_string()),
            timezone: Some("America/New_York".to_string()),
            enabled: true,
            created_ts: 1700000000000,
            updated_ts: Some(1700000001000),
            last_used_at: None,
            last_error: None,
            error_count: 0,
            metadata: json!({}),
        };

        let json_str = serde_json::to_string(&device).unwrap();
        let deserialized: PushDevice = serde_json::from_str(&json_str).unwrap();

        assert_eq!(deserialized.id, device.id);
        assert_eq!(deserialized.user_id, device.user_id);
        assert_eq!(deserialized.device_id, device.device_id);
        assert_eq!(deserialized.push_type, device.push_type);
        assert_eq!(deserialized.enabled, device.enabled);
    }

    #[test]
    fn test_push_rule_serialization() {
        let rule = PushRule {
            id: 1,
            user_id: "@alice:example.com".to_string(),
            rule_id: ".m.rule.message".to_string(),
            scope: "global".to_string(),
            kind: "content".to_string(),
            priority: 0,
            conditions: json!([{"kind": "event_match"}]),
            actions: json!(["notify"]),
            enabled: true,
            is_default: false,
            created_ts: 1700000000000,
            updated_ts: None,
            pattern: Some("*.com".to_string()),
        };

        let json_str = serde_json::to_string(&rule).unwrap();
        let deserialized: PushRule = serde_json::from_str(&json_str).unwrap();

        assert_eq!(deserialized.id, rule.id);
        assert_eq!(deserialized.rule_id, rule.rule_id);
        assert_eq!(deserialized.scope, rule.scope);
        assert_eq!(deserialized.kind, rule.kind);
        assert_eq!(deserialized.enabled, rule.enabled);
        assert_eq!(deserialized.pattern, rule.pattern);
    }

    #[test]
    fn test_push_notification_queue_status_values() {
        let valid_statuses = vec!["pending", "sent", "failed"];

        for status in valid_statuses {
            let queue_item = PushNotificationQueue {
                id: 1,
                user_id: "@alice:example.com".to_string(),
                device_id: "DEVICE123".to_string(),
                event_id: None,
                room_id: None,
                notification_type: None,
                content: json!({}),
                priority: 0,
                status: status.to_string(),
                attempts: 0,
                max_attempts: 3,
                next_attempt_at: Utc::now(),
                created_ts: 1700000000000,
                sent_at: None,
                error_message: None,
            };

            assert_eq!(queue_item.status, status);
        }
    }

    #[test]
    fn test_push_notification_queue_retry_logic() {
        let queue_item = PushNotificationQueue {
            id: 1,
            user_id: "@alice:example.com".to_string(),
            device_id: "DEVICE123".to_string(),
            event_id: Some("$event123".to_string()),
            room_id: Some("!room123:example.com".to_string()),
            notification_type: Some("m.room.message".to_string()),
            content: json!({"body": "test"}),
            priority: 5,
            status: "pending".to_string(),
            attempts: 2,
            max_attempts: 5,
            next_attempt_at: Utc::now(),
            created_ts: 1700000000000,
            sent_at: None,
            error_message: Some("Temporary failure".to_string()),
        };

        assert!(queue_item.attempts < queue_item.max_attempts);
        assert_eq!(queue_item.status, "pending");
        assert!(queue_item.error_message.is_some());
    }

    #[test]
    fn test_push_notification_log_success() {
        let log = PushNotificationLog {
            id: 1,
            user_id: "@alice:example.com".to_string(),
            device_id: "DEVICE123".to_string(),
            event_id: Some("$event123".to_string()),
            room_id: Some("!room123:example.com".to_string()),
            notification_type: Some("m.room.message".to_string()),
            push_type: "apns".to_string(),
            sent_at: Utc::now(),
            success: true,
            error_message: None,
            provider_response: Some("{\"status\": \"ok\"}".to_string()),
            response_time_ms: Some(100),
            metadata: json!({}),
        };

        assert!(log.success);
        assert!(log.error_message.is_none());
        assert!(log.response_time_ms.is_some());
    }

    #[test]
    fn test_push_notification_log_failure() {
        let log = PushNotificationLog {
            id: 1,
            user_id: "@bob:example.com".to_string(),
            device_id: "DEVICE456".to_string(),
            event_id: None,
            room_id: None,
            notification_type: None,
            push_type: "fcm".to_string(),
            sent_at: Utc::now(),
            success: false,
            error_message: Some("InvalidRegistration".to_string()),
            provider_response: Some("{\"error\": \"InvalidRegistration\"}".to_string()),
            response_time_ms: Some(50),
            metadata: json!({}),
        };

        assert!(!log.success);
        assert!(log.error_message.is_some());
        assert_eq!(log.push_type, "fcm");
    }

    #[test]
    fn test_push_device_error_tracking() {
        let device = PushDevice {
            id: 1,
            user_id: "@alice:example.com".to_string(),
            device_id: "DEVICE123".to_string(),
            push_token: "token123".to_string(),
            push_type: "apns".to_string(),
            app_id: None,
            platform: None,
            platform_version: None,
            app_version: None,
            locale: None,
            timezone: None,
            enabled: true,
            created_ts: 1700000000000,
            updated_ts: None,
            last_used_at: None,
            last_error: Some("Unregistered".to_string()),
            error_count: 3,
            metadata: json!({}),
        };

        assert!(device.last_error.is_some());
        assert!(device.error_count > 0);
        assert!(device.enabled);
    }

    #[test]
    fn test_push_rule_priority_ordering() {
        let rule_high = PushRule {
            id: 1,
            user_id: "@alice:example.com".to_string(),
            rule_id: "high_priority".to_string(),
            scope: "global".to_string(),
            kind: "override".to_string(),
            priority: 0,
            conditions: json!([]),
            actions: json!(["notify"]),
            enabled: true,
            is_default: false,
            created_ts: 1700000000000,
            updated_ts: None,
            pattern: None,
        };

        let rule_low = PushRule {
            id: 2,
            user_id: "@alice:example.com".to_string(),
            rule_id: "low_priority".to_string(),
            scope: "global".to_string(),
            kind: "content".to_string(),
            priority: 100,
            conditions: json!([]),
            actions: json!(["notify"]),
            enabled: true,
            is_default: false,
            created_ts: 1700000000000,
            updated_ts: None,
            pattern: None,
        };

        assert!(rule_high.priority < rule_low.priority);
    }

    #[test]
    fn test_queue_notification_priority_boundaries() {
        let high_priority = QueueNotificationRequest {
            user_id: "@alice:example.com".to_string(),
            device_id: "DEVICE123".to_string(),
            event_id: None,
            room_id: None,
            notification_type: None,
            content: json!({}),
            priority: i32::MAX,
        };

        let low_priority = QueueNotificationRequest {
            user_id: "@bob:example.com".to_string(),
            device_id: "DEVICE456".to_string(),
            event_id: None,
            room_id: None,
            notification_type: None,
            content: json!({}),
            priority: i32::MIN,
        };

        assert_eq!(high_priority.priority, i32::MAX);
        assert_eq!(low_priority.priority, i32::MIN);
        assert!(high_priority.priority > low_priority.priority);
    }

    #[test]
    fn test_notification_content_json_format() {
        let content = json!({
            "room_name": "Test Room",
            "sender_display_name": "Alice",
            "sender_avatar_url": "mxc://example.com/avatar",
            "event_id": "$event123",
            "room_id": "!room123:example.com",
            "counts": {
                "unread": 5,
                "missed_calls": 2
            }
        });

        let request = QueueNotificationRequest {
            user_id: "@alice:example.com".to_string(),
            device_id: "DEVICE123".to_string(),
            event_id: Some("$event123".to_string()),
            room_id: Some("!room123:example.com".to_string()),
            notification_type: Some("m.room.message".to_string()),
            content: content.clone(),
            priority: 10,
        };

        assert!(request.content.get("counts").is_some());
        assert_eq!(request.content["counts"]["unread"], 5);
        assert_eq!(request.content["room_name"], "Test Room");
    }
}

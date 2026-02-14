use crate::common::ApiError;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ServerNotification {
    pub id: i32,
    pub title: String,
    pub content: String,
    pub notification_type: String,
    pub priority: i32,
    pub target_audience: String,
    pub target_user_ids: serde_json::Value,
    pub starts_at: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
    pub is_active: bool,
    pub is_dismissible: bool,
    pub action_url: Option<String>,
    pub action_text: Option<String>,
    pub created_by: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct UserNotificationStatus {
    pub id: i32,
    pub user_id: String,
    pub notification_id: i32,
    pub is_read: bool,
    pub is_dismissed: bool,
    pub read_at: Option<DateTime<Utc>>,
    pub dismissed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct NotificationTemplate {
    pub id: i32,
    pub name: String,
    pub title_template: String,
    pub content_template: String,
    pub notification_type: String,
    pub variables: serde_json::Value,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct NotificationDeliveryLog {
    pub id: i32,
    pub notification_id: i32,
    pub user_id: Option<String>,
    pub delivery_method: String,
    pub status: String,
    pub error_message: Option<String>,
    pub delivered_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ScheduledNotification {
    pub id: i32,
    pub notification_id: i32,
    pub scheduled_for: DateTime<Utc>,
    pub is_sent: bool,
    pub sent_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateNotificationRequest {
    pub title: String,
    pub content: String,
    pub notification_type: Option<String>,
    pub priority: Option<i32>,
    pub target_audience: Option<String>,
    pub target_user_ids: Option<Vec<String>>,
    pub starts_at: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
    pub is_dismissible: Option<bool>,
    pub action_url: Option<String>,
    pub action_text: Option<String>,
    pub created_by: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateTemplateRequest {
    pub name: String,
    pub title_template: String,
    pub content_template: String,
    pub notification_type: Option<String>,
    pub variables: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationWithStatus {
    #[serde(flatten)]
    pub notification: ServerNotification,
    pub is_read: bool,
    pub is_dismissed: bool,
}

#[derive(Clone)]
pub struct ServerNotificationStorage {
    pool: PgPool,
}

impl ServerNotificationStorage {
    pub fn new(pool: &Arc<PgPool>) -> Self {
        Self { pool: (**pool).clone() }
    }

    pub async fn create_notification(&self, request: CreateNotificationRequest) -> Result<ServerNotification, ApiError> {
        let target_user_ids = serde_json::to_value(request.target_user_ids.unwrap_or_default())
            .unwrap_or(serde_json::json!([]));

        let notification = sqlx::query_as::<_, ServerNotification>(
            r#"
            INSERT INTO server_notifications (
                title, content, notification_type, priority, target_audience,
                target_user_ids, starts_at, expires_at, is_dismissible,
                action_url, action_text, created_by
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
            RETURNING *
            "#,
        )
        .bind(&request.title)
        .bind(&request.content)
        .bind(request.notification_type.unwrap_or_else(|| "info".to_string()))
        .bind(request.priority.unwrap_or(0))
        .bind(request.target_audience.unwrap_or_else(|| "all".to_string()))
        .bind(&target_user_ids)
        .bind(request.starts_at)
        .bind(request.expires_at)
        .bind(request.is_dismissible.unwrap_or(true))
        .bind(&request.action_url)
        .bind(&request.action_text)
        .bind(&request.created_by)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to create notification: {}", e)))?;

        Ok(notification)
    }

    pub async fn get_notification(&self, notification_id: i32) -> Result<Option<ServerNotification>, ApiError> {
        let notification = sqlx::query_as::<_, ServerNotification>(
            r#"SELECT * FROM server_notifications WHERE id = $1"#,
        )
        .bind(notification_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get notification: {}", e)))?;

        Ok(notification)
    }

    pub async fn list_active_notifications(&self) -> Result<Vec<ServerNotification>, ApiError> {
        let now = Utc::now();

        let notifications = sqlx::query_as::<_, ServerNotification>(
            r#"
            SELECT * FROM server_notifications
            WHERE is_active = TRUE
            AND (starts_at IS NULL OR starts_at <= $1)
            AND (expires_at IS NULL OR expires_at > $1)
            ORDER BY priority DESC, created_at DESC
            "#,
        )
        .bind(now)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to list active notifications: {}", e)))?;

        Ok(notifications)
    }

    pub async fn list_all_notifications(&self, limit: i64, offset: i64) -> Result<Vec<ServerNotification>, ApiError> {
        let notifications = sqlx::query_as::<_, ServerNotification>(
            r#"
            SELECT * FROM server_notifications
            ORDER BY created_at DESC
            LIMIT $1 OFFSET $2
            "#,
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to list notifications: {}", e)))?;

        Ok(notifications)
    }

    pub async fn update_notification(&self, notification_id: i32, request: CreateNotificationRequest) -> Result<ServerNotification, ApiError> {
        let now = Utc::now();
        let target_user_ids = serde_json::to_value(request.target_user_ids.unwrap_or_default())
            .unwrap_or(serde_json::json!([]));

        let notification = sqlx::query_as::<_, ServerNotification>(
            r#"
            UPDATE server_notifications
            SET
                title = $1,
                content = $2,
                notification_type = $3,
                priority = $4,
                target_audience = $5,
                target_user_ids = $6,
                starts_at = $7,
                expires_at = $8,
                is_dismissible = $9,
                action_url = $10,
                action_text = $11,
                updated_at = $12
            WHERE id = $13
            RETURNING *
            "#,
        )
        .bind(&request.title)
        .bind(&request.content)
        .bind(request.notification_type.unwrap_or_else(|| "info".to_string()))
        .bind(request.priority.unwrap_or(0))
        .bind(request.target_audience.unwrap_or_else(|| "all".to_string()))
        .bind(&target_user_ids)
        .bind(request.starts_at)
        .bind(request.expires_at)
        .bind(request.is_dismissible.unwrap_or(true))
        .bind(&request.action_url)
        .bind(&request.action_text)
        .bind(now)
        .bind(notification_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to update notification: {}", e)))?;

        Ok(notification)
    }

    pub async fn delete_notification(&self, notification_id: i32) -> Result<bool, ApiError> {
        let result = sqlx::query(
            r#"DELETE FROM server_notifications WHERE id = $1"#,
        )
        .bind(notification_id)
        .execute(&self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to delete notification: {}", e)))?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn deactivate_notification(&self, notification_id: i32) -> Result<bool, ApiError> {
        let result = sqlx::query(
            r#"UPDATE server_notifications SET is_active = FALSE WHERE id = $1"#,
        )
        .bind(notification_id)
        .execute(&self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to deactivate notification: {}", e)))?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn get_user_notifications(&self, user_id: &str) -> Result<Vec<NotificationWithStatus>, ApiError> {
        let now = Utc::now();

        let notifications = sqlx::query_as::<_, ServerNotification>(
            r#"
            SELECT * FROM server_notifications
            WHERE is_active = TRUE
            AND (starts_at IS NULL OR starts_at <= $1)
            AND (expires_at IS NULL OR expires_at > $1)
            AND (target_audience = 'all' OR target_user_ids ? $2)
            ORDER BY priority DESC, created_at DESC
            "#,
        )
        .bind(now)
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get user notifications: {}", e)))?;

        let mut result = Vec::new();
        for notification in notifications {
            let status = self.get_or_create_status(user_id, notification.id).await?;
            result.push(NotificationWithStatus {
                notification,
                is_read: status.is_read,
                is_dismissed: status.is_dismissed,
            });
        }

        Ok(result)
    }

    pub async fn get_or_create_status(&self, user_id: &str, notification_id: i32) -> Result<UserNotificationStatus, ApiError> {
        let status = sqlx::query_as::<_, UserNotificationStatus>(
            r#"
            INSERT INTO user_notification_status (user_id, notification_id)
            VALUES ($1, $2)
            ON CONFLICT (user_id, notification_id) DO NOTHING
            RETURNING *
            "#,
        )
        .bind(user_id)
        .bind(notification_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to create notification status: {}", e)))?;

        if let Some(status) = status {
            return Ok(status);
        }

        sqlx::query_as::<_, UserNotificationStatus>(
            r#"
            SELECT * FROM user_notification_status
            WHERE user_id = $1 AND notification_id = $2
            "#,
        )
        .bind(user_id)
        .bind(notification_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get notification status: {}", e)))
    }

    pub async fn mark_as_read(&self, user_id: &str, notification_id: i32) -> Result<bool, ApiError> {
        let now = Utc::now();
        let result = sqlx::query(
            r#"
            INSERT INTO user_notification_status (user_id, notification_id, is_read, read_at)
            VALUES ($1, $2, TRUE, $3)
            ON CONFLICT (user_id, notification_id)
            DO UPDATE SET is_read = TRUE, read_at = $3
            "#,
        )
        .bind(user_id)
        .bind(notification_id)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to mark notification as read: {}", e)))?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn mark_as_dismissed(&self, user_id: &str, notification_id: i32) -> Result<bool, ApiError> {
        let now = Utc::now();
        let result = sqlx::query(
            r#"
            INSERT INTO user_notification_status (user_id, notification_id, is_dismissed, dismissed_at)
            VALUES ($1, $2, TRUE, $3)
            ON CONFLICT (user_id, notification_id)
            DO UPDATE SET is_dismissed = TRUE, dismissed_at = $3
            "#,
        )
        .bind(user_id)
        .bind(notification_id)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to dismiss notification: {}", e)))?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn mark_all_as_read(&self, user_id: &str) -> Result<i64, ApiError> {
        let now = Utc::now();
        let notifications = self.get_user_notifications(user_id).await?;

        let mut count = 0i64;
        for n in notifications {
            let result = sqlx::query(
                r#"
                INSERT INTO user_notification_status (user_id, notification_id, is_read, read_at)
                VALUES ($1, $2, TRUE, $3)
                ON CONFLICT (user_id, notification_id)
                DO UPDATE SET is_read = TRUE, read_at = $3
                "#,
            )
            .bind(user_id)
            .bind(n.notification.id)
            .bind(now)
            .execute(&self.pool)
            .await;

            if let Ok(r) = result {
                count += r.rows_affected() as i64;
            }
        }

        Ok(count)
    }

    pub async fn create_template(&self, request: CreateTemplateRequest) -> Result<NotificationTemplate, ApiError> {
        let variables = serde_json::to_value(request.variables.unwrap_or_default())
            .unwrap_or(serde_json::json!([]));

        let template = sqlx::query_as::<_, NotificationTemplate>(
            r#"
            INSERT INTO notification_templates (
                name, title_template, content_template, notification_type, variables
            )
            VALUES ($1, $2, $3, $4, $5)
            RETURNING *
            "#,
        )
        .bind(&request.name)
        .bind(&request.title_template)
        .bind(&request.content_template)
        .bind(request.notification_type.unwrap_or_else(|| "info".to_string()))
        .bind(&variables)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to create template: {}", e)))?;

        Ok(template)
    }

    pub async fn get_template(&self, name: &str) -> Result<Option<NotificationTemplate>, ApiError> {
        let template = sqlx::query_as::<_, NotificationTemplate>(
            r#"SELECT * FROM notification_templates WHERE name = $1 AND is_active = TRUE"#,
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get template: {}", e)))?;

        Ok(template)
    }

    pub async fn list_templates(&self) -> Result<Vec<NotificationTemplate>, ApiError> {
        let templates = sqlx::query_as::<_, NotificationTemplate>(
            r#"SELECT * FROM notification_templates WHERE is_active = TRUE ORDER BY name"#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to list templates: {}", e)))?;

        Ok(templates)
    }

    pub async fn delete_template(&self, name: &str) -> Result<bool, ApiError> {
        let result = sqlx::query(
            r#"UPDATE notification_templates SET is_active = FALSE WHERE name = $1"#,
        )
        .bind(name)
        .execute(&self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to delete template: {}", e)))?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn log_delivery(
        &self,
        notification_id: i32,
        user_id: Option<&str>,
        delivery_method: &str,
        status: &str,
        error_message: Option<&str>,
    ) -> Result<(), ApiError> {
        sqlx::query(
            r#"
            INSERT INTO notification_delivery_log (
                notification_id, user_id, delivery_method, status, error_message
            )
            VALUES ($1, $2, $3, $4, $5)
            "#,
        )
        .bind(notification_id)
        .bind(user_id)
        .bind(delivery_method)
        .bind(status)
        .bind(error_message)
        .execute(&self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to log delivery: {}", e)))?;

        Ok(())
    }

    pub async fn schedule_notification(&self, notification_id: i32, scheduled_for: DateTime<Utc>) -> Result<ScheduledNotification, ApiError> {
        let scheduled = sqlx::query_as::<_, ScheduledNotification>(
            r#"
            INSERT INTO scheduled_notifications (notification_id, scheduled_for)
            VALUES ($1, $2)
            RETURNING *
            "#,
        )
        .bind(notification_id)
        .bind(scheduled_for)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to schedule notification: {}", e)))?;

        Ok(scheduled)
    }

    pub async fn get_pending_scheduled_notifications(&self) -> Result<Vec<ScheduledNotification>, ApiError> {
        let now = Utc::now();

        let scheduled = sqlx::query_as::<_, ScheduledNotification>(
            r#"
            SELECT * FROM scheduled_notifications
            WHERE is_sent = FALSE AND scheduled_for <= $1
            "#,
        )
        .bind(now)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get pending scheduled notifications: {}", e)))?;

        Ok(scheduled)
    }

    pub async fn mark_scheduled_sent(&self, scheduled_id: i32) -> Result<bool, ApiError> {
        let now = Utc::now();
        let result = sqlx::query(
            r#"UPDATE scheduled_notifications SET is_sent = TRUE, sent_at = $1 WHERE id = $2"#,
        )
        .bind(now)
        .bind(scheduled_id)
        .execute(&self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to mark scheduled as sent: {}", e)))?;

        Ok(result.rows_affected() > 0)
    }
}

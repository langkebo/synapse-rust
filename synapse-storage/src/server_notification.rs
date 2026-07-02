use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use std::collections::HashMap;
use std::sync::Arc;
use synapse_common::ApiError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerNotificationCursor {
    pub created_ts: i64,
    pub id: i64,
}

pub fn encode_server_notification_cursor(cursor: &ServerNotificationCursor) -> String {
    format!("{}|{}", cursor.created_ts, cursor.id)
}

pub fn decode_server_notification_cursor(cursor: Option<&str>) -> Option<ServerNotificationCursor> {
    let cursor = cursor?;
    let (created_ts, id) = cursor.split_once('|')?;
    Some(ServerNotificationCursor { created_ts: created_ts.parse::<i64>().ok()?, id: id.parse::<i64>().ok()? })
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ServerNotification {
    pub id: i64,
    pub title: String,
    pub content: String,
    pub notification_type: String,
    pub priority: i32,
    pub target_audience: String,
    pub target_user_ids: serde_json::Value,
    pub starts_at: Option<i64>,
    pub expires_at: Option<i64>,
    pub is_enabled: bool,
    pub is_dismissable: bool,
    pub action_url: Option<String>,
    pub action_text: Option<String>,
    pub created_by: Option<String>,
    pub created_ts: i64,
    pub updated_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct UserNotificationStatus {
    pub id: i64,
    pub user_id: String,
    pub notification_id: i64,
    pub is_read: bool,
    pub is_dismissed: bool,
    pub read_ts: Option<i64>,
    pub dismissed_ts: Option<i64>,
    pub created_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct NotificationTemplate {
    pub id: i64,
    pub name: String,
    pub title_template: String,
    pub content_template: String,
    pub notification_type: String,
    pub variables: serde_json::Value,
    pub is_enabled: bool,
    pub created_ts: i64,
    pub updated_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct NotificationDeliveryLog {
    pub id: i64,
    pub notification_id: i64,
    pub user_id: Option<String>,
    pub delivery_method: String,
    pub status: String,
    pub error_message: Option<String>,
    pub delivered_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ScheduledNotification {
    pub id: i64,
    pub notification_id: i64,
    pub scheduled_for: i64,
    pub is_sent: bool,
    pub sent_ts: Option<i64>,
    pub created_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateNotificationRequest {
    pub title: String,
    pub content: String,
    pub notification_type: Option<String>,
    pub priority: Option<i32>,
    pub target_audience: Option<String>,
    pub target_user_ids: Option<Vec<String>>,
    pub starts_at: Option<i64>,
    pub expires_at: Option<i64>,
    pub is_dismissable: Option<bool>,
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
    pub pool: PgPool,
}

impl ServerNotificationStorage {
    pub fn new(pool: &Arc<PgPool>) -> Self {
        Self { pool: (**pool).clone() }
    }

    pub async fn create_notification(
        &self,
        request: CreateNotificationRequest,
    ) -> Result<ServerNotification, ApiError> {
        let target_user_ids =
            serde_json::to_value(request.target_user_ids.unwrap_or_default()).unwrap_or(serde_json::json!([]));
        let now = chrono::Utc::now().timestamp_millis();

        let notification = sqlx::query_as::<_, ServerNotification>(
            r#"
            INSERT INTO server_notifications (
                title, content, notification_type, priority, target_audience,
                target_user_ids, starts_at, expires_at, is_dismissable,
                action_url, action_text, created_by, created_ts, updated_ts
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $13)
            RETURNING id, title, content, notification_type, priority, target_audience, target_user_ids, starts_at, expires_at, is_enabled, is_dismissable, action_url, action_text, created_by, created_ts, updated_ts
            "#,
        )
        .bind(&request.title)
        .bind(&request.content)
        .bind(
            request
                .notification_type
                .unwrap_or_else(|| "info".to_string()),
        )
        .bind(request.priority.unwrap_or(0))
        .bind(request.target_audience.unwrap_or_else(|| "all".to_string()))
        .bind(&target_user_ids)
        .bind(request.starts_at)
        .bind(request.expires_at)
        .bind(request.is_dismissable.unwrap_or(true))
        .bind(&request.action_url)
        .bind(&request.action_text)
        .bind(&request.created_by)
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to create notification", &e))?;

        Ok(notification)
    }

    pub async fn get_notification(&self, notification_id: i64) -> Result<Option<ServerNotification>, ApiError> {
        let notification = sqlx::query_as::<_, ServerNotification>(
            r#"SELECT id, title, content, notification_type, priority, target_audience, target_user_ids, starts_at, expires_at, is_enabled, is_dismissable, action_url, action_text, created_by, created_ts, updated_ts FROM server_notifications WHERE id = $1"#,
        )
        .bind(notification_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to get notification", &e))?;

        Ok(notification)
    }

    pub async fn list_active_notifications(&self) -> Result<Vec<ServerNotification>, ApiError> {
        let now = Utc::now().timestamp_millis();

        let notifications = sqlx::query_as::<_, ServerNotification>(
            r#"
            SELECT id, title, content, notification_type, priority, target_audience, target_user_ids, starts_at, expires_at, is_enabled, is_dismissable, action_url, action_text, created_by, created_ts, updated_ts
            FROM server_notifications
            WHERE is_enabled = TRUE
            AND (starts_at IS NULL OR starts_at <= $1)
            AND (expires_at IS NULL OR expires_at > $1)
            ORDER BY priority DESC, created_ts DESC
            "#,
        )
        .bind(now)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to list active notifications", &e))?;

        Ok(notifications)
    }

    pub async fn list_all_notifications(
        &self,
        audience: Option<&str>,
        limit: i64,
        from: Option<ServerNotificationCursor>,
    ) -> Result<(Vec<ServerNotification>, Option<String>), ApiError> {
        let notifications = sqlx::query_as::<_, ServerNotification>(
            r#"
            SELECT id, title, content, notification_type, priority, target_audience, target_user_ids, starts_at, expires_at, is_enabled, is_dismissable, action_url, action_text, created_by, created_ts, updated_ts
            FROM server_notifications
            WHERE ($1::text IS NULL OR target_audience = $1)
              AND (
                ($2::BIGINT IS NULL AND $3::BIGINT IS NULL)
                OR created_ts < $2
                OR (created_ts = $2 AND id < $3)
              )
            ORDER BY created_ts DESC, id DESC
            LIMIT $4
            "#,
        )
        .bind(audience)
        .bind(from.as_ref().map(|cursor| cursor.created_ts))
        .bind(from.as_ref().map(|cursor| cursor.id))
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to list notifications", &e))?;

        let next_batch = if notifications.len() as i64 == limit {
            notifications.last().map(|notification| {
                encode_server_notification_cursor(&ServerNotificationCursor {
                    created_ts: notification.created_ts,
                    id: notification.id,
                })
            })
        } else {
            None
        };

        Ok((notifications, next_batch))
    }

    pub async fn update_notification(
        &self,
        notification_id: i64,
        request: CreateNotificationRequest,
    ) -> Result<ServerNotification, ApiError> {
        let now = Utc::now().timestamp_millis();
        let target_user_ids =
            serde_json::to_value(request.target_user_ids.unwrap_or_default()).unwrap_or(serde_json::json!([]));

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
                is_dismissable = $9,
                action_url = $10,
                action_text = $11,
                updated_ts = $12
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
        .bind(request.is_dismissable.unwrap_or(true))
        .bind(&request.action_url)
        .bind(&request.action_text)
        .bind(now)
        .bind(notification_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to update notification", &e))?;

        Ok(notification)
    }

    pub async fn delete_notification(&self, notification_id: i64) -> Result<bool, ApiError> {
        let result = sqlx::query(r#"DELETE FROM server_notifications WHERE id = $1"#)
            .bind(notification_id)
            .execute(&self.pool)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to delete notification", &e))?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn deactivate_notification(&self, notification_id: i64) -> Result<bool, ApiError> {
        let result =
            sqlx::query(r#"UPDATE server_notifications SET is_enabled = FALSE WHERE id = $1 AND is_enabled = TRUE"#)
                .bind(notification_id)
                .execute(&self.pool)
                .await
                .map_err(|e| ApiError::internal_with_log("Failed to deactivate notification", &e))?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn get_user_notifications(&self, user_id: &str) -> Result<Vec<NotificationWithStatus>, ApiError> {
        let now = Utc::now().timestamp_millis();

        let notifications = sqlx::query_as::<_, ServerNotification>(
            r#"
            SELECT id, title, content, notification_type, priority, target_audience,
                   target_user_ids, starts_at, expires_at, is_enabled, is_dismissable,
                   action_url, action_text, created_by, created_ts, updated_ts
            FROM server_notifications
            WHERE is_enabled = TRUE
            AND (starts_at IS NULL OR starts_at <= $1)
            AND (expires_at IS NULL OR expires_at > $1)
            AND (
                target_audience = 'all'
                OR (target_audience = 'specific' AND target_user_ids ? $2)
            )
            ORDER BY priority DESC, created_ts DESC
            "#,
        )
        .bind(now)
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to get user notifications", &e))?;

        let notification_ids: Vec<i64> = notifications.iter().map(|n| n.id).collect();
        let statuses = self.get_or_create_statuses_batch(user_id, &notification_ids).await?;

        let mut result = Vec::new();
        for notification in notifications {
            if let Some(status) = statuses.get(&notification.id) {
                result.push(NotificationWithStatus {
                    notification,
                    is_read: status.is_read,
                    is_dismissed: status.is_dismissed,
                });
            }
        }

        Ok(result)
    }

    pub async fn get_or_create_status(
        &self,
        user_id: &str,
        notification_id: i64,
    ) -> Result<UserNotificationStatus, ApiError> {
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
        .map_err(|e| ApiError::internal_with_log("Failed to create notification status", &e))?;

        if let Some(status) = status {
            return Ok(status);
        }

        sqlx::query_as::<_, UserNotificationStatus>(
            r#"
            SELECT id, user_id, notification_id, is_read, is_dismissed, read_ts, dismissed_ts, created_ts
            FROM user_notification_status
            WHERE user_id = $1 AND notification_id = $2
            "#,
        )
        .bind(user_id)
        .bind(notification_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to get notification status", &e))
    }

    pub async fn get_or_create_statuses_batch(
        &self,
        user_id: &str,
        notification_ids: &[i64],
    ) -> Result<HashMap<i64, UserNotificationStatus>, ApiError> {
        if notification_ids.is_empty() {
            return Ok(HashMap::new());
        }

        sqlx::query(
            r#"
            INSERT INTO user_notification_status (user_id, notification_id)
            SELECT $1, notification_id FROM UNNEST($2::BIGINT[]) AS notification_id
            ON CONFLICT (user_id, notification_id) DO NOTHING
            "#,
        )
        .bind(user_id)
        .bind(notification_ids)
        .execute(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to create notification statuses", &e))?;

        let statuses: Vec<UserNotificationStatus> = sqlx::query_as::<_, UserNotificationStatus>(
            r#"
            SELECT id, user_id, notification_id, is_read, is_dismissed, read_ts, dismissed_ts, created_ts
            FROM user_notification_status
            WHERE user_id = $1 AND notification_id = ANY($2)
            "#,
        )
        .bind(user_id)
        .bind(notification_ids)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to get notification statuses", &e))?;

        Ok(statuses.into_iter().map(|s| (s.notification_id, s)).collect())
    }

    pub async fn mark_as_read(&self, user_id: &str, notification_id: i64) -> Result<bool, ApiError> {
        let exists = sqlx::query_scalar::<_, i64>(
            r#"SELECT COUNT(*) FROM server_notifications WHERE id = $1 AND is_enabled = TRUE"#,
        )
        .bind(notification_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to check notification", &e))?;

        if exists == 0 {
            return Err(ApiError::not_found("Notification not found"));
        }

        let now = Utc::now().timestamp_millis();
        let result = sqlx::query(
            r#"
            INSERT INTO user_notification_status (user_id, notification_id, is_read, read_ts)
            VALUES ($1, $2, TRUE, $3)
            ON CONFLICT (user_id, notification_id)
            DO UPDATE SET is_read = TRUE, read_ts = $3
            "#,
        )
        .bind(user_id)
        .bind(notification_id)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to mark notification as read", &e))?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn mark_as_dismissed(&self, user_id: &str, notification_id: i64) -> Result<bool, ApiError> {
        let exists = sqlx::query_scalar::<_, i64>(
            r#"SELECT COUNT(*) FROM server_notifications WHERE id = $1 AND is_enabled = TRUE"#,
        )
        .bind(notification_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to check notification", &e))?;

        if exists == 0 {
            return Err(ApiError::not_found("Notification not found"));
        }

        let now = Utc::now().timestamp_millis();
        let result = sqlx::query(
            r#"
            INSERT INTO user_notification_status (user_id, notification_id, is_dismissed, dismissed_ts)
            VALUES ($1, $2, TRUE, $3)
            ON CONFLICT (user_id, notification_id)
            DO UPDATE SET is_dismissed = TRUE, dismissed_ts = $3
            "#,
        )
        .bind(user_id)
        .bind(notification_id)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to dismiss notification", &e))?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn mark_all_as_read(&self, user_id: &str) -> Result<i64, ApiError> {
        let now = Utc::now().timestamp_millis();
        let notifications = self.get_user_notifications(user_id).await?;

        let mut count = 0i64;
        for n in notifications {
            let result = sqlx::query(
                r#"
                INSERT INTO user_notification_status (user_id, notification_id, is_read, read_ts)
                VALUES ($1, $2, TRUE, $3)
                ON CONFLICT (user_id, notification_id)
                DO UPDATE SET is_read = TRUE, read_ts = $3
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
        let variables = serde_json::to_value(request.variables.unwrap_or_default()).unwrap_or(serde_json::json!([]));

        let template = sqlx::query_as::<_, NotificationTemplate>(
            r#"
            INSERT INTO notification_templates (
                name, title_template, content_template, notification_type, variables
            )
            VALUES ($1, $2, $3, $4, $5)
            RETURNING id, name, title_template, content_template, notification_type, variables, is_enabled, created_ts, updated_ts
            "#,
        )
        .bind(&request.name)
        .bind(&request.title_template)
        .bind(&request.content_template)
        .bind(
            request
                .notification_type
                .unwrap_or_else(|| "info".to_string()),
        )
        .bind(&variables)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to create template", &e))?;

        Ok(template)
    }

    pub async fn get_template(&self, name: &str) -> Result<Option<NotificationTemplate>, ApiError> {
        let template = sqlx::query_as::<_, NotificationTemplate>(
            r#"SELECT id, name, title_template, content_template, notification_type, variables, is_enabled, created_ts, updated_ts FROM notification_templates WHERE name = $1 AND is_enabled = TRUE"#,
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to get template", &e))?;

        Ok(template)
    }

    pub async fn list_templates(&self) -> Result<Vec<NotificationTemplate>, ApiError> {
        let templates = sqlx::query_as::<_, NotificationTemplate>(
            r#"SELECT id, name, title_template, content_template, notification_type, variables, is_enabled, created_ts, updated_ts FROM notification_templates WHERE is_enabled = TRUE ORDER BY name"#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to list templates", &e))?;

        Ok(templates)
    }

    pub async fn delete_template(&self, name: &str) -> Result<bool, ApiError> {
        let result = sqlx::query(
            r#"UPDATE notification_templates SET is_enabled = FALSE WHERE name = $1 AND is_enabled = TRUE"#,
        )
        .bind(name)
        .execute(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to delete template", &e))?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn log_delivery(
        &self,
        notification_id: i64,
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
        .map_err(|e| ApiError::internal_with_log("Failed to log delivery", &e))?;

        Ok(())
    }

    pub async fn schedule_notification(
        &self,
        notification_id: i64,
        scheduled_for: i64,
    ) -> Result<ScheduledNotification, ApiError> {
        let scheduled = sqlx::query_as::<_, ScheduledNotification>(
            r#"
            INSERT INTO scheduled_notifications (notification_id, scheduled_for)
            VALUES ($1, $2)
            RETURNING id, notification_id, scheduled_for, is_sent, sent_ts, created_ts
            "#,
        )
        .bind(notification_id)
        .bind(scheduled_for)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to schedule notification", &e))?;

        Ok(scheduled)
    }

    pub async fn get_pending_scheduled_notifications(&self) -> Result<Vec<ScheduledNotification>, ApiError> {
        let now = Utc::now().timestamp_millis();

        let scheduled = sqlx::query_as::<_, ScheduledNotification>(
            r#"
            SELECT id, notification_id, scheduled_for, is_sent, sent_ts, created_ts
            FROM scheduled_notifications
            WHERE is_sent = FALSE AND scheduled_for <= $1
            "#,
        )
        .bind(now)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to get pending scheduled notifications", &e))?;

        Ok(scheduled)
    }

    pub async fn mark_scheduled_sent(&self, scheduled_id: i64) -> Result<bool, ApiError> {
        let now = Utc::now().timestamp_millis();
        let result = sqlx::query(
            r#"UPDATE scheduled_notifications SET is_sent = TRUE, sent_ts = $1 WHERE id = $2 AND is_sent = FALSE"#,
        )
        .bind(now)
        .bind(scheduled_id)
        .execute(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to mark scheduled as sent", &e))?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn get_user_notification_setting(&self, user_id: &str) -> Result<Option<bool>, ApiError> {
        let row = sqlx::query("SELECT is_enabled FROM user_notification_settings WHERE user_id = $1")
            .bind(user_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get notification setting", &e))?;

        match row {
            Some(row) => {
                use sqlx::Row;
                Ok(Some(row.get::<Option<bool>, _>("is_enabled").unwrap_or(true)))
            }
            None => Ok(None),
        }
    }

    pub async fn upsert_user_notification_setting(&self, user_id: &str, enabled: bool) -> Result<(), ApiError> {
        sqlx::query(
            "INSERT INTO user_notification_settings (user_id, is_enabled) VALUES ($1, $2) ON CONFLICT (user_id) DO UPDATE SET is_enabled = $2",
        )
        .bind(user_id)
        .bind(enabled)
        .execute(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to upsert notification setting", &e))?;

        Ok(())
    }

    pub async fn get_user_pushers(&self, user_id: &str) -> Result<Vec<serde_json::Value>, ApiError> {
        let rows = sqlx::query(
            "SELECT pushkey, kind, app_id, app_display_name, device_display_name, profile_tag, lang, data FROM pushers WHERE user_id = $1",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to get pushers", &e))?;

        use sqlx::Row;
        let pusher_list: Vec<serde_json::Value> = rows
            .iter()
            .map(|row| {
                serde_json::json!({
                    "pushkey": row.get::<Option<String>, _>("pushkey"),
                    "kind": row.get::<Option<String>, _>("kind"),
                    "app_id": row.get::<Option<String>, _>("app_id"),
                    "app_display_name": row.get::<Option<String>, _>("app_display_name"),
                    "device_display_name": row.get::<Option<String>, _>("device_display_name"),
                    "profile_tag": row.get::<Option<String>, _>("profile_tag"),
                    "lang": row.get::<Option<String>, _>("lang"),
                    "data": row.get::<Option<serde_json::Value>, _>("data").unwrap_or(serde_json::json!({}))
                })
            })
            .collect();

        Ok(pusher_list)
    }

    pub async fn delete_user_pusher(&self, user_id: &str, pushkey: &str) -> Result<bool, ApiError> {
        let result = sqlx::query("DELETE FROM pushers WHERE user_id = $1 AND pushkey = $2")
            .bind(user_id)
            .bind(pushkey)
            .execute(&self.pool)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to delete pusher", &e))?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn get_server_notices_count(&self) -> Result<i64, ApiError> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*)::BIGINT FROM server_notices")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to count server notices", &e))?;

        Ok(count)
    }

    pub async fn get_server_notices_paginated(
        &self,
        cursor: Option<(i64, i64)>,
        limit: i64,
    ) -> Result<(Vec<serde_json::Value>, i64, Option<String>), ApiError> {
        let total = self.get_server_notices_count().await?;

        let rows = sqlx::query(
            "SELECT id, user_id, event_id, content, sent_ts
             FROM server_notices
             WHERE ($1::BIGINT IS NULL AND $2::BIGINT IS NULL)
                OR sent_ts < $1
                OR (sent_ts = $1 AND id < $2)
             ORDER BY sent_ts DESC, id DESC
             LIMIT $3",
        )
        .bind(cursor.map(|(sent_ts, _)| sent_ts))
        .bind(cursor.map(|(_, id)| id))
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to get server notices", &e))?;

        use sqlx::Row;
        let notice_list: Vec<serde_json::Value> = rows
            .iter()
            .map(|row| {
                serde_json::json!({
                    "id": row.get::<Option<i64>, _>("id"),
                    "user_id": row.get::<Option<String>, _>("user_id"),
                    "event_id": row.get::<Option<String>, _>("event_id"),
                    "content": row.get::<Option<String>, _>("content"),
                    "sent_ts": row.get::<Option<i64>, _>("sent_ts")
                })
            })
            .collect();

        let next_batch = if rows.len() as i64 == limit {
            rows.last().map(|row| {
                format!(
                    "{}|{}",
                    row.get::<Option<i64>, _>("sent_ts").unwrap_or_default(),
                    row.get::<Option<i64>, _>("id").unwrap_or_default()
                )
            })
        } else {
            None
        };

        Ok((notice_list, total, next_batch))
    }

    pub async fn get_server_notice_by_id(&self, notice_id: i64) -> Result<Option<serde_json::Value>, ApiError> {
        let row = sqlx::query("SELECT id, user_id, event_id, content, sent_ts FROM server_notices WHERE id = $1")
            .bind(notice_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get server notice", &e))?;

        use sqlx::Row;
        match row {
            Some(row) => Ok(Some(serde_json::json!({
                "id": row.get::<Option<i64>, _>("id"),
                "user_id": row.get::<Option<String>, _>("user_id"),
                "event_id": row.get::<Option<String>, _>("event_id"),
                "content": row.get::<Option<String>, _>("content"),
                "sent_ts": row.get::<Option<i64>, _>("sent_ts")
            }))),
            None => Ok(None),
        }
    }

    pub async fn get_server_notice_with_room(
        &self,
        notice_id: i64,
    ) -> Result<Option<(Option<String>, Option<String>)>, ApiError> {
        let row = sqlx::query(
            r#"
            SELECT sn.event_id, e.room_id
            FROM server_notices sn
            LEFT JOIN events e ON e.event_id = sn.event_id
            WHERE sn.id = $1
            "#,
        )
        .bind(notice_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to get server notice info", &e))?;

        use sqlx::Row;
        match row {
            Some(row) => Ok(Some((row.get("event_id"), row.get("room_id")))),
            None => Ok(None),
        }
    }

    pub async fn delete_server_notice_by_id(&self, notice_id: i64) -> Result<bool, ApiError> {
        let result = sqlx::query("DELETE FROM server_notices WHERE id = $1")
            .bind(notice_id)
            .execute(&self.pool)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to delete server notice", &e))?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn delete_room_cascade(&self, room_id: &str) -> Result<(), ApiError> {
        sqlx::query("DELETE FROM room_memberships WHERE room_id = $1").bind(room_id).execute(&self.pool).await.ok();
        sqlx::query("DELETE FROM room_summaries WHERE room_id = $1").bind(room_id).execute(&self.pool).await.ok();
        sqlx::query("DELETE FROM room_summary_members WHERE room_id = $1").bind(room_id).execute(&self.pool).await.ok();
        sqlx::query("DELETE FROM events WHERE room_id = $1").bind(room_id).execute(&self.pool).await.ok();
        sqlx::query("DELETE FROM rooms WHERE room_id = $1")
            .bind(room_id)
            .execute(&self.pool)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to delete room", &e))?;

        Ok(())
    }

    pub async fn delete_event_by_id(&self, event_id: &str) -> Result<(), ApiError> {
        sqlx::query("DELETE FROM events WHERE event_id = $1")
            .bind(event_id)
            .execute(&self.pool)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to delete event", &e))?;

        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn send_server_notice(
        &self,
        room_id: &str,
        server_user: &str,
        target_user_id: &str,
        target_displayname: &Option<String>,
        target_avatar_url: &Option<String>,
        message_event_id: &str,
        create_event_id: &str,
        membership_event_id: &str,
        msgtype: &str,
        body: &str,
        now: i64,
    ) -> Result<i64, ApiError> {
        let mut tx =
            self.pool.begin().await.map_err(|e| ApiError::internal_with_log("Failed to begin transaction", &e))?;

        let room_result = sqlx::query(
            r#"
            INSERT INTO rooms (
                room_id, name, topic, creator, is_public, join_rules,
                room_version, history_visibility, created_ts, last_activity_ts
            )
            VALUES ($1, $2, $3, $4, false, 'private', '6', 'joined', $5, $5)
            ON CONFLICT (room_id) DO NOTHING
            "#,
        )
        .bind(room_id)
        .bind("Server Notice")
        .bind("System notifications")
        .bind(server_user)
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to create server notice room", &e))?;

        if room_result.rows_affected() == 0 {
            return Err(ApiError::internal("Failed to create server notice room".to_string()));
        }

        let create_result = sqlx::query(
            r#"
            INSERT INTO events (event_id, room_id, user_id, event_type, content, origin_server_ts, sender, state_key)
            VALUES ($1, $2, $3, 'm.room.create', $4, $5, $6, '')
            ON CONFLICT (event_id) DO NOTHING
            "#,
        )
        .bind(create_event_id)
        .bind(room_id)
        .bind(server_user)
        .bind(serde_json::json!({"creator": server_user}))
        .bind(now)
        .bind(server_user)
        .execute(&mut *tx)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to create server notice create event", &e))?;

        if create_result.rows_affected() == 0 {
            return Err(ApiError::internal("Failed to create server notice create event".to_string()));
        }

        let membership_result = sqlx::query(
            r#"
            INSERT INTO events (event_id, room_id, user_id, event_type, content, origin_server_ts, sender, state_key)
            VALUES ($1, $2, $3, 'm.room.member', $4, $5, $6, $7)
            ON CONFLICT (event_id) DO NOTHING
            "#,
        )
        .bind(membership_event_id)
        .bind(room_id)
        .bind(target_user_id)
        .bind(serde_json::json!({ "membership": "join" }))
        .bind(now)
        .bind(server_user)
        .bind(target_user_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to create server notice membership event", &e))?;

        if membership_result.rows_affected() == 0 {
            return Err(ApiError::internal("Failed to create server notice membership event".to_string()));
        }

        let member_result = sqlx::query(
            r#"
            INSERT INTO room_memberships (
                room_id, user_id, sender, membership, event_id, event_type,
                display_name, avatar_url, updated_ts, joined_ts
            )
            VALUES ($1, $2, $3, 'join', $4, 'm.room.member', $5, $6, $7, $7)
            ON CONFLICT (room_id, user_id) DO NOTHING
            "#,
        )
        .bind(room_id)
        .bind(target_user_id)
        .bind(server_user)
        .bind(membership_event_id)
        .bind(target_displayname)
        .bind(target_avatar_url)
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to persist server notice member", &e))?;

        if member_result.rows_affected() == 0 {
            return Err(ApiError::internal("Failed to persist server notice member".to_string()));
        }

        let message_result = sqlx::query(
            r#"
            INSERT INTO events (event_id, room_id, user_id, event_type, content, origin_server_ts, sender)
            VALUES ($1, $2, $3, 'm.room.message', $4, $5, $6)
            "#,
        )
        .bind(message_event_id)
        .bind(room_id)
        .bind(target_user_id)
        .bind(serde_json::json!({
            "msgtype": msgtype,
            "body": body
        }))
        .bind(now)
        .bind(server_user)
        .execute(&mut *tx)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to persist m.room.message event for server notice", &e))?;

        if message_result.rows_affected() == 0 {
            return Err(ApiError::internal("Failed to persist m.room.message event for server notice".to_string()));
        }

        let notice_content = serde_json::json!({
            "msgtype": msgtype,
            "body": body
        });
        let notice_id: i64 = sqlx::query_scalar(
            r#"
            INSERT INTO server_notices (user_id, event_id, content, sent_ts)
            VALUES ($1, $2, $3, $4)
            RETURNING id
            "#,
        )
        .bind(target_user_id)
        .bind(message_event_id)
        .bind(notice_content.to_string())
        .bind(now)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to create server notice record", &e))?;

        let summary_result = sqlx::query(
            r#"
            INSERT INTO room_summaries (
                room_id, name, topic, join_rules, history_visibility, guest_access,
                is_direct, is_space, is_encrypted, member_count, joined_member_count,
                invited_member_count, hero_users, last_event_id, last_event_ts,
                last_message_ts, unread_notifications, unread_highlight, updated_ts, created_ts
            )
            VALUES (
                $1, $2, $3, 'private', 'joined', 'forbidden',
                false, false, false, 1, 1,
                0, '[]'::jsonb, $4, $5,
                $5, 0, 0, $5, $5
            )
            ON CONFLICT (room_id) DO NOTHING
            "#,
        )
        .bind(room_id)
        .bind("Server Notice")
        .bind("System notifications")
        .bind(message_event_id)
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to persist server notice room summary", &e))?;

        if summary_result.rows_affected() == 0 {
            return Err(ApiError::internal("Failed to persist server notice room summary".to_string()));
        }

        let summary_member_result = sqlx::query(
            r#"
            INSERT INTO room_summary_members (
                room_id, user_id, display_name, avatar_url, membership, is_hero,
                last_active_ts, updated_ts, created_ts
            )
            VALUES ($1, $2, $3, $4, 'join', false, $5, $5, $5)
            ON CONFLICT (room_id, user_id) DO NOTHING
            "#,
        )
        .bind(room_id)
        .bind(target_user_id)
        .bind(target_displayname)
        .bind(target_avatar_url)
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to persist server notice room summary member", &e))?;

        if summary_member_result.rows_affected() == 0 {
            return Err(ApiError::internal("Failed to persist server notice room summary member".to_string()));
        }

        tx.commit().await.map_err(|e| ApiError::internal_with_log("Failed to commit server notice transaction", &e))?;

        Ok(notice_id)
    }
}

#[cfg(test)]
mod cursor_tests {
    use super::{decode_server_notification_cursor, encode_server_notification_cursor, ServerNotificationCursor};

    #[test]
    fn server_notification_cursor_round_trip() {
        let cursor = ServerNotificationCursor { created_ts: 1_700_000_000_000, id: 7 };
        let encoded = encode_server_notification_cursor(&cursor);
        assert_eq!(decode_server_notification_cursor(Some(&encoded)), Some(cursor));
    }

    #[test]
    fn server_notification_cursor_rejects_invalid_value() {
        assert_eq!(decode_server_notification_cursor(Some("bad-cursor")), None);
        assert_eq!(decode_server_notification_cursor(Some("123|")), None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};

    #[test]
    fn test_create_notification_request_defaults() {
        let req = CreateNotificationRequest {
            title: "Test".to_string(),
            content: "Content".to_string(),
            notification_type: None,
            priority: None,
            target_audience: None,
            target_user_ids: None,
            starts_at: None,
            expires_at: None,
            is_dismissable: None,
            action_url: None,
            action_text: None,
            created_by: None,
        };

        assert_eq!(req.title, "Test");
        assert_eq!(req.content, "Content");
        assert!(req.notification_type.is_none());
        assert!(req.priority.is_none());
    }

    #[test]
    fn test_notification_type_validation() {
        let valid_types = vec!["info", "warning", "error", "maintenance"];
        for t in valid_types {
            let req = CreateNotificationRequest {
                title: "Test".to_string(),
                content: "Content".to_string(),
                notification_type: Some(t.to_string()),
                priority: None,
                target_audience: None,
                target_user_ids: None,
                starts_at: None,
                expires_at: None,
                is_dismissable: None,
                action_url: None,
                action_text: None,
                created_by: None,
            };
            assert!(req.notification_type.is_some());
        }
    }

    #[test]
    fn test_notification_priority_range() {
        for priority in 0..=2 {
            let req = CreateNotificationRequest {
                title: "Test".to_string(),
                content: "Content".to_string(),
                notification_type: None,
                priority: Some(priority),
                target_audience: None,
                target_user_ids: None,
                starts_at: None,
                expires_at: None,
                is_dismissable: None,
                action_url: None,
                action_text: None,
                created_by: None,
            };
            assert!(req.priority.unwrap() <= 2);
        }
    }

    #[test]
    fn test_target_audience_types() {
        let audiences = vec!["all", "admins", "users"];
        for audience in audiences {
            let req = CreateNotificationRequest {
                title: "Test".to_string(),
                content: "Content".to_string(),
                notification_type: None,
                priority: None,
                target_audience: Some(audience.to_string()),
                target_user_ids: None,
                starts_at: None,
                expires_at: None,
                is_dismissable: None,
                action_url: None,
                action_text: None,
                created_by: None,
            };
            assert_eq!(req.target_audience.unwrap(), audience);
        }
    }

    #[test]
    fn test_scheduled_notification_future_time() {
        let _future_time = Utc::now() + Duration::hours(1);
        let req = CreateNotificationRequest {
            title: "Scheduled".to_string(),
            content: "Future notification".to_string(),
            notification_type: None,
            priority: None,
            target_audience: None,
            target_user_ids: None,
            starts_at: None,
            expires_at: None,
            is_dismissable: None,
            action_url: None,
            action_text: None,
            created_by: None,
        };
        assert!(req.title == "Scheduled");
    }

    #[test]
    fn test_server_notification_struct() {
        let notification = ServerNotification {
            id: 1,
            title: "Test".to_string(),
            content: "Content".to_string(),
            notification_type: "info".to_string(),
            priority: 0,
            target_audience: "all".to_string(),
            target_user_ids: serde_json::json!([]),
            starts_at: None,
            expires_at: None,
            is_enabled: true,
            is_dismissable: true,
            action_url: None,
            action_text: None,
            created_by: Some("admin".to_string()),
            created_ts: Utc::now().timestamp_millis(),
            updated_ts: Utc::now().timestamp_millis(),
        };
        assert_eq!(notification.title, "Test");
        assert!(notification.is_enabled);
    }

    #[test]
    fn test_user_notification_status() {
        let status = UserNotificationStatus {
            id: 1,
            user_id: "@user:example.com".to_string(),
            notification_id: 1,
            is_read: false,
            is_dismissed: false,
            read_ts: None,
            dismissed_ts: None,
            created_ts: Utc::now().timestamp_millis(),
        };
        assert!(!status.is_read);
        assert!(!status.is_dismissed);
    }

    #[test]
    fn test_notification_template() {
        let template = NotificationTemplate {
            id: 1,
            name: "welcome".to_string(),
            title_template: "Welcome!".to_string(),
            content_template: "Hello, {username}!".to_string(),
            notification_type: "info".to_string(),
            variables: serde_json::json!(["username"]),
            is_enabled: true,
            created_ts: Utc::now().timestamp_millis(),
            updated_ts: Utc::now().timestamp_millis(),
        };
        assert_eq!(template.name, "welcome");
        assert!(template.is_enabled);
    }
}

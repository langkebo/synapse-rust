use std::collections::HashMap;
use std::sync::Arc;
use synapse_common::current_timestamp_millis;

use sqlx::PgPool;
use synapse_common::ApiError;

use super::models::*;

/// The `ServerNotificationStorage` struct.
pub struct ServerNotificationStorage {
    /// The `pool` field.
    pub pool: PgPool,
}

impl ServerNotificationStorage {
    /// See [`new`].
    pub fn new(pool: &Arc<PgPool>) -> Self {
        Self { pool: (**pool).clone() }
    }

    /// See [`create_notification`].
    pub async fn create_notification(
        &self,
        request: CreateNotificationRequest,
    ) -> Result<ServerNotification, ApiError> {
        let target_user_ids =
            serde_json::to_value(request.target_user_ids.unwrap_or_default()).unwrap_or(serde_json::json!([]));
        let now = current_timestamp_millis();

        let notification = sqlx::query_as!(
            ServerNotification,
            r#"
            INSERT INTO server_notifications (
                title, content, notification_type, priority, target_audience,
                target_user_ids, starts_at, expires_at, is_dismissable,
                action_url, action_text, created_by, created_ts, updated_ts
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $13)
            RETURNING id, title, content, notification_type, priority, target_audience, target_user_ids, starts_at, expires_at, is_enabled, is_dismissable, action_url, action_text, created_by, created_ts, updated_ts
            "#,
            &request.title,
            &request.content,
            request.notification_type.unwrap_or_else(|| "info".to_string()),
            request.priority.unwrap_or(0),
            request.target_audience.unwrap_or_else(|| "all".to_string()),
            &target_user_ids,
            request.starts_at,
            request.expires_at,
            request.is_dismissable.unwrap_or(true),
            request.action_url.as_deref(),
            request.action_text.as_deref(),
            request.created_by.as_deref(),
            now,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to create notification", e))?;

        Ok(notification)
    }

    /// See [`get_notification`].
    pub async fn get_notification(&self, notification_id: i64) -> Result<Option<ServerNotification>, ApiError> {
        let notification = sqlx::query_as!(
            ServerNotification,
            r#"SELECT id, title, content, notification_type, priority, target_audience, target_user_ids, starts_at, expires_at, is_enabled, is_dismissable, action_url, action_text, created_by, created_ts, updated_ts FROM server_notifications WHERE id = $1"#,
            notification_id,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to get notification", e))?;

        Ok(notification)
    }

    /// See [`list_active_notifications`].
    pub async fn list_active_notifications(&self) -> Result<Vec<ServerNotification>, ApiError> {
        let now = current_timestamp_millis();

        let notifications = sqlx::query_as!(
            ServerNotification,
            r#"
            SELECT id, title, content, notification_type, priority, target_audience, target_user_ids, starts_at, expires_at, is_enabled, is_dismissable, action_url, action_text, created_by, created_ts, updated_ts
            FROM server_notifications
            WHERE is_enabled = TRUE
            AND (starts_at IS NULL OR starts_at <= $1)
            AND (expires_at IS NULL OR expires_at > $1)
            ORDER BY priority DESC, created_ts DESC
            "#,
            now,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to list active notifications", e))?;

        Ok(notifications)
    }

    /// See [`list_all_notifications`].
    pub async fn list_all_notifications(
        &self,
        audience: Option<&str>,
        limit: i64,
        from: Option<ServerNotificationCursor>,
    ) -> Result<(Vec<ServerNotification>, Option<String>), ApiError> {
        let notifications = sqlx::query_as!(
            ServerNotification,
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
            audience,
            from.as_ref().map(|cursor| cursor.created_ts),
            from.as_ref().map(|cursor| cursor.id),
            limit,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to list notifications", e))?;

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

    /// See [`update_notification`].
    pub async fn update_notification(
        &self,
        notification_id: i64,
        request: CreateNotificationRequest,
    ) -> Result<ServerNotification, ApiError> {
        let now = current_timestamp_millis();
        let target_user_ids =
            serde_json::to_value(request.target_user_ids.unwrap_or_default()).unwrap_or(serde_json::json!([]));

        let notification = sqlx::query_as!(
            ServerNotification,
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
            RETURNING id, title, content, notification_type, priority, target_audience, target_user_ids, starts_at, expires_at, is_enabled, is_dismissable, action_url, action_text, created_by, created_ts, updated_ts
            "#,
            &request.title,
            &request.content,
            request.notification_type.unwrap_or_else(|| "info".to_string()),
            request.priority.unwrap_or(0),
            request.target_audience.unwrap_or_else(|| "all".to_string()),
            &target_user_ids,
            request.starts_at,
            request.expires_at,
            request.is_dismissable.unwrap_or(true),
            request.action_url.as_deref(),
            request.action_text.as_deref(),
            now,
            notification_id,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to update notification", e))?;

        Ok(notification)
    }

    /// See [`delete_notification`].
    pub async fn delete_notification(&self, notification_id: i64) -> Result<bool, ApiError> {
        let result = sqlx::query!(r#"DELETE FROM server_notifications WHERE id = $1"#, notification_id)
            .execute(&self.pool)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to delete notification", e))?;

        Ok(result.rows_affected() > 0)
    }

    /// See [`deactivate_notification`].
    pub async fn deactivate_notification(&self, notification_id: i64) -> Result<bool, ApiError> {
        let result = sqlx::query!(
            r#"UPDATE server_notifications SET is_enabled = FALSE WHERE id = $1 AND is_enabled = TRUE"#,
            notification_id,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to deactivate notification", e))?;

        Ok(result.rows_affected() > 0)
    }

    /// See [`get_user_notifications`].
    pub async fn get_user_notifications(&self, user_id: &str) -> Result<Vec<NotificationWithStatus>, ApiError> {
        let now = current_timestamp_millis();

        let notifications = sqlx::query_as!(
            ServerNotification,
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
            now,
            user_id,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to get user notifications", e))?;

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

    /// See [`get_or_create_status`].
    pub async fn get_or_create_status(
        &self,
        user_id: &str,
        notification_id: i64,
    ) -> Result<UserNotificationStatus, ApiError> {
        let status = sqlx::query_as!(
            UserNotificationStatus,
            r#"
            INSERT INTO user_notification_status (user_id, notification_id)
            VALUES ($1, $2)
            ON CONFLICT (user_id, notification_id) DO NOTHING
            RETURNING id, user_id, notification_id, is_read, is_dismissed, read_ts, dismissed_ts, created_ts
            "#,
            user_id,
            notification_id,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to create notification status", e))?;

        if let Some(status) = status {
            return Ok(status);
        }

        sqlx::query_as!(
            UserNotificationStatus,
            r#"
            SELECT id, user_id, notification_id, is_read, is_dismissed, read_ts, dismissed_ts, created_ts
            FROM user_notification_status
            WHERE user_id = $1 AND notification_id = $2
            "#,
            user_id,
            notification_id,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to get notification status", e))
    }

    /// See [`get_or_create_statuses_batch`].
    pub async fn get_or_create_statuses_batch(
        &self,
        user_id: &str,
        notification_ids: &[i64],
    ) -> Result<HashMap<i64, UserNotificationStatus>, ApiError> {
        if notification_ids.is_empty() {
            return Ok(HashMap::new());
        }

        sqlx::query!(
            r#"
            INSERT INTO user_notification_status (user_id, notification_id)
            SELECT $1, notification_id FROM UNNEST($2::BIGINT[]) AS notification_id
            ON CONFLICT (user_id, notification_id) DO NOTHING
            "#,
            user_id,
            notification_ids,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to create notification statuses", e))?;

        let statuses = sqlx::query_as!(
            UserNotificationStatus,
            r#"
            SELECT id, user_id, notification_id, is_read, is_dismissed, read_ts, dismissed_ts, created_ts
            FROM user_notification_status
            WHERE user_id = $1 AND notification_id = ANY($2)
            "#,
            user_id,
            notification_ids,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to get notification statuses", e))?;

        Ok(statuses.into_iter().map(|s| (s.notification_id, s)).collect())
    }

    /// See [`mark_as_read`].
    pub async fn mark_as_read(&self, user_id: &str, notification_id: i64) -> Result<bool, ApiError> {
        let exists = sqlx::query_scalar!(
            r#"SELECT COUNT(*) AS "count!" FROM server_notifications WHERE id = $1 AND is_enabled = TRUE"#,
            notification_id,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to check notification", e))?;

        if exists == 0 {
            return Err(ApiError::not_found("Notification not found"));
        }

        let now = current_timestamp_millis();
        let result = sqlx::query!(
            r#"
            INSERT INTO user_notification_status (user_id, notification_id, is_read, read_ts)
            VALUES ($1, $2, TRUE, $3)
            ON CONFLICT (user_id, notification_id)
            DO UPDATE SET is_read = TRUE, read_ts = $3
            "#,
            user_id,
            notification_id,
            now,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to mark notification as read", e))?;

        Ok(result.rows_affected() > 0)
    }

    /// See [`mark_as_dismissed`].
    pub async fn mark_as_dismissed(&self, user_id: &str, notification_id: i64) -> Result<bool, ApiError> {
        let exists = sqlx::query_scalar!(
            r#"SELECT COUNT(*) AS "count!" FROM server_notifications WHERE id = $1 AND is_enabled = TRUE"#,
            notification_id,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to check notification", e))?;

        if exists == 0 {
            return Err(ApiError::not_found("Notification not found"));
        }

        let now = current_timestamp_millis();
        let result = sqlx::query!(
            r#"
            INSERT INTO user_notification_status (user_id, notification_id, is_dismissed, dismissed_ts)
            VALUES ($1, $2, TRUE, $3)
            ON CONFLICT (user_id, notification_id)
            DO UPDATE SET is_dismissed = TRUE, dismissed_ts = $3
            "#,
            user_id,
            notification_id,
            now,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to dismiss notification", e))?;

        Ok(result.rows_affected() > 0)
    }

    /// See [`mark_all_as_read`].
    pub async fn mark_all_as_read(&self, user_id: &str) -> Result<i64, ApiError> {
        let now = current_timestamp_millis();
        let notifications = self.get_user_notifications(user_id).await?;

        let mut count = 0i64;
        for n in notifications {
            let result = sqlx::query!(
                r#"
                INSERT INTO user_notification_status (user_id, notification_id, is_read, read_ts)
                VALUES ($1, $2, TRUE, $3)
                ON CONFLICT (user_id, notification_id)
                DO UPDATE SET is_read = TRUE, read_ts = $3
                "#,
                user_id,
                n.notification.id,
                now,
            )
            .execute(&self.pool)
            .await;

            if let Ok(r) = result {
                count += r.rows_affected() as i64;
            }
        }

        Ok(count)
    }

    /// See [`create_template`].
    pub async fn create_template(&self, request: CreateTemplateRequest) -> Result<NotificationTemplate, ApiError> {
        let variables = serde_json::to_value(request.variables.unwrap_or_default()).unwrap_or(serde_json::json!([]));

        let template = sqlx::query_as!(
            NotificationTemplate,
            r#"
            INSERT INTO notification_templates (
                name, title_template, content_template, notification_type, variables
            )
            VALUES ($1, $2, $3, $4, $5)
            RETURNING id, name, title_template, content_template, notification_type, variables, is_enabled, created_ts, updated_ts
            "#,
            &request.name,
            &request.title_template,
            &request.content_template,
            request.notification_type.unwrap_or_else(|| "info".to_string()),
            &variables,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to create template", e))?;

        Ok(template)
    }

    /// See [`get_template`].
    pub async fn get_template(&self, name: &str) -> Result<Option<NotificationTemplate>, ApiError> {
        let template = sqlx::query_as!(
            NotificationTemplate,
            r#"SELECT id, name, title_template, content_template, notification_type, variables, is_enabled, created_ts, updated_ts FROM notification_templates WHERE name = $1 AND is_enabled = TRUE"#,
            name,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to get template", e))?;

        Ok(template)
    }

    /// See [`list_templates`].
    pub async fn list_templates(&self) -> Result<Vec<NotificationTemplate>, ApiError> {
        let templates = sqlx::query_as!(
            NotificationTemplate,
            r#"SELECT id, name, title_template, content_template, notification_type, variables, is_enabled, created_ts, updated_ts FROM notification_templates WHERE is_enabled = TRUE ORDER BY name"#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to list templates", e))?;

        Ok(templates)
    }

    /// See [`delete_template`].
    pub async fn delete_template(&self, name: &str) -> Result<bool, ApiError> {
        let result = sqlx::query!(
            r#"UPDATE notification_templates SET is_enabled = FALSE WHERE name = $1 AND is_enabled = TRUE"#,
            name,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to delete template", e))?;

        Ok(result.rows_affected() > 0)
    }

    /// See [`log_delivery`].
    pub async fn log_delivery(
        &self,
        notification_id: i64,
        user_id: Option<&str>,
        delivery_method: &str,
        status: &str,
        error_message: Option<&str>,
    ) -> Result<(), ApiError> {
        sqlx::query!(
            r#"
            INSERT INTO notification_delivery_log (
                notification_id, user_id, delivery_method, status, error_message
            )
            VALUES ($1, $2, $3, $4, $5)
            "#,
            notification_id,
            user_id,
            delivery_method,
            status,
            error_message,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to log delivery", e))?;

        Ok(())
    }

    /// See [`schedule_notification`].
    pub async fn schedule_notification(
        &self,
        notification_id: i64,
        scheduled_for: i64,
    ) -> Result<ScheduledNotification, ApiError> {
        let scheduled = sqlx::query_as!(
            ScheduledNotification,
            r#"
            INSERT INTO scheduled_notifications (notification_id, scheduled_for)
            VALUES ($1, $2)
            RETURNING id, notification_id, scheduled_for, is_sent, sent_ts, created_ts
            "#,
            notification_id,
            scheduled_for,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to schedule notification", e))?;

        Ok(scheduled)
    }

    /// See [`get_pending_scheduled_notifications`].
    pub async fn get_pending_scheduled_notifications(&self) -> Result<Vec<ScheduledNotification>, ApiError> {
        let now = current_timestamp_millis();

        let scheduled = sqlx::query_as!(
            ScheduledNotification,
            r#"
            SELECT id, notification_id, scheduled_for, is_sent, sent_ts, created_ts
            FROM scheduled_notifications
            WHERE is_sent = FALSE AND scheduled_for <= $1
            "#,
            now,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to get pending scheduled notifications", e))?;

        Ok(scheduled)
    }

    /// See [`mark_scheduled_sent`].
    pub async fn mark_scheduled_sent(&self, scheduled_id: i64) -> Result<bool, ApiError> {
        let now = current_timestamp_millis();
        let result = sqlx::query!(
            r#"UPDATE scheduled_notifications SET is_sent = TRUE, sent_ts = $1 WHERE id = $2 AND is_sent = FALSE"#,
            now,
            scheduled_id,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to mark scheduled as sent", e))?;

        Ok(result.rows_affected() > 0)
    }

    /// See [`get_user_notification_setting`].
    pub async fn get_user_notification_setting(&self, user_id: &str) -> Result<Option<bool>, ApiError> {
        let enabled = sqlx::query_scalar!(
            r#"SELECT is_enabled AS "is_enabled?" FROM user_notification_settings WHERE user_id = $1"#,
            user_id,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to get notification setting", e))?;

        Ok(enabled.map(|value| value.unwrap_or(true)))
    }

    /// See [`upsert_user_notification_setting`].
    pub async fn upsert_user_notification_setting(&self, user_id: &str, enabled: bool) -> Result<(), ApiError> {
        sqlx::query!(
            "INSERT INTO user_notification_settings (user_id, is_enabled) VALUES ($1, $2) ON CONFLICT (user_id) DO UPDATE SET is_enabled = $2",
            user_id,
            enabled,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to upsert notification setting", e))?;

        Ok(())
    }

    /// See [`get_user_pushers`].
    pub async fn get_user_pushers(&self, user_id: &str) -> Result<Vec<serde_json::Value>, ApiError> {
        let rows = sqlx::query!(
            "SELECT pushkey, kind, app_id, app_display_name, device_display_name, profile_tag, lang, data FROM pushers WHERE user_id = $1",
            user_id,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to get pushers", e))?;

        let pusher_list: Vec<serde_json::Value> = rows
            .iter()
            .map(|row| {
                serde_json::json!({
                    "pushkey": row.pushkey,
                    "kind": row.kind,
                    "app_id": row.app_id,
                    "app_display_name": row.app_display_name,
                    "device_display_name": row.device_display_name,
                    "profile_tag": row.profile_tag,
                    "lang": row.lang,
                    "data": row.data.clone().unwrap_or(serde_json::json!({}))
                })
            })
            .collect();

        Ok(pusher_list)
    }

    /// See [`delete_user_pusher`].
    pub async fn delete_user_pusher(&self, user_id: &str, pushkey: &str) -> Result<bool, ApiError> {
        let result = sqlx::query!("DELETE FROM pushers WHERE user_id = $1 AND pushkey = $2", user_id, pushkey)
            .execute(&self.pool)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to delete pusher", e))?;

        Ok(result.rows_affected() > 0)
    }

    /// See [`get_server_notices_count`].
    pub async fn get_server_notices_count(&self) -> Result<i64, ApiError> {
        let count = sqlx::query_scalar!(r#"SELECT COUNT(*)::BIGINT AS "count!" FROM server_notices"#)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to count server notices", e))?;

        Ok(count)
    }

    /// See [`get_server_notices_paginated`].
    pub async fn get_server_notices_paginated(
        &self,
        cursor: Option<(i64, i64)>,
        limit: i64,
    ) -> Result<(Vec<serde_json::Value>, i64, Option<String>), ApiError> {
        let total = self.get_server_notices_count().await?;

        let rows = sqlx::query!(
            "SELECT id, user_id, event_id, content, sent_ts
             FROM server_notices
             WHERE ($1::BIGINT IS NULL AND $2::BIGINT IS NULL)
                OR sent_ts < $1
                OR (sent_ts = $1 AND id < $2)
             ORDER BY sent_ts DESC, id DESC
             LIMIT $3",
            cursor.map(|(sent_ts, _)| sent_ts),
            cursor.map(|(_, id)| id),
            limit,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to get server notices", e))?;

        let notice_list: Vec<serde_json::Value> = rows
            .iter()
            .map(|row| {
                serde_json::json!({
                    "id": row.id,
                    "user_id": row.user_id,
                    "event_id": row.event_id,
                    "content": row.content,
                    "sent_ts": row.sent_ts
                })
            })
            .collect();

        let next_batch = if rows.len() as i64 == limit {
            rows.last().map(|row| format!("{}|{}", row.sent_ts, row.id))
        } else {
            None
        };

        Ok((notice_list, total, next_batch))
    }

    /// See [`get_server_notice_by_id`].
    pub async fn get_server_notice_by_id(&self, notice_id: i64) -> Result<Option<serde_json::Value>, ApiError> {
        let row = sqlx::query!(
            "SELECT id, user_id, event_id, content, sent_ts FROM server_notices WHERE id = $1",
            notice_id,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to get server notice", e))?;

        match row {
            Some(row) => Ok(Some(serde_json::json!({
                "id": row.id,
                "user_id": row.user_id,
                "event_id": row.event_id,
                "content": row.content,
                "sent_ts": row.sent_ts
            }))),
            None => Ok(None),
        }
    }

    /// See [`get_server_notice_with_room`].
    pub async fn get_server_notice_with_room(
        &self,
        notice_id: i64,
    ) -> Result<Option<(Option<String>, Option<String>)>, ApiError> {
        let row = sqlx::query!(
            r#"
            SELECT sn.event_id AS "event_id?", e.room_id AS "room_id?"
            FROM server_notices sn
            LEFT JOIN events e ON e.event_id = sn.event_id
            WHERE sn.id = $1
            "#,
            notice_id,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to get server notice info", e))?;

        match row {
            Some(row) => Ok(Some((row.event_id, row.room_id))),
            None => Ok(None),
        }
    }

    /// See [`delete_server_notice_by_id`].
    pub async fn delete_server_notice_by_id(&self, notice_id: i64) -> Result<bool, ApiError> {
        let result = sqlx::query!("DELETE FROM server_notices WHERE id = $1", notice_id)
            .execute(&self.pool)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to delete server notice", e))?;

        Ok(result.rows_affected() > 0)
    }

    /// See [`delete_room_cascade`].
    ///
    /// Removes a room and the rows that reference it.
    ///
    /// ## Why this propagates errors and uses a transaction
    ///
    /// This previously chained four `.execute(&self.pool).await.ok()` calls —
    /// discarding every child-row deletion failure — and only checked the final
    /// `DELETE FROM rooms`. A failure in any of the first four (constraint,
    /// permission, timeout) was therefore invisible **and** left orphaned
    /// `room_memberships` / `room_summaries` / `room_summary_members` / `events`
    /// rows behind while the function still returned `Ok(())`.
    ///
    /// `CLAUDE.md` states the rule directly: a DB error must not be silently
    /// converted into a success-ish default. The statements also ran
    /// independently, so a mid-way failure left the database half-cascaded.
    ///
    /// Now every delete propagates its error, and all five run in one
    /// transaction so the cascade is all-or-nothing.
    pub async fn delete_room_cascade(&self, room_id: &str) -> Result<(), ApiError> {
        let mut tx =
            self.pool.begin().await.map_err(|e| ApiError::internal_with_cause("Failed to begin room cascade", e))?;

        // Written as explicit static statements (not `format!`-built SQL) so the
        // SQL is greppable and any schema change shows up in review. A
        // `format!("DELETE FROM {table} ...")` would hide the target from both
        // readers and the SQLx dynamic-query ratchet.
        sqlx::query!("DELETE FROM room_memberships WHERE room_id = $1", room_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to delete room memberships", e))?;
        sqlx::query!("DELETE FROM room_summaries WHERE room_id = $1", room_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to delete room summary", e))?;
        sqlx::query!("DELETE FROM room_summary_members WHERE room_id = $1", room_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to delete room summary members", e))?;
        sqlx::query!("DELETE FROM events WHERE room_id = $1", room_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to delete room events", e))?;

        sqlx::query!("DELETE FROM rooms WHERE room_id = $1", room_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to delete room", e))?;

        tx.commit().await.map_err(|e| ApiError::internal_with_cause("Failed to commit room cascade", e))?;

        Ok(())
    }

    /// See [`delete_event_by_id`].
    pub async fn delete_event_by_id(&self, event_id: &str) -> Result<(), ApiError> {
        sqlx::query!("DELETE FROM events WHERE event_id = $1", event_id)
            .execute(&self.pool)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to delete event", e))?;

        Ok(())
    }

    /// See [`send_server_notice`].
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
            self.pool.begin().await.map_err(|e| ApiError::internal_with_cause("Failed to begin transaction", e))?;

        let room_result = sqlx::query!(
            r#"
            INSERT INTO rooms (
                room_id, name, topic, creator, is_public, join_rules,
                room_version, history_visibility, created_ts, last_activity_ts
            )
            VALUES ($1, $2, $3, $4, false, 'invite', '6', 'joined', $5, $5)
            ON CONFLICT (room_id) DO NOTHING
            "#,
            room_id,
            "Server Notice",
            "System notifications",
            server_user,
            now,
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to create server notice room", e))?;

        if room_result.rows_affected() == 0 {
            return Err(ApiError::internal("Failed to create server notice room".to_string()));
        }

        let create_result = sqlx::query!(
            r#"
            INSERT INTO events (event_id, room_id, user_id, event_type, content, origin_server_ts, sender, state_key)
            VALUES ($1, $2, $3, 'm.room.create', $4, $5, $6, '')
            ON CONFLICT (event_id) DO NOTHING
            "#,
            create_event_id,
            room_id,
            server_user,
            serde_json::json!({"creator": server_user}),
            now,
            server_user,
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to create server notice create event", e))?;

        if create_result.rows_affected() == 0 {
            return Err(ApiError::internal("Failed to create server notice create event".to_string()));
        }

        let membership_result = sqlx::query!(
            r#"
            INSERT INTO events (event_id, room_id, user_id, event_type, content, origin_server_ts, sender, state_key)
            VALUES ($1, $2, $3, 'm.room.member', $4, $5, $6, $7)
            ON CONFLICT (event_id) DO NOTHING
            "#,
            membership_event_id,
            room_id,
            target_user_id,
            serde_json::json!({ "membership": "join" }),
            now,
            server_user,
            target_user_id,
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to create server notice membership event", e))?;

        if membership_result.rows_affected() == 0 {
            return Err(ApiError::internal("Failed to create server notice membership event".to_string()));
        }

        let member_result = sqlx::query!(
            r#"
            INSERT INTO room_memberships (
                room_id, user_id, sender, membership, event_id, event_type,
                display_name, avatar_url, updated_ts, joined_ts
            )
            VALUES ($1, $2, $3, 'join', $4, 'm.room.member', $5, $6, $7, $7)
            ON CONFLICT (room_id, user_id) DO NOTHING
            "#,
            room_id,
            target_user_id,
            server_user,
            membership_event_id,
            target_displayname.as_deref(),
            target_avatar_url.as_deref(),
            now,
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to persist server notice member", e))?;

        if member_result.rows_affected() == 0 {
            return Err(ApiError::internal("Failed to persist server notice member".to_string()));
        }

        let message_result = sqlx::query!(
            r#"
            INSERT INTO events (event_id, room_id, user_id, event_type, content, origin_server_ts, sender)
            VALUES ($1, $2, $3, 'm.room.message', $4, $5, $6)
            "#,
            message_event_id,
            room_id,
            target_user_id,
            serde_json::json!({
                "msgtype": msgtype,
                "body": body
            }),
            now,
            server_user,
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to persist m.room.message event for server notice", e))?;

        if message_result.rows_affected() == 0 {
            return Err(ApiError::internal("Failed to persist m.room.message event for server notice".to_string()));
        }

        let notice_content = serde_json::json!({
            "msgtype": msgtype,
            "body": body
        });
        let notice_id = sqlx::query_scalar!(
            r#"
            INSERT INTO server_notices (user_id, event_id, content, sent_ts)
            VALUES ($1, $2, $3, $4)
            RETURNING id
            "#,
            target_user_id,
            message_event_id,
            notice_content.to_string(),
            now,
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to create server notice record", e))?;

        let summary_result = sqlx::query!(
            r#"
            INSERT INTO room_summaries (
                room_id, name, topic, join_rules, history_visibility, guest_access,
                is_direct, is_space, is_encrypted, member_count, joined_member_count,
                invited_member_count, hero_users, last_event_id, last_event_ts,
                last_message_ts, unread_notifications, unread_highlight, updated_ts, created_ts
            )
            VALUES (
                $1, $2, $3, 'invite', 'joined', 'forbidden',
                false, false, false, 1, 1,
                0, '[]'::jsonb, $4, $5,
                $5, 0, 0, $5, $5
            )
            ON CONFLICT (room_id) DO NOTHING
            "#,
            room_id,
            "Server Notice",
            "System notifications",
            message_event_id,
            now,
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to persist server notice room summary", e))?;

        if summary_result.rows_affected() == 0 {
            return Err(ApiError::internal("Failed to persist server notice room summary".to_string()));
        }

        let summary_member_result = sqlx::query!(
            r#"
            INSERT INTO room_summary_members (
                room_id, user_id, display_name, avatar_url, membership, is_hero,
                last_active_ts, updated_ts, created_ts
            )
            VALUES ($1, $2, $3, $4, 'join', false, $5, $5, $5)
            ON CONFLICT (room_id, user_id) DO NOTHING
            "#,
            room_id,
            target_user_id,
            target_displayname.as_deref(),
            target_avatar_url.as_deref(),
            now,
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to persist server notice room summary member", e))?;

        if summary_member_result.rows_affected() == 0 {
            return Err(ApiError::internal("Failed to persist server notice room summary member".to_string()));
        }

        tx.commit()
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to commit server notice transaction", e))?;

        Ok(notice_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_create_notification_request_serialization() {
        let request = CreateNotificationRequest {
            title: "Test Title".to_string(),
            content: "Test Content".to_string(),
            notification_type: Some("warning".to_string()),
            priority: Some(5),
            target_audience: Some("all".to_string()),
            target_user_ids: Some(vec!["@user1:example.com".to_string(), "@user2:example.com".to_string()]),
            starts_at: Some(1000),
            expires_at: Some(2000),
            is_dismissable: Some(true),
            action_url: Some("https://example.com".to_string()),
            action_text: Some("Click here".to_string()),
            created_by: Some("@admin:example.com".to_string()),
        };

        let target_user_ids = serde_json::to_value(request.target_user_ids.unwrap_or_default()).unwrap();
        assert_eq!(target_user_ids, json!(["@user1:example.com", "@user2:example.com"]));
    }

    #[test]
    fn test_create_notification_request_defaults() {
        let request = CreateNotificationRequest {
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
            created_by: Some("@admin:example.com".to_string()),
        };

        let notification_type = request.notification_type.unwrap_or_else(|| "info".to_string());
        let priority = request.priority.unwrap_or(0);
        let target_audience = request.target_audience.unwrap_or_else(|| "all".to_string());
        let target_user_ids = request.target_user_ids.unwrap_or_default();
        let is_dismissable = request.is_dismissable.unwrap_or(true);

        assert_eq!(notification_type, "info");
        assert_eq!(priority, 0);
        assert_eq!(target_audience, "all");
        assert!(target_user_ids.is_empty());
        assert!(is_dismissable);
    }

    #[test]
    fn test_server_notification_model_fields() {
        let notification = ServerNotification {
            id: 1,
            title: "Test".to_string(),
            content: "Content".to_string(),
            notification_type: "info".to_string(),
            priority: 0,
            target_audience: "all".to_string(),
            target_user_ids: json!([]),
            starts_at: None,
            expires_at: None,
            is_enabled: true,
            is_dismissable: true,
            action_url: None,
            action_text: None,
            created_by: Some("@admin:example.com".to_string()),
            created_ts: 1000,
            updated_ts: 1000,
        };

        assert_eq!(notification.id, 1);
        assert_eq!(notification.notification_type, "info");
        assert!(notification.is_enabled);
    }

    #[test]
    fn test_notification_priority_ordering() {
        // Test that higher priority notifications should be listed first
        let mut notifications = [
            ServerNotification {
                id: 1,
                title: "Low Priority".to_string(),
                content: "Content".to_string(),
                notification_type: "info".to_string(),
                priority: 1,
                target_audience: "all".to_string(),
                target_user_ids: json!([]),
                starts_at: None,
                expires_at: None,
                is_enabled: true,
                is_dismissable: true,
                action_url: None,
                action_text: None,
                created_by: Some("@admin:example.com".to_string()),
                created_ts: 1000,
                updated_ts: 1000,
            },
            ServerNotification {
                id: 2,
                title: "High Priority".to_string(),
                content: "Content".to_string(),
                notification_type: "alert".to_string(),
                priority: 10,
                target_audience: "all".to_string(),
                target_user_ids: json!([]),
                starts_at: None,
                expires_at: None,
                is_enabled: true,
                is_dismissable: true,
                action_url: None,
                action_text: None,
                created_by: Some("@admin:example.com".to_string()),
                created_ts: 2000,
                updated_ts: 2000,
            },
        ];

        // Sort by priority DESC, created_ts DESC (as in list_active_notifications query)
        notifications.sort_by(|a, b| b.priority.cmp(&a.priority).then_with(|| b.created_ts.cmp(&a.created_ts)));

        assert_eq!(notifications[0].id, 2);
        assert_eq!(notifications[0].priority, 10);
        assert_eq!(notifications[1].id, 1);
        assert_eq!(notifications[1].priority, 1);
    }

    #[test]
    fn test_target_user_ids_json_conversion() {
        // Test empty vector
        let empty_ids: Vec<String> = vec![];
        let json_val = serde_json::to_value(empty_ids).unwrap();
        assert_eq!(json_val, json!([]));

        // Test single user
        let single_id = vec!["@user:example.com".to_string()];
        let json_val = serde_json::to_value(single_id).unwrap();
        assert_eq!(json_val, json!(["@user:example.com"]));

        // Test multiple users
        let multi_ids = vec!["@user1:example.com".to_string(), "@user2:example.com".to_string()];
        let json_val = serde_json::to_value(multi_ids).unwrap();
        assert_eq!(json_val, json!(["@user1:example.com", "@user2:example.com"]));
    }
}

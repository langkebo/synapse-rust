use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use std::sync::Arc;
use synapse_common::current_timestamp_millis;
use synapse_common::error::ApiError;
use tracing::info;

/// `PushDevice` 结构体映射数据库 push_devices 表。
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct PushDevice {
    /// The `id` field.
    pub id: i64,
    /// The `user_id` field.
    pub user_id: String,
    /// The `device_id` field.
    pub device_id: String,
    /// The `push_token` field.
    pub push_token: String,
    /// The `push_type` field.
    pub push_type: String,
    /// The `app_id` field.
    pub app_id: Option<String>,
    /// The `platform` field.
    pub platform: Option<String>,
    /// The `platform_version` field.
    pub platform_version: Option<String>,
    /// The `app_version` field.
    pub app_version: Option<String>,
    /// The `locale` field.
    pub locale: Option<String>,
    /// The `timezone` field.
    pub timezone: Option<String>,
    /// The `is_enabled` field.
    pub is_enabled: bool,
    /// The `created_ts` field.
    pub created_ts: i64,
    /// The `updated_ts` field.
    pub updated_ts: Option<i64>,
    #[sqlx(rename = "last_used_at")]
    /// The `last_used_ts` field.
    pub last_used_ts: Option<i64>,
    /// The `last_error` field.
    pub last_error: Option<String>,
    /// The `error_count` field.
    pub error_count: i32,
    /// The `metadata` field.
    pub metadata: serde_json::Value,
}

/// The `PushNotificationQueue` struct.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct PushNotificationQueue {
    /// The `id` field.
    pub id: i64,
    /// The `user_id` field.
    pub user_id: String,
    /// The `device_id` field.
    pub device_id: String,
    /// The `event_id` field.
    pub event_id: Option<String>,
    /// The `room_id` field.
    pub room_id: Option<String>,
    /// The `notification_type` field.
    pub notification_type: Option<String>,
    /// The `content` field.
    pub content: serde_json::Value,
    /// The `priority` field.
    pub priority: i32,
    /// The `status` field.
    pub status: String,
    /// The `attempts` field.
    pub attempts: i32,
    /// The `max_attempts` field.
    pub max_attempts: i32,
    /// The `next_attempt_at` field.
    pub next_attempt_at: Option<i64>,
    /// The `created_ts` field.
    pub created_ts: i64,
    /// The `sent_at` field.
    pub sent_at: Option<i64>,
    /// The `error_message` field.
    pub error_message: Option<String>,
}

/// The `PushNotificationLog` struct.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct PushNotificationLog {
    /// The `id` field.
    pub id: i64,
    /// The `user_id` field.
    pub user_id: String,
    /// The `device_id` field.
    pub device_id: String,
    /// The `event_id` field.
    pub event_id: Option<String>,
    /// The `room_id` field.
    pub room_id: Option<String>,
    /// The `notification_type` field.
    pub notification_type: Option<String>,
    /// The `push_type` field.
    pub push_type: String,
    /// The `sent_at` field.
    pub sent_at: Option<i64>,
    /// The `is_success` field.
    pub is_success: bool,
    /// The `error_message` field.
    pub error_message: Option<String>,
    /// The `provider_response` field.
    pub provider_response: Option<String>,
    /// The `response_time_ms` field.
    pub response_time_ms: Option<i32>,
    /// The `metadata` field.
    pub metadata: serde_json::Value,
}

/// One row of the global push provider configuration (`push_config`).
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct PushConfigEntry {
    /// The `config_key` field (e.g. `fcm.enabled`).
    pub config_key: String,
    /// The `config_value` field.
    pub config_value: String,
    /// The `updated_ts` field.
    pub updated_ts: Option<i64>,
}

/// The `RegisterDeviceRequest` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterDeviceRequest {
    /// The `user_id` field.
    pub user_id: String,
    /// The `device_id` field.
    pub device_id: String,
    /// The `push_token` field.
    pub push_token: String,
    /// The `push_type` field.
    pub push_type: String,
    /// The `app_id` field.
    pub app_id: Option<String>,
    /// The `platform` field.
    pub platform: Option<String>,
    /// The `platform_version` field.
    pub platform_version: Option<String>,
    /// The `app_version` field.
    pub app_version: Option<String>,
    /// The `locale` field.
    pub locale: Option<String>,
    /// The `timezone` field.
    pub timezone: Option<String>,
    /// The `metadata` field.
    pub metadata: Option<serde_json::Value>,
}

/// The `QueueNotificationRequest` struct.
#[derive(Debug, Clone, Deserialize)]
pub struct QueueNotificationRequest {
    /// The `user_id` field.
    pub user_id: String,
    /// The `device_id` field.
    pub device_id: String,
    /// The `event_id` field.
    pub event_id: Option<String>,
    /// The `room_id` field.
    pub room_id: Option<String>,
    /// The `notification_type` field.
    pub notification_type: Option<String>,
    /// The `content` field.
    pub content: serde_json::Value,
    /// The `priority` field.
    pub priority: i32,
}

/// The `CreateNotificationLogRequest` struct.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct CreateNotificationLogRequest {
    /// The `user_id` field.
    pub user_id: String,
    /// The `device_id` field.
    pub device_id: String,
    /// The `event_id` field.
    pub event_id: Option<String>,
    /// The `room_id` field.
    pub room_id: Option<String>,
    /// The `notification_type` field.
    pub notification_type: Option<String>,
    /// The `push_type` field.
    pub push_type: String,
    /// The `is_success` field.
    pub is_success: bool,
    /// The `error_message` field.
    pub error_message: Option<String>,
    /// The `provider_response` field.
    pub provider_response: Option<String>,
    /// The `response_time_ms` field.
    pub response_time_ms: Option<i32>,
}

impl CreateNotificationLogRequest {
    /// See [`new`].
    pub fn new(
        user_id: impl Into<String>,
        device_id: impl Into<String>,
        push_type: impl Into<String>,
        is_success: bool,
    ) -> Self {
        Self {
            user_id: user_id.into(),
            device_id: device_id.into(),
            push_type: push_type.into(),
            is_success,
            ..Default::default()
        }
    }

    /// See [`event_id`].
    pub fn event_id(mut self, event_id: impl Into<String>) -> Self {
        self.event_id = Some(event_id.into());
        self
    }

    /// See [`room_id`].
    pub fn room_id(mut self, room_id: impl Into<String>) -> Self {
        self.room_id = Some(room_id.into());
        self
    }

    /// See [`notification_type`].
    pub fn notification_type(mut self, notification_type: impl Into<String>) -> Self {
        self.notification_type = Some(notification_type.into());
        self
    }

    /// See [`error_message`].
    pub fn error_message(mut self, error_message: impl Into<String>) -> Self {
        self.error_message = Some(error_message.into());
        self
    }

    /// See [`provider_response`].
    pub fn provider_response(mut self, provider_response: impl Into<String>) -> Self {
        self.provider_response = Some(provider_response.into());
        self
    }

    /// See [`response_time_ms`].
    pub fn response_time_ms(mut self, response_time_ms: i32) -> Self {
        self.response_time_ms = Some(response_time_ms);
        self
    }
}

/// The `PushNotificationStorage` struct.
#[derive(Debug, Clone)]
pub struct PushNotificationStorage {
    pool: Arc<PgPool>,
}

impl PushNotificationStorage {
    /// See [`new`].
    pub fn new(pool: &Arc<PgPool>) -> Self {
        Self { pool: pool.clone() }
    }

    /// See [`register_device`].
    pub async fn register_device(&self, request: RegisterDeviceRequest) -> Result<PushDevice, ApiError> {
        let now = current_timestamp_millis();
        let metadata = request.metadata.unwrap_or(serde_json::json!({}));

        // C18: the struct field is `last_used_ts` with `#[sqlx(rename = "last_used_at")]`,
        // but `query_as!` honours neither `#[sqlx(rename)]` nor `#[sqlx(skip)]` — it
        // builds the struct literal from the *described column names*, so the projection
        // has to carry the alias itself. `RETURNING *` likewise has to be expanded to the
        // struct's exact column set (extra column ⇒ E0560).
        let row = sqlx::query_as!(
            PushDevice,
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
            RETURNING
                id, user_id, device_id, push_token, push_type, app_id, platform,
                platform_version, app_version, locale, timezone, is_enabled,
                created_ts, updated_ts, last_used_at AS "last_used_ts", last_error,
                error_count, metadata
            "#,
            request.user_id.as_str(),
            request.device_id.as_str(),
            request.push_token.as_str(),
            request.push_type.as_str(),
            request.app_id.as_deref(),
            request.platform.as_deref(),
            request.platform_version.as_deref(),
            request.app_version.as_deref(),
            request.locale.as_deref(),
            request.timezone.as_deref(),
            now,
            &metadata,
        )
        .fetch_one(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to register device", e))?;

        info!("Registered push device: {} for user: {}", request.device_id, request.user_id);
        Ok(row)
    }

    /// See [`unregister_device`].
    pub async fn unregister_device(&self, user_id: &str, device_id: &str) -> Result<(), ApiError> {
        sqlx::query!(
            "UPDATE push_device SET is_enabled = false WHERE user_id = $1 AND device_id = $2 AND is_enabled = TRUE",
            user_id,
            device_id,
        )
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to unregister device", e))?;

        info!("Unregistered push device: {} for user: {}", device_id, user_id);
        Ok(())
    }

    /// See [`get_user_devices`].
    pub async fn get_user_devices(&self, user_id: &str) -> Result<Vec<PushDevice>, ApiError> {
        let rows = sqlx::query_as!(
            PushDevice,
            r#"
                SELECT id, user_id, device_id, push_token, push_type, app_id, platform,
                    platform_version, app_version, locale, timezone, is_enabled,
                    created_ts, updated_ts, last_used_at AS "last_used_ts", last_error,
                    error_count, metadata
                FROM push_device WHERE user_id = $1 AND is_enabled = true
                "#,
            user_id,
        )
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to get user devices", e))?;

        Ok(rows)
    }

    /// See [`get_device`].
    pub async fn get_device(&self, user_id: &str, device_id: &str) -> Result<Option<PushDevice>, ApiError> {
        let row = sqlx::query_as!(
            PushDevice,
            r#"
            SELECT id, user_id, device_id, push_token, push_type, app_id, platform,
                platform_version, app_version, locale, timezone, is_enabled,
                created_ts, updated_ts, last_used_at AS "last_used_ts", last_error,
                error_count, metadata
            FROM push_device WHERE user_id = $1 AND device_id = $2 AND is_enabled = true
            "#,
            user_id,
            device_id,
        )
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to get device", e))?;

        Ok(row)
    }

    /// See [`update_device_last_used`].
    pub async fn update_device_last_used(&self, user_id: &str, device_id: &str) -> Result<(), ApiError> {
        let now = current_timestamp_millis();

        sqlx::query!(
            "UPDATE push_device SET last_used_at = $1, updated_ts = $1 WHERE user_id = $2 AND device_id = $3",
            now,
            user_id,
            device_id,
        )
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to update device last used", e))?;

        Ok(())
    }

    /// See [`record_device_error`].
    pub async fn record_device_error(&self, user_id: &str, device_id: &str, error: &str) -> Result<(), ApiError> {
        let now = current_timestamp_millis();
        sqlx::query!(
            r"
            UPDATE push_device
            SET last_error = $1, error_count = error_count + 1, updated_ts = $4
            WHERE user_id = $2 AND device_id = $3
            ",
            error,
            user_id,
            device_id,
            now,
        )
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to record device error", e))?;

        Ok(())
    }

    /// See [`queue_notification`].
    pub async fn queue_notification(
        &self,
        request: QueueNotificationRequest,
    ) -> Result<PushNotificationQueue, ApiError> {
        let now_ms = current_timestamp_millis();

        // `content` is nullable in the catalog (`jsonb DEFAULT '{}'`, no NOT NULL) while
        // the struct field is a plain `serde_json::Value` ⇒ `AS "content!"`.
        let row = sqlx::query_as!(
            PushNotificationQueue,
            r#"
            INSERT INTO push_notification_queue (
                user_id, device_id, event_id, room_id, notification_type, content, priority, status, next_attempt_at, created_ts
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, 'pending', $8, $9)
            RETURNING
                id, user_id, device_id, event_id, room_id, notification_type,
                content AS "content!", priority, status, attempts, max_attempts,
                next_attempt_at, created_ts, sent_at, error_message
            "#,
            request.user_id.as_str(),
            request.device_id.as_str(),
            request.event_id.as_deref(),
            request.room_id.as_deref(),
            request.notification_type.as_deref(),
            &request.content,
            request.priority,
            now_ms,
            now_ms,
        )
        .fetch_one(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to queue notification", e))?;

        Ok(row)
    }

    /// P2: Batch-insert push notifications using a single multi-row INSERT.
    /// Each request becomes one row; all rows share the same timestamp for
    /// consistent ordering semantics.
    pub async fn queue_notifications_batch(
        &self,
        requests: &[QueueNotificationRequest],
    ) -> Result<Vec<PushNotificationQueue>, ApiError> {
        if requests.is_empty() {
            return Ok(Vec::new());
        }

        let now_ms = current_timestamp_millis();

        let mut query_builder: sqlx::QueryBuilder<'_, sqlx::Postgres> = sqlx::QueryBuilder::new(
            r#"
            INSERT INTO push_notification_queue (
                user_id, device_id, event_id, room_id, notification_type, content, priority, status, next_attempt_at, created_ts
            ) "#,
        );

        for (i, _req) in requests.iter().enumerate() {
            if i == 0 {
                query_builder.push(" VALUES ");
            } else {
                query_builder.push(", ");
            }
            let bi = i * 10;
            query_builder
                .push("($")
                .push((bi + 1).to_string())
                .push(", $")
                .push((bi + 2).to_string())
                .push(", $")
                .push((bi + 3).to_string())
                .push(", $")
                .push((bi + 4).to_string())
                .push(", $")
                .push((bi + 5).to_string())
                .push(", $")
                .push((bi + 6).to_string())
                .push(", $")
                .push((bi + 7).to_string())
                .push(", 'pending', $")
                .push((bi + 8).to_string())
                .push(", $")
                .push((bi + 9).to_string())
                .push(")");
        }

        query_builder.push(" RETURNING *");

        let mut q = query_builder.build_query_as::<PushNotificationQueue>();
        for req in requests {
            q = q.bind(&req.user_id)
                .bind(&req.device_id)
                .bind(&req.event_id)
                .bind(&req.room_id)
                .bind(&req.notification_type)
                .bind(&req.content)
                .bind(req.priority)
                .bind(now_ms)  // next_attempt_at
                .bind(now_ms); // created_ts
        }

        let rows = q
            .fetch_all(&*self.pool)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to batch-queue notifications", e))?;

        Ok(rows)
    }

    /// See [`get_pending_notifications`].
    pub async fn get_pending_notifications(&self, limit: i32) -> Result<Vec<PushNotificationQueue>, ApiError> {
        let now_ms = current_timestamp_millis();

        let rows = sqlx::query_as!(
            PushNotificationQueue,
            r#"
            SELECT id, user_id, device_id, event_id, room_id, notification_type,
                content AS "content!", priority, status, attempts, max_attempts,
                next_attempt_at, created_ts, sent_at, error_message
            FROM push_notification_queue
            WHERE status = 'pending' AND next_attempt_at <= $1
            ORDER BY priority DESC, created_ts ASC
            LIMIT $2
            FOR UPDATE SKIP LOCKED
            "#,
            now_ms,
            // PG types `LIMIT $2` as BIGINT while the signature takes `i32`; the old
            // `.bind()` sent INT4 and relied on PG's implicit widening cast. The `as i64`
            // cast also acts as the macro's type override, so no ty_match check runs.
            limit as i64,
        )
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to get pending notifications", e))?;

        Ok(rows)
    }

    /// See [`mark_notification_sent`].
    pub async fn mark_notification_sent(&self, id: i64) -> Result<(), ApiError> {
        let now_ms = current_timestamp_millis();

        sqlx::query!("UPDATE push_notification_queue SET status = 'sent', sent_at = $1 WHERE id = $2", now_ms, id,)
            .execute(&*self.pool)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to mark notification sent", e))?;

        Ok(())
    }

    /// See [`mark_notification_failed`].
    pub async fn mark_notification_failed(&self, id: i64, error: &str, retry: bool) -> Result<(), ApiError> {
        let now_ms = current_timestamp_millis();

        if retry {
            let retry_at = now_ms + 60_000; // 60 seconds from now
            sqlx::query!(
                r"
                UPDATE push_notification_queue
                SET status = 'pending', attempts = attempts + 1, error_message = $1, next_attempt_at = $2
                WHERE id = $3 AND attempts < max_attempts
                ",
                error,
                retry_at,
                id,
            )
            .execute(&*self.pool)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to mark notification for retry", e))?;
        } else {
            sqlx::query!(
                "UPDATE push_notification_queue SET status = 'failed', error_message = $1 WHERE id = $2",
                error,
                id,
            )
            .execute(&*self.pool)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to mark notification failed", e))?;
        }

        Ok(())
    }

    /// See [`create_notification_log`].
    pub async fn create_notification_log(
        &self,
        request: &CreateNotificationLogRequest,
    ) -> Result<PushNotificationLog, ApiError> {
        // `push_notification_log.created_ts` is `BIGINT NOT NULL` with no default, so
        // omitting it made every delivery log write fail with 23502 — which then
        // flipped already-delivered pushes into the retry path.
        // `push_notification_log` carries eight more columns than the struct maps
        // (`pushkey`, `status`, `retry_count`, `last_attempt_at`, `created_ts`, …), so
        // `RETURNING *` has to be replaced by the struct's exact column set; `push_type`
        // and `is_success` are nullable in the catalog while the fields are not ⇒ `!`.
        //
        // D-33: this method is only called *after* the push attempt has been made, so
        // "sent" and "logged" are the same instant; `sent_at` used to stay NULL for every
        // row, which made `cleanup_old_logs`'s `WHERE sent_at < $1` a no-op and left the
        // append-only table unbounded. It is now written alongside `created_ts`.
        let now = current_timestamp_millis();
        let row = sqlx::query_as!(
            PushNotificationLog,
            r#"
            INSERT INTO push_notification_log (
                user_id, device_id, event_id, room_id, notification_type, push_type,
                is_success, error_message, provider_response, response_time_ms, created_ts, sent_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $11)
            RETURNING
                id, user_id, device_id, event_id, room_id, notification_type,
                push_type AS "push_type!", sent_at, is_success AS "is_success!",
                error_message, provider_response, response_time_ms, metadata
            "#,
            request.user_id.as_str(),
            request.device_id.as_str(),
            request.event_id.as_deref(),
            request.room_id.as_deref(),
            request.notification_type.as_deref(),
            request.push_type.as_str(),
            request.is_success,
            request.error_message.as_deref(),
            request.provider_response.as_deref(),
            request.response_time_ms,
            now,
        )
        .fetch_one(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to create notification log", e))?;

        Ok(row)
    }

    /// See [`get_config`].
    pub async fn get_config(&self, config_key: &str) -> Result<Option<String>, ApiError> {
        let row = sqlx::query_scalar!("SELECT config_value FROM push_config WHERE config_key = $1", config_key)
            .fetch_optional(&*self.pool)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to get config", e))?;

        Ok(row)
    }

    /// See [`get_config_as_bool`].
    pub async fn get_config_as_bool(&self, config_key: &str, default: bool) -> Result<bool, ApiError> {
        let value = self.get_config(config_key).await?;

        Ok(match value {
            Some(v) => v.to_lowercase() == "true",
            None => default,
        })
    }

    /// See [`get_config_as_int`].
    pub async fn get_config_as_int(&self, config_key: &str, default: i32) -> Result<i32, ApiError> {
        let value = self.get_config(config_key).await?;

        Ok(match value {
            Some(v) => v.parse().unwrap_or(default),
            None => default,
        })
    }

    /// See [`list_config`].
    pub async fn list_config(&self) -> Result<Vec<PushConfigEntry>, ApiError> {
        sqlx::query_as!(
            PushConfigEntry,
            "SELECT config_key, config_value, updated_ts FROM push_config ORDER BY config_key",
        )
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to list push config", e))
    }

    /// See [`set_config`].
    pub async fn set_config(&self, config_key: &str, config_value: &str) -> Result<PushConfigEntry, ApiError> {
        let now = current_timestamp_millis();

        sqlx::query_as!(
            PushConfigEntry,
            r"
            INSERT INTO push_config (config_key, config_value, created_ts, updated_ts)
            VALUES ($1, $2, $3, $3)
            ON CONFLICT (config_key) DO UPDATE
                SET config_value = EXCLUDED.config_value, updated_ts = EXCLUDED.updated_ts
            RETURNING config_key, config_value, updated_ts
            ",
            config_key,
            config_value,
            now,
        )
        .fetch_one(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to set push config", e))
    }

    /// See [`delete_config`].
    pub async fn delete_config(&self, config_key: &str) -> Result<bool, ApiError> {
        let result = sqlx::query!("DELETE FROM push_config WHERE config_key = $1", config_key)
            .execute(&*self.pool)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to delete push config", e))?;

        Ok(result.rows_affected() > 0)
    }

    /// See [`cleanup_old_logs`].
    pub async fn cleanup_old_logs(&self, days: i32) -> Result<u64, ApiError> {
        let cutoff_ms = current_timestamp_millis() - (days as i64 * 86_400_000);

        // D-33: rows written before `sent_at` was populated carry NULL there, and
        // `NULL < $1` is NULL — so a bare `sent_at < $1` silently matched nothing and the
        // retention endpoint always reported `{"cleaned":0}`. `created_ts` is NOT NULL and
        // is the same instant for every row this crate writes, so it is the fallback.
        let result =
            sqlx::query!("DELETE FROM push_notification_log WHERE COALESCE(sent_at, created_ts) < $1", cutoff_ms)
                .execute(&*self.pool)
                .await
                .map_err(|e| ApiError::internal_with_cause("Failed to cleanup logs", e))?;

        info!("Cleaned up {} old notification logs", result.rows_affected());
        Ok(result.rows_affected())
    }

    /// Get room notifications for a user.
    pub async fn get_room_notifications(
        &self,
        user_id: &str,
        room_id: &str,
        limit: i64,
    ) -> Result<Vec<RoomNotification>, sqlx::Error> {
        sqlx::query_as!(
            RoomNotification,
            r"
            SELECT event_id, room_id, ts, notification_type, is_read
            FROM notifications
            WHERE user_id = $1 AND room_id = $2
            ORDER BY ts DESC
            LIMIT $3
            ",
            user_id,
            room_id,
            limit,
        )
        .fetch_all(&*self.pool)
        .await
    }
}

// ---------------------------------------------------------------------------
// Room notification model
// ---------------------------------------------------------------------------

/// The `RoomNotification` struct.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RoomNotification {
    /// The `event_id` field.
    pub event_id: Option<String>,
    /// The `room_id` field.
    pub room_id: Option<String>,
    /// The `ts` field.
    pub ts: Option<i64>,
    /// The `notification_type` field.
    pub notification_type: Option<String>,
    /// The `is_read` field.
    pub is_read: Option<bool>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn now_ms() -> i64 {
        current_timestamp_millis()
    }

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
        let request = CreateNotificationLogRequest::new("@alice:example.com", "DEVICE123", "apns", true)
            .event_id("$event456")
            .room_id("!room456:example.com")
            .notification_type("m.room.message")
            .response_time_ms(150);

        assert_eq!(request.user_id, "@alice:example.com");
        assert_eq!(request.device_id, "DEVICE123");
        assert_eq!(request.push_type, "apns");
        assert!(request.is_success);
        assert_eq!(request.event_id, Some("$event456".to_string()));
        assert_eq!(request.room_id, Some("!room456:example.com".to_string()));
        assert_eq!(request.notification_type, Some("m.room.message".to_string()));
        assert_eq!(request.response_time_ms, Some(150));
        assert!(request.error_message.is_none());
        assert!(request.provider_response.is_none());
    }

    #[test]
    fn test_create_notification_log_request_failure() {
        let request = CreateNotificationLogRequest::new("@bob:example.com", "DEVICE456", "fcm", false)
            .error_message("Invalid token")
            .provider_response("{\"error\": \"InvalidRegistration\"}");

        assert_eq!(request.user_id, "@bob:example.com");
        assert_eq!(request.push_type, "fcm");
        assert!(!request.is_success);
        assert_eq!(request.error_message, Some("Invalid token".to_string()));
        assert_eq!(request.provider_response, Some("{\"error\": \"InvalidRegistration\"}".to_string()));
    }

    #[test]
    fn test_create_notification_log_request_default() {
        let request = CreateNotificationLogRequest::default();

        assert!(request.user_id.is_empty());
        assert!(request.device_id.is_empty());
        assert!(request.push_type.is_empty());
        assert!(!request.is_success);
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
            is_enabled: true,
            created_ts: 1700000000000,
            updated_ts: Some(1700000001000),
            last_used_ts: None,
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
        assert_eq!(deserialized.is_enabled, device.is_enabled);
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
                next_attempt_at: Some(now_ms()),
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
            next_attempt_at: Some(now_ms()),
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
            sent_at: Some(now_ms()),
            is_success: true,
            error_message: None,
            provider_response: Some("{\"status\": \"ok\"}".to_string()),
            response_time_ms: Some(100),
            metadata: json!({}),
        };

        assert!(log.is_success);
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
            sent_at: Some(now_ms()),
            is_success: false,
            error_message: Some("InvalidRegistration".to_string()),
            provider_response: Some("{\"error\": \"InvalidRegistration\"}".to_string()),
            response_time_ms: Some(50),
            metadata: json!({}),
        };

        assert!(!log.is_success);
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
            is_enabled: true,
            created_ts: 1700000000000,
            updated_ts: None,
            last_used_ts: None,
            last_error: Some("Unregistered".to_string()),
            error_count: 3,
            metadata: json!({}),
        };

        assert!(device.last_error.is_some());
        assert!(device.error_count > 0);
        assert!(device.is_enabled);
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
            content,
            priority: 10,
        };

        assert!(request.content.get("counts").is_some());
        assert_eq!(request.content["counts"]["unread"], 5);
        assert_eq!(request.content["room_name"], "Test Room");
    }
}

#[cfg(test)]
mod db_tests {
    use super::*;

    const DAY_MS: i64 = 86_400_000;

    async fn test_pool() -> Option<(crate::test_isolation::IsolatedTestPool, Arc<PgPool>)> {
        match crate::test_isolation::isolated_test_pool().await {
            Ok(isolated) => {
                let pool = isolated.pool();
                Some((isolated, pool))
            }
            Err(error) => {
                tracing::warn!("Skipping push_notification DB test because test database is unavailable: {error}");
                None
            }
        }
    }

    fn log_request(user_id: &str, device_id: &str) -> CreateNotificationLogRequest {
        CreateNotificationLogRequest {
            user_id: user_id.to_string(),
            device_id: device_id.to_string(),
            event_id: Some("$event:test.local".to_string()),
            room_id: Some("!room:test.local".to_string()),
            notification_type: Some("m.room.message".to_string()),
            push_type: "apns".to_string(),
            is_success: true,
            error_message: None,
            provider_response: Some("{}".to_string()),
            response_time_ms: Some(12),
        }
    }

    /// D-15.6 / D-33: `create_notification_log` must persist the row *and* stamp
    /// `sent_at`, which retention keys on.
    #[tokio::test]
    async fn test_create_notification_log_roundtrip() {
        let Some((_isolated, pool)) = test_pool().await else {
            return;
        };
        let storage = PushNotificationStorage::new(&pool);
        let uuid = uuid::Uuid::new_v4();
        let user_id = format!("@push_{}:test.local", uuid.as_simple());

        let logged = storage
            .create_notification_log(&log_request(&user_id, "DEVICE1"))
            .await
            .expect("create_notification_log must succeed on the migrated schema");

        assert_eq!(logged.user_id, user_id);
        assert!(logged.sent_at.is_some(), "D-33: create_notification_log must stamp sent_at, got {:?}", logged.sent_at);
        let sent_at = logged.sent_at.unwrap_or(0);
        assert!(sent_at > 0, "sent_at must be a real timestamp, got {sent_at}");
    }

    /// D-33 RED/GREEN: a row whose only timestamp is `created_ts` (the shape every row
    /// written before this fix has — `sent_at IS NULL`) must be reclaimed by retention.
    /// With the old `WHERE sent_at < $1` predicate this deleted 0 rows, which is exactly
    /// the endpoint's permanent `{"cleaned":0}`.
    #[tokio::test]
    async fn test_cleanup_old_logs_deletes_expired_null_sent_at_row() {
        let Some((_isolated, pool)) = test_pool().await else {
            return;
        };
        let storage = PushNotificationStorage::new(&pool);
        let uuid = uuid::Uuid::new_v4();
        let old_ts = current_timestamp_millis() - 30 * DAY_MS;

        sqlx::query(
            "INSERT INTO push_notification_log (user_id, device_id, push_type, created_ts, is_success) \
             VALUES ($1, $2, 'apns', $3, true)",
        )
        .bind(format!("@old_{}:test.local", uuid.as_simple()))
        .bind("DEVICE_OLD")
        .bind(old_ts)
        .execute(&*pool)
        .await
        .expect("failed to insert an expired log row with NULL sent_at");

        let deleted = storage.cleanup_old_logs(7).await.expect("cleanup_old_logs must succeed");
        assert_eq!(deleted, 1, "retention must reclaim the expired row instead of reporting 0");
    }

    /// Negative case: retention must not touch rows inside the window. This is what
    /// breaks if the `COALESCE` fallback is replaced by an unconditional delete.
    #[tokio::test]
    async fn test_cleanup_old_logs_keeps_recent_rows() {
        let Some((_isolated, pool)) = test_pool().await else {
            return;
        };
        let storage = PushNotificationStorage::new(&pool);
        let uuid = uuid::Uuid::new_v4();

        storage
            .create_notification_log(&log_request(&format!("@recent_{}:test.local", uuid.as_simple()), "DEVICE_NEW"))
            .await
            .expect("create_notification_log must succeed");

        let deleted = storage.cleanup_old_logs(7).await.expect("cleanup_old_logs must succeed");
        assert_eq!(deleted, 0, "a log row written just now must survive a 7-day retention window");

        let remaining: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM push_notification_log")
            .fetch_one(&*pool)
            .await
            .expect("count must succeed");
        assert_eq!(remaining, 1);
    }
}

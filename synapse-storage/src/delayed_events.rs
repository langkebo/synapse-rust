//! MSC4140 — Cancellable delayed events storage.
//!
//! Stores delayed event metadata for the MSC4140 "Cancellable delayed events"
//! feature. The `delayed_events` table is created by the unified schema
//! migration. This module provides the Rust storage API.
//!
//! See: https://github.com/matrix-org/matrix-spec-proposals/pull/4140

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use std::sync::Arc;
use synapse_common::ApiError;

/// A delayed event record (MSC4140).
///
/// Maps to the `delayed_events` table. The `event_id` field is a synthetic
/// placeholder at scheduling time (the real Matrix event_id is not known
/// until the event is actually sent).
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct DelayedEvent {
    /// The `id` field.
    pub id: i64,
    /// The `room_id` field.
    pub room_id: String,
    /// The `user_id` field.
    pub user_id: String,
    /// The `device_id` field.
    pub device_id: String,
    /// Synthetic placeholder event_id (not the real Matrix event_id, which
    /// is unknown until the event is sent). Used as a unique key.
    pub event_id: String,
    /// The `event_type` field.
    pub event_type: String,
    /// The `state_key` field.
    pub state_key: Option<String>,
    /// The `content` field.
    pub content: serde_json::Value,
    /// The `delay_ms` field.
    pub delay_ms: i64,
    /// When the event is scheduled to be sent (created_ts + delay_ms).
    /// Reset to now+delay_ms on `restart` (heartbeat).
    pub scheduled_ts: i64,
    /// The `created_ts` field.
    pub created_ts: i64,
    /// One of: "pending", "sent", "cancelled", "failed".
    pub status: String,
    /// The `retry_count` field.
    pub retry_count: i32,
    /// The `last_error` field.
    pub last_error: Option<String>,
}

/// Request to create a new delayed event.
#[derive(Debug, Clone)]
pub struct CreateDelayedEventRequest {
    /// The `room_id` field.
    pub room_id: String,
    /// The `user_id` field.
    pub user_id: String,
    /// The `device_id` field.
    pub device_id: String,
    /// The `event_type` field.
    pub event_type: String,
    /// The `state_key` field.
    pub state_key: Option<String>,
    /// The `content` field.
    pub content: serde_json::Value,
    /// The `delay_ms` field.
    pub delay_ms: i64,
}

/// Actions for the generic management endpoint (MSC4140 Gen 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DelayedEventAction {
    /// The `Send` variant.
    Send,
    /// The `Cancel` variant.
    Cancel,
    /// The `Restart` variant.
    Restart,
}

impl DelayedEventAction {
    /// Parse an action string from the generic management endpoint body.
    pub fn parse(s: &str) -> Result<Self, ApiError> {
        match s {
            "send" => Ok(Self::Send),
            "cancel" => Ok(Self::Cancel),
            "restart" => Ok(Self::Restart),
            _ => Err(ApiError::invalid_param(format!("Invalid action: '{s}'. Must be one of: send, cancel, restart"))),
        }
    }

    /// See [`as_str`].
    /// See [`as_str`].
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Send => "send",
            Self::Cancel => "cancel",
            Self::Restart => "restart",
        }
    }
}

/// Storage API for delayed events (MSC4140).
#[async_trait]
pub trait DelayedEventStorageApi: Send + Sync {
    /// Create a new delayed event. Returns the created record (including
    /// the server-generated `id` which serves as the `delay_id`).
    async fn create_delayed_event(&self, request: CreateDelayedEventRequest) -> Result<DelayedEvent, ApiError>;

    /// Get a delayed event by its `id` (the `delay_id` returned to clients).
    async fn get_delayed_event(&self, delay_id: i64) -> Result<Option<DelayedEvent>, ApiError>;

    /// List all pending delayed events for a user.
    async fn list_delayed_events_for_user(&self, user_id: &str) -> Result<Vec<DelayedEvent>, ApiError>;

    /// Restart (heartbeat) a delayed event: reset `scheduled_ts` to now+delay_ms.
    async fn restart_delayed_event(&self, delay_id: i64) -> Result<bool, ApiError>;

    /// Cancel a delayed event: set status to "cancelled".
    async fn cancel_delayed_event(&self, delay_id: i64) -> Result<bool, ApiError>;

    /// Mark a delayed event as sent (set status to "sent").
    async fn mark_sent(&self, delay_id: i64) -> Result<bool, ApiError>;

    /// Get all pending delayed events that are due (scheduled_ts <= now).
    async fn get_due_events(&self, now_ts: i64, limit: i64) -> Result<Vec<DelayedEvent>, ApiError>;
}

/// PostgreSQL implementation of `DelayedEventStorageApi`.
pub struct DelayedEventStorage {
    pool: Arc<sqlx::PgPool>,
}

impl DelayedEventStorage {
    /// See [`new`].
    /// See [`new`].
    pub fn new(pool: Arc<sqlx::PgPool>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl DelayedEventStorageApi for DelayedEventStorage {
    async fn create_delayed_event(&self, request: CreateDelayedEventRequest) -> Result<DelayedEvent, ApiError> {
        let now = synapse_common::current_timestamp_millis();
        let scheduled_ts = now + request.delay_ms;
        // Synthetic event_id: not a real Matrix event_id, just a unique
        // placeholder. The real event_id is assigned when the event is sent.
        let synthetic_event_id = format!("$delayed:{}:{}:{}", request.room_id, request.user_id, now);

        let event = sqlx::query_as::<_, DelayedEvent>(
            r#"
            INSERT INTO delayed_events
                (room_id, user_id, device_id, event_id, event_type, state_key,
                 content, delay_ms, scheduled_ts, created_ts, status)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, 'pending')
            RETURNING id, room_id, user_id, device_id, event_id, event_type,
                      state_key, content, delay_ms, scheduled_ts, created_ts,
                      status, retry_count, last_error
            "#,
        )
        .bind(&request.room_id)
        .bind(&request.user_id)
        .bind(&request.device_id)
        .bind(&synthetic_event_id)
        .bind(&request.event_type)
        .bind(request.state_key.as_ref())
        .bind(&request.content)
        .bind(request.delay_ms)
        .bind(scheduled_ts)
        .bind(now)
        .fetch_one(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_context("Failed to create delayed event", &e))?;

        Ok(event)
    }

    async fn get_delayed_event(&self, delay_id: i64) -> Result<Option<DelayedEvent>, ApiError> {
        let event = sqlx::query_as::<_, DelayedEvent>(
            r#"
            SELECT id, room_id, user_id, device_id, event_id, event_type,
                   state_key, content, delay_ms, scheduled_ts, created_ts,
                   status, retry_count, last_error
            FROM delayed_events WHERE id = $1
            "#,
        )
        .bind(delay_id)
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_context("Failed to get delayed event", &e))?;

        Ok(event)
    }

    async fn list_delayed_events_for_user(&self, user_id: &str) -> Result<Vec<DelayedEvent>, ApiError> {
        let events = sqlx::query_as::<_, DelayedEvent>(
            r#"
            SELECT id, room_id, user_id, device_id, event_id, event_type,
                   state_key, content, delay_ms, scheduled_ts, created_ts,
                   status, retry_count, last_error
            FROM delayed_events
            WHERE user_id = $1 AND status = 'pending'
            ORDER BY scheduled_ts ASC
            "#,
        )
        .bind(user_id)
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_context("Failed to list delayed events", &e))?;

        Ok(events)
    }

    async fn restart_delayed_event(&self, delay_id: i64) -> Result<bool, ApiError> {
        let now = synapse_common::current_timestamp_millis();
        let result = sqlx::query(
            r#"
            UPDATE delayed_events
            SET scheduled_ts = $2 + delay_ms,
                status = 'pending'
            WHERE id = $1 AND status = 'pending'
            "#,
        )
        .bind(delay_id)
        .bind(now)
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_context("Failed to restart delayed event", &e))?;

        Ok(result.rows_affected() > 0)
    }

    async fn cancel_delayed_event(&self, delay_id: i64) -> Result<bool, ApiError> {
        let result = sqlx::query(
            r#"
            UPDATE delayed_events
            SET status = 'cancelled'
            WHERE id = $1 AND status = 'pending'
            "#,
        )
        .bind(delay_id)
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_context("Failed to cancel delayed event", &e))?;

        Ok(result.rows_affected() > 0)
    }

    async fn mark_sent(&self, delay_id: i64) -> Result<bool, ApiError> {
        let result = sqlx::query(
            r#"
            UPDATE delayed_events
            SET status = 'sent'
            WHERE id = $1 AND status = 'pending'
            "#,
        )
        .bind(delay_id)
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_context("Failed to mark delayed event as sent", &e))?;

        Ok(result.rows_affected() > 0)
    }

    async fn get_due_events(&self, now_ts: i64, limit: i64) -> Result<Vec<DelayedEvent>, ApiError> {
        let events = sqlx::query_as::<_, DelayedEvent>(
            r#"
            SELECT id, room_id, user_id, device_id, event_id, event_type,
                   state_key, content, delay_ms, scheduled_ts, created_ts,
                   status, retry_count, last_error
            FROM delayed_events
            WHERE status = 'pending' AND scheduled_ts <= $1
            ORDER BY scheduled_ts ASC
            LIMIT $2
            "#,
        )
        .bind(now_ts)
        .bind(limit)
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_context("Failed to get due delayed events", &e))?;

        Ok(events)
    }
}

/// Maximum allowed delay in milliseconds (24 hours, matching Synapse default).
pub const MAX_DELAY_MS: i64 = 86_400_000;

/// Validate that a delay value is within the allowed range.
/// Returns `Ok(())` if valid, or an `ApiError` if the delay is invalid.
///
/// MSC4140 Gen 1 specifies `M_MAX_DELAY_EXCEEDED` as the error code, but
/// `ApiError` currently only supports standard Matrix error codes. We use
/// `M_BAD_JSON` (via `bad_request`) with a descriptive message that includes
/// the `max_delay` value. The SDK checks for `org.matrix.msc4140.errcode` in
/// the error data; since `ApiError` doesn't support extra fields, the SDK will
/// see `M_BAD_JSON` as the errcode. This is a known limitation to be addressed
/// when `ApiError` gains extra-field support.
pub fn validate_delay_ms(delay_ms: i64) -> Result<(), ApiError> {
    if delay_ms <= 0 {
        return Err(ApiError::bad_request("Delay must be a positive non-zero integer".to_string()));
    }
    if delay_ms > MAX_DELAY_MS {
        return Err(ApiError::bad_request(format!(
            "M_MAX_DELAY_EXCEEDED: Delay {delay_ms}ms exceeds maximum allowed {MAX_DELAY_MS}ms (max_delay={MAX_DELAY_MS})"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_delay_ms_accepts_valid_delay() {
        assert!(validate_delay_ms(1000).is_ok());
        assert!(validate_delay_ms(MAX_DELAY_MS).is_ok());
    }

    #[test]
    fn test_validate_delay_ms_rejects_zero() {
        assert!(validate_delay_ms(0).is_err());
    }

    #[test]
    fn test_validate_delay_ms_rejects_negative() {
        assert!(validate_delay_ms(-100).is_err());
    }

    #[test]
    fn test_validate_delay_ms_rejects_exceeds_max() {
        let err = validate_delay_ms(MAX_DELAY_MS + 1).unwrap_err();
        // The error should contain M_MAX_DELAY_EXCEEDED information.
        let msg = err.to_string();
        assert!(msg.contains("MAX_DELAY") || msg.contains("max_delay"));
    }

    #[test]
    fn test_delayed_event_action_parse_valid() {
        assert_eq!(DelayedEventAction::parse("send").unwrap(), DelayedEventAction::Send);
        assert_eq!(DelayedEventAction::parse("cancel").unwrap(), DelayedEventAction::Cancel);
        assert_eq!(DelayedEventAction::parse("restart").unwrap(), DelayedEventAction::Restart);
    }

    #[test]
    fn test_delayed_event_action_parse_invalid() {
        assert!(DelayedEventAction::parse("invalid").is_err());
        assert!(DelayedEventAction::parse("").is_err());
    }

    #[test]
    fn test_delayed_event_action_as_str() {
        assert_eq!(DelayedEventAction::Send.as_str(), "send");
        assert_eq!(DelayedEventAction::Cancel.as_str(), "cancel");
        assert_eq!(DelayedEventAction::Restart.as_str(), "restart");
    }
}

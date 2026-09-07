use serde::{Deserialize, Serialize};
use sqlx::FromRow;

/// The `ServerNotificationCursor` struct.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerNotificationCursor {
    /// The `created_ts` field.
    pub created_ts: i64,
    /// The `id` field.
    pub id: i64,
}

/// See [`encode_server_notification_cursor`].
pub fn encode_server_notification_cursor(cursor: &ServerNotificationCursor) -> String {
    format!("{}|{}", cursor.created_ts, cursor.id)
}

/// See [`decode_server_notification_cursor`].
pub fn decode_server_notification_cursor(cursor: Option<&str>) -> Option<ServerNotificationCursor> {
    let cursor = cursor?;
    let (created_ts, id) = cursor.split_once('|')?;
    Some(ServerNotificationCursor { created_ts: created_ts.parse::<i64>().ok()?, id: id.parse::<i64>().ok()? })
}

/// The `ServerNotification` struct.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ServerNotification {
    /// The `id` field.
    pub id: i64,
    /// The `title` field.
    pub title: String,
    /// The `content` field.
    pub content: String,
    /// The `notification_type` field.
    pub notification_type: String,
    /// The `priority` field.
    pub priority: i32,
    /// The `target_audience` field.
    pub target_audience: String,
    /// The `target_user_ids` field.
    pub target_user_ids: serde_json::Value,
    /// The `starts_at` field.
    pub starts_at: Option<i64>,
    /// The `expires_at` field.
    pub expires_at: Option<i64>,
    /// The `is_enabled` field.
    pub is_enabled: bool,
    /// The `is_dismissable` field.
    pub is_dismissable: bool,
    /// The `action_url` field.
    pub action_url: Option<String>,
    /// The `action_text` field.
    pub action_text: Option<String>,
    /// The `created_by` field.
    pub created_by: Option<String>,
    /// The `created_ts` field.
    pub created_ts: i64,
    /// The `updated_ts` field.
    pub updated_ts: i64,
}

/// The `UserNotificationStatus` struct.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct UserNotificationStatus {
    /// The `id` field.
    pub id: i64,
    /// The `user_id` field.
    pub user_id: String,
    /// The `notification_id` field.
    pub notification_id: i64,
    /// The `is_read` field.
    pub is_read: bool,
    /// The `is_dismissed` field.
    pub is_dismissed: bool,
    /// The `read_ts` field.
    pub read_ts: Option<i64>,
    /// The `dismissed_ts` field.
    pub dismissed_ts: Option<i64>,
    /// The `created_ts` field.
    pub created_ts: i64,
}

/// The `NotificationTemplate` struct.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct NotificationTemplate {
    /// The `id` field.
    pub id: i64,
    /// The `name` field.
    pub name: String,
    /// The `title_template` field.
    pub title_template: String,
    /// The `content_template` field.
    pub content_template: String,
    /// The `notification_type` field.
    pub notification_type: String,
    /// The `variables` field.
    pub variables: serde_json::Value,
    /// The `is_enabled` field.
    pub is_enabled: bool,
    /// The `created_ts` field.
    pub created_ts: i64,
    /// The `updated_ts` field.
    pub updated_ts: i64,
}

/// The `NotificationDeliveryLog` struct.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct NotificationDeliveryLog {
    /// The `id` field.
    pub id: i64,
    /// The `notification_id` field.
    pub notification_id: i64,
    /// The `user_id` field.
    pub user_id: Option<String>,
    /// The `delivery_method` field.
    pub delivery_method: String,
    /// The `status` field.
    pub status: String,
    /// The `error_message` field.
    pub error_message: Option<String>,
    /// The `delivered_ts` field.
    pub delivered_ts: i64,
}

/// The `ScheduledNotification` struct.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ScheduledNotification {
    /// The `id` field.
    pub id: i64,
    /// The `notification_id` field.
    pub notification_id: i64,
    /// The `scheduled_for` field.
    pub scheduled_for: i64,
    /// The `is_sent` field.
    pub is_sent: bool,
    /// The `sent_ts` field.
    pub sent_ts: Option<i64>,
    /// The `created_ts` field.
    pub created_ts: i64,
}

/// The `CreateNotificationRequest` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateNotificationRequest {
    /// The `title` field.
    pub title: String,
    /// The `content` field.
    pub content: String,
    /// The `notification_type` field.
    pub notification_type: Option<String>,
    /// The `priority` field.
    pub priority: Option<i32>,
    /// The `target_audience` field.
    pub target_audience: Option<String>,
    /// The `target_user_ids` field.
    pub target_user_ids: Option<Vec<String>>,
    /// The `starts_at` field.
    pub starts_at: Option<i64>,
    /// The `expires_at` field.
    pub expires_at: Option<i64>,
    /// The `is_dismissable` field.
    pub is_dismissable: Option<bool>,
    /// The `action_url` field.
    pub action_url: Option<String>,
    /// The `action_text` field.
    pub action_text: Option<String>,
    /// The `created_by` field.
    pub created_by: Option<String>,
}

/// The `CreateTemplateRequest` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateTemplateRequest {
    /// The `name` field.
    pub name: String,
    /// The `title_template` field.
    pub title_template: String,
    /// The `content_template` field.
    pub content_template: String,
    /// The `notification_type` field.
    pub notification_type: Option<String>,
    /// The `variables` field.
    pub variables: Option<Vec<String>>,
}

/// The `NotificationWithStatus` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationWithStatus {
    #[serde(flatten)]
    /// The `notification` field.
    pub notification: ServerNotification,
    /// The `is_read` field.
    pub is_read: bool,
    /// The `is_dismissed` field.
    pub is_dismissed: bool,
}

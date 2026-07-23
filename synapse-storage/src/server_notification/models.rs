use serde::{Deserialize, Serialize};
use sqlx::FromRow;

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

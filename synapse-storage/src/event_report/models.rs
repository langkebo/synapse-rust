use serde::{Deserialize, Serialize};
use sqlx::FromRow;

/// The `EventReport` struct.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct EventReport {
    /// The `id` field.
    pub id: i64,
    /// The `event_id` field.
    pub event_id: String,
    /// The `room_id` field.
    pub room_id: String,
    /// The `reporter_user_id` field.
    pub reporter_user_id: String,
    /// The `reported_user_id` field.
    pub reported_user_id: Option<String>,
    /// The `event_json` field.
    pub event_json: Option<serde_json::Value>,
    /// The `reason` field.
    pub reason: Option<String>,
    /// The `description` field.
    pub description: Option<String>,
    /// The `status` field.
    pub status: String,
    /// The `score` field.
    pub score: i32,
    /// The `received_ts` field.
    pub received_ts: i64,
    #[sqlx(rename = "resolved_at")]
    /// The `resolved_ts` field.
    pub resolved_ts: Option<i64>,
    /// The `resolved_by` field.
    pub resolved_by: Option<String>,
    /// The `resolution_reason` field.
    pub resolution_reason: Option<String>,
}

/// The `EventReportHistory` struct.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct EventReportHistory {
    /// The `id` field.
    pub id: i64,
    /// The `report_id` field.
    pub report_id: i64,
    /// The `action` field.
    pub action: String,
    /// The `actor_user_id` field.
    pub actor_user_id: Option<String>,
    /// The `actor_role` field.
    pub actor_role: Option<String>,
    /// The `old_status` field.
    pub old_status: Option<String>,
    /// The `new_status` field.
    pub new_status: Option<String>,
    /// The `reason` field.
    pub reason: Option<String>,
    /// The `created_ts` field.
    pub created_ts: i64,
    /// The `metadata` field.
    pub metadata: Option<serde_json::Value>,
}

/// The `ReportRateLimit` struct.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ReportRateLimit {
    /// The `id` field.
    pub id: i64,
    /// The `user_id` field.
    pub user_id: String,
    /// The `report_count` field.
    pub report_count: i32,
    /// The `last_report_at` field.
    pub last_report_at: Option<i64>,
    /// The `blocked_until_at` field.
    pub blocked_until_at: Option<i64>,
    /// The `is_blocked` field.
    pub is_blocked: bool,
    /// The `block_reason` field.
    pub block_reason: Option<String>,
    /// The `created_ts` field.
    pub created_ts: i64,
    /// The `updated_ts` field.
    pub updated_ts: i64,
}

/// The `EventReportStats` struct.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct EventReportStats {
    /// The `id` field.
    pub id: i64,
    /// The `stat_date` field.
    pub stat_date: chrono::NaiveDate,
    /// The `total_reports` field.
    pub total_reports: i32,
    /// The `open_reports` field.
    pub open_reports: i32,
    /// The `resolved_reports` field.
    pub resolved_reports: i32,
    /// The `dismissed_reports` field.
    pub dismissed_reports: i32,
    /// The `avg_resolution_time_ms` field.
    pub avg_resolution_time_ms: Option<i64>,
    /// The `created_ts` field.
    pub created_ts: i64,
    /// The `updated_ts` field.
    pub updated_ts: i64,
}

/// The `CreateEventReportRequest` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateEventReportRequest {
    /// The `event_id` field.
    pub event_id: String,
    /// The `room_id` field.
    pub room_id: String,
    /// The `reporter_user_id` field.
    pub reporter_user_id: String,
    /// The `reported_user_id` field.
    pub reported_user_id: Option<String>,
    /// The `event_json` field.
    pub event_json: Option<serde_json::Value>,
    /// The `reason` field.
    pub reason: Option<String>,
    /// The `description` field.
    pub description: Option<String>,
    /// The `score` field.
    pub score: Option<i32>,
}

/// The `UpdateEventReportRequest` struct.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UpdateEventReportRequest {
    /// The `status` field.
    pub status: Option<String>,
    /// The `score` field.
    pub score: Option<i32>,
    /// The `resolved_by` field.
    pub resolved_by: Option<String>,
    /// The `resolution_reason` field.
    pub resolution_reason: Option<String>,
}

/// The `ReportRateLimitCheck` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportRateLimitCheck {
    /// The `is_allowed` field.
    pub is_allowed: bool,
    /// The `remaining_reports` field.
    pub remaining_reports: i32,
    /// The `block_reason` field.
    pub block_reason: Option<String>,
}

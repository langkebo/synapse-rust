use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct EventReport {
    pub id: i64,
    pub event_id: String,
    pub room_id: String,
    pub reporter_user_id: String,
    pub reported_user_id: Option<String>,
    pub event_json: Option<serde_json::Value>,
    pub reason: Option<String>,
    pub description: Option<String>,
    pub status: String,
    pub score: i32,
    pub received_ts: i64,
    #[sqlx(rename = "resolved_at")]
    pub resolved_ts: Option<i64>,
    pub resolved_by: Option<String>,
    pub resolution_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct EventReportHistory {
    pub id: i64,
    pub report_id: i64,
    pub action: String,
    pub actor_user_id: Option<String>,
    pub actor_role: Option<String>,
    pub old_status: Option<String>,
    pub new_status: Option<String>,
    pub reason: Option<String>,
    pub created_ts: i64,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ReportRateLimit {
    pub id: i64,
    pub user_id: String,
    pub report_count: i32,
    pub last_report_at: Option<i64>,
    pub blocked_until_at: Option<i64>,
    pub is_blocked: bool,
    pub block_reason: Option<String>,
    pub created_ts: i64,
    pub updated_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct EventReportStats {
    pub id: i64,
    pub stat_date: chrono::NaiveDate,
    pub total_reports: i32,
    pub open_reports: i32,
    pub resolved_reports: i32,
    pub dismissed_reports: i32,
    pub avg_resolution_time_ms: Option<i64>,
    pub created_ts: i64,
    pub updated_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateEventReportRequest {
    pub event_id: String,
    pub room_id: String,
    pub reporter_user_id: String,
    pub reported_user_id: Option<String>,
    pub event_json: Option<serde_json::Value>,
    pub reason: Option<String>,
    pub description: Option<String>,
    pub score: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UpdateEventReportRequest {
    pub status: Option<String>,
    pub score: Option<i32>,
    pub resolved_by: Option<String>,
    pub resolution_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportRateLimitCheck {
    pub is_allowed: bool,
    pub remaining_reports: i32,
    pub block_reason: Option<String>,
}

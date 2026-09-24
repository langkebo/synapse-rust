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

/// 事件报告实时聚合统计（取代不存在的 `event_report_stats` 预计算表）。
///
/// 字段名与 SDK 侧既有契约逐字段一致 —— `matrix-js-sdk`
/// `src/event-report/index.ts` 的 `StatsResponse`
/// （`{ total, open, resolved, dismissed, escalated }`），
/// 因此既有 `EventReportManager.getStats()` 无需改类型即可直接消费本端点。
///
/// `escalated` 对应后端 `escalate_report` 写入的 `status = 'investigating'`
/// （见 `synapse-services/src/event_report_service.rs`）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventReportAggregateStats {
    /// 报告总数（含全部状态；`status` 可空，空值只计入 `total`）
    pub total: i64,
    /// 状态为 `open` 的报告数
    pub open: i64,
    /// 状态为 `resolved` 的报告数
    pub resolved: i64,
    /// 状态为 `dismissed` 的报告数
    pub dismissed: i64,
    /// 状态为 `investigating`（已升级）的报告数
    pub escalated: i64,
}

use serde::{Deserialize, Serialize};
use sqlx::FromRow;

/// The `MediaQuotaConfig` struct.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct MediaQuotaConfig {
    /// The `id` field.
    pub id: i64,
    /// The `name` field.
    pub name: String,
    /// The `description` field.
    pub description: Option<String>,
    /// The `max_storage_bytes` field.
    pub max_storage_bytes: i64,
    /// The `max_file_size_bytes` field.
    pub max_file_size_bytes: i64,
    /// The `max_files_count` field.
    pub max_files_count: i32,
    /// The `allowed_mime_types` field.
    pub allowed_mime_types: serde_json::Value,
    /// The `blocked_mime_types` field.
    pub blocked_mime_types: serde_json::Value,
    /// The `is_default` field.
    pub is_default: bool,
    /// The `is_enabled` field.
    pub is_enabled: bool,
    /// The `created_ts` field.
    pub created_ts: i64,
    /// The `updated_ts` field.
    pub updated_ts: Option<i64>,
}

/// The `UserMediaQuota` struct.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct UserMediaQuota {
    /// The `id` field.
    pub id: i64,
    /// The `user_id` field.
    pub user_id: String,
    /// The `quota_config_id` field.
    pub quota_config_id: Option<i64>,
    /// The `custom_max_storage_bytes` field.
    pub custom_max_storage_bytes: Option<i64>,
    /// The `custom_max_file_size_bytes` field.
    pub custom_max_file_size_bytes: Option<i64>,
    /// The `custom_max_files_count` field.
    pub custom_max_files_count: Option<i32>,
    /// The `current_storage_bytes` field.
    pub current_storage_bytes: i64,
    /// The `current_files_count` field.
    pub current_files_count: i32,
    /// The `created_ts` field.
    pub created_ts: i64,
    /// The `updated_ts` field.
    pub updated_ts: Option<i64>,
}

/// The `MediaUsageLog` struct.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct MediaUsageLog {
    /// The `id` field.
    pub id: i64,
    /// The `user_id` field.
    pub user_id: String,
    /// The `media_id` field.
    pub media_id: String,
    /// The `file_size_bytes` field.
    pub file_size_bytes: i64,
    /// The `mime_type` field.
    pub mime_type: Option<String>,
    /// The `operation` field.
    pub operation: String,
    /// The `timestamp` field.
    pub timestamp: i64,
}

/// The `MediaQuotaAlert` struct.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct MediaQuotaAlert {
    /// The `id` field.
    pub id: i64,
    /// The `user_id` field.
    pub user_id: String,
    /// The `alert_type` field.
    pub alert_type: String,
    /// The `threshold_percent` field.
    pub threshold_percent: i32,
    /// The `current_usage_bytes` field.
    pub current_usage_bytes: i64,
    /// The `quota_limit_bytes` field.
    pub quota_limit_bytes: i64,
    /// The `message` field.
    pub message: Option<String>,
    /// The `is_read` field.
    pub is_read: bool,
    /// The `created_ts` field.
    pub created_ts: i64,
}

/// The `ServerMediaQuota` struct.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ServerMediaQuota {
    /// The `id` field.
    pub id: i64,
    /// The `max_storage_bytes` field.
    pub max_storage_bytes: Option<i64>,
    /// The `max_file_size_bytes` field.
    pub max_file_size_bytes: Option<i64>,
    /// The `max_files_count` field.
    pub max_files_count: Option<i32>,
    /// The `current_storage_bytes` field.
    pub current_storage_bytes: i64,
    /// The `current_files_count` field.
    pub current_files_count: i32,
    /// The `alert_threshold_percent` field.
    pub alert_threshold_percent: i32,
    /// The `updated_ts` field.
    pub updated_ts: i64,
}

/// The `CreateQuotaConfigRequest` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateQuotaConfigRequest {
    /// The `name` field.
    pub name: String,
    /// The `description` field.
    pub description: Option<String>,
    /// The `max_storage_bytes` field.
    pub max_storage_bytes: i64,
    /// The `max_file_size_bytes` field.
    pub max_file_size_bytes: i64,
    /// The `max_files_count` field.
    pub max_files_count: i32,
    /// The `allowed_mime_types` field.
    pub allowed_mime_types: Option<Vec<String>>,
    /// The `blocked_mime_types` field.
    pub blocked_mime_types: Option<Vec<String>>,
    /// The `is_default` field.
    pub is_default: Option<bool>,
}

/// The `SetUserQuotaRequest` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetUserQuotaRequest {
    /// The `user_id` field.
    pub user_id: String,
    /// The `quota_config_id` field.
    pub quota_config_id: Option<i64>,
    /// The `custom_max_storage_bytes` field.
    pub custom_max_storage_bytes: Option<i64>,
    /// The `custom_max_file_size_bytes` field.
    pub custom_max_file_size_bytes: Option<i64>,
    /// The `custom_max_files_count` field.
    pub custom_max_files_count: Option<i32>,
}

/// The `UpdateUsageRequest` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateUsageRequest {
    /// The `user_id` field.
    pub user_id: String,
    /// The `media_id` field.
    pub media_id: String,
    /// The `file_size_bytes` field.
    pub file_size_bytes: i64,
    /// The `mime_type` field.
    pub mime_type: Option<String>,
    /// The `operation` field.
    pub operation: String,
}

/// The `QuotaCheckResult` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuotaCheckResult {
    #[serde(rename = "allowed")]
    /// The `is_allowed` field.
    pub is_allowed: bool,
    /// The `reason` field.
    pub reason: Option<String>,
    /// The `current_usage` field.
    pub current_usage: i64,
    /// The `quota_limit` field.
    pub quota_limit: i64,
    /// The `usage_percent` field.
    pub usage_percent: f64,
}

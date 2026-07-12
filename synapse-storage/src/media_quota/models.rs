use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct MediaQuotaConfig {
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
    pub max_storage_bytes: i64,
    pub max_file_size_bytes: i64,
    pub max_files_count: i32,
    pub allowed_mime_types: serde_json::Value,
    pub blocked_mime_types: serde_json::Value,
    pub is_default: bool,
    pub is_enabled: bool,
    pub created_ts: i64,
    pub updated_ts: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct UserMediaQuota {
    pub id: i64,
    pub user_id: String,
    pub quota_config_id: Option<i64>,
    pub custom_max_storage_bytes: Option<i64>,
    pub custom_max_file_size_bytes: Option<i64>,
    pub custom_max_files_count: Option<i32>,
    pub current_storage_bytes: i64,
    pub current_files_count: i32,
    pub created_ts: i64,
    pub updated_ts: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct MediaUsageLog {
    pub id: i64,
    pub user_id: String,
    pub media_id: String,
    pub file_size_bytes: i64,
    pub mime_type: Option<String>,
    pub operation: String,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct MediaQuotaAlert {
    pub id: i64,
    pub user_id: String,
    pub alert_type: String,
    pub threshold_percent: i32,
    pub current_usage_bytes: i64,
    pub quota_limit_bytes: i64,
    pub message: Option<String>,
    pub is_read: bool,
    pub created_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ServerMediaQuota {
    pub id: i64,
    pub max_storage_bytes: Option<i64>,
    pub max_file_size_bytes: Option<i64>,
    pub max_files_count: Option<i32>,
    pub current_storage_bytes: i64,
    pub current_files_count: i32,
    pub alert_threshold_percent: i32,
    pub updated_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateQuotaConfigRequest {
    pub name: String,
    pub description: Option<String>,
    pub max_storage_bytes: i64,
    pub max_file_size_bytes: i64,
    pub max_files_count: i32,
    pub allowed_mime_types: Option<Vec<String>>,
    pub blocked_mime_types: Option<Vec<String>>,
    pub is_default: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetUserQuotaRequest {
    pub user_id: String,
    pub quota_config_id: Option<i64>,
    pub custom_max_storage_bytes: Option<i64>,
    pub custom_max_file_size_bytes: Option<i64>,
    pub custom_max_files_count: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateUsageRequest {
    pub user_id: String,
    pub media_id: String,
    pub file_size_bytes: i64,
    pub mime_type: Option<String>,
    pub operation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuotaCheckResult {
    #[serde(rename = "allowed")]
    pub is_allowed: bool,
    pub reason: Option<String>,
    pub current_usage: i64,
    pub quota_limit: i64,
    pub usage_percent: f64,
}

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyBackup {
    pub id: Uuid,
    pub user_id: String,
    pub version: String,
    pub algorithm: String,
    pub auth_data: serde_json::Value,
    pub encrypted_data: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupVersion {
    pub version: String,
    pub algorithm: String,
    pub auth_data: serde_json::Value,
    pub count: i64,
    pub etag: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupUploadRequest {
    pub algorithm: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupKeyUploadRequest {
    pub first_message_index: i64,
    pub forwarded_count: i64,
    pub is_verified: bool,
    pub session_data: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupUploadResponse {
    pub etag: String,
    pub count: i64,
}
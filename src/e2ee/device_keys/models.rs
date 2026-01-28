use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceKey {
    pub id: Uuid,
    pub user_id: String,
    pub device_id: String,
    pub display_name: Option<String>,
    pub algorithm: String,
    pub key_id: String,
    pub public_key: String,
    pub signatures: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceKeys {
    pub user_id: String,
    pub device_id: String,
    pub algorithms: Vec<String>,
    pub keys: serde_json::Value,
    pub signatures: serde_json::Value,
    pub unsigned: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyQueryRequest {
    pub timeout: Option<u64>,
    pub device_keys: serde_json::Value,
    pub token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyQueryResponse {
    pub device_keys: serde_json::Value,
    pub failures: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyUploadRequest {
    pub device_keys: Option<DeviceKeys>,
    pub one_time_keys: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyUploadResponse {
    pub one_time_key_counts: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyClaimRequest {
    pub timeout: Option<u64>,
    pub one_time_keys: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyClaimResponse {
    pub one_time_keys: serde_json::Value,
    pub failures: serde_json::Value,
}
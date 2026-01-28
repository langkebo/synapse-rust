use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossSigningKey {
    pub id: Uuid,
    pub user_id: String,
    pub key_type: String,
    pub public_key: String,
    pub usage: Vec<String>,
    pub signatures: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossSigningKeys {
    pub user_id: String,
    pub master_key: String,
    pub self_signing_key: String,
    pub user_signing_key: String,
    pub self_signing_signature: String,
    pub user_signing_signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossSigningUpload {
    pub master_key: serde_json::Value,
    pub self_signing_key: serde_json::Value,
    pub user_signing_key: serde_json::Value,
}
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventSignature {
    pub id: Uuid,
    pub event_id: String,
    pub user_id: String,
    pub device_id: String,
    pub signature: String,
    pub key_id: String,
    pub created_at: DateTime<Utc>,
}
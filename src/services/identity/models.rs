use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThirdPartyId {
    pub address: String,
    pub medium: String,
    pub user_id: String,
    pub validated_at: i64,
    pub added_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThirdPartyIdValidation {
    pub sid: String,
    pub client_secret: String,
    pub medium: String,
    pub address: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentityServerInfo {
    pub trusted_servers: Vec<String>,
    pub api_endpoint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BindingRequest {
    pub sid: String,
    pub client_secret: String,
    pub id_server: String,
    pub id_access_token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BindingResponse {
    pub user_id: String,
    pub device_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnbindingRequest {
    pub id_server: String,
    pub id_access_token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Invitation {
    pub room_id: String,
    pub sender: String,
    pub medium: String,
    pub address: String,
    pub id_server: String,
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvitationResponse {
    pub user_id: Option<String>,
    pub signed: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Invite3pid {
    pub id_server: String,
    pub id_access_token: String,
    pub medium: String,
    pub address: String,
    pub signer: String,
    pub signature: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LookupRequest {
    pub medium: String,
    pub address: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LookupResponse {
    pub user_id: String,
    pub medium: String,
    pub address: String,
    pub not_before: Option<i64>,
    pub not_after: Option<i64>,
    pub devices: Option<Vec<serde_json::Value>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HashLookupRequest {
    pub algorithm: String,
    pub addresses: Vec<String>,
    pub mediums: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HashLookupResponse {
    pub chunk: Vec<serde_json::Value>,
}

impl ThirdPartyId {
    pub fn new(address: &str, medium: &str, user_id: &str) -> Self {
        let now = chrono::Utc::now().timestamp_millis();
        Self {
            address: address.to_string(),
            medium: medium.to_string(),
            user_id: user_id.to_string(),
            validated_at: now,
            added_at: now,
        }
    }
}

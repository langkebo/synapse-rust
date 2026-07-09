use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SamlSession {
    pub id: i64,
    pub session_id: String,
    pub user_id: String,
    pub name_id: Option<String>,
    pub issuer: Option<String>,
    pub session_index: Option<String>,
    pub attributes: serde_json::Value,
    pub created_ts: i64,
    pub expires_at: i64,
    pub last_used_ts: i64,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SamlUserMapping {
    pub id: i64,
    pub name_id: String,
    pub user_id: String,
    pub issuer: String,
    pub first_seen_ts: i64,
    pub last_authenticated_ts: i64,
    pub authentication_count: i32,
    pub attributes: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SamlIdentityProvider {
    pub id: i64,
    pub entity_id: String,
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub metadata_url: Option<String>,
    pub metadata_xml: Option<String>,
    pub is_enabled: bool,
    pub priority: i32,
    pub attribute_mapping: serde_json::Value,
    pub created_ts: i64,
    pub updated_ts: Option<i64>,
    #[sqlx(rename = "last_metadata_refresh_at")]
    pub last_metadata_refresh_ts: Option<i64>,
    #[sqlx(rename = "metadata_valid_until_at")]
    pub metadata_valid_until: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SamlAuthEvent {
    pub id: i64,
    pub session_id: Option<String>,
    pub user_id: Option<String>,
    pub name_id: Option<String>,
    pub issuer: Option<String>,
    pub event_type: String,
    pub status: String,
    pub error_message: Option<String>,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub request_id: Option<String>,
    pub attributes: serde_json::Value,
    pub created_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SamlLogoutRequest {
    pub id: i64,
    pub request_id: String,
    pub session_id: Option<String>,
    pub user_id: Option<String>,
    pub name_id: Option<String>,
    pub issuer: Option<String>,
    pub reason: Option<String>,
    pub status: String,
    pub created_ts: i64,
    #[sqlx(rename = "processed_at")]
    pub processed_ts: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSamlSessionRequest {
    pub session_id: String,
    pub user_id: String,
    pub name_id: Option<String>,
    pub issuer: Option<String>,
    pub session_index: Option<String>,
    pub attributes: HashMap<String, Vec<String>>,
    pub expires_in_seconds: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSamlUserMappingRequest {
    pub name_id: String,
    pub user_id: String,
    pub issuer: String,
    pub attributes: HashMap<String, Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSamlIdentityProviderRequest {
    pub entity_id: String,
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub metadata_url: Option<String>,
    pub metadata_xml: Option<String>,
    pub enabled: Option<bool>,
    pub priority: Option<i32>,
    pub attribute_mapping: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSamlAuthEventRequest {
    pub session_id: Option<String>,
    pub user_id: Option<String>,
    pub name_id: Option<String>,
    pub issuer: Option<String>,
    pub event_type: String,
    pub status: String,
    pub error_message: Option<String>,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub request_id: Option<String>,
    pub attributes: HashMap<String, Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSamlLogoutRequestRequest {
    pub request_id: String,
    pub session_id: Option<String>,
    pub user_id: Option<String>,
    pub name_id: Option<String>,
    pub issuer: Option<String>,
    pub reason: Option<String>,
}

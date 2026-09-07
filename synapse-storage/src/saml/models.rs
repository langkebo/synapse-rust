use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use sqlx::FromRow;

/// The `SamlSession` struct.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SamlSession {
    /// The `id` field.
    pub id: i64,
    /// The `session_id` field.
    pub session_id: String,
    /// The `user_id` field.
    pub user_id: String,
    /// The `name_id` field.
    pub name_id: Option<String>,
    /// The `issuer` field.
    pub issuer: Option<String>,
    /// The `session_index` field.
    pub session_index: Option<String>,
    /// The `attributes` field.
    pub attributes: serde_json::Value,
    /// The `created_ts` field.
    pub created_ts: i64,
    /// The `expires_at` field.
    pub expires_at: i64,
    /// The `last_used_ts` field.
    pub last_used_ts: i64,
    /// The `status` field.
    pub status: String,
}

/// The `SamlUserMapping` struct.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SamlUserMapping {
    /// The `id` field.
    pub id: i64,
    /// The `name_id` field.
    pub name_id: String,
    /// The `user_id` field.
    pub user_id: String,
    /// The `issuer` field.
    pub issuer: String,
    /// The `first_seen_ts` field.
    pub first_seen_ts: i64,
    /// The `last_authenticated_ts` field.
    pub last_authenticated_ts: i64,
    /// The `authentication_count` field.
    pub authentication_count: i32,
    /// The `attributes` field.
    pub attributes: serde_json::Value,
}

/// The `SamlIdentityProvider` struct.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SamlIdentityProvider {
    /// The `id` field.
    pub id: i64,
    /// The `entity_id` field.
    pub entity_id: String,
    /// The `display_name` field.
    pub display_name: Option<String>,
    /// The `description` field.
    pub description: Option<String>,
    /// The `metadata_url` field.
    pub metadata_url: Option<String>,
    /// The `metadata_xml` field.
    pub metadata_xml: Option<String>,
    /// The `is_enabled` field.
    pub is_enabled: bool,
    /// The `priority` field.
    pub priority: i32,
    /// The `attribute_mapping` field.
    pub attribute_mapping: serde_json::Value,
    /// The `created_ts` field.
    pub created_ts: i64,
    /// The `updated_ts` field.
    pub updated_ts: Option<i64>,
    #[sqlx(rename = "last_metadata_refresh_at")]
    /// The `last_metadata_refresh_ts` field.
    pub last_metadata_refresh_ts: Option<i64>,
    #[sqlx(rename = "metadata_valid_until_at")]
    /// The `metadata_valid_until` field.
    pub metadata_valid_until: Option<i64>,
}

/// The `SamlAuthEvent` struct.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SamlAuthEvent {
    /// The `id` field.
    pub id: i64,
    /// The `session_id` field.
    pub session_id: Option<String>,
    /// The `user_id` field.
    pub user_id: Option<String>,
    /// The `name_id` field.
    pub name_id: Option<String>,
    /// The `issuer` field.
    pub issuer: Option<String>,
    /// The `event_type` field.
    pub event_type: String,
    /// The `status` field.
    pub status: String,
    /// The `error_message` field.
    pub error_message: Option<String>,
    /// The `ip_address` field.
    pub ip_address: Option<String>,
    /// The `user_agent` field.
    pub user_agent: Option<String>,
    /// The `request_id` field.
    pub request_id: Option<String>,
    /// The `attributes` field.
    pub attributes: serde_json::Value,
    /// The `created_ts` field.
    pub created_ts: i64,
}

/// The `SamlLogoutRequest` struct.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SamlLogoutRequest {
    /// The `id` field.
    pub id: i64,
    /// The `request_id` field.
    pub request_id: String,
    /// The `session_id` field.
    pub session_id: Option<String>,
    /// The `user_id` field.
    pub user_id: Option<String>,
    /// The `name_id` field.
    pub name_id: Option<String>,
    /// The `issuer` field.
    pub issuer: Option<String>,
    /// The `reason` field.
    pub reason: Option<String>,
    /// The `status` field.
    pub status: String,
    /// The `created_ts` field.
    pub created_ts: i64,
    #[sqlx(rename = "processed_at")]
    /// The `processed_ts` field.
    pub processed_ts: Option<i64>,
}

/// SAML AuthnRequest 待处理记录：以 `relay_state` 为 key，绑定 `request_id`
/// 用于验证 AuthnResponse 的 `InResponseTo`，防重放。
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SamlPendingRequest {
    /// The `id` field.
    pub id: i64,
    /// The `relay_state` field.
    pub relay_state: String,
    /// The `request_id` field.
    pub request_id: String,
    /// The `created_ts` field.
    pub created_ts: i64,
    /// The `expires_at` field.
    pub expires_at: i64,
}

/// The `CreateSamlSessionRequest` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSamlSessionRequest {
    /// The `session_id` field.
    pub session_id: String,
    /// The `user_id` field.
    pub user_id: String,
    /// The `name_id` field.
    pub name_id: Option<String>,
    /// The `issuer` field.
    pub issuer: Option<String>,
    /// The `session_index` field.
    pub session_index: Option<String>,
    /// The `attributes` field.
    pub attributes: HashMap<String, Vec<String>>,
    /// The `expires_in_seconds` field.
    pub expires_in_seconds: i64,
}

/// The `CreateSamlUserMappingRequest` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSamlUserMappingRequest {
    /// The `name_id` field.
    pub name_id: String,
    /// The `user_id` field.
    pub user_id: String,
    /// The `issuer` field.
    pub issuer: String,
    /// The `attributes` field.
    pub attributes: HashMap<String, Vec<String>>,
}

/// The `CreateSamlIdentityProviderRequest` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSamlIdentityProviderRequest {
    /// The `entity_id` field.
    pub entity_id: String,
    /// The `display_name` field.
    pub display_name: Option<String>,
    /// The `description` field.
    pub description: Option<String>,
    /// The `metadata_url` field.
    pub metadata_url: Option<String>,
    /// The `metadata_xml` field.
    pub metadata_xml: Option<String>,
    /// The `enabled` field.
    pub enabled: Option<bool>,
    /// The `priority` field.
    pub priority: Option<i32>,
    /// The `attribute_mapping` field.
    pub attribute_mapping: Option<serde_json::Value>,
}

/// The `CreateSamlAuthEventRequest` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSamlAuthEventRequest {
    /// The `session_id` field.
    pub session_id: Option<String>,
    /// The `user_id` field.
    pub user_id: Option<String>,
    /// The `name_id` field.
    pub name_id: Option<String>,
    /// The `issuer` field.
    pub issuer: Option<String>,
    /// The `event_type` field.
    pub event_type: String,
    /// The `status` field.
    pub status: String,
    /// The `error_message` field.
    pub error_message: Option<String>,
    /// The `ip_address` field.
    pub ip_address: Option<String>,
    /// The `user_agent` field.
    pub user_agent: Option<String>,
    /// The `request_id` field.
    pub request_id: Option<String>,
    /// The `attributes` field.
    pub attributes: HashMap<String, Vec<String>>,
}

/// The `CreateSamlLogoutRequestRequest` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSamlLogoutRequestRequest {
    /// The `request_id` field.
    pub request_id: String,
    /// The `session_id` field.
    pub session_id: Option<String>,
    /// The `user_id` field.
    pub user_id: Option<String>,
    /// The `name_id` field.
    pub name_id: Option<String>,
    /// The `issuer` field.
    pub issuer: Option<String>,
    /// The `reason` field.
    pub reason: Option<String>,
}

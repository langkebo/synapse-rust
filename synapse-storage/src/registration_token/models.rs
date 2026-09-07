use serde::{Deserialize, Serialize};
use sqlx::FromRow;

/// The `RegistrationTokenCursor` struct.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistrationTokenCursor {
    /// The `created_ts` field.
    pub created_ts: i64,
    /// The `id` field.
    pub id: i64,
}

/// See [`encode_registration_token_cursor`].
pub fn encode_registration_token_cursor(cursor: &RegistrationTokenCursor) -> String {
    format!("{}|{}", cursor.created_ts, cursor.id)
}

/// See [`decode_registration_token_cursor`].
pub fn decode_registration_token_cursor(cursor: Option<&str>) -> Option<RegistrationTokenCursor> {
    let cursor = cursor?;
    let mut parts = cursor.split('|');
    let created_ts = parts.next()?.parse::<i64>().ok()?;
    let id = parts.next()?.parse::<i64>().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some(RegistrationTokenCursor { created_ts, id })
}

/// The `RegistrationToken` struct.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct RegistrationToken {
    /// The `id` field.
    pub id: i64,
    /// The `token` field.
    pub token: String,
    /// The `token_type` field.
    pub token_type: String,
    /// The `description` field.
    pub description: Option<String>,
    /// The `max_uses` field.
    pub max_uses: i32,
    /// The `uses_count` field.
    pub uses_count: i32,
    /// The `is_used` field.
    pub is_used: bool,
    /// The `is_enabled` field.
    pub is_enabled: bool,
    /// The `expires_at` field.
    pub expires_at: Option<i64>,
    /// The `created_by` field.
    pub created_by: Option<String>,
    /// The `created_ts` field.
    pub created_ts: i64,
    /// The `updated_ts` field.
    pub updated_ts: Option<i64>,
    /// The `last_used_ts` field.
    pub last_used_ts: Option<i64>,
    /// The `allowed_email_domains` field.
    pub allowed_email_domains: Option<Vec<String>>,
    /// The `allowed_user_ids` field.
    pub allowed_user_ids: Option<Vec<String>>,
    /// The `auto_join_rooms` field.
    pub auto_join_rooms: Option<Vec<String>>,
    /// The `display_name` field.
    pub display_name: Option<String>,
    /// The `email` field.
    pub email: Option<String>,
}

/// The `RegistrationTokenUsage` struct.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct RegistrationTokenUsage {
    /// The `id` field.
    pub id: i64,
    /// The `token_id` field.
    pub token_id: Option<i64>,
    /// The `token` field.
    pub token: String,
    /// The `user_id` field.
    pub user_id: String,
    /// The `username` field.
    pub username: Option<String>,
    /// The `email` field.
    pub email: Option<String>,
    /// The `ip_address` field.
    pub ip_address: Option<String>,
    /// The `user_agent` field.
    pub user_agent: Option<String>,
    /// The `used_ts` field.
    pub used_ts: i64,
    /// The `is_success` field.
    pub is_success: bool,
    /// The `error_message` field.
    pub error_message: Option<String>,
}

/// The `RoomInvite` struct.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct RoomInvite {
    /// The `id` field.
    pub id: i64,
    /// The `invite_code` field.
    pub invite_code: String,
    /// The `room_id` field.
    pub room_id: String,
    /// The `inviter_user_id` field.
    pub inviter_user_id: String,
    /// The `invitee_email` field.
    pub invitee_email: Option<String>,
    /// The `invitee_user_id` field.
    pub invitee_user_id: Option<String>,
    /// The `is_used` field.
    pub is_used: bool,
    /// The `is_revoked` field.
    pub is_revoked: bool,
    /// The `expires_at` field.
    pub expires_at: Option<i64>,
    /// The `created_ts` field.
    pub created_ts: i64,
    /// The `used_ts` field.
    pub used_ts: Option<i64>,
    /// The `revoked_at` field.
    pub revoked_at: Option<i64>,
    /// The `revoked_reason` field.
    pub revoked_reason: Option<String>,
}

/// The `RegistrationTokenBatch` struct.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct RegistrationTokenBatch {
    /// The `id` field.
    pub id: i64,
    /// The `batch_id` field.
    pub batch_id: String,
    /// The `description` field.
    pub description: Option<String>,
    /// The `token_count` field.
    pub token_count: i32,
    /// The `tokens_used` field.
    pub tokens_used: i32,
    /// The `created_by` field.
    pub created_by: Option<String>,
    /// The `created_ts` field.
    pub created_ts: i64,
    /// The `expires_at` field.
    pub expires_at: Option<i64>,
    /// The `is_enabled` field.
    pub is_enabled: bool,
    /// The `allowed_email_domains` field.
    pub allowed_email_domains: Option<Vec<String>>,
    /// The `auto_join_rooms` field.
    pub auto_join_rooms: Option<Vec<String>>,
}

/// The `CreateRegistrationTokenRequest` struct.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CreateRegistrationTokenRequest {
    /// The `token` field.
    pub token: Option<String>,
    /// The `token_type` field.
    pub token_type: Option<String>,
    /// The `description` field.
    pub description: Option<String>,
    /// The `max_uses` field.
    pub max_uses: Option<i32>,
    /// The `expires_at` field.
    pub expires_at: Option<i64>,
    /// The `created_by` field.
    pub created_by: Option<String>,
    /// The `allowed_email_domains` field.
    pub allowed_email_domains: Option<Vec<String>>,
    /// The `allowed_user_ids` field.
    pub allowed_user_ids: Option<Vec<String>>,
    /// The `auto_join_rooms` field.
    pub auto_join_rooms: Option<Vec<String>>,
    /// The `display_name` field.
    pub display_name: Option<String>,
    /// The `email` field.
    pub email: Option<String>,
}

/// The `UpdateRegistrationTokenRequest` struct.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UpdateRegistrationTokenRequest {
    /// The `description` field.
    pub description: Option<String>,
    /// The `max_uses` field.
    pub max_uses: Option<i32>,
    /// The `is_enabled` field.
    pub is_enabled: Option<bool>,
    /// The `expires_at` field.
    pub expires_at: Option<i64>,
}

/// The `CreateRoomInviteRequest` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRoomInviteRequest {
    /// The `room_id` field.
    pub room_id: String,
    /// The `inviter_user_id` field.
    pub inviter_user_id: String,
    /// The `invitee_email` field.
    pub invitee_email: Option<String>,
    /// The `expires_at` field.
    pub expires_at: Option<i64>,
}

/// The `TokenValidationResult` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenValidationResult {
    /// The `is_valid` field.
    pub is_valid: bool,
    /// The `token_id` field.
    pub token_id: Option<i64>,
    /// The `error_message` field.
    pub error_message: Option<String>,
}

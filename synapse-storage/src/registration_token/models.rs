use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistrationTokenCursor {
    pub created_ts: i64,
    pub id: i64,
}

pub fn encode_registration_token_cursor(cursor: &RegistrationTokenCursor) -> String {
    format!("{}|{}", cursor.created_ts, cursor.id)
}

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

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct RegistrationToken {
    pub id: i64,
    pub token: String,
    pub token_type: String,
    pub description: Option<String>,
    pub max_uses: i32,
    pub uses_count: i32,
    pub is_used: bool,
    pub is_enabled: bool,
    pub expires_at: Option<i64>,
    pub created_by: Option<String>,
    pub created_ts: i64,
    pub updated_ts: Option<i64>,
    pub last_used_ts: Option<i64>,
    pub allowed_email_domains: Option<Vec<String>>,
    pub allowed_user_ids: Option<Vec<String>>,
    pub auto_join_rooms: Option<Vec<String>>,
    pub display_name: Option<String>,
    pub email: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct RegistrationTokenUsage {
    pub id: i64,
    pub token_id: Option<i64>,
    pub token: String,
    pub user_id: String,
    pub username: Option<String>,
    pub email: Option<String>,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub used_ts: i64,
    pub is_success: bool,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct RoomInvite {
    pub id: i64,
    pub invite_code: String,
    pub room_id: String,
    pub inviter_user_id: String,
    pub invitee_email: Option<String>,
    pub invitee_user_id: Option<String>,
    pub is_used: bool,
    pub is_revoked: bool,
    pub expires_at: Option<i64>,
    pub created_ts: i64,
    pub used_ts: Option<i64>,
    pub revoked_at: Option<i64>,
    pub revoked_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct RegistrationTokenBatch {
    pub id: i64,
    pub batch_id: String,
    pub description: Option<String>,
    pub token_count: i32,
    pub tokens_used: i32,
    pub created_by: Option<String>,
    pub created_ts: i64,
    pub expires_at: Option<i64>,
    pub is_enabled: bool,
    pub allowed_email_domains: Option<Vec<String>>,
    pub auto_join_rooms: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CreateRegistrationTokenRequest {
    pub token: Option<String>,
    pub token_type: Option<String>,
    pub description: Option<String>,
    pub max_uses: Option<i32>,
    pub expires_at: Option<i64>,
    pub created_by: Option<String>,
    pub allowed_email_domains: Option<Vec<String>>,
    pub allowed_user_ids: Option<Vec<String>>,
    pub auto_join_rooms: Option<Vec<String>>,
    pub display_name: Option<String>,
    pub email: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UpdateRegistrationTokenRequest {
    pub description: Option<String>,
    pub max_uses: Option<i32>,
    pub is_enabled: Option<bool>,
    pub expires_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRoomInviteRequest {
    pub room_id: String,
    pub inviter_user_id: String,
    pub invitee_email: Option<String>,
    pub expires_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenValidationResult {
    pub is_valid: bool,
    pub token_id: Option<i64>,
    pub error_message: Option<String>,
}

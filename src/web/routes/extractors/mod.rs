pub mod auth;
pub mod json;
pub mod localhost_guard;
mod pagination;

use crate::common::ApiError;
use serde::{Deserialize, Serialize};

// ============== RoomId ==============

/// Room ID 提取器
/// 格式: !room_id:domain
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomId(pub String);

// ============== UserId ==============

/// User ID 提取器
/// 格式: @user_id:domain
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserId(pub String);

impl UserId {
    pub fn new(id: String) -> Self {
        Self(id)
    }

    pub fn parse(raw: &str) -> Result<Self, ApiError> {
        if raw.starts_with('@') {
            Ok(Self(raw.to_string()))
        } else {
            Err(ApiError::bad_request(format!("Invalid user ID format: {raw}")))
        }
    }
}

// ============== Pagination ==============

// extract_token_from_headers removed — use crate::web::utils::auth::bearer_token directly
pub use auth::{AdminUser, AuthenticatedUser, OptionalAuthenticatedUser};
pub use json::MatrixJson;
pub use pagination::Pagination;

// ============== DeviceId ==============

/// Device ID 提取器
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceId(pub String);

impl DeviceId {
    pub fn new(id: String) -> Self {
        Self(id)
    }
}

// ============== EventId ==============

/// Event ID 提取器
/// 格式: $event_id:domain
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventId(pub String);

impl EventId {
    pub fn new(id: String) -> Self {
        Self(id)
    }
}

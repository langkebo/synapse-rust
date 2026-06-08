use serde::{Deserialize, Serialize};

/// JWT Claims structure used for authentication tokens.
/// Moved to common to resolve circular dependency between cache and auth modules.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    pub sub: String,
    pub user_id: String,
    pub jti: String,
    #[serde(rename = "admin")]
    pub is_admin: bool,
    pub exp: i64,
    pub iat: i64,
    pub device_id: Option<String>,
}

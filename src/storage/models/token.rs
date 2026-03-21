use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct AccessToken {
    pub id: i64,
    pub token: String,
    pub user_id: String,
    pub device_id: Option<String>,
    pub created_ts: i64,
    pub expires_at: Option<i64>,
    pub last_used_ts: Option<i64>,
    pub user_agent: Option<String>,
    pub ip_address: Option<String>,
    pub is_revoked: bool,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct RefreshToken {
    pub id: i64,
    pub token_hash: String,
    pub user_id: String,
    pub device_id: Option<String>,
    pub access_token_id: Option<String>,
    pub scope: Option<String>,
    pub created_ts: i64,
    pub expires_at: Option<i64>,
    pub last_used_ts: Option<i64>,
    pub use_count: i32,
    pub is_revoked: bool,
    pub revoked_reason: Option<String>,
    pub client_info: Option<serde_json::Value>,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct TokenBlacklistEntry {
    pub id: i64,
    pub token_hash: String,
    pub token: Option<String>,
    pub token_type: String,
    pub user_id: Option<String>,
    pub is_revoked: bool,
    pub reason: Option<String>,
    pub expires_at: Option<i64>,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct OpenIdToken {
    pub id: i64,
    pub token: String,
    pub user_id: String,
    pub device_id: Option<String>,
    pub created_ts: i64,
    pub expires_at: i64,
    pub is_valid: bool,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct RefreshTokenFamily {
    pub id: i64,
    pub family_id: String,
    pub user_id: String,
    pub device_id: Option<String>,
    pub created_ts: i64,
    pub last_refresh_ts: Option<i64>,
    pub refresh_count: i32,
    pub is_compromised: bool,
    pub compromised_ts: Option<i64>,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct RefreshTokenRotation {
    pub id: i64,
    pub family_id: String,
    pub old_token_hash: Option<String>,
    pub new_token_hash: String,
    pub rotated_ts: i64,
    pub rotation_reason: Option<String>,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct RefreshTokenUsage {
    pub id: i64,
    pub refresh_token_id: i64,
    pub user_id: String,
    pub old_access_token_id: Option<String>,
    pub new_access_token_id: Option<String>,
    pub used_ts: i64,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub is_success: bool,
    pub error_message: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_access_token_struct() {
        let token = AccessToken {
            id: 1,
            token: "access_token_abc123".to_string(),
            user_id: "@alice:example.com".to_string(),
            device_id: Some("DEVICE123".to_string()),
            created_ts: 1234567890000,
            expires_at: Some(1234567890000 + 3600000),
            last_used_ts: None,
            user_agent: None,
            ip_address: None,
            is_revoked: false,
        };

        assert_eq!(token.id, 1);
        assert_eq!(token.token, "access_token_abc123");
        assert_eq!(token.user_id, "@alice:example.com");
        assert!(token.device_id.is_some());
        assert!(!token.is_revoked);
    }

    #[test]
    fn test_refresh_token_struct() {
        let token = RefreshToken {
            id: 1,
            token_hash: "hash_abc123".to_string(),
            user_id: "@alice:example.com".to_string(),
            device_id: Some("DEVICE123".to_string()),
            access_token_id: Some("1".to_string()),
            scope: Some("read write".to_string()),
            created_ts: 1234567890000,
            expires_at: Some(1234567890000 + 86400000),
            last_used_ts: None,
            use_count: 0,
            is_revoked: false,
            revoked_reason: None,
            client_info: None,
            ip_address: None,
            user_agent: None,
        };

        assert_eq!(token.token_hash, "hash_abc123");
        assert!(!token.is_revoked);
        assert_eq!(token.use_count, 0);
    }

    #[test]
    fn test_token_blacklist_entry() {
        let entry = TokenBlacklistEntry {
            id: 1,
            token_hash: "hash_xyz".to_string(),
            token: Some("revoked_token".to_string()),
            token_type: "access".to_string(),
            user_id: Some("@alice:example.com".to_string()),
            is_revoked: true,
            reason: Some("User logout".to_string()),
            expires_at: None,
        };

        assert_eq!(entry.token_type, "access");
        assert!(entry.reason.is_some());
    }

    #[test]
    fn test_openid_token() {
        let token = OpenIdToken {
            id: 1,
            token: "openid_token_xyz".to_string(),
            user_id: "@alice:example.com".to_string(),
            device_id: Some("DEVICE123".to_string()),
            created_ts: 1234567890000,
            expires_at: 1234567890000 + 3600000,
            is_valid: true,
        };

        assert!(token.is_valid);
        assert!(token.expires_at > token.created_ts);
    }

    #[test]
    fn test_refresh_token_family() {
        let family = RefreshTokenFamily {
            id: 1,
            family_id: "family_abc123".to_string(),
            user_id: "@alice:example.com".to_string(),
            device_id: Some("DEVICE123".to_string()),
            created_ts: 1234567890000,
            last_refresh_ts: Some(1234567895000),
            refresh_count: 5,
            is_compromised: false,
            compromised_ts: None,
        };

        assert_eq!(family.family_id, "family_abc123");
        assert!(!family.is_compromised);
        assert_eq!(family.refresh_count, 5);
    }
}

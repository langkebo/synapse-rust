pub use synapse_storage::openid_token::*;

#[cfg(test)]
mod tests {
    use super::{CreateOpenIdTokenRequest, OpenIdToken, OpenIdTokenStorage};
    use sqlx::PgPool;
    use std::sync::Arc;

    #[test]
    fn root_openid_token_storage_reexport_keeps_constructor_shape() {
        let _ctor: fn(&Arc<PgPool>) -> OpenIdTokenStorage = OpenIdTokenStorage::new;
    }

    #[test]
    fn root_openid_token_request_remains_accessible() {
        let request = CreateOpenIdTokenRequest {
            token: "openid_token_123".to_string(),
            user_id: "@test:example.com".to_string(),
            device_id: Some("DEVICE123".to_string()),
            expires_at: 1234567890000,
        };
        assert_eq!(request.token, "openid_token_123");
        assert_eq!(request.user_id, "@test:example.com");
    }

    #[test]
    fn root_openid_token_type_remains_accessible() {
        let token = OpenIdToken {
            id: 1,
            token: "openid_token_123".to_string(),
            user_id: "@test:example.com".to_string(),
            device_id: Some("DEVICE123".to_string()),
            created_ts: 1234567890000,
            expires_at: 1234571490000,
            is_valid: true,
        };
        assert_eq!(token.token, "openid_token_123");
        assert!(token.is_valid);
    }
}

#[cfg(test)]
mod tests {
    use synapse_rust::common::config::IdentityConfig;
    use synapse_rust::common::config::OidcConfig;

    fn create_test_oidc_config() -> OidcConfig {
        OidcConfig {
            enabled: true,
            issuer: "https://accounts.google.com".to_string(),
            client_id: "test_client_id".to_string(),
            client_secret: Some("test_client_secret".to_string()),
            scopes: vec![
                "openid".to_string(),
                "profile".to_string(),
                "email".to_string(),
            ],
            attribute_mapping: Default::default(),
            callback_url: Some(
                "http://localhost:28008/_matrix/client/v3/oidc/callback".to_string(),
            ),
            allow_existing_users: true,
            block_unknown_users: false,
            authorization_endpoint: Some(
                "https://accounts.google.com/o/oauth2/v2/auth".to_string(),
            ),
            token_endpoint: Some("https://oauth2.googleapis.com/token".to_string()),
            userinfo_endpoint: Some("https://openidconnect.googleapis.com/v1/userinfo".to_string()),
            jwks_uri: Some("https://www.googleapis.com/oauth2/v3/certs".to_string()),
            timeout: 30,
        }
    }

    #[test]
    fn test_oidc_config_enabled() {
        let config = create_test_oidc_config();
        assert!(config.enabled);
    }

    #[test]
    fn test_oidc_config_disabled() {
        let config = OidcConfig::default();
        assert!(!config.enabled);
    }

    #[test]
    fn test_oidc_config_issuer() {
        let config = create_test_oidc_config();
        assert_eq!(config.issuer, "https://accounts.google.com");
    }

    #[test]
    fn test_oidc_config_client_id() {
        let config = create_test_oidc_config();
        assert_eq!(config.client_id, "test_client_id");
    }

    #[test]
    fn test_oidc_config_client_secret() {
        let config = create_test_oidc_config();
        assert!(config.client_secret.is_some());
        assert_eq!(config.client_secret.as_deref(), Some("test_client_secret"));
    }

    #[test]
    fn test_oidc_config_scopes() {
        let config = create_test_oidc_config();
        assert!(config.scopes.contains(&"openid".to_string()));
        assert!(config.scopes.contains(&"profile".to_string()));
        assert!(config.scopes.contains(&"email".to_string()));
    }

    #[test]
    fn test_oidc_config_callback_url() {
        let config = create_test_oidc_config();
        assert!(config.callback_url.is_some());
    }

    #[test]
    fn test_oidc_config_allow_existing_users() {
        let config = create_test_oidc_config();
        assert!(config.allow_existing_users);
    }

    #[test]
    fn test_oidc_config_block_unknown_users() {
        let config = create_test_oidc_config();
        assert!(!config.block_unknown_users);
    }

    #[test]
    fn test_oidc_config_endpoints() {
        let config = create_test_oidc_config();
        assert!(config.authorization_endpoint.is_some());
        assert!(config.token_endpoint.is_some());
        assert!(config.userinfo_endpoint.is_some());
        assert!(config.jwks_uri.is_some());
    }

    #[test]
    fn test_oidc_config_timeout() {
        let config = create_test_oidc_config();
        assert_eq!(config.timeout, 30);
    }

    #[test]
    fn test_identity_config_default_trusted_servers() {
        let config = IdentityConfig::default();
        assert!(!config.trusted_servers.is_empty());
        assert!(config.trusted_servers.contains(&"vector.im".to_string()));
        assert!(config.trusted_servers.contains(&"matrix.org".to_string()));
    }

    #[test]
    fn test_identity_config_custom_trusted_servers() {
        let config = IdentityConfig {
            trusted_servers: vec!["custom.example.com".to_string()],
        };
        assert_eq!(config.trusted_servers.len(), 1);
        assert!(config
            .trusted_servers
            .contains(&"custom.example.com".to_string()));
    }

    #[test]
    fn test_oidc_discovery_document_structure() {
        let config = create_test_oidc_config();
        let discovery = serde_json::json!({
            "issuer": config.issuer,
            "authorization_endpoint": config.authorization_endpoint.unwrap(),
            "token_endpoint": config.token_endpoint.unwrap(),
            "userinfo_endpoint": config.userinfo_endpoint.unwrap(),
            "jwks_uri": config.jwks_uri.unwrap(),
            "response_types_supported": ["code"],
            "subject_types_supported": ["public"],
            "id_token_signing_alg_values_supported": ["RS256"],
        });

        assert!(discovery.get("issuer").is_some());
        assert!(discovery.get("authorization_endpoint").is_some());
        assert!(discovery.get("token_endpoint").is_some());
        assert!(discovery.get("userinfo_endpoint").is_some());
        assert!(discovery.get("jwks_uri").is_some());
        assert!(discovery.get("response_types_supported").is_some());
        assert!(discovery.get("subject_types_supported").is_some());
        assert!(discovery
            .get("id_token_signing_alg_values_supported")
            .is_some());
    }

    #[test]
    fn test_oidc_pkce_code_verifier_generation() {
        use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
        let mut random_bytes = [0u8; 32];
        rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut random_bytes);
        let code_verifier = URL_SAFE_NO_PAD.encode(random_bytes);
        assert!(!code_verifier.is_empty());
        assert!(code_verifier.len() >= 43);
    }

    #[test]
    fn test_oidc_pkce_code_challenge_generation() {
        use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
        use sha2::{Digest, Sha256};

        let code_verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        let mut hasher = Sha256::new();
        hasher.update(code_verifier.as_bytes());
        let hash = hasher.finalize();
        let code_challenge = URL_SAFE_NO_PAD.encode(hash);

        assert!(!code_challenge.is_empty());
        assert_ne!(code_challenge, code_verifier);
    }

    #[test]
    fn test_oidc_state_parameter_format() {
        use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
        let mut random_bytes = [0u8; 32];
        rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut random_bytes);
        let state = URL_SAFE_NO_PAD.encode(random_bytes);
        assert!(!state.is_empty());
        assert!(state.len() >= 43);
    }

    #[test]
    fn test_oidc_token_response_structure() {
        let token_response = serde_json::json!({
            "access_token": "test_access_token",
            "token_type": "Bearer",
            "expires_in": 3600,
            "refresh_token": "test_refresh_token",
            "id_token": "test_id_token",
            "scope": "openid profile email"
        });

        assert!(token_response.get("access_token").is_some());
        assert!(token_response.get("token_type").is_some());
        assert!(token_response.get("id_token").is_some());
    }

    #[test]
    fn test_oidc_userinfo_structure() {
        let userinfo = serde_json::json!({
            "sub": "1234567890",
            "name": "Test User",
            "given_name": "Test",
            "family_name": "User",
            "preferred_username": "testuser",
            "email": "test@example.com",
            "email_verified": true,
            "picture": "https://example.com/avatar.png",
            "locale": "en"
        });

        assert!(userinfo.get("sub").is_some());
        assert!(userinfo.get("email").is_some());
        assert!(userinfo.get("preferred_username").is_some());
    }

    #[test]
    fn test_oidc_jwks_structure() {
        let jwks = serde_json::json!({
            "keys": [{
                "kty": "RSA",
                "use": "sig",
                "kid": "test-key-id",
                "alg": "RS256",
                "n": "test_n_value",
                "e": "AQAB"
            }]
        });

        assert!(jwks.get("keys").is_some());
        let keys = jwks.get("keys").unwrap().as_array().unwrap();
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0]["kty"], "RSA");
        assert_eq!(keys[0]["alg"], "RS256");
    }

    #[test]
    fn test_oidc_authorization_url_construction() {
        let config = create_test_oidc_config();
        let auth_url = format!(
            "{}?response_type=code&client_id={}&redirect_uri={}&scope={}&state=test_state",
            config.authorization_endpoint.unwrap(),
            config.client_id,
            urlencoding::encode(config.callback_url.as_deref().unwrap_or("")),
            urlencoding::encode(&config.scopes.join(" "))
        );

        assert!(auth_url.contains("response_type=code"));
        assert!(auth_url.contains("client_id=test_client_id"));
        assert!(auth_url.contains("state=test_state"));
    }
}

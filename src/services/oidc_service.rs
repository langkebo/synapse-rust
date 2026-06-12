pub use synapse_services::oidc_service::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::config::OidcAttributeMapping;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn create_test_config() -> OidcConfig {
        OidcConfig {
            enabled: true,
            issuer: "https://accounts.example.com".to_string(),
            client_id: "test-client-id".to_string(),
            client_secret: Some("test-client-secret".to_string()),
            scopes: vec!["openid".to_string(), "profile".to_string(), "email".to_string()],
            attribute_mapping: OidcAttributeMapping {
                localpart: Some("preferred_username".to_string()),
                displayname: Some("name".to_string()),
                email: Some("email".to_string()),
            },
            callback_url: Some("https://matrix.example.com/_matrix/client/r0/login/sso/redirect".to_string()),
            allow_existing_users: true,
            block_unknown_users: false,
            authorization_endpoint: None,
            token_endpoint: None,
            userinfo_endpoint: None,
            jwks_uri: None,
            timeout: 10,
            user_mapping_provider: None,
        }
    }

    fn create_test_service() -> OidcService {
        let config = Arc::new(create_test_config());
        OidcService::new(config)
    }

    #[test]
    fn test_oidc_config_enabled() {
        let config = create_test_config();
        assert!(config.is_enabled());
    }

    #[test]
    fn test_oidc_config_disabled() {
        let config = OidcConfig::default();
        assert!(!config.is_enabled());
    }

    #[test]
    fn test_service_enabled() {
        let service = create_test_service();
        assert!(service.is_enabled());
    }

    #[test]
    fn test_generate_state() {
        let state = OidcService::generate_state();
        assert_eq!(state.len(), 32);
        assert!(state.chars().all(|c| c.is_alphanumeric()));
    }

    #[test]
    fn test_get_authorization_url() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let service = create_test_service();
        let url = rt
            .block_on(service.get_authorization_url("test-state", "https://matrix.example.com/callback", None, None))
            .unwrap();

        assert!(url.contains("client_id=test-client-id"));
        assert!(url.contains("response_type=code"));
        assert!(url.contains("state=test-state"));
        assert!(url.contains("scope="));
    }

    #[test]
    fn test_map_user() {
        let service = create_test_service();
        let user_info = OidcUserInfo {
            sub: "user123".to_string(),
            name: Some("Test User".to_string()),
            given_name: Some("Test".to_string()),
            family_name: Some("User".to_string()),
            preferred_username: Some("testuser".to_string()),
            email: Some("test@example.com".to_string()),
            email_verified: Some(true),
            picture: Some("https://example.com/avatar.png".to_string()),
            locale: Some("en".to_string()),
        };

        let user = service.map_user(&user_info);

        assert_eq!(user.subject, "user123");
        assert_eq!(user.localpart, "testuser");
        assert_eq!(user.displayname, Some("Test User".to_string()));
        assert_eq!(user.email, Some("test@example.com".to_string()));
    }

    #[test]
    fn test_map_user_default_localpart() {
        let mut config = create_test_config();
        config.attribute_mapping.localpart = None;
        let service = OidcService::new(Arc::new(config));

        let user_info = OidcUserInfo {
            sub: "user123".to_string(),
            name: None,
            given_name: None,
            family_name: None,
            preferred_username: None,
            email: None,
            email_verified: None,
            picture: None,
            locale: None,
        };

        let user = service.map_user(&user_info);
        assert_eq!(user.localpart, "user123");
    }

    #[tokio::test]
    async fn test_exchange_code_sends_pkce_verifier() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "access-token",
                "token_type": "Bearer",
                "expires_in": 3600,
                "refresh_token": "refresh-token",
                "scope": "openid profile email"
            })))
            .mount(&server)
            .await;

        let mut config = create_test_config();
        config.issuer = server.uri();
        config.token_endpoint = Some(format!("{}/token", server.uri()));
        let service = OidcService::new(Arc::new(config));

        let response = service
            .exchange_code("auth-code", "https://matrix.example.com/callback", Some("verifier-123"))
            .await
            .unwrap();

        assert_eq!(response.access_token, "access-token");

        let requests = server.received_requests().await.unwrap();
        let body = String::from_utf8_lossy(&requests[0].body);
        assert!(body.contains("code_verifier=verifier-123"));
    }
}

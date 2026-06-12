#[cfg(feature = "builtin-oidc")]
pub use synapse_services::builtin_oidc_provider::*;

#[cfg(test)]
#[cfg(feature = "builtin-oidc")]
mod tests {
    use super::*;

    // ========== compute_at_hash tests ==========

    #[test]
    fn test_compute_at_hash_known_value() {
        // at_hash is BASE64URL( left-128-bit( SHA256(access_token) ) )
        let hash = BuiltinOidcProvider::compute_at_hash("test_access_token");
        // Should be a valid base64url string of 16 bytes = ~22 chars without padding
        assert!(!hash.is_empty());
        assert!(!hash.contains('='), "at_hash should be base64url without padding");
        // Should be decodable
        use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
        let decoded = URL_SAFE_NO_PAD.decode(&hash).unwrap();
        assert_eq!(decoded.len(), 16, "at_hash should decode to 16 bytes (left 128 bits of SHA256)");
    }

    #[test]
    fn test_compute_at_hash_deterministic() {
        let hash1 = BuiltinOidcProvider::compute_at_hash("same_token");
        let hash2 = BuiltinOidcProvider::compute_at_hash("same_token");
        assert_eq!(hash1, hash2, "compute_at_hash should be deterministic");
    }

    #[test]
    fn test_compute_at_hash_different_inputs() {
        let hash1 = BuiltinOidcProvider::compute_at_hash("token_a");
        let hash2 = BuiltinOidcProvider::compute_at_hash("token_b");
        assert_ne!(hash1, hash2, "Different inputs should produce different hashes");
    }

    #[test]
    fn test_compute_at_hash_empty() {
        let hash = BuiltinOidcProvider::compute_at_hash("");
        assert!(!hash.is_empty());
    }

    // ========== OidcDiscoveryDocument tests ==========

    #[test]
    fn test_oidc_discovery_document_defaults() {
        let doc = OidcDiscoveryDocument {
            issuer: "https://example.com".to_string(),
            authorization_endpoint: "https://example.com/auth".to_string(),
            token_endpoint: "https://example.com/token".to_string(),
            userinfo_endpoint: "https://example.com/userinfo".to_string(),
            jwks_uri: "https://example.com/jwks".to_string(),
            registration_endpoint: None,
            revocation_endpoint: None,
            end_session_endpoint: None,
            response_types_supported: vec!["code".to_string()],
            subject_types_supported: vec!["public".to_string()],
            id_token_signing_alg_values_supported: vec!["RS256".to_string()],
            scopes_supported: vec!["openid".to_string()],
            token_endpoint_auth_methods_supported: vec!["client_secret_basic".to_string()],
            claims_supported: vec!["sub".to_string()],
            code_challenge_methods_supported: vec!["S256".to_string()],
        };
        assert_eq!(doc.issuer, "https://example.com");
        assert_eq!(doc.response_types_supported, vec!["code"]);
        assert!(doc.registration_endpoint.is_none());
    }

    // ========== OidcTokenRequest tests ==========

    #[test]
    fn test_oidc_token_request_authorization_code() {
        let req = OidcTokenRequest {
            grant_type: "authorization_code".to_string(),
            code: Some("auth_code_123".to_string()),
            redirect_uri: Some("https://app.example.com/callback".to_string()),
            client_id: Some("client_123".to_string()),
            code_verifier: Some("verifier_123".to_string()),
            refresh_token: None,
            scope: None,
        };
        assert_eq!(req.grant_type, "authorization_code");
        assert_eq!(req.code, Some("auth_code_123".to_string()));
        assert_eq!(req.code_verifier, Some("verifier_123".to_string()));
    }

    #[test]
    fn test_oidc_token_request_refresh_token() {
        let req = OidcTokenRequest {
            grant_type: "refresh_token".to_string(),
            code: None,
            redirect_uri: None,
            client_id: Some("client_123".to_string()),
            code_verifier: None,
            refresh_token: Some("refresh_token_abc".to_string()),
            scope: None,
        };
        assert_eq!(req.grant_type, "refresh_token");
        assert_eq!(req.refresh_token, Some("refresh_token_abc".to_string()));
    }

    // ========== OidcTokenResponse tests ==========

    #[test]
    fn test_oidc_token_response() {
        let resp = OidcTokenResponse {
            access_token: "access_123".to_string(),
            token_type: "Bearer".to_string(),
            expires_in: 3600,
            id_token: "id_456".to_string(),
            refresh_token: Some("refresh_789".to_string()),
            scope: Some("openid profile".to_string()),
        };
        assert_eq!(resp.access_token, "access_123");
        assert_eq!(resp.token_type, "Bearer");
        assert_eq!(resp.expires_in, 3600);
        assert_eq!(resp.id_token, "id_456");
        assert_eq!(resp.refresh_token, Some("refresh_789".to_string()));
        assert_eq!(resp.scope, Some("openid profile".to_string()));
    }

    // ========== OidcUserInfo tests ==========

    #[test]
    fn test_oidc_user_info() {
        let info = OidcUserInfo {
            sub: "user_123".to_string(),
            name: Some("Test User".to_string()),
            given_name: Some("Test".to_string()),
            family_name: Some("User".to_string()),
            preferred_username: Some("testuser".to_string()),
            email: Some("test@example.com".to_string()),
            email_verified: true,
            picture: None,
        };
        assert_eq!(info.sub, "user_123");
        assert_eq!(info.name, Some("Test User".to_string()));
        assert!(info.email_verified);
        assert!(info.picture.is_none());
    }

    // ========== Jwks/Jwk tests ==========

    #[test]
    fn test_jwk_fields() {
        let jwk = Jwk {
            kty: "RSA".to_string(),
            use_: "sig".to_string(),
            kid: "key_123".to_string(),
            alg: "RS256".to_string(),
            n: "base64url_n".to_string(),
            e: "AQAB".to_string(),
        };
        assert_eq!(jwk.kty, "RSA");
        assert_eq!(jwk.use_, "sig");
        assert_eq!(jwk.alg, "RS256");
    }

    #[test]
    fn test_jwks() {
        let jwks = Jwks {
            keys: vec![
                Jwk {
                    kty: "RSA".to_string(),
                    use_: "sig".to_string(),
                    kid: "key_1".to_string(),
                    alg: "RS256".to_string(),
                    n: "n1".to_string(),
                    e: "AQAB".to_string(),
                },
            ],
        };
        assert_eq!(jwks.keys.len(), 1);
        assert_eq!(jwks.keys[0].kid, "key_1");
    }

    // ========== JwtClaims tests ==========

    #[test]
    fn test_jwt_claims() {
        let claims = JwtClaims {
            iss: "https://example.com".to_string(),
            sub: "user_123".to_string(),
            aud: "client_456".to_string(),
            exp: 1700000000,
            iat: 1699996400,
            nonce: Some("nonce_123".to_string()),
            at_hash: Some("hash_abc".to_string()),
            email: Some("test@example.com".to_string()),
            email_verified: true,
            name: Some("Test User".to_string()),
            picture: None,
        };
        assert_eq!(claims.iss, "https://example.com");
        assert_eq!(claims.sub, "user_123");
        assert_eq!(claims.aud, "client_456");
        assert_eq!(claims.exp, 1700000000);
        assert_eq!(claims.iat, 1699996400);
    }

    // ========== AuthSession tests ==========

    #[test]
    fn test_auth_session() {
        let session = AuthSession {
            code: "code_123".to_string(),
            client_id: "client_456".to_string(),
            redirect_uri: "https://app.example.com/callback".to_string(),
            scope: "openid profile".to_string(),
            state: "state_789".to_string(),
            nonce: Some("nonce_012".to_string()),
            code_challenge: Some("challenge_345".to_string()),
            user_id: "user_123".to_string(),
            created_at: std::time::Instant::now(),
        };
        assert_eq!(session.code, "code_123");
        assert_eq!(session.client_id, "client_456");
        assert_eq!(session.user_id, "user_123");
        assert_eq!(session.code_challenge, Some("challenge_345".to_string()));
    }

    // ========== RefreshToken (OIDC) tests ==========

    #[test]
    fn test_oidc_refresh_token() {
        let token = RefreshToken {
            user_id: "user_123".to_string(),
            client_id: "client_456".to_string(),
            scope: "openid".to_string(),
            created_at: std::time::Instant::now(),
        };
        assert_eq!(token.user_id, "user_123");
        assert_eq!(token.client_id, "client_456");
        assert_eq!(token.scope, "openid");
    }

    // ========== AuthorizeRequest tests ==========

    #[test]
    fn test_authorize_request() {
        let req = AuthorizeRequest {
            client_id: "client_123".to_string(),
            redirect_uri: "https://app.example.com/callback".to_string(),
            scope: "openid".to_string(),
            state: "state_456".to_string(),
            nonce: Some("nonce_789".to_string()),
            code_verifier: Some("challenge_012".to_string()),
            username: "testuser".to_string(),
            password: "password123".to_string(),
        };
        assert_eq!(req.client_id, "client_123");
        assert_eq!(req.username, "testuser");
        assert_eq!(req.password, "password123");
        assert_eq!(req.code_verifier, Some("challenge_012".to_string()));
    }
}

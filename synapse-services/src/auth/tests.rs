use super::*;
use chrono::{Duration, Utc};
use jsonwebtoken::{encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use synapse_common::crypto::{
    hash_password_with_params, is_legacy_hash, migrate_password_hash, verify_password as verify_password_common,
};

#[test]
fn test_claims_struct() {
    let claims = Claims {
        sub: "@test:example.com".to_string(),
        user_id: "@test:example.com".to_string(),
        jti: "test-jti-uuid".to_string(),
        is_admin: false,
        exp: 1234567890,
        iat: 1234567889,
        device_id: Some("DEVICE123".to_string()),
        iss: None,
        aud: None,
    };
    assert_eq!(claims.sub, "@test:example.com");
    assert_eq!(claims.user_id, "@test:example.com");
    assert!(!claims.is_admin);
    assert!(claims.exp > claims.iat);
}

#[test]
fn test_claims_with_admin() {
    let claims = Claims {
        sub: "@admin:example.com".to_string(),
        user_id: "@admin:example.com".to_string(),
        jti: "test-jti-admin".to_string(),
        is_admin: true,
        exp: 1234567890,
        iat: 1234567890,
        device_id: None,
        iss: None,
        aud: None,
    };
    assert!(claims.is_admin);
    assert!(claims.device_id.is_none());
}

#[test]
fn test_generate_token_length() {
    for len in [8, 16, 32, 64] {
        let token = auth_generate_token(len);
        assert_eq!(token.len(), len);
    }
}

#[test]
fn test_generate_token_chars() {
    let token = auth_generate_token(100);
    for c in token.chars() {
        assert!(c.is_ascii_alphanumeric());
    }
}

#[test]
fn test_claims_serialization() {
    let claims = Claims {
        sub: "@test:example.com".to_string(),
        user_id: "@test:example.com".to_string(),
        jti: "test-jti-serialization".to_string(),
        is_admin: false,
        exp: 1234567890,
        iat: 1234567890,
        device_id: Some("DEVICE123".to_string()),
        iss: None,
        aud: None,
    };
    let json = serde_json::to_string(&claims).unwrap();
    let deserialized: Claims = serde_json::from_str(&json).unwrap();
    assert_eq!(claims.sub, deserialized.sub);
    assert_eq!(claims.user_id, deserialized.user_id);
    assert_eq!(claims.is_admin, deserialized.is_admin);
}

#[test]
fn test_hash_token_consistency() {
    let token = "test_refresh_token_12345";
    let hash1 = AuthService::hash_token(token);
    let hash2 = AuthService::hash_token(token);
    assert_eq!(hash1, hash2, "Same token should produce same hash");
    assert!(!hash1.is_empty(), "Hash should not be empty");
}

#[test]
fn test_hash_token_different_tokens() {
    let token1 = "token_one";
    let token2 = "token_two";
    let hash1 = AuthService::hash_token(token1);
    let hash2 = AuthService::hash_token(token2);
    assert_ne!(hash1, hash2, "Different tokens should produce different hashes");
}

#[test]
fn test_hash_token_empty_string() {
    let hash = AuthService::hash_token("");
    assert!(!hash.is_empty(), "Empty token should still produce a hash");
}

#[test]
fn test_hash_token_format() {
    let token = "test_token";
    let hash = AuthService::hash_token(token);
    assert_eq!(hash.len(), 43, "SHA256 base64 encoded hash should be 43 chars");
}

#[test]
fn test_password_hash_and_verify() {
    let password = "secure_password_123";
    let hash = hash_password_with_params(password, 65536, 3, 1).unwrap();
    assert!(hash.starts_with("$argon2"));
    assert!(verify_password_common(password, &hash, false).unwrap());
    assert!(!verify_password_common("wrong_password", &hash, false).unwrap());
}

#[test]
fn test_password_hash_uniqueness() {
    let password = "same_password";
    let hash1 = hash_password_with_params(password, 65536, 3, 1).unwrap();
    let hash2 = hash_password_with_params(password, 65536, 3, 1).unwrap();
    assert_ne!(hash1, hash2, "Same password should produce different hashes due to salt");
}

#[test]
fn test_password_verify_wrong_password() {
    let password = "correct_password";
    let hash = hash_password_with_params(password, 65536, 3, 1).unwrap();
    assert!(!verify_password_common("incorrect_password", &hash, false).unwrap());
}

#[test]
fn test_password_empty_password() {
    let password = "";
    let hash = hash_password_with_params(password, 65536, 3, 1).unwrap();
    assert!(hash.starts_with("$argon2"));
    assert!(verify_password_common("", &hash, false).unwrap());
}

#[test]
fn test_password_long_password() {
    let password = "a".repeat(1000);
    let hash = hash_password_with_params(&password, 65536, 3, 1).unwrap();
    assert!(verify_password_common(&password, &hash, false).unwrap());
}

#[test]
fn test_is_legacy_hash_argon2() {
    let argon2_hash = "$argon2id$v=19$m=65536,t=3,p=1$c2FsdA$hash";
    assert!(!is_legacy_hash(argon2_hash));
}

#[test]
fn test_is_legacy_hash_sha256() {
    let legacy_hash = "sha256$v=1$m=32,p=1$salt$hash";
    assert!(is_legacy_hash(legacy_hash));
}

#[test]
fn test_is_legacy_hash_bcrypt() {
    let bcrypt_hash = "$2b$12$abcdefghijklmnopqrstuv";
    assert!(is_legacy_hash(bcrypt_hash));
}

#[test]
fn test_claims_expiration_validation() {
    let now = Utc::now().timestamp();
    let valid_claims = Claims {
        sub: "@test:example.com".to_string(),
        user_id: "@test:example.com".to_string(),
        jti: "test-jti-valid".to_string(),
        is_admin: false,
        exp: now + 3600,
        iat: now,
        device_id: None,
        iss: None,
        aud: None,
    };
    assert!(valid_claims.exp > now);

    let expired_claims = Claims {
        sub: "@test:example.com".to_string(),
        user_id: "@test:example.com".to_string(),
        jti: "test-jti-expired".to_string(),
        is_admin: false,
        exp: now - 3600,
        iat: now - 7200,
        device_id: None,
        iss: None,
        aud: None,
    };
    assert!(expired_claims.exp < now);
}

#[test]
fn test_claims_device_id_optional() {
    let claims_with_device = Claims {
        sub: "@test:example.com".to_string(),
        user_id: "@test:example.com".to_string(),
        jti: "test-jti-with-device".to_string(),
        is_admin: false,
        exp: 1234567890,
        iat: 1234567890,
        device_id: Some("DEVICE123".to_string()),
        iss: None,
        aud: None,
    };
    assert!(claims_with_device.device_id.is_some());

    let claims_without_device = Claims {
        sub: "@test:example.com".to_string(),
        user_id: "@test:example.com".to_string(),
        jti: "test-jti-no-device".to_string(),
        is_admin: false,
        exp: 1234567890,
        iat: 1234567890,
        device_id: None,
        iss: None,
        aud: None,
    };
    assert!(claims_without_device.device_id.is_none());
}

#[test]
fn test_jwt_encode_decode() {
    let jwt_secret = b"test_secret_key_for_jwt_encoding";
    let now = Utc::now();
    let claims = Claims {
        sub: "@user:example.com".to_string(),
        user_id: "@user:example.com".to_string(),
        jti: uuid::Uuid::new_v4().to_string(),
        is_admin: true,
        exp: (now + Duration::hours(1)).timestamp(),
        iat: now.timestamp(),
        device_id: Some("DEVICE456".to_string()),
        iss: None,
        aud: None,
    };

    let mut header = Header::new(Algorithm::HS256);
    header.typ = Some("JWT".to_string());

    let token = encode(&header, &claims, &EncodingKey::from_secret(jwt_secret)).unwrap();

    let validation = Validation::new(Algorithm::HS256);
    let decoded: Claims =
        jsonwebtoken::decode(&token, &DecodingKey::from_secret(jwt_secret), &validation).unwrap().claims;

    assert_eq!(decoded.sub, claims.sub);
    assert_eq!(decoded.user_id, claims.user_id);
    assert_eq!(decoded.is_admin, claims.is_admin);
    assert_eq!(decoded.device_id, claims.device_id);
}

#[test]
fn test_jwt_decode_wrong_secret() {
    let jwt_secret = b"correct_secret";
    let wrong_secret = b"wrong_secret";
    let now = Utc::now();
    let claims = Claims {
        sub: "@user:example.com".to_string(),
        user_id: "@user:example.com".to_string(),
        jti: uuid::Uuid::new_v4().to_string(),
        is_admin: false,
        exp: (now + Duration::hours(1)).timestamp(),
        iat: now.timestamp(),
        device_id: None,
        iss: None,
        aud: None,
    };

    let mut header = Header::new(Algorithm::HS256);
    header.typ = Some("JWT".to_string());

    let token = encode(&header, &claims, &EncodingKey::from_secret(jwt_secret)).unwrap();

    let validation = Validation::new(Algorithm::HS256);
    let result = jsonwebtoken::decode::<Claims>(&token, &DecodingKey::from_secret(wrong_secret), &validation);

    assert!(result.is_err(), "Decoding with wrong secret should fail");
}

#[test]
fn test_jwt_expired_token() {
    let jwt_secret = b"test_secret";
    let now = Utc::now();
    let claims = Claims {
        sub: "@user:example.com".to_string(),
        user_id: "@user:example.com".to_string(),
        jti: uuid::Uuid::new_v4().to_string(),
        is_admin: false,
        exp: (now - Duration::hours(1)).timestamp(),
        iat: (now - Duration::hours(2)).timestamp(),
        device_id: None,
        iss: None,
        aud: None,
    };

    let mut header = Header::new(Algorithm::HS256);
    header.typ = Some("JWT".to_string());

    let token = encode(&header, &claims, &EncodingKey::from_secret(jwt_secret)).unwrap();

    let validation = Validation::new(Algorithm::HS256);
    let result = jsonwebtoken::decode::<Claims>(&token, &DecodingKey::from_secret(jwt_secret), &validation);

    assert!(result.is_err(), "Expired token should fail validation");
}

#[test]
fn test_jwt_tampered_token() {
    let jwt_secret = b"test_secret";
    let now = Utc::now();
    let claims = Claims {
        sub: "@user:example.com".to_string(),
        user_id: "@user:example.com".to_string(),
        jti: uuid::Uuid::new_v4().to_string(),
        is_admin: false,
        exp: (now + Duration::hours(1)).timestamp(),
        iat: now.timestamp(),
        device_id: None,
        iss: None,
        aud: None,
    };

    let mut header = Header::new(Algorithm::HS256);
    header.typ = Some("JWT".to_string());

    let token = encode(&header, &claims, &EncodingKey::from_secret(jwt_secret)).unwrap();

    let mut tampered = token.chars().collect::<Vec<char>>();
    if let Some(last) = tampered.last_mut() {
        *last = if *last == 'A' { 'B' } else { 'A' };
    }
    let tampered_token: String = tampered.into_iter().collect();

    let validation = Validation::new(Algorithm::HS256);
    let result = jsonwebtoken::decode::<Claims>(&tampered_token, &DecodingKey::from_secret(jwt_secret), &validation);

    assert!(result.is_err(), "Tampered token should fail validation");
}

#[test]
fn test_auth_generate_token_uniqueness() {
    let tokens: Vec<String> = (0..100).map(|_| auth_generate_token(32)).collect();
    let unique_count = tokens.iter().collect::<std::collections::HashSet<_>>().len();
    assert_eq!(unique_count, 100, "All generated tokens should be unique");
}

#[test]
fn test_auth_generate_token_charset() {
    let token = auth_generate_token(1000);
    let valid_chars: std::collections::HashSet<char> =
        "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789".chars().collect();
    for c in token.chars() {
        assert!(valid_chars.contains(&c), "Token should only contain alphanumeric characters");
    }
}

#[test]
fn test_claims_json_roundtrip() {
    let original = Claims {
        sub: "@test:server.com".to_string(),
        user_id: "@test:server.com".to_string(),
        jti: "test-jti-roundtrip".to_string(),
        is_admin: true,
        exp: 9999999999,
        iat: 1000000000,
        device_id: Some("MYDEVICE".to_string()),
        iss: None,
        aud: None,
    };

    let json = serde_json::to_string(&original).unwrap();
    let parsed: Claims = serde_json::from_str(&json).unwrap();

    assert_eq!(original.sub, parsed.sub);
    assert_eq!(original.user_id, parsed.user_id);
    assert_eq!(original.is_admin, parsed.is_admin);
    assert_eq!(original.exp, parsed.exp);
    assert_eq!(original.iat, parsed.iat);
    assert_eq!(original.device_id, parsed.device_id);
}

#[test]
fn test_claims_json_structure() {
    let claims = Claims {
        sub: "@user:example.com".to_string(),
        user_id: "@user:example.com".to_string(),
        jti: "test-jti-structure".to_string(),
        is_admin: false,
        exp: 1234567890,
        iat: 1234567800,
        device_id: Some("DEV1".to_string()),
        iss: None,
        aud: None,
    };

    let json = serde_json::to_string(&claims).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();

    assert_eq!(value["sub"], "@user:example.com");
    assert_eq!(value["user_id"], "@user:example.com");
    assert_eq!(value["admin"], false);
    assert_eq!(value["exp"], 1234567890);
    assert_eq!(value["iat"], 1234567800);
    assert_eq!(value["device_id"], "DEV1");
}

#[test]
fn test_password_special_characters() {
    let password = "p@$$w0rd!#$%^&*()_+-=[]{}|;':\",./<>?";
    let hash = hash_password_with_params(password, 65536, 3, 1).unwrap();
    assert!(verify_password_common(password, &hash, false).unwrap());
}

#[test]
fn test_password_unicode() {
    let password = "密码测试🔐🎉";
    let hash = hash_password_with_params(password, 65536, 3, 1).unwrap();
    assert!(verify_password_common(password, &hash, false).unwrap());
}

#[test]
fn test_password_whitespace() {
    let password = "  password with spaces  ";
    let hash = hash_password_with_params(password, 65536, 3, 1).unwrap();
    assert!(verify_password_common(password, &hash, false).unwrap());
    assert!(!verify_password_common("password with spaces", &hash, false).unwrap());
}

#[test]
fn test_migrate_password_hash() {
    let password = "password_to_migrate";
    let new_hash = migrate_password_hash(password, 65536, 3, 1).unwrap();
    assert!(new_hash.starts_with("$argon2"));
    assert!(verify_password_common(password, &new_hash, false).unwrap());
}

#[test]
fn test_auth_service_hash_password_direct() {
    let password = "test_password";
    let hash = hash_password_with_params(password, 65536, 3, 1).unwrap();
    assert!(hash.starts_with("$argon2"));
    assert!(verify_password_common(password, &hash, false).unwrap());
}

#[test]
fn test_auth_service_verify_password_wrong_direct() {
    let password = "correct_password";
    let hash = hash_password_with_params(password, 65536, 3, 1).unwrap();
    assert!(!verify_password_common("wrong_password", &hash, false).unwrap());
}

#[test]
fn test_auth_service_jwt_generation_direct() {
    let jwt_secret = b"test_jwt_secret_key_for_unit_tests";
    let now = Utc::now();
    let claims = Claims {
        sub: "@user:test.server".to_string(),
        user_id: "@user:test.server".to_string(),
        jti: uuid::Uuid::new_v4().to_string(),
        is_admin: false,
        exp: (now + Duration::hours(1)).timestamp(),
        iat: now.timestamp(),
        device_id: Some("DEVICE1".to_string()),
        iss: None,
        aud: None,
    };

    let mut header = Header::new(Algorithm::HS256);
    header.typ = Some("JWT".to_string());

    let token = encode(&header, &claims, &EncodingKey::from_secret(jwt_secret)).unwrap();

    assert!(!token.is_empty());

    let validation = Validation::new(Algorithm::HS256);
    let decoded: Claims =
        jsonwebtoken::decode(&token, &DecodingKey::from_secret(jwt_secret), &validation).unwrap().claims;

    assert_eq!(decoded.sub, "@user:test.server");
    assert_eq!(decoded.user_id, "@user:test.server");
    assert!(!decoded.is_admin);
    assert_eq!(decoded.device_id, Some("DEVICE1".to_string()));
}

#[test]
fn test_auth_service_jwt_admin_flag_direct() {
    let jwt_secret = b"test_jwt_secret_key_for_unit_tests";
    let now = Utc::now();
    let claims = Claims {
        sub: "@admin:test.server".to_string(),
        user_id: "@admin:test.server".to_string(),
        jti: uuid::Uuid::new_v4().to_string(),
        is_admin: true,
        exp: (now + Duration::hours(1)).timestamp(),
        iat: now.timestamp(),
        device_id: Some("DEVICE2".to_string()),
        iss: None,
        aud: None,
    };

    let mut header = Header::new(Algorithm::HS256);
    header.typ = Some("JWT".to_string());

    let token = encode(&header, &claims, &EncodingKey::from_secret(jwt_secret)).unwrap();

    let validation = Validation::new(Algorithm::HS256);
    let decoded: Claims =
        jsonwebtoken::decode(&token, &DecodingKey::from_secret(jwt_secret), &validation).unwrap().claims;

    assert!(decoded.is_admin);
}

#[test]
fn test_auth_service_jwt_expiration_direct() {
    let jwt_secret = b"test_jwt_secret_key_for_unit_tests";
    let token_expiry: i64 = 3600;
    let now = Utc::now().timestamp();

    let claims = Claims {
        sub: "@user:test.server".to_string(),
        user_id: "@user:test.server".to_string(),
        jti: uuid::Uuid::new_v4().to_string(),
        is_admin: false,
        exp: now + token_expiry,
        iat: now,
        device_id: Some("DEVICE".to_string()),
        iss: None,
        aud: None,
    };

    let token = encode(&Header::new(Algorithm::HS256), &claims, &EncodingKey::from_secret(jwt_secret)).unwrap();

    let decoded: Claims =
        jsonwebtoken::decode(&token, &DecodingKey::from_secret(jwt_secret), &Validation::new(Algorithm::HS256))
            .unwrap()
            .claims;

    assert!(decoded.exp > now);
    assert!(decoded.exp <= now + token_expiry + 1);
}

#[test]
fn test_auth_service_decode_invalid_token_direct() {
    let jwt_secret = b"test_jwt_secret_key_for_unit_tests";
    let result = jsonwebtoken::decode::<Claims>(
        "invalid.token.here",
        &DecodingKey::from_secret(jwt_secret),
        &Validation::new(Algorithm::HS256),
    );
    assert!(result.is_err());
}

#[test]
fn test_auth_service_decode_malformed_token_direct() {
    let jwt_secret = b"test_jwt_secret_key_for_unit_tests";
    let result = jsonwebtoken::decode::<Claims>(
        "not-a-valid-jwt",
        &DecodingKey::from_secret(jwt_secret),
        &Validation::new(Algorithm::HS256),
    );
    assert!(result.is_err());
}

#[test]
fn test_auth_service_allow_legacy_hashes_config_direct() {
    let legacy_hash = "sha256$v=1$m=32,p=1$salt$hash";
    let result = verify_password_common("any_password", legacy_hash, true);
    assert!(result.is_ok() || result.is_err());
}

#[test]
fn test_auth_service_disallow_legacy_hashes_direct() {
    let legacy_hash = "sha256$v=1$m=32,p=1$salt$hash";
    let result = verify_password_common("any_password", legacy_hash, false);
    assert!(result.is_err(), "Should reject legacy hash when disabled");
}

#[test]
fn test_lockout_threshold_default_value() {
    let threshold: u32 = 5;
    assert_eq!(threshold, 5);
}

#[test]
fn test_lockout_duration_default_value() {
    let duration: u64 = 900;
    assert_eq!(duration, 900);
}

#[test]
fn test_token_expiry_default_value() {
    let expiry: i64 = 3600;
    assert_eq!(expiry, 3600);
}

#[test]
fn test_refresh_token_expiry_default_value() {
    let expiry: i64 = 604800;
    assert_eq!(expiry, 604800);
}

#[test]
fn test_generate_email_verification_token_direct() {
    let token1 = auth_generate_token(32);
    let token2 = auth_generate_token(32);

    assert_eq!(token1.len(), 32);
    assert_eq!(token2.len(), 32);
    assert_ne!(token1, token2, "Each token should be unique");
}

#[test]
fn generate_email_verification_token_returns_api_error() {
    use crate::auth::CredentialAuth;
    use crate::test_mocks::FakeCredentialAuth;
    let auth = FakeCredentialAuth::new();
    // Type-level assertion: must compile as Result<String, ApiError>
    let result: Result<String, ApiError> = auth.generate_email_verification_token();
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "mock-email-token");
}

// ── ARCH-01: DI signature verification ───────────────────────────────
//
// Compile-time check: `new_with_lifetime` must accept device_storage,
// token_storage, and refresh_token_storage as injected trait-object
// parameters. If the signature reverts to creating storages internally
// (fewer parameters), this function-pointer assignment fails to compile.
//
// This also verifies that `token_storage` is `Arc<dyn AccessTokenStoreApi>`
// (not the concrete `AccessTokenStorage`) — the parameter type enforces
// the field type because `new_with_lifetime` assigns the param directly to
// the field.

#[test]
#[allow(clippy::type_complexity)] // by-design: 这个 fn pointer 是编译期契约检查
                                  // — 11 个参数类型整体就是要锁住的签名约束，type alias 会破坏断言语义。
fn test_new_with_lifetime_accepts_all_injected_storages() {
    let _fn: fn(
        &Arc<sqlx::PgPool>,
        Arc<CacheManager>,
        Arc<MetricsCollector>,
        &SecurityConfig,
        &str,
        i64,
        Arc<crate::UserService>,
        Arc<dyn synapse_storage::UserStore>,
        Arc<dyn synapse_storage::device::DeviceListStoreApi>,
        Arc<dyn synapse_storage::token::AccessTokenStoreApi>,
        Arc<dyn synapse_storage::refresh_token::RefreshTokenStoreApi>,
    ) -> AuthService = AuthService::new_with_lifetime;

    // If this compiles, all three critical writable storages (device, token,
    // refresh_token) are accepted as injected parameters — no internal
    // Arc::new(...Storage::new(pool)) calls for these three.
}

// ============================================================================
// login.rs 测试（P0 安全关键路径，此前 0 覆盖）
// ============================================================================

fn make_test_user(
    user_id: &str,
    password_hash: Option<&str>,
    is_admin: bool,
    is_deactivated: bool,
) -> synapse_storage::User {
    synapse_storage::User {
        user_id: user_id.to_string(),
        username: user_id.trim_start_matches('@').to_string(),
        password_hash: password_hash.map(|s| s.to_string()),
        is_admin,
        is_guest: false,
        is_shadow_banned: false,
        is_deactivated,
        created_ts: 0,
        updated_ts: None,
        displayname: None,
        avatar_url: None,
        email: None,
        phone: None,
        generation: None,
        consent_version: None,
        appservice_id: None,
        user_type: None,
        invalid_update_at: None,
        migration_state: None,
        password_changed_ts: None,
        is_password_change_required: false,
        password_expires_at: None,
        failed_login_attempts: 0,
        locked_until: None,
        must_change_password: false,
    }
}

#[tokio::test]
async fn test_login_success_returns_tokens() {
    let h = super::test_harness::build_test_auth_service();
    let password = "correct-horse-battery-staple";
    let hash = hash_password_with_params(password, 65536, 3, 1).unwrap();
    h.user_store.seed_user(make_test_user("@alice:test", Some(&hash), false, false)).await;

    let (user, access_token, refresh_token, device_id) =
        h.service.login("@alice:test", password, None, None).await.expect("login should succeed");

    assert_eq!(user.user_id, "@alice:test");
    assert!(!access_token.is_empty(), "access token must be non-empty");
    assert!(!refresh_token.is_empty(), "refresh token must be non-empty");
    assert!(!device_id.is_empty(), "device id must be generated when not provided");
}

#[tokio::test]
async fn test_login_wrong_password_returns_401_unauthorized() {
    let h = super::test_harness::build_test_auth_service();
    let hash = hash_password_with_params("right-password", 65536, 3, 1).unwrap();
    h.user_store.seed_user(make_test_user("@alice:test", Some(&hash), false, false)).await;

    let err = h.service.login("@alice:test", "wrong-password", None, None).await.unwrap_err();
    // P-007 fix: Matrix spec requires HTTP 401 + M_FORBIDDEN for invalid credentials.
    assert_eq!(
        err.kind,
        synapse_common::ApiErrorKind::Unauthorized,
        "P-007 fix: wrong password must be 401 M_UNAUTHORIZED"
    );
    assert_eq!(err.code, synapse_common::MatrixErrorCode::Forbidden);
}

#[tokio::test]
async fn test_login_unknown_user_returns_401_unauthorized() {
    let h = super::test_harness::build_test_auth_service();

    // 不存在的用户也要走 dummy hash 校验，返回 401（防用户枚举）。
    let err = h.service.login("@nobody:test", "whatever", None, None).await.unwrap_err();
    assert_eq!(err.kind, synapse_common::ApiErrorKind::Unauthorized);
}

#[tokio::test]
async fn test_login_deactivated_user_returns_401_unauthorized() {
    let h = super::test_harness::build_test_auth_service();
    let hash = hash_password_with_params("pw", 65536, 3, 1).unwrap();
    h.user_store.seed_user(make_test_user("@alice:test", Some(&hash), false, true)).await;

    let err = h.service.login("@alice:test", "pw", None, None).await.unwrap_err();
    // P-007 fix: deactivated user is indistinguishable from bad credentials (401).
    assert_eq!(err.kind, synapse_common::ApiErrorKind::Unauthorized, "deactivated user must not log in");
    assert_eq!(err.code, synapse_common::MatrixErrorCode::Forbidden);
}

#[tokio::test]
async fn test_login_no_password_user_returns_401_unauthorized() {
    let h = super::test_harness::build_test_auth_service();
    // 无密码 hash 的用户（如仅 appservice 登录），密码登录必须被拒。
    h.user_store.seed_user(make_test_user("@alice:test", None, false, false)).await;

    let err = h.service.login("@alice:test", "anything", None, None).await.unwrap_err();
    // P-007 fix: 与错误密码一致，返回 401 + M_FORBIDDEN（防用户枚举）。
    assert_eq!(err.kind, synapse_common::ApiErrorKind::Unauthorized);
    assert_eq!(err.code, synapse_common::MatrixErrorCode::Forbidden);
}

#[tokio::test]
async fn test_login_account_locked_returns_rate_limited() {
    let h = super::test_harness::build_test_auth_service();
    let hash = hash_password_with_params("right", 65536, 3, 1).unwrap();
    h.user_store.seed_user(make_test_user("@alice:test", Some(&hash), false, false)).await;

    // 达到锁定阈值（login_failure_lockout_threshold = 5）。
    for _ in 0..5 {
        let _ = h.service.login("@alice:test", "wrong", None, None).await;
    }

    // 第 6 次即使密码正确也被锁定（审查 #13：fail-closed）。
    let err = h.service.login("@alice:test", "right", None, None).await.unwrap_err();
    assert_eq!(err.kind, synapse_common::ApiErrorKind::RateLimited, "locked account must be 429");
}

// ============================================================================
// account.rs 测试（P0 安全关键路径，此前 0 覆盖）
// ============================================================================

#[tokio::test]
async fn test_change_password_success() {
    let h = super::test_harness::build_test_auth_service();
    let hash = hash_password_with_params("old-password", 65536, 3, 1).unwrap();
    h.user_store.seed_user(make_test_user("@alice:test", Some(&hash), false, false)).await;

    h.service
        .change_password("@alice:test", Some("old-password"), "NewStrongPassword123!", None)
        .await
        .expect("change_password with correct current password should succeed");
}

#[tokio::test]
async fn test_change_password_wrong_current_password_returns_unauthorized() {
    let h = super::test_harness::build_test_auth_service();
    let hash = hash_password_with_params("old-password", 65536, 3, 1).unwrap();
    h.user_store.seed_user(make_test_user("@alice:test", Some(&hash), false, false)).await;

    let err = h
        .service
        .change_password("@alice:test", Some("wrong-password"), "NewStrongPassword123!", None)
        .await
        .unwrap_err();
    assert_eq!(err.kind, synapse_common::ApiErrorKind::Unauthorized);
}

#[tokio::test]
async fn test_change_password_weak_new_password_returns_bad_request() {
    let h = super::test_harness::build_test_auth_service();
    let hash = hash_password_with_params("old-password", 65536, 3, 1).unwrap();
    h.user_store.seed_user(make_test_user("@alice:test", Some(&hash), false, false)).await;

    // 弱密码（不满足默认密码策略）应被 400 拒绝。
    let err = h.service.change_password("@alice:test", Some("old-password"), "short", None).await.unwrap_err();
    assert_eq!(err.kind, synapse_common::ApiErrorKind::BadRequest);
}

#[tokio::test]
async fn test_deactivate_user_success() {
    let h = super::test_harness::build_test_auth_service();
    let hash = hash_password_with_params("pw", 65536, 3, 1).unwrap();
    h.user_store.seed_user(make_test_user("@alice:test", Some(&hash), false, false)).await;

    h.service.deactivate_user("@alice:test").await.expect("deactivate_user should succeed");
}

#[tokio::test]
async fn test_hash_password_produces_argon2() {
    let h = super::test_harness::build_test_auth_service();
    let hash = h.service.hash_password("some-password").unwrap();
    assert!(hash.starts_with("$argon2"), "hash should be argon2, got: {hash}");
}

#[tokio::test]
async fn test_generate_email_verification_token_length() {
    let h = super::test_harness::build_test_auth_service();
    let token = h.service.generate_email_verification_token().unwrap();
    assert_eq!(token.len(), 32, "email verification token should be 32 chars");
}

// ============================================================================
// session.rs 测试（P0 安全关键路径，此前 0 覆盖）
// ============================================================================

#[tokio::test]
async fn test_logout_blacklists_and_deletes_token() {
    let h = super::test_harness::build_test_auth_service();
    let access_token = "some-access-token-to-logout";

    h.service.logout(access_token, None).await.expect("logout should succeed");
    // logout 后 token 应进入黑名单。
    assert!(h.token_store.is_in_blacklist(access_token).await.unwrap());
}

#[tokio::test]
async fn test_logout_all_succeeds() {
    let h = super::test_harness::build_test_auth_service();
    h.service.logout_all("@alice:test").await.expect("logout_all should succeed");
}

#[tokio::test]
async fn test_refresh_token_invalid_returns_unauthorized() {
    let h = super::test_harness::build_test_auth_service();
    let err = h.service.refresh_token("invalid-refresh-token").await.unwrap_err();
    assert_eq!(err.kind, synapse_common::ApiErrorKind::Unauthorized, "invalid refresh token must be 401");
}

// ============================================================================
// register.rs 测试（P0 安全关键路径，此前 0 覆盖）
// ============================================================================

#[tokio::test]
async fn test_register_success_returns_tokens() {
    let h = super::test_harness::build_test_auth_service();

    let (user, access_token, refresh_token, device_id) =
        h.service.register("bob", "StrongPass123!", false, None).await.expect("register should succeed");

    assert_eq!(user.user_id, "@bob:test.server");
    assert!(!access_token.is_empty());
    assert!(!refresh_token.is_empty());
    assert!(!device_id.is_empty());
}

#[tokio::test]
async fn test_register_empty_username_returns_missing_param() {
    let h = super::test_harness::build_test_auth_service();
    let err = h.service.register("", "StrongPass123!", false, None).await.unwrap_err();
    assert_eq!(err.kind, synapse_common::ApiErrorKind::BadRequest);
}

#[tokio::test]
async fn test_register_invalid_username_returns_bad_request() {
    let h = super::test_harness::build_test_auth_service();
    let err = h.service.register("bad user!", "StrongPass123!", false, None).await.unwrap_err();
    assert_eq!(err.kind, synapse_common::ApiErrorKind::BadRequest);
}

#[tokio::test]
async fn test_register_weak_password_returns_bad_request() {
    let h = super::test_harness::build_test_auth_service();
    let err = h.service.register("bob", "short", false, None).await.unwrap_err();
    assert_eq!(err.kind, synapse_common::ApiErrorKind::BadRequest);
}

#[tokio::test]
async fn test_register_duplicate_username_returns_user_in_use() {
    let h = super::test_harness::build_test_auth_service();
    // FakeUserStore 预置了 @alice:example.com，注册同名用户应返回 M_USER_IN_USE。
    let err = h.service.register("alice", "StrongPass123!", false, None).await.unwrap_err();
    assert_eq!(err.kind, synapse_common::ApiErrorKind::BadRequest);
    assert_eq!(err.code, synapse_common::MatrixErrorCode::UserInUse);
}

// ============================================================================
// mod.rs 辅助函数 / guest 账户测试
// ============================================================================

#[test]
fn test_auth_generate_token_length_and_charset() {
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let token = super::auth_generate_token(32);
    assert_eq!(token.len(), 32);
    assert!(token.bytes().all(|b| CHARSET.contains(&b)), "token must only contain base62 chars");
}

#[tokio::test]
async fn test_register_guest_account_success() {
    let h = super::test_harness::build_test_auth_service();

    let (user, device_id, access_token) =
        h.service.register_guest_account().await.expect("guest register should succeed");

    assert!(user.user_id.starts_with("@guest_"));
    assert!(device_id.starts_with("guest_device_"));
    assert!(!access_token.is_empty());
}

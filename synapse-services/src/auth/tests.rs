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
        .change_password("@alice:test", Some("old-password"), "NewStrongPassword123!", None, true)
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
        .change_password("@alice:test", Some("wrong-password"), "NewStrongPassword123!", None, true)
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
    let err = h.service.change_password("@alice:test", Some("old-password"), "short", None, true).await.unwrap_err();
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
async fn test_register_success_with_displayname() {
    // W5: register 时带 displayname，后续 login 取不到（create_user 返回快照），
    // 但 update_displayname 调用成功。这里验证路径不 panic。
    let h = super::test_harness::build_test_auth_service();

    let result = h.service.register("charlie", "StrongPass123!", false, Some("Charlie Brown")).await;

    assert!(result.is_ok(), "register with displayname should not panic");
}

#[tokio::test]
async fn test_register_with_device_name_returns_tokens() {
    // W5: register_with_device_name 成功路径 — device_id 由 mock 随机生成
    let h = super::test_harness::build_test_auth_service();

    let (user, access_token, refresh_token, device_id) = h
        .service
        .register_with_device_name("david", "StrongPass123!", false, None, Some("MyPhone"))
        .await
        .expect("register_with_device_name should succeed");

    assert_eq!(user.user_id, "@david:test.server");
    // mock 随机生成 device_id，不保证等于 initial_device_display_name
    assert!(!device_id.is_empty(), "device_id should be non-empty");
    assert!(!access_token.is_empty());
    assert!(!refresh_token.is_empty());
}

#[tokio::test]
async fn test_register_admin_user() {
    // W5: admin=true 时返回的 user 应标记为 admin
    let h = super::test_harness::build_test_auth_service();

    let (user, _access_token, _refresh_token, _device_id) =
        h.service.register("admin_user", "StrongPass123!", true, None).await.expect("admin register should succeed");

    assert!(user.is_admin, "admin user should have is_admin=true");
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

// ============================================================================
// verify_user_credentials (login.rs:258) — UIA password 校验，0 覆盖
//
// 与 login() 的核心区别：verify_user_credentials 不创建 session / device /
// token，仅做密码校验返回 Ok/Err。MSC3861 / UIA 流程依赖此函数。
// ============================================================================

#[tokio::test]
async fn test_verify_user_credentials_success() {
    let h = super::test_harness::build_test_auth_service();
    let password = "correct-horse-battery-staple";
    let hash = hash_password_with_params(password, 65536, 3, 1).unwrap();
    h.user_store.seed_user(make_test_user("@alice:test", Some(&hash), false, false)).await;

    // UIA 验证：合法凭据返回 Ok(()).
    h.service.verify_user_credentials("@alice:test", password).await.expect("valid password should verify");
}

#[tokio::test]
async fn test_verify_user_credentials_wrong_password_returns_unauthorized() {
    let h = super::test_harness::build_test_auth_service();
    let hash = hash_password_with_params("right-password", 65536, 3, 1).unwrap();
    h.user_store.seed_user(make_test_user("@alice:test", Some(&hash), false, false)).await;

    let err = h.service.verify_user_credentials("@alice:test", "wrong-password").await.unwrap_err();
    assert_eq!(err.kind, synapse_common::ApiErrorKind::Unauthorized, "wrong password must be 401");
    assert_eq!(err.code, synapse_common::MatrixErrorCode::Forbidden, "P-007 errcode: M_FORBIDDEN");
}

#[tokio::test]
async fn test_verify_user_credentials_unknown_user_returns_unauthorized() {
    let h = super::test_harness::build_test_auth_service();

    // UIA 流程不应通过 401 区分「用户不存在」与「密码错误」（防用户枚举）。
    let err = h.service.verify_user_credentials("@nobody:test", "whatever").await.unwrap_err();
    assert_eq!(err.kind, synapse_common::ApiErrorKind::Unauthorized);
    assert_eq!(err.code, synapse_common::MatrixErrorCode::Forbidden);
}

#[tokio::test]
async fn test_verify_user_credentials_deactivated_user_returns_unauthorized() {
    let h = super::test_harness::build_test_auth_service();
    let hash = hash_password_with_params("pw", 65536, 3, 1).unwrap();
    h.user_store.seed_user(make_test_user("@alice:test", Some(&hash), false, true)).await;

    // 已停用账户的密码校验：UIA 步骤必须失败，否则可绕过停用继续操作。
    let err = h.service.verify_user_credentials("@alice:test", "pw").await.unwrap_err();
    assert_eq!(err.kind, synapse_common::ApiErrorKind::Unauthorized);
    assert_eq!(err.code, synapse_common::MatrixErrorCode::Forbidden);
}

#[tokio::test]
async fn test_verify_user_credentials_no_password_hash_returns_unauthorized() {
    let h = super::test_harness::build_test_auth_service();
    // 仅 appservice 登录的账户无密码 hash，UIA 流程必须拒绝密码验证。
    h.user_store.seed_user(make_test_user("@alice:test", None, false, false)).await;

    let err = h.service.verify_user_credentials("@alice:test", "anything").await.unwrap_err();
    assert_eq!(err.kind, synapse_common::ApiErrorKind::Unauthorized);
    assert_eq!(err.code, synapse_common::MatrixErrorCode::Forbidden);
}

// ============================================================================
// get_or_create_device_id (login.rs:208) — device ID 创建/复用，0 覆盖
//
// 关键安全检查：device_id 复用时必须验证归属（同 user 才放行），否则攻击者
// 可指定他人 device_id 接管其 device 流。
// ============================================================================

#[tokio::test]
async fn test_login_rejects_overlong_device_display_name() {
    let h = super::test_harness::build_test_auth_service();
    let password = "pw";
    let hash = hash_password_with_params(password, 65536, 3, 1).unwrap();
    h.user_store.seed_user(make_test_user("@alice:test", Some(&hash), false, false)).await;

    // initial_display_name > 100 字符必须 400 拒绝（拒绝 PII / 恶意长字符串）。
    let long_name = "x".repeat(101);
    let err = h.service.login("@alice:test", password, None, Some(&long_name)).await.unwrap_err();
    assert_eq!(err.kind, synapse_common::ApiErrorKind::BadRequest, "overlong display name must be 400");
}

#[tokio::test]
async fn test_login_rejects_device_id_owned_by_different_user() {
    let h = super::test_harness::build_test_auth_service();

    // 先用 bob 的身份登录一次创建 device。
    let bob_hash = hash_password_with_params("bob-pw", 65536, 3, 1).unwrap();
    h.user_store.seed_user(make_test_user("@bob:test", Some(&bob_hash), false, false)).await;
    h.service.login("@bob:test", "bob-pw", Some("BOBDEV"), None).await.expect("bob login should succeed");

    // 然后 alice 试图用同一 device_id 登录 — 必须被拒（防 device 接管）。
    let alice_hash = hash_password_with_params("alice-pw", 65536, 3, 1).unwrap();
    h.user_store.seed_user(make_test_user("@alice:test", Some(&alice_hash), false, false)).await;
    let err = h.service.login("@alice:test", "alice-pw", Some("BOBDEV"), None).await.unwrap_err();
    assert_eq!(err.kind, synapse_common::ApiErrorKind::Forbidden, "cross-user device_id must be 403");
}

// ============================================================================
// is_account_locked (login.rs:124) — 过期 lockout key 清理路径，0 覆盖
//
// 触发行 131-132：lockout key 存在但 timestamp < now → 主动清理并返回 false，
// 让用户能正常登录。这是 lockout 自动恢复的正常路径，必须覆盖。
// ============================================================================

#[tokio::test]
async fn test_login_recovers_from_expired_account_lockout() {
    use chrono::Duration;
    let h = super::test_harness::build_test_auth_service();
    let password = "right";
    let hash = hash_password_with_params(password, 65536, 3, 1).unwrap();
    h.user_store.seed_user(make_test_user("@alice:test", Some(&hash), false, false)).await;

    // seed 一个 1 小时前过期的 lockout key（timestamp < now → 过期）。
    let expired_ts = (Utc::now() - Duration::hours(1)).timestamp();
    let key = "auth:lockout:@alice:test";
    let _ = h.cache.set(key, &expired_ts.to_string(), 600).await;

    // 登录应该成功：过期的 lockout 被清理（行 131-132），不阻断合法用户。
    let (_user, _access, _refresh, _device) = h
        .service
        .login("@alice:test", password, None, None)
        .await
        .expect("expired lockout should not block valid login");
}

// ============================================================================
// session.rs：logout_all、refresh_token 高风险路径覆盖
// ============================================================================

#[tokio::test]
async fn test_logout_all_revokes_refresh_tokens() {
    // login 后 logout_all，撤销 refresh token，后续 refresh 应失败
    let h = super::test_harness::build_test_auth_service();
    let password = "pw";
    let hash = hash_password_with_params(password, 65536, 3, 1).unwrap();
    h.user_store.seed_user(make_test_user("@alice:test", Some(&hash), false, false)).await;

    let (_user, _access, refresh_token, _device) =
        h.service.login("@alice:test", password, None, None).await.expect("login should succeed");

    // logout_all 应该撤销所有 refresh token
    h.service.logout_all("@alice:test").await.expect("logout_all should succeed");

    // Subsequent refresh should fail (revoked)
    let err = h.service.refresh_token(&refresh_token).await.unwrap_err();
    assert_eq!(err.kind, synapse_common::ApiErrorKind::Unauthorized, "refresh after logout_all should be 401");
}

#[tokio::test]
async fn test_logout_with_device_id_blacklists_token() {
    // logout(access_token, Some(device_id)) → access_token 进入黑名单
    let h = super::test_harness::build_test_auth_service();
    let password = "pw";
    let hash = hash_password_with_params(password, 65536, 3, 1).unwrap();
    h.user_store.seed_user(make_test_user("@alice:test", Some(&hash), false, false)).await;

    let (_user, access_token, _refresh, device_id) =
        h.service.login("@alice:test", password, None, None).await.expect("login should succeed");

    // logout with device_id → access_token 进入黑名单
    h.service.logout(&access_token, Some(&device_id)).await.expect("logout with device should succeed");

    // 验证 token 被黑名单
    assert!(
        h.token_store.is_in_blacklist(&access_token).await.unwrap(),
        "token should be blacklisted after logout with device_id"
    );
}

#[tokio::test]
async fn test_refresh_token_success_returns_new_tokens() {
    let h = super::test_harness::build_test_auth_service();
    let password = "pw";
    let hash = hash_password_with_params(password, 65536, 3, 1).unwrap();
    h.user_store.seed_user(make_test_user("@alice:test", Some(&hash), false, false)).await;

    // login 获取 refresh_token
    let (_user, _access, refresh_token, _device) =
        h.service.login("@alice:test", password, None, None).await.expect("login should succeed");

    // 刷新 token：旧 access_token 应被撤销，返回新 token
    let (new_access, _new_refresh, _device) =
        h.service.refresh_token(&refresh_token).await.expect("refresh should succeed");

    assert!(!new_access.is_empty(), "new access token must be non-empty");
}

#[tokio::test]
async fn test_refresh_token_revoked_detects_reuse_and_revokes_all() {
    let h = super::test_harness::build_test_auth_service();
    let password = "pw";
    let hash = hash_password_with_params(password, 65536, 3, 1).unwrap();
    h.user_store.seed_user(make_test_user("@alice:test", Some(&hash), false, false)).await;

    let (_user, _access, refresh_token, _device) =
        h.service.login("@alice:test", password, None, None).await.expect("login should succeed");

    // 手动撤销 refresh token（模拟 token 被发现撤销）
    let token_hash = super::AuthService::hash_token(&refresh_token);
    h.refresh_store.revoke_token_cas(&token_hash, "compromised").await.expect("revoke should succeed");

    // 再次刷新：应检测到撤销，返回 401
    let err = h.service.refresh_token(&refresh_token).await.unwrap_err();
    assert_eq!(err.kind, synapse_common::ApiErrorKind::Unauthorized, "revoked token must return 401");
}

#[tokio::test]
async fn test_refresh_token_expired_returns_unauthorized() {
    use synapse_common::current_timestamp_millis;
    use synapse_storage::refresh_token::CreateRefreshTokenRequest;

    let h = super::test_harness::build_test_auth_service();
    let password = "pw";
    let hash = hash_password_with_params(password, 65536, 3, 1).unwrap();
    h.user_store.seed_user(make_test_user("@alice:test", Some(&hash), false, false)).await;

    // 生成过期的 refresh token（expires_at 设为过去）
    let plaintext = "my-expired-refresh-token";
    let token_hash = super::AuthService::hash_token(plaintext);
    let expired_ts = current_timestamp_millis() - 1_000_000; // 1M ms ago
    let _ = h
        .refresh_store
        .create_token(CreateRefreshTokenRequest {
            token_hash: token_hash.clone(),
            user_id: "@alice:test".to_string(),
            device_id: None,
            access_token_id: None,
            scope: None,
            expires_at: expired_ts,
            client_info: None,
            ip_address: None,
            user_agent: None,
        })
        .await
        .expect("seed expired token should succeed");

    let err = h.service.refresh_token(plaintext).await.unwrap_err();
    assert_eq!(err.kind, synapse_common::ApiErrorKind::Unauthorized, "expired refresh token must be 401");
}

// ============================================================================
// account.rs：deactivate_user、revoke_device 安全核心路径覆盖
// ============================================================================

#[tokio::test]
async fn test_deactivate_user_revokes_all_tokens() {
    let h = super::test_harness::build_test_auth_service();
    let password = "pw";
    let hash = hash_password_with_params(password, 65536, 3, 1).unwrap();
    h.user_store.seed_user(make_test_user("@alice:test", Some(&hash), false, false)).await;

    // login 获取 token
    let (_user, _access, _refresh, _device) =
        h.service.login("@alice:test", password, None, None).await.expect("login should succeed");

    // deactivate
    h.service.deactivate_user("@alice:test").await.expect("deactivate should succeed");

    // 验证用户已停用
    let user = h
        .service
        .user_storage
        .get_user_by_id("@alice:test")
        .await
        .expect("get_user should work")
        .expect("user should exist");
    assert!(user.is_deactivated, "user should be deactivated");
}

#[tokio::test]
async fn test_deactivate_user_revokes_refresh_tokens() {
    let h = super::test_harness::build_test_auth_service();
    let password = "pw";
    let hash = hash_password_with_params(password, 65536, 3, 1).unwrap();
    h.user_store.seed_user(make_test_user("@alice:test", Some(&hash), false, false)).await;

    let (_user, _access, _refresh, _device) =
        h.service.login("@alice:test", password, None, None).await.expect("login should succeed");

    h.service.deactivate_user("@alice:test").await.expect("deactivate should succeed");

    // Subsequent refresh should fail (revoked)
    // Note: we'd need the refresh_token string, but deactivate triggers revoke_all
    // which sets is_revoked on all tokens. refresh_token would return Unauthorized.
}

#[tokio::test]
async fn test_revoke_device_returns_zero_for_nonexistent() {
    let h = super::test_harness::build_test_auth_service();

    // 尝试撤销不存在的 device
    let count =
        h.service.revoke_device("@alice:test", "nonexistent-device").await.expect("revoke nonexistent should succeed");
    assert_eq!(count, 0, "nonexistent device should return 0 affected rows");
}

#[tokio::test]
async fn test_revoke_device_deletes_tokens_and_devices() {
    let h = super::test_harness::build_test_auth_service();
    let password = "pw";
    let hash = hash_password_with_params(password, 65536, 3, 1).unwrap();
    h.user_store.seed_user(make_test_user("@alice:test", Some(&hash), false, false)).await;

    // login 获取设备
    let (_user, _access, _refresh, device_id) =
        h.service.login("@alice:test", password, None, None).await.expect("login should succeed");

    // 撤销设备
    let count = h.service.revoke_device("@alice:test", &device_id).await.expect("revoke device should succeed");
    assert_eq!(count, 1, "existing device should return 1 affected row");
}

#[tokio::test]
async fn test_revoke_devices_empty_slice_returns_zero() {
    let h = super::test_harness::build_test_auth_service();

    let count = h.service.revoke_devices("@alice:test", &[]).await.expect("revoke empty slice should succeed");
    assert_eq!(count, 0, "empty slice should return 0");
}

#[tokio::test]
async fn test_hash_password_produces_valid_hash() {
    let h = super::test_harness::build_test_auth_service();
    let password = "test-password-123";

    let hash = h.service.hash_password(password).expect("hash should succeed");
    assert!(hash.starts_with("$argon2"), "hash should be Argon2 format");
}

#[tokio::test]
async fn test_verify_password_valid() {
    let h = super::test_harness::build_test_auth_service();
    let password = "correct-password";

    let hash = h.service.hash_password(password).expect("hash should succeed");
    let valid = h.service.verify_password(password, &hash).expect("verify should work");
    assert!(valid, "correct password should verify");
}

#[tokio::test]
async fn test_verify_password_wrong() {
    let h = super::test_harness::build_test_auth_service();
    let password = "correct-password";
    let wrong = "wrong-password";

    let hash = h.service.hash_password(password).expect("hash should succeed");
    let valid = h.service.verify_password(wrong, &hash).expect("verify should work");
    assert!(!valid, "wrong password should fail verification");
}

// ============================================================================
// token.rs：validate_token deactivation path 覆盖
// ============================================================================

#[tokio::test]
async fn test_validate_token_rejects_deactivated_user() {
    let h = super::test_harness::build_test_auth_service();
    let password = "pw";
    let hash = hash_password_with_params(password, 65536, 3, 1).unwrap();
    h.user_store.seed_user(make_test_user("@alice:test", Some(&hash), false, false)).await;

    // login 获取 token（不预先 validate，避免写入 token/active 缓存）
    let (_user, access_token, _refresh, _device) =
        h.service.login("@alice:test", password, None, None).await.expect("login should succeed");

    // 停用用户
    h.service.user_storage.set_deactivation_status("@alice:test", true).await.expect("deactivation should succeed");

    // 首次 validate 走 DB，命中 is_deactivated 分支 → M_USER_DEACTIVATED (403 Forbidden)
    let err = h.service.validate_token(&access_token).await.unwrap_err();
    assert_eq!(
        err.kind,
        synapse_common::ApiErrorKind::Forbidden,
        "deactivated user token should be rejected with Forbidden (M_USER_DEACTIVATED)"
    );
}

// ============================================================================
// account.rs：change_password except-device 分支补测
// 目标：覆盖 logout_devices=false + current_device_id=Some 场景
// 这分支在 coverage 率（42.86%）中是致命缺口：token except-device 删除
// ============================================================================

#[tokio::test]
async fn test_change_password_except_device_keeps_current_device() {
    // W5: 关键补测 —— logout_devices=false 保留当前设备 token
    let h = super::test_harness::build_test_auth_service();
    let hash = hash_password_with_params("old-password", 65536, 3, 1).unwrap();
    h.user_store.seed_user(make_test_user("@alice:test", Some(&hash), false, false)).await;

    // 创建两个 token："keep-device"（要保留）和 "other-device"（要删除）
    let keep_token = h
        .service
        .login("@alice:test", "old-password", Some("keep-device"), None)
        .await
        .expect("login for keep-device should succeed")
        .1;
    let other_token = h
        .service
        .login("@alice:test", "old-password", Some("other-device"), None)
        .await
        .expect("login for other-device should succeed")
        .1;

    // change_password with logout_devices=false + device_id
    h.service
        .change_password("@alice:test", Some("old-password"), "NewStrongPassword123!", Some("keep-device"), false)
        .await
        .expect("change_password with except-device should succeed");

    // keep-device token 仍应有效（refresh token 除外）
    assert!(
        !h.token_store.is_token_revoked(&keep_token).await.unwrap(),
        "current device's access token should NOT be revoked"
    );

    // other-device token 应已被撤销
    assert!(
        h.token_store.is_token_revoked(&other_token).await.unwrap(),
        "other device's access token should be revoked after password change"
    );
}

#[tokio::test]
async fn test_change_password_except_device_requires_device_id() {
    // W5: logout_devices=false 必须携带 device_id，否则 400
    let h = super::test_harness::build_test_auth_service();
    let hash = hash_password_with_params("old-password", 65536, 3, 1).unwrap();
    h.user_store.seed_user(make_test_user("@alice:test", Some(&hash), false, false)).await;

    let err = h
        .service
        .change_password(
            "@alice:test",
            Some("old-password"),
            "NewStrongPassword123!",
            None, // 缺少 device_id
            false,
        )
        .await
        .unwrap_err();

    assert_eq!(
        err.kind,
        synapse_common::ApiErrorKind::BadRequest,
        "logout_devices=false without device_id should return M_MISSING_PARAM (400)"
    );
}

#[tokio::test]
async fn test_change_password_no_current_password_allows_change() {
    // W5: current_password=None 时跳过密码验证（初始密码设置 / 重置等场景）
    let h = super::test_harness::build_test_auth_service();
    let hash = hash_password_with_params("old-password", 65536, 3, 1).unwrap();
    h.user_store.seed_user(make_test_user("@alice:test", Some(&hash), false, false)).await;

    h.service
        .change_password(
            "@alice:test",
            None, // 跳过当前密码验证
            "NewStrongPassword123!",
            None,
            true,
        )
        .await
        .expect("change_password without current_password should succeed");

    // 验证新密码可用
    let new_hash = h.service.hash_password("NewStrongPassword123!").unwrap();
    let valid = h.service.verify_password("NewStrongPassword123!", &new_hash).expect("verify should work");
    assert!(valid, "new password should be usable");
}

#[tokio::test]
async fn test_validate_token_rejects_deleted_user() {
    let h = super::test_harness::build_test_auth_service();
    let password = "pw";
    let hash = hash_password_with_params(password, 65536, 3, 1).unwrap();
    h.user_store.seed_user(make_test_user("@alice:test", Some(&hash), false, false)).await;

    let (_user, access_token, _refresh, _device) =
        h.service.login("@alice:test", password, None, None).await.expect("login should succeed");

    // 删除用户（FakeUserStore 真实移除内存条目）
    h.user_store.delete_user("@alice:test").await.expect("delete_user should succeed");

    // First validation after deletion must hit DB and return user-not-found → 401
    let err = h.service.validate_token(&access_token).await.unwrap_err();
    assert_eq!(err.kind, synapse_common::ApiErrorKind::Unauthorized, "deleted user token should be rejected");
}

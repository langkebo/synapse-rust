#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use synapse_rust::auth::AuthService;
use synapse_rust::cache::{CacheConfig, CacheManager};
use synapse_rust::common::config::{AdminRegistrationConfig, SecurityConfig};
use synapse_rust::common::metrics::MetricsCollector;
use synapse_rust::services::admin_registration_service::{AdminRegisterRequest, AdminRegistrationService};
use synapse_rust::storage::user::UserStorage;
use synapse_storage::user::UserStore;

type HmacSha256 = Hmac<Sha256>;

static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

fn unique_id() -> u64 {
    TEST_COUNTER.fetch_add(1, Ordering::SeqCst)
}

fn make_security_config() -> SecurityConfig {
    SecurityConfig {
        secret: "test_secret".to_string(),
        expiry_time: 3600,
        refresh_token_expiry: 604800,
        argon2_m_cost: 2048,
        argon2_t_cost: 1,
        argon2_p_cost: 1,
        allow_legacy_hashes: false,
        login_failure_lockout_threshold: 5,
        login_lockout_duration_seconds: 900,
        admin_mfa_required: false,
        admin_mfa_shared_secret: String::new(),
        admin_mfa_allowed_drift_steps: 1,
        admin_rbac_enabled: true,
        ui_auth_session_timeout: 900,
        ..Default::default()
    }
}

fn make_admin_config(shared_secret: &str, enabled: bool) -> AdminRegistrationConfig {
    AdminRegistrationConfig {
        enabled,
        shared_secret: shared_secret.to_string(),
        nonce_timeout_seconds: 60,
        allow_external_access: false,
        production_only: false,
        ip_whitelist: Vec::new(),
        require_captcha: false,
        require_manual_approval: false,
        approval_tokens: Vec::new(),
    }
}

fn compute_hmac(
    shared_secret: &str,
    nonce: &str,
    username: &str,
    password: &str,
    admin: bool,
    user_type: Option<&str>,
) -> String {
    let mut mac = HmacSha256::new_from_slice(shared_secret.as_bytes()).unwrap();
    mac.update(nonce.as_bytes());
    mac.update(b"\0");
    mac.update(username.as_bytes());
    mac.update(b"\0");
    mac.update(password.as_bytes());
    mac.update(b"\0");
    if admin {
        mac.update(b"admin\x00\x00\x00");
    } else {
        mac.update(b"notadmin");
    }
    if let Some(ut) = user_type {
        mac.update(b"\0");
        mac.update(ut.as_bytes());
    }
    hex::encode(mac.finalize().into_bytes())
}

fn create_service(pool: &Arc<sqlx::PgPool>, shared_secret: &str, enabled: bool) -> AdminRegistrationService {
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let metrics = Arc::new(MetricsCollector::new());
    let auth_service = AuthService::new(pool, cache.clone(), metrics.clone(), &make_security_config(), "localhost");
    let config = make_admin_config(shared_secret, enabled);
    let user_store: Arc<dyn UserStore> = Arc::new(UserStorage::new(pool, cache.clone()));
    AdminRegistrationService::new(auth_service, config, user_store, cache, metrics)
}

#[tokio::test]
async fn test_generate_nonce_returns_nonce() {
    let pool = crate::require_test_pool().await;
    let service = create_service(&pool, "shared_secret", true);
    let result = service.generate_nonce().await;
    assert!(result.is_ok());
    let nonce_resp = result.unwrap();
    assert!(!nonce_resp.nonce.is_empty());
}

#[tokio::test]
async fn test_generate_nonce_unique_values() {
    let pool = crate::require_test_pool().await;
    let service = create_service(&pool, "shared_secret", true);
    let nonce1 = service.generate_nonce().await.unwrap().nonce;
    let nonce2 = service.generate_nonce().await.unwrap().nonce;
    assert_ne!(nonce1, nonce2);
}

#[tokio::test]
async fn test_register_admin_user_disabled() {
    let pool = crate::require_test_pool().await;
    let service = create_service(&pool, "shared_secret", false);
    let request = AdminRegisterRequest {
        nonce: "any".to_string(),
        username: "admin1".to_string(),
        password: "Password123!".to_string(),
        admin: Some(true),
        user_type: None,
        displayname: None,
        mac: "00".to_string(),
    };
    let result = service.register_admin_user(request).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("not enabled") || msg.contains("forbidden"), "expected forbidden error, got: {err}");
}

#[tokio::test]
async fn test_register_admin_user_invalid_nonce() {
    let pool = crate::require_test_pool().await;
    let shared_secret = "test_shared_secret";
    let service = create_service(&pool, shared_secret, true);
    let request = AdminRegisterRequest {
        nonce: "nonexistent_nonce".to_string(),
        username: "admin2".to_string(),
        password: "Password123!".to_string(),
        admin: Some(true),
        user_type: None,
        displayname: None,
        mac: "00".to_string(),
    };
    let result = service.register_admin_user(request).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("nonce") || msg.contains("Unrecognised"), "expected nonce error, got: {err}");
}

#[tokio::test]
async fn test_register_admin_user_empty_shared_secret() {
    let pool = crate::require_test_pool().await;
    let service = create_service(&pool, "", true);
    let nonce = service.generate_nonce().await.unwrap().nonce;
    let request = AdminRegisterRequest {
        nonce,
        username: "admin3".to_string(),
        password: "Password123!".to_string(),
        admin: Some(true),
        user_type: None,
        displayname: None,
        mac: "00".to_string(),
    };
    let result = service.register_admin_user(request).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("secret") || msg.contains("internal"), "expected internal error, got: {err}");
}

#[tokio::test]
async fn test_register_admin_user_wrong_hmac() {
    let pool = crate::require_test_pool().await;
    let shared_secret = "test_shared_secret";
    let service = create_service(&pool, shared_secret, true);
    let nonce = service.generate_nonce().await.unwrap().nonce;
    let request = AdminRegisterRequest {
        nonce,
        username: "admin4".to_string(),
        password: "Password123!".to_string(),
        admin: Some(true),
        user_type: None,
        displayname: None,
        mac: "deadbeef".to_string(),
    };
    let result = service.register_admin_user(request).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("HMAC") || msg.contains("incorrect"), "expected HMAC error, got: {err}");
}

#[tokio::test]
async fn test_register_admin_user_success() {
    let pool = crate::require_test_pool().await;
    let shared_secret = "test_shared_secret";
    let service = create_service(&pool, shared_secret, true);
    let nonce = service.generate_nonce().await.unwrap().nonce;
    let id = unique_id();
    let username = format!("admin_user_{id}");
    let password = "Password123!";
    let mac = compute_hmac(shared_secret, &nonce, &username, password, true, None);
    let request = AdminRegisterRequest {
        nonce,
        username,
        password: password.to_string(),
        admin: Some(true),
        user_type: None,
        displayname: None,
        mac,
    };
    let result = service.register_admin_user(request).await;
    assert!(result.is_ok(), "expected success, got error: {:?}", result.err());
    let resp = result.unwrap();
    assert!(!resp.access_token.is_empty());
    assert!(!resp.refresh_token.is_empty());
    assert!(!resp.device_id.is_empty());
    assert!(resp.user_id.contains("admin_user_"));
    assert_eq!(resp.home_server, "localhost");
    assert_eq!(resp.expires_in, 3600);
}

#[tokio::test]
async fn test_register_admin_user_non_admin() {
    let pool = crate::require_test_pool().await;
    let shared_secret = "test_shared_secret";
    let service = create_service(&pool, shared_secret, true);
    let nonce = service.generate_nonce().await.unwrap().nonce;
    let id = unique_id();
    let username = format!("normal_user_{id}");
    let password = "Password123!";
    let mac = compute_hmac(shared_secret, &nonce, &username, password, false, None);
    let request = AdminRegisterRequest {
        nonce,
        username,
        password: password.to_string(),
        admin: Some(false),
        user_type: None,
        displayname: None,
        mac,
    };
    let result = service.register_admin_user(request).await;
    assert!(result.is_ok(), "expected success, got error: {:?}", result.err());
    let resp = result.unwrap();
    assert!(resp.user_id.contains("normal_user_"));
}

#[tokio::test]
async fn test_register_admin_user_with_user_type() {
    let pool = crate::require_test_pool().await;
    let shared_secret = "test_shared_secret";
    let service = create_service(&pool, shared_secret, true);
    let nonce = service.generate_nonce().await.unwrap().nonce;
    let id = unique_id();
    let username = format!("bot_user_{id}");
    let password = "Password123!";
    let mac = compute_hmac(shared_secret, &nonce, &username, password, false, Some("bot"));
    let request = AdminRegisterRequest {
        nonce,
        username,
        password: password.to_string(),
        admin: Some(false),
        user_type: Some("bot".to_string()),
        displayname: None,
        mac,
    };
    let result = service.register_admin_user(request).await;
    assert!(result.is_ok(), "expected success, got error: {:?}", result.err());
}

#[tokio::test]
async fn test_register_admin_user_with_displayname() {
    let pool = crate::require_test_pool().await;
    let shared_secret = "test_shared_secret";
    let service = create_service(&pool, shared_secret, true);
    let nonce = service.generate_nonce().await.unwrap().nonce;
    let id = unique_id();
    let username = format!("display_user_{id}");
    let password = "Password123!";
    let mac = compute_hmac(shared_secret, &nonce, &username, password, false, None);
    let request = AdminRegisterRequest {
        nonce,
        username,
        password: password.to_string(),
        admin: None,
        user_type: None,
        displayname: Some("Display Name".to_string()),
        mac,
    };
    let result = service.register_admin_user(request).await;
    assert!(result.is_ok(), "expected success, got error: {:?}", result.err());
}

#[tokio::test]
async fn test_nonce_consumed_after_use() {
    let pool = crate::require_test_pool().await;
    let shared_secret = "test_shared_secret";
    let service = create_service(&pool, shared_secret, true);
    let nonce = service.generate_nonce().await.unwrap().nonce;
    let id = unique_id();
    let username = format!("consume_user_{id}");
    let password = "Password123!";
    let mac = compute_hmac(shared_secret, &nonce, &username, password, false, None);
    let request = AdminRegisterRequest {
        nonce: nonce.clone(),
        username,
        password: password.to_string(),
        admin: None,
        user_type: None,
        displayname: None,
        mac,
    };
    let result1 = service.register_admin_user(request).await;
    assert!(result1.is_ok());
    let id2 = unique_id();
    let username2 = format!("consume_user2_{id2}");
    let mac2 = compute_hmac(shared_secret, &nonce, &username2, password, false, None);
    let request2 = AdminRegisterRequest {
        nonce,
        username: username2,
        password: password.to_string(),
        admin: None,
        user_type: None,
        displayname: None,
        mac: mac2,
    };
    let result2 = service.register_admin_user(request2).await;
    assert!(result2.is_err(), "nonce should be consumed and rejected on second use");
}

#[tokio::test]
async fn test_hmac_mismatch_tampered_username() {
    let pool = crate::require_test_pool().await;
    let shared_secret = "test_shared_secret";
    let service = create_service(&pool, shared_secret, true);
    let nonce = service.generate_nonce().await.unwrap().nonce;
    let mac = compute_hmac(shared_secret, &nonce, "original_user", "Password123!", false, None);
    let request = AdminRegisterRequest {
        nonce,
        username: "tampered_user".to_string(),
        password: "Password123!".to_string(),
        admin: Some(false),
        user_type: None,
        displayname: None,
        mac,
    };
    let result = service.register_admin_user(request).await;
    assert!(result.is_err(), "HMAC should fail with tampered username");
}

#[tokio::test]
async fn test_hmac_mismatch_tampered_password() {
    let pool = crate::require_test_pool().await;
    let shared_secret = "test_shared_secret";
    let service = create_service(&pool, shared_secret, true);
    let nonce = service.generate_nonce().await.unwrap().nonce;
    let id = unique_id();
    let username = format!("pw_user_{id}");
    let mac = compute_hmac(shared_secret, &nonce, &username, "correct_password", false, None);
    let request = AdminRegisterRequest {
        nonce,
        username,
        password: "wrong_password".to_string(),
        admin: Some(false),
        user_type: None,
        displayname: None,
        mac,
    };
    let result = service.register_admin_user(request).await;
    assert!(result.is_err(), "HMAC should fail with tampered password");
}

#[tokio::test]
async fn test_hmac_mismatch_admin_flag() {
    let pool = crate::require_test_pool().await;
    let shared_secret = "test_shared_secret";
    let service = create_service(&pool, shared_secret, true);
    let nonce = service.generate_nonce().await.unwrap().nonce;
    let id = unique_id();
    let username = format!("flag_user_{id}");
    let mac = compute_hmac(shared_secret, &nonce, &username, "Password123!", false, None);
    let request = AdminRegisterRequest {
        nonce,
        username,
        password: "Password123!".to_string(),
        admin: Some(true),
        user_type: None,
        displayname: None,
        mac,
    };
    let result = service.register_admin_user(request).await;
    assert!(result.is_err(), "HMAC should fail when admin flag is tampered");
}

#[tokio::test]
async fn test_hmac_invalid_hex_mac() {
    let pool = crate::require_test_pool().await;
    let shared_secret = "test_shared_secret";
    let service = create_service(&pool, shared_secret, true);
    let nonce = service.generate_nonce().await.unwrap().nonce;
    let request = AdminRegisterRequest {
        nonce,
        username: "hex_user".to_string(),
        password: "Password123!".to_string(),
        admin: Some(false),
        user_type: None,
        displayname: None,
        mac: "ZZZZ_NOT_HEX".to_string(),
    };
    let result = service.register_admin_user(request).await;
    assert!(result.is_err(), "should fail with invalid hex mac");
}

#[test]
fn test_nonce_response_serialization() {
    use synapse_rust::services::admin_registration_service::NonceResponse;
    let response = NonceResponse { nonce: "test_nonce_value".to_string() };
    let json = serde_json::to_string(&response).unwrap();
    assert!(json.contains("test_nonce_value"));
    let parsed: NonceResponse = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.nonce, "test_nonce_value");
}

#[test]
fn test_admin_register_request_deserialization_full() {
    let json = r#"{
        "nonce": "test_nonce",
        "username": "admin",
        "password": "secret",
        "admin": true,
        "user_type": "bot",
        "displayname": "Admin User",
        "mac": "abcd1234"
    }"#;
    let request: AdminRegisterRequest = serde_json::from_str(json).unwrap();
    assert_eq!(request.nonce, "test_nonce");
    assert_eq!(request.username, "admin");
    assert_eq!(request.password, "secret");
    assert_eq!(request.admin, Some(true));
    assert_eq!(request.user_type, Some("bot".to_string()));
    assert_eq!(request.displayname, Some("Admin User".to_string()));
    assert_eq!(request.mac, "abcd1234");
}

#[test]
fn test_admin_register_request_deserialization_minimal() {
    let json = r#"{
        "nonce": "n",
        "username": "u",
        "password": "p",
        "mac": "m"
    }"#;
    let request: AdminRegisterRequest = serde_json::from_str(json).unwrap();
    assert_eq!(request.nonce, "n");
    assert_eq!(request.username, "u");
    assert_eq!(request.password, "p");
    assert_eq!(request.admin, None);
    assert_eq!(request.user_type, None);
    assert_eq!(request.displayname, None);
    assert_eq!(request.mac, "m");
}

#[test]
fn test_admin_register_response_serialization() {
    use synapse_rust::services::admin_registration_service::AdminRegisterResponse;
    let response = AdminRegisterResponse {
        access_token: "at_123".to_string(),
        refresh_token: "rt_456".to_string(),
        expires_in: 3600,
        device_id: "DEV1".to_string(),
        user_id: "@admin:localhost".to_string(),
        home_server: "localhost".to_string(),
    };
    let json = serde_json::to_string(&response).unwrap();
    assert!(json.contains("at_123"));
    assert!(json.contains("rt_456"));
    assert!(json.contains("@admin:localhost"));
    assert!(json.contains("3600"));
    let parsed: AdminRegisterResponse = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.access_token, "at_123");
    assert_eq!(parsed.refresh_token, "rt_456");
    assert_eq!(parsed.expires_in, 3600);
    assert_eq!(parsed.device_id, "DEV1");
    assert_eq!(parsed.user_id, "@admin:localhost");
    assert_eq!(parsed.home_server, "localhost");
}

#[test]
fn test_compute_hmac_consistency() {
    let secret = "my_secret";
    let nonce = "nonce123";
    let username = "user1";
    let password = "pass1";
    let mac1 = compute_hmac(secret, nonce, username, password, false, None);
    let mac2 = compute_hmac(secret, nonce, username, password, false, None);
    assert_eq!(mac1, mac2, "same inputs should produce same HMAC");
}

#[test]
fn test_compute_hmac_different_secrets() {
    let nonce = "nonce123";
    let username = "user1";
    let password = "pass1";
    let mac1 = compute_hmac("secret_a", nonce, username, password, false, None);
    let mac2 = compute_hmac("secret_b", nonce, username, password, false, None);
    assert_ne!(mac1, mac2, "different secrets should produce different HMACs");
}

#[test]
fn test_compute_hmac_admin_vs_nonadmin() {
    let secret = "my_secret";
    let nonce = "nonce123";
    let username = "user1";
    let password = "pass1";
    let mac_admin = compute_hmac(secret, nonce, username, password, true, None);
    let mac_notadmin = compute_hmac(secret, nonce, username, password, false, None);
    assert_ne!(mac_admin, mac_notadmin, "admin and non-admin HMACs should differ");
}

#[test]
fn test_compute_hmac_with_user_type() {
    let secret = "my_secret";
    let nonce = "nonce123";
    let username = "user1";
    let password = "pass1";
    let mac_without = compute_hmac(secret, nonce, username, password, false, None);
    let mac_with = compute_hmac(secret, nonce, username, password, false, Some("bot"));
    assert_ne!(mac_without, mac_with, "HMAC with user_type should differ from without");
}

#[tokio::test]
async fn test_register_admin_user_duplicate_username() {
    let pool = crate::require_test_pool().await;
    let shared_secret = "test_shared_secret";
    let service = create_service(&pool, shared_secret, true);
    let id = unique_id();
    let username = format!("dup_user_{id}");
    let password = "Password123!";
    let nonce1 = service.generate_nonce().await.unwrap().nonce;
    let mac1 = compute_hmac(shared_secret, &nonce1, &username, password, false, None);
    let request1 = AdminRegisterRequest {
        nonce: nonce1,
        username: username.clone(),
        password: password.to_string(),
        admin: Some(false),
        user_type: None,
        displayname: None,
        mac: mac1,
    };
    let result1 = service.register_admin_user(request1).await;
    assert!(result1.is_ok(), "first registration should succeed");
    let nonce2 = service.generate_nonce().await.unwrap().nonce;
    let mac2 = compute_hmac(shared_secret, &nonce2, &username, password, false, None);
    let request2 = AdminRegisterRequest {
        nonce: nonce2,
        username,
        password: password.to_string(),
        admin: Some(false),
        user_type: None,
        displayname: None,
        mac: mac2,
    };
    let result2 = service.register_admin_user(request2).await;
    assert!(result2.is_err(), "duplicate username registration should fail");
}

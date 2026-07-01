//! Integration tests for `OpenClawService` at `synapse-services/src/openclaw_service.rs`.
//!
//! Covers all 30 public methods of `OpenClawService`, exercising the service
//! layer on top of the `OpenClawStorage` PostgreSQL persistence layer.
//! Tables exercised: `openclaw_connections`, `ai_conversations`, `ai_messages`,
//! `ai_generations`, `ai_chat_roles`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![allow(clippy::await_holding_lock)]

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use synapse_services::openclaw_service::OpenClawService;
use synapse_storage::openclaw::OpenClawStorage;

static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

fn unique_id() -> u64 {
    TEST_COUNTER.fetch_add(1, Ordering::SeqCst)
}

fn unique_user_id() -> String {
    format!("@openclaw_test_{}:localhost", unique_id())
}

fn openclaw_guard() -> &'static Mutex<()> {
    static GUARD: OnceLock<Mutex<()>> = OnceLock::new();
    GUARD.get_or_init(|| Mutex::new(()))
}

/// Warm up the shared pool on the current tokio runtime.
async fn warm_up_pool(pool: &Arc<sqlx::PgPool>) {
    for _ in 0..8 {
        match tokio::time::timeout(
            std::time::Duration::from_secs(5),
            sqlx::query("SELECT 1").execute(pool.as_ref()),
        )
        .await
        {
            Ok(Ok(_)) => return,
            Ok(Err(_)) | Err(_) => {
                tokio::time::sleep(std::time::Duration::from_millis(400)).await;
            }
        }
    }
    let _ = sqlx::query("SELECT 1").execute(pool.as_ref()).await;
}

/// Clean all OpenClaw tables in dependency order (children first).
async fn setup(pool: &Arc<sqlx::PgPool>) {
    warm_up_pool(pool).await;
    // ai_messages → ai_conversations → openclaw_connections (FK cascade handles some, clean explicitly)
    sqlx::query("DELETE FROM ai_messages").execute(pool.as_ref()).await.ok();
    sqlx::query("DELETE FROM ai_generations").execute(pool.as_ref()).await.ok();
    sqlx::query("DELETE FROM ai_conversations").execute(pool.as_ref()).await.ok();
    sqlx::query("DELETE FROM ai_chat_roles").execute(pool.as_ref()).await.ok();
    sqlx::query("DELETE FROM openclaw_connections").execute(pool.as_ref()).await.ok();
}

/// Build a service with no encryption key configured.
fn make_service(storage: &Arc<OpenClawStorage>) -> OpenClawService {
    OpenClawService::new(storage.clone(), None)
}

/// Build a service with a fixed encryption key for API-key encryption tests.
fn make_service_with_key(storage: &Arc<OpenClawStorage>) -> OpenClawService {
    let key = {
        use sha2::{Digest, Sha256};
        let mut k = [0u8; 32];
        let digest = Sha256::digest(b"openclaw-test-encryption-key");
        k.copy_from_slice(&digest);
        k
    };
    OpenClawService::new(storage.clone(), Some(key))
}

// =============================================================================
// new / resolve_encryption_key
// =============================================================================

#[tokio::test]
async fn test_service_construction() {
    let _guard = openclaw_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = Arc::new(OpenClawStorage::new(pool.clone()));
    let service = make_service(&storage);

    // A trivial call proves the service was constructed with a usable pool.
    let user = unique_user_id();
    let result = service.list_connections(&user).await;
    assert!(result.is_ok(), "list_connections on empty user should succeed: {result:?}");
    assert!(result.unwrap().is_empty());
}

#[tokio::test]
async fn test_resolve_encryption_key_env_var_precedence() {
    let _guard = openclaw_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = Arc::new(OpenClawStorage::new(pool.clone()));
    let _service = make_service(&storage);

    // Set the env var; it should take precedence over macaroon / security secrets.
    let saved = std::env::var("API_KEY_ENCRYPTION_KEY").ok();
    std::env::set_var("API_KEY_ENCRYPTION_KEY", "env-explicit-key");
    let key = OpenClawService::resolve_encryption_key(Some("macaroon-secret"), "security-secret");
    std::env::remove_var("API_KEY_ENCRYPTION_KEY");
    if let Some(s) = saved {
        std::env::set_var("API_KEY_ENCRYPTION_KEY", s);
    }

    assert!(key.is_some(), "env var should produce a key");
    // The derived key must not be the raw secret bytes.
    let k = key.unwrap();
    assert_ne!(&k[..12], b"env-explicit");
}

#[tokio::test]
async fn test_resolve_encryption_key_macaroon_secret() {
    let _guard = openclaw_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = Arc::new(OpenClawStorage::new(pool.clone()));
    let _service = make_service(&storage);

    let saved = std::env::var("API_KEY_ENCRYPTION_KEY").ok();
    std::env::remove_var("API_KEY_ENCRYPTION_KEY");
    let key = OpenClawService::resolve_encryption_key(Some("macaroon-secret-value"), "security-fallback");
    if let Some(s) = saved {
        std::env::set_var("API_KEY_ENCRYPTION_KEY", s);
    }

    assert!(key.is_some(), "macaroon secret should produce a key");
}

#[tokio::test]
async fn test_resolve_encryption_key_security_secret_fallback() {
    let _guard = openclaw_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = Arc::new(OpenClawStorage::new(pool.clone()));
    let _service = make_service(&storage);

    let saved = std::env::var("API_KEY_ENCRYPTION_KEY").ok();
    std::env::remove_var("API_KEY_ENCRYPTION_KEY");
    // No macaroon secret → should fall back to security_secret.
    let key = OpenClawService::resolve_encryption_key(None, "security-only-secret");
    if let Some(s) = saved {
        std::env::set_var("API_KEY_ENCRYPTION_KEY", s);
    }

    assert!(key.is_some(), "security secret should produce a key as fallback");
}

#[tokio::test]
async fn test_resolve_encryption_key_none_when_all_empty() {
    let _guard = openclaw_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = Arc::new(OpenClawStorage::new(pool.clone()));
    let _service = make_service(&storage);

    let saved = std::env::var("API_KEY_ENCRYPTION_KEY").ok();
    std::env::remove_var("API_KEY_ENCRYPTION_KEY");
    // Empty macaroon + empty security → None.
    let key = OpenClawService::resolve_encryption_key(Some("  "), "  ");
    if let Some(s) = saved {
        std::env::set_var("API_KEY_ENCRYPTION_KEY", s);
    }

    assert!(key.is_none(), "all-empty inputs should yield no key");
}

// =============================================================================
// ensure_user_allowed
// =============================================================================

#[tokio::test]
async fn test_ensure_user_allowed_non_guest() {
    let _guard = openclaw_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = Arc::new(OpenClawStorage::new(pool.clone()));
    let service = make_service(&storage);

    assert!(service.ensure_user_allowed(false).is_ok());
}

#[tokio::test]
async fn test_ensure_user_allowed_guest_forbidden() {
    let _guard = openclaw_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = Arc::new(OpenClawStorage::new(pool.clone()));
    let service = make_service(&storage);

    let err = service.ensure_user_allowed(true).unwrap_err();
    assert!(err.is_forbidden(), "guest access should be forbidden: {err:?}");
}

// =============================================================================
// ensure_resource_owner
// =============================================================================

#[tokio::test]
async fn test_ensure_resource_owner_same_user() {
    let _guard = openclaw_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = Arc::new(OpenClawStorage::new(pool.clone()));
    let service = make_service(&storage);

    let user = "@owner:localhost";
    assert!(service.ensure_resource_owner(user, user, "not found msg").is_ok());
}

#[tokio::test]
async fn test_ensure_resource_owner_different_not_found() {
    let _guard = openclaw_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = Arc::new(OpenClawStorage::new(pool.clone()));
    let service = make_service(&storage);

    let err = service.ensure_resource_owner("@owner:localhost", "@other:localhost", "Resource not found").unwrap_err();
    assert!(err.is_not_found(), "non-owner should get NotFound: {err:?}");
}

// =============================================================================
// validate_base_url
// =============================================================================

#[tokio::test]
async fn test_validate_base_url_accepts_https() {
    let _guard = openclaw_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = Arc::new(OpenClawStorage::new(pool.clone()));
    let service = make_service(&storage);

    assert!(service.validate_base_url("https://api.openai.com").is_ok());
    assert!(service.validate_base_url("https://api.example.com/v1/").is_ok());
}

#[tokio::test]
async fn test_validate_base_url_accepts_http() {
    let _guard = openclaw_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = Arc::new(OpenClawStorage::new(pool.clone()));
    let service = make_service(&storage);

    assert!(service.validate_base_url("http://api.example.com").is_ok());
}

#[tokio::test]
async fn test_validate_base_url_rejects_localhost() {
    let _guard = openclaw_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = Arc::new(OpenClawStorage::new(pool.clone()));
    let service = make_service(&storage);

    assert!(service.validate_base_url("http://localhost:8080").is_err());
    assert!(service.validate_base_url("http://localhost.").is_err());
    assert!(service.validate_base_url("http://sub.localhost").is_err());
}

#[tokio::test]
async fn test_validate_base_url_rejects_private_ipv4() {
    let _guard = openclaw_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = Arc::new(OpenClawStorage::new(pool.clone()));
    let service = make_service(&storage);

    assert!(service.validate_base_url("http://10.0.0.1").is_err());
    assert!(service.validate_base_url("http://172.16.0.1").is_err());
    assert!(service.validate_base_url("http://192.168.1.1").is_err());
}

#[tokio::test]
async fn test_validate_base_url_rejects_loopback_ipv4() {
    let _guard = openclaw_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = Arc::new(OpenClawStorage::new(pool.clone()));
    let service = make_service(&storage);

    assert!(service.validate_base_url("http://127.0.0.1").is_err());
}

#[tokio::test]
async fn test_validate_base_url_rejects_non_http_scheme() {
    let _guard = openclaw_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = Arc::new(OpenClawStorage::new(pool.clone()));
    let service = make_service(&storage);

    assert!(service.validate_base_url("ftp://api.example.com").is_err());
    assert!(service.validate_base_url("file:///etc/passwd").is_err());
}

#[tokio::test]
async fn test_validate_base_url_rejects_invalid_and_missing_host() {
    let _guard = openclaw_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = Arc::new(OpenClawStorage::new(pool.clone()));
    let service = make_service(&storage);

    // Invalid URL.
    assert!(service.validate_base_url("not-a-url").is_err());
    // Missing host (scheme-only).
    assert!(service.validate_base_url("https://").is_err());
    assert!(service.validate_base_url("https:///path").is_err());
}

// =============================================================================
// encrypt_optional_api_key
// =============================================================================

#[tokio::test]
async fn test_encrypt_optional_api_key_none() {
    let _guard = openclaw_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = Arc::new(OpenClawStorage::new(pool.clone()));
    let service = make_service(&storage);

    let result = service.encrypt_optional_api_key(None).unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn test_encrypt_optional_api_key_no_key_configured() {
    let _guard = openclaw_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = Arc::new(OpenClawStorage::new(pool.clone()));
    let service = make_service(&storage);

    let err = service.encrypt_optional_api_key(Some("sk-test".to_string())).unwrap_err();
    assert!(err.is_internal(), "encrypting without a configured key should be Internal: {err:?}");
}

#[tokio::test]
async fn test_encrypt_optional_api_key_with_key() {
    let _guard = openclaw_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = Arc::new(OpenClawStorage::new(pool.clone()));
    let service = make_service_with_key(&storage);

    let encrypted = service.encrypt_optional_api_key(Some("sk-test-key-123".to_string())).unwrap();
    assert!(encrypted.is_some());
    // The encrypted value must differ from the plaintext.
    let enc = encrypted.unwrap();
    assert!(!enc.contains("sk-test-key-123"));
    // Encrypted value should be base64-decodable (nonce + ciphertext).
    use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
    assert!(BASE64.decode(&enc).is_ok());
}

// =============================================================================
// test_connection_health
// =============================================================================

#[tokio::test]
async fn test_test_connection_health_unreachable() {
    let _guard = openclaw_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = Arc::new(OpenClawStorage::new(pool.clone()));
    let service = make_service(&storage);

    // .invalid TLD (RFC 2606) never resolves → health check must return false quickly.
    let healthy = service.test_connection_health("https://nonexistent-health-check.invalid").await;
    assert!(!healthy, "unreachable host should report unhealthy");
}

// =============================================================================
// Connection CRUD: list_connections / create_connection
// =============================================================================

#[tokio::test]
async fn test_list_connections_empty() {
    let _guard = openclaw_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = Arc::new(OpenClawStorage::new(pool.clone()));
    let service = make_service(&storage);

    let user = unique_user_id();
    let conns = service.list_connections(&user).await.unwrap();
    assert!(conns.is_empty());
}

#[tokio::test]
async fn test_create_connection_basic() {
    let _guard = openclaw_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = Arc::new(OpenClawStorage::new(pool.clone()));
    let service = make_service(&storage);

    let user = unique_user_id();
    let name = format!("conn-{}", unique_id());
    let conn = service
        .create_connection(
            &user,
            &name,
            "openai",
            "https://api.openai.com",
            None,
            None,
            false,
        )
        .await
        .unwrap();

    assert_eq!(conn.user_id, user);
    assert_eq!(conn.name, name);
    assert_eq!(conn.provider, "openai");
    assert_eq!(conn.base_url, "https://api.openai.com");
    assert!(!conn.is_default);
    assert!(conn.is_active);
    assert!(conn.encrypted_api_key.is_none());
    assert!(conn.config.is_none() || conn.config == Some(serde_json::json!({})));
}

#[tokio::test]
async fn test_create_connection_with_api_key_encrypted() {
    let _guard = openclaw_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = Arc::new(OpenClawStorage::new(pool.clone()));
    let service = make_service_with_key(&storage);

    let user = unique_user_id();
    let conn = service
        .create_connection(
            &user,
            &format!("conn-{}", unique_id()),
            "anthropic",
            "https://api.anthropic.com",
            Some("sk-live-key-xyz".to_string()),
            Some(serde_json::json!({"model": "claude-3"})),
            false,
        )
        .await
        .unwrap();

    // The stored api key must be encrypted, not plaintext.
    let enc = conn.encrypted_api_key.expect("api key should be stored");
    assert!(!enc.contains("sk-live-key-xyz"));
    assert_eq!(conn.config, Some(serde_json::json!({"model": "claude-3"})));
}

#[tokio::test]
async fn test_create_connection_invalid_base_url() {
    let _guard = openclaw_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = Arc::new(OpenClawStorage::new(pool.clone()));
    let service = make_service(&storage);

    let user = unique_user_id();
    let err = service
        .create_connection(
            &user,
            &format!("conn-{}", unique_id()),
            "custom",
            "http://localhost:9090",
            None,
            None,
            false,
        )
        .await
        .unwrap_err();
    assert!(err.is_bad_request(), "invalid base_url should be BadRequest: {err:?}");
}

#[tokio::test]
async fn test_create_connection_default_demotes_previous() {
    let _guard = openclaw_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = Arc::new(OpenClawStorage::new(pool.clone()));
    let service = make_service(&storage);

    let user = unique_user_id();
    let conn1 = service
        .create_connection(&user, "first", "openai", "https://api.openai.com", None, None, true)
        .await
        .unwrap();
    assert!(conn1.is_default);

    // Creating a second default connection should demote the first.
    let conn2 = service
        .create_connection(&user, "second", "anthropic", "https://api.anthropic.com", None, None, true)
        .await
        .unwrap();
    assert!(conn2.is_default);

    // Verify the first is no longer default.
    let conns = service.list_connections(&user).await.unwrap();
    let first = conns.iter().find(|c| c.name == "first").unwrap();
    assert!(!first.is_default, "previous default should be demoted");
    let second = conns.iter().find(|c| c.name == "second").unwrap();
    assert!(second.is_default);
}

// =============================================================================
// Connection CRUD: get_connection_for_user
// =============================================================================

#[tokio::test]
async fn test_get_connection_existing_and_nonexistent() {
    let _guard = openclaw_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = Arc::new(OpenClawStorage::new(pool.clone()));
    let service = make_service(&storage);

    let user = unique_user_id();
    let conn = service
        .create_connection(&user, "test-conn", "openai", "https://api.openai.com", None, None, false)
        .await
        .unwrap();

    let fetched = service.get_connection_for_user(conn.id, &user).await.unwrap();
    assert_eq!(fetched.id, conn.id);
    assert_eq!(fetched.name, "test-conn");

    // Nonexistent ID → NotFound.
    let err = service.get_connection_for_user(99_999_999, &user).await.unwrap_err();
    assert!(err.is_not_found(), "nonexistent connection should be NotFound: {err:?}");
}

#[tokio::test]
async fn test_get_connection_wrong_owner() {
    let _guard = openclaw_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = Arc::new(OpenClawStorage::new(pool.clone()));
    let service = make_service(&storage);

    let owner = unique_user_id();
    let other = unique_user_id();
    let conn = service
        .create_connection(&owner, "owned", "openai", "https://api.openai.com", None, None, false)
        .await
        .unwrap();

    // A different user requesting → NotFound (ownership hides existence).
    let err = service.get_connection_for_user(conn.id, &other).await.unwrap_err();
    assert!(err.is_not_found(), "non-owner should get NotFound: {err:?}");
}

// =============================================================================
// Connection CRUD: update_connection
// =============================================================================

#[tokio::test]
async fn test_update_connection_fields() {
    let _guard = openclaw_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = Arc::new(OpenClawStorage::new(pool.clone()));
    let service = make_service(&storage);

    let user = unique_user_id();
    let conn = service
        .create_connection(&user, "orig", "openai", "https://api.openai.com", None, None, false)
        .await
        .unwrap();

    let updated = service
        .update_connection(
            conn.id,
            &user,
            Some("renamed".to_string()),
            Some("https://api.anthropic.com".to_string()),
            None,
            Some(serde_json::json!({"k": "v"})),
            Some(true),
            Some(false),
        )
        .await
        .unwrap();

    assert_eq!(updated.name, "renamed");
    assert_eq!(updated.base_url, "https://api.anthropic.com");
    assert_eq!(updated.config, Some(serde_json::json!({"k": "v"})));
    assert!(updated.is_default);
    assert!(!updated.is_active);
}

#[tokio::test]
async fn test_update_connection_wrong_owner() {
    let _guard = openclaw_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = Arc::new(OpenClawStorage::new(pool.clone()));
    let service = make_service(&storage);

    let owner = unique_user_id();
    let other = unique_user_id();
    let conn = service
        .create_connection(&owner, "owned", "openai", "https://api.openai.com", None, None, false)
        .await
        .unwrap();

    let err = service
        .update_connection(conn.id, &other, Some("hijack".to_string()), None, None, None, None, None)
        .await
        .unwrap_err();
    assert!(err.is_not_found(), "non-owner update should be NotFound: {err:?}");
}

// =============================================================================
// Connection CRUD: delete_connection
// =============================================================================

#[tokio::test]
async fn test_delete_connection() {
    let _guard = openclaw_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = Arc::new(OpenClawStorage::new(pool.clone()));
    let service = make_service(&storage);

    let user = unique_user_id();
    let conn = service
        .create_connection(&user, "to-delete", "openai", "https://api.openai.com", None, None, false)
        .await
        .unwrap();

    service.delete_connection(conn.id, &user).await.unwrap();

    // After deletion, get should return NotFound.
    let err = service.get_connection_for_user(conn.id, &user).await.unwrap_err();
    assert!(err.is_not_found());
}

#[tokio::test]
async fn test_delete_connection_wrong_owner() {
    let _guard = openclaw_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = Arc::new(OpenClawStorage::new(pool.clone()));
    let service = make_service(&storage);

    let owner = unique_user_id();
    let other = unique_user_id();
    let conn = service
        .create_connection(&owner, "owned", "openai", "https://api.openai.com", None, None, false)
        .await
        .unwrap();

    let err = service.delete_connection(conn.id, &other).await.unwrap_err();
    assert!(err.is_not_found(), "non-owner delete should be NotFound: {err:?}");
}

// =============================================================================
// test_connection
// =============================================================================

#[tokio::test]
async fn test_test_connection_returns_tuple() {
    let _guard = openclaw_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = Arc::new(OpenClawStorage::new(pool.clone()));
    let service = make_service(&storage);

    let user = unique_user_id();
    // Use a .invalid URL that passes SSRF validation but is unreachable.
    let conn = service
        .create_connection(&user, "unreachable", "custom", "https://unreachable.invalid", None, None, false)
        .await
        .unwrap();

    let (returned_conn, healthy, latency_ms) = service.test_connection(conn.id, &user).await.unwrap();
    assert_eq!(returned_conn.id, conn.id);
    assert!(!healthy, "unreachable host should report unhealthy");
    assert!(latency_ms >= 0, "latency should be non-negative");
}

// =============================================================================
// Conversation CRUD: list_conversations / create_conversation
// =============================================================================

#[tokio::test]
async fn test_list_conversations_empty() {
    let _guard = openclaw_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = Arc::new(OpenClawStorage::new(pool.clone()));
    let service = make_service(&storage);

    let user = unique_user_id();
    let (convs, next) = service.list_conversations(&user, 10, None).await.unwrap();
    assert!(convs.is_empty());
    assert!(next.is_none());
}

#[tokio::test]
async fn test_create_conversation_basic() {
    let _guard = openclaw_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = Arc::new(OpenClawStorage::new(pool.clone()));
    let service = make_service(&storage);

    let user = unique_user_id();
    let conv = service
        .create_conversation(
            &user,
            None,
            Some("My Chat"),
            Some("gpt-4"),
            Some("You are helpful."),
            Some(0.5),
            Some(2048),
        )
        .await
        .unwrap();

    assert_eq!(conv.user_id, user);
    assert_eq!(conv.title.as_deref(), Some("My Chat"));
    assert_eq!(conv.model_id.as_deref(), Some("gpt-4"));
    assert_eq!(conv.system_prompt.as_deref(), Some("You are helpful."));
    assert!(!conv.is_pinned);
    assert!(conv.connection_id.is_none());
}

#[tokio::test]
async fn test_create_conversation_with_connection_ownership() {
    let _guard = openclaw_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = Arc::new(OpenClawStorage::new(pool.clone()));
    let service = make_service(&storage);

    let owner = unique_user_id();
    let other = unique_user_id();
    let conn = service
        .create_connection(&owner, "my-conn", "openai", "https://api.openai.com", None, None, false)
        .await
        .unwrap();

    // Non-owner trying to use the connection → NotFound.
    let err = service
        .create_conversation(&other, Some(conn.id), Some("title"), None, None, None, None)
        .await
        .unwrap_err();
    assert!(err.is_not_found(), "non-owner connection use should be NotFound: {err:?}");

    // Owner succeeds.
    let conv = service
        .create_conversation(&owner, Some(conn.id), Some("title"), None, None, None, None)
        .await
        .unwrap();
    assert_eq!(conv.connection_id, Some(conn.id));
}

// =============================================================================
// Conversation CRUD: get_conversation_for_user
// =============================================================================

#[tokio::test]
async fn test_get_conversation_existing_and_nonexistent() {
    let _guard = openclaw_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = Arc::new(OpenClawStorage::new(pool.clone()));
    let service = make_service(&storage);

    let user = unique_user_id();
    let conv = service
        .create_conversation(&user, None, Some("Test"), None, None, None, None)
        .await
        .unwrap();

    let fetched = service.get_conversation_for_user(conv.id, &user).await.unwrap();
    assert_eq!(fetched.id, conv.id);
    assert_eq!(fetched.title.as_deref(), Some("Test"));

    // Nonexistent.
    let err = service.get_conversation_for_user(99_999_999, &user).await.unwrap_err();
    assert!(err.is_not_found());
}

#[tokio::test]
async fn test_get_conversation_wrong_owner() {
    let _guard = openclaw_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = Arc::new(OpenClawStorage::new(pool.clone()));
    let service = make_service(&storage);

    let owner = unique_user_id();
    let other = unique_user_id();
    let conv = service
        .create_conversation(&owner, None, Some("owned"), None, None, None, None)
        .await
        .unwrap();

    let err = service.get_conversation_for_user(conv.id, &other).await.unwrap_err();
    assert!(err.is_not_found(), "non-owner should get NotFound: {err:?}");
}

// =============================================================================
// Conversation CRUD: update_conversation
// =============================================================================

#[tokio::test]
async fn test_update_conversation() {
    let _guard = openclaw_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = Arc::new(OpenClawStorage::new(pool.clone()));
    let service = make_service(&storage);

    let user = unique_user_id();
    let conv = service
        .create_conversation(&user, None, Some("orig"), None, None, None, None)
        .await
        .unwrap();

    let updated = service
        .update_conversation(
            conv.id,
            &user,
            Some("renamed"),
            Some("new prompt"),
            Some(0.9),
            Some(8192),
            Some(true),
        )
        .await
        .unwrap();

    assert_eq!(updated.title.as_deref(), Some("renamed"));
    assert_eq!(updated.system_prompt.as_deref(), Some("new prompt"));
    assert!(updated.is_pinned);
}

#[tokio::test]
async fn test_update_conversation_wrong_owner() {
    let _guard = openclaw_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = Arc::new(OpenClawStorage::new(pool.clone()));
    let service = make_service(&storage);

    let owner = unique_user_id();
    let other = unique_user_id();
    let conv = service
        .create_conversation(&owner, None, Some("owned"), None, None, None, None)
        .await
        .unwrap();

    let err = service
        .update_conversation(conv.id, &other, Some("hijack"), None, None, None, None)
        .await
        .unwrap_err();
    assert!(err.is_not_found());
}

// =============================================================================
// Conversation CRUD: delete_conversation
// =============================================================================

#[tokio::test]
async fn test_delete_conversation() {
    let _guard = openclaw_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = Arc::new(OpenClawStorage::new(pool.clone()));
    let service = make_service(&storage);

    let user = unique_user_id();
    let conv = service
        .create_conversation(&user, None, Some("to-delete"), None, None, None, None)
        .await
        .unwrap();

    service.delete_conversation(conv.id, &user).await.unwrap();

    let err = service.get_conversation_for_user(conv.id, &user).await.unwrap_err();
    assert!(err.is_not_found());
}

#[tokio::test]
async fn test_delete_conversation_wrong_owner() {
    let _guard = openclaw_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = Arc::new(OpenClawStorage::new(pool.clone()));
    let service = make_service(&storage);

    let owner = unique_user_id();
    let other = unique_user_id();
    let conv = service
        .create_conversation(&owner, None, Some("owned"), None, None, None, None)
        .await
        .unwrap();

    let err = service.delete_conversation(conv.id, &other).await.unwrap_err();
    assert!(err.is_not_found());
}

// =============================================================================
// Conversation: list_conversations pagination
// =============================================================================

#[tokio::test]
async fn test_list_conversations_pagination() {
    let _guard = openclaw_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = Arc::new(OpenClawStorage::new(pool.clone()));
    let service = make_service(&storage);

    let user = unique_user_id();
    // Create 3 conversations.
    let mut conv_ids = Vec::new();
    for i in 0..3 {
        let conv = service
            .create_conversation(&user, None, Some(&format!("conv-{i}")), None, None, None, None)
            .await
            .unwrap();
        conv_ids.push(conv.id);
        // Small delay to ensure distinct updated_ts values.
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    }

    // Page 1: limit=2.
    let (page1, next1) = service.list_conversations(&user, 2, None).await.unwrap();
    assert_eq!(page1.len(), 2, "first page should have 2 conversations");
    let cursor1 = next1.expect("first page should yield a cursor");

    // Page 2: continue from cursor.
    let (page2, next2) = service.list_conversations(&user, 2, Some(cursor1)).await.unwrap();
    assert_eq!(page2.len(), 1, "second page should have 1 conversation");
    assert!(next2.is_none(), "no further pages");

    // No overlap between pages.
    let page1_ids: std::collections::HashSet<i64> = page1.iter().map(|c| c.id).collect();
    for c in &page2 {
        assert!(!page1_ids.contains(&c.id), "pages should not overlap");
    }
}

#[tokio::test]
async fn test_list_conversations_invalid_cursor() {
    let _guard = openclaw_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = Arc::new(OpenClawStorage::new(pool.clone()));
    let service = make_service(&storage);

    let user = unique_user_id();
    let err = service.list_conversations(&user, 10, Some("not-a-valid-cursor".to_string())).await.unwrap_err();
    assert!(err.is_bad_request(), "invalid cursor should be BadRequest: {err:?}");
}

// =============================================================================
// Message CRUD: list_messages / send_message
// =============================================================================

#[tokio::test]
async fn test_list_messages_empty() {
    let _guard = openclaw_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = Arc::new(OpenClawStorage::new(pool.clone()));
    let service = make_service(&storage);

    let user = unique_user_id();
    let conv = service
        .create_conversation(&user, None, Some("empty"), None, None, None, None)
        .await
        .unwrap();

    let (msgs, next) = service.list_messages(conv.id, &user, 10, None, None).await.unwrap();
    assert!(msgs.is_empty());
    assert!(next.is_none());
}

#[tokio::test]
async fn test_send_message_and_list() {
    let _guard = openclaw_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = Arc::new(OpenClawStorage::new(pool.clone()));
    let service = make_service(&storage);

    let user = unique_user_id();
    let conv = service
        .create_conversation(&user, None, Some("chat"), None, None, None, None)
        .await
        .unwrap();

    // Send a user message (default role).
    let msg1 = service.send_message(conv.id, &user, "Hello, world!", None, None, None).await.unwrap();
    assert_eq!(msg1.conversation_id, conv.id);
    assert_eq!(msg1.role, "user");
    assert_eq!(msg1.content, "Hello, world!");

    // Send an assistant message with explicit role.
    let msg2 = service
        .send_message(conv.id, &user, "Hi there!", Some("assistant"), None, None)
        .await
        .unwrap();
    assert_eq!(msg2.role, "assistant");

    // List messages — should be DESC by (created_ts, id).
    let (msgs, next) = service.list_messages(conv.id, &user, 10, None, None).await.unwrap();
    assert_eq!(msgs.len(), 2);
    assert!(next.is_none());
    // Most recent first.
    assert_eq!(msgs[0].id, msg2.id);
    assert_eq!(msgs[1].id, msg1.id);
}

#[tokio::test]
async fn test_send_message_wrong_owner() {
    let _guard = openclaw_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = Arc::new(OpenClawStorage::new(pool.clone()));
    let service = make_service(&storage);

    let owner = unique_user_id();
    let other = unique_user_id();
    let conv = service
        .create_conversation(&owner, None, Some("owned"), None, None, None, None)
        .await
        .unwrap();

    let err = service.send_message(conv.id, &other, "hi", None, None, None).await.unwrap_err();
    assert!(err.is_not_found(), "non-owner send should be NotFound: {err:?}");
}

// =============================================================================
// Message CRUD: delete_message
// =============================================================================

#[tokio::test]
async fn test_delete_message() {
    let _guard = openclaw_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = Arc::new(OpenClawStorage::new(pool.clone()));
    let service = make_service(&storage);

    let user = unique_user_id();
    let conv = service
        .create_conversation(&user, None, Some("chat"), None, None, None, None)
        .await
        .unwrap();

    let msg = service.send_message(conv.id, &user, "to be deleted", None, None, None).await.unwrap();
    service.delete_message(msg.id, &user).await.unwrap();

    // After deletion, listing should be empty.
    let (msgs, _) = service.list_messages(conv.id, &user, 10, None, None).await.unwrap();
    assert!(msgs.is_empty());
}

#[tokio::test]
async fn test_delete_message_nonexistent() {
    let _guard = openclaw_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = Arc::new(OpenClawStorage::new(pool.clone()));
    let service = make_service(&storage);

    let user = unique_user_id();
    let err = service.delete_message(99_999_999, &user).await.unwrap_err();
    assert!(err.is_not_found(), "nonexistent message should be NotFound: {err:?}");
}

#[tokio::test]
async fn test_delete_message_wrong_owner() {
    let _guard = openclaw_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = Arc::new(OpenClawStorage::new(pool.clone()));
    let service = make_service(&storage);

    let owner = unique_user_id();
    let other = unique_user_id();
    let conv = service
        .create_conversation(&owner, None, Some("owned"), None, None, None, None)
        .await
        .unwrap();

    let msg = service.send_message(conv.id, &owner, "secret", None, None, None).await.unwrap();
    let err = service.delete_message(msg.id, &other).await.unwrap_err();
    assert!(err.is_not_found(), "non-owner delete should be NotFound: {err:?}");
}

// =============================================================================
// Message: list_messages pagination
// =============================================================================

#[tokio::test]
async fn test_list_messages_pagination() {
    let _guard = openclaw_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = Arc::new(OpenClawStorage::new(pool.clone()));
    let service = make_service(&storage);

    let user = unique_user_id();
    let conv = service
        .create_conversation(&user, None, Some("paginated"), None, None, None, None)
        .await
        .unwrap();

    // Create 3 messages.
    let mut msg_ids = Vec::new();
    for i in 0..3 {
        let msg = service.send_message(conv.id, &user, &format!("msg-{i}"), None, None, None).await.unwrap();
        msg_ids.push(msg.id);
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    }

    // Page 1: limit=2.
    let (page1, next1) = service.list_messages(conv.id, &user, 2, None, None).await.unwrap();
    assert_eq!(page1.len(), 2);
    let cursor1 = next1.expect("first page should yield a cursor");

    // Page 2.
    let (page2, next2) = service.list_messages(conv.id, &user, 2, Some(cursor1), None).await.unwrap();
    assert_eq!(page2.len(), 1);
    assert!(next2.is_none());

    // No overlap.
    let page1_ids: std::collections::HashSet<i64> = page1.iter().map(|m| m.id).collect();
    for m in &page2 {
        assert!(!page1_ids.contains(&m.id));
    }
}

// =============================================================================
// Generation CRUD: list_generations / create_generation
// =============================================================================

#[tokio::test]
async fn test_list_generations_empty() {
    let _guard = openclaw_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = Arc::new(OpenClawStorage::new(pool.clone()));
    let service = make_service(&storage);

    let user = unique_user_id();
    let (gens, next) = service.list_generations(&user, None, 10, None).await.unwrap();
    assert!(gens.is_empty());
    assert!(next.is_none());
}

#[tokio::test]
async fn test_create_generation_basic() {
    let _guard = openclaw_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = Arc::new(OpenClawStorage::new(pool.clone()));
    let service = make_service(&storage);

    let user = unique_user_id();
    let gen = service.create_generation(&user, None, "image", "a cat").await.unwrap();
    assert_eq!(gen.user_id, user);
    assert_eq!(gen.r#type, "image");
    assert_eq!(gen.prompt, "a cat");
    assert_eq!(gen.status, "pending");
    assert!(gen.conversation_id.is_none());
    assert!(gen.completed_ts.is_none());
}

#[tokio::test]
async fn test_create_generation_with_conversation_ownership() {
    let _guard = openclaw_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = Arc::new(OpenClawStorage::new(pool.clone()));
    let service = make_service(&storage);

    let owner = unique_user_id();
    let other = unique_user_id();
    let conv = service
        .create_conversation(&owner, None, Some("gen-chat"), None, None, None, None)
        .await
        .unwrap();

    // Non-owner → NotFound.
    let err = service.create_generation(&other, Some(conv.id), "video", "prompt").await.unwrap_err();
    assert!(err.is_not_found(), "non-owner generation on conversation should be NotFound: {err:?}");

    // Owner succeeds.
    let gen = service.create_generation(&owner, Some(conv.id), "audio", "podcast").await.unwrap();
    assert_eq!(gen.conversation_id, Some(conv.id));
    assert_eq!(gen.r#type, "audio");
}

#[tokio::test]
async fn test_list_generations_with_type_filter() {
    let _guard = openclaw_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = Arc::new(OpenClawStorage::new(pool.clone()));
    let service = make_service(&storage);

    let user = unique_user_id();
    service.create_generation(&user, None, "image", "img prompt").await.unwrap();
    service.create_generation(&user, None, "video", "vid prompt").await.unwrap();
    service.create_generation(&user, None, "image", "img 2").await.unwrap();

    // Filter by type=image.
    let (images, _) = service.list_generations(&user, Some("image"), 10, None).await.unwrap();
    assert_eq!(images.len(), 2);
    assert!(images.iter().all(|g| g.r#type == "image"));

    // No filter → all 3.
    let (all, _) = service.list_generations(&user, None, 10, None).await.unwrap();
    assert_eq!(all.len(), 3);
}

// =============================================================================
// Generation CRUD: get_generation_for_user
// =============================================================================

#[tokio::test]
async fn test_get_generation_for_user() {
    let _guard = openclaw_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = Arc::new(OpenClawStorage::new(pool.clone()));
    let service = make_service(&storage);

    let owner = unique_user_id();
    let other = unique_user_id();
    let gen = service.create_generation(&owner, None, "image", "prompt").await.unwrap();

    // Owner can fetch.
    let fetched = service.get_generation_for_user(gen.id, &owner).await.unwrap();
    assert_eq!(fetched.id, gen.id);
    assert_eq!(fetched.prompt, "prompt");

    // Nonexistent → NotFound.
    let err = service.get_generation_for_user(99_999_999, &owner).await.unwrap_err();
    assert!(err.is_not_found());

    // Wrong owner → NotFound.
    let err = service.get_generation_for_user(gen.id, &other).await.unwrap_err();
    assert!(err.is_not_found(), "non-owner should get NotFound: {err:?}");
}

// =============================================================================
// Generation CRUD: delete_generation
// =============================================================================

#[tokio::test]
async fn test_delete_generation() {
    let _guard = openclaw_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = Arc::new(OpenClawStorage::new(pool.clone()));
    let service = make_service(&storage);

    let owner = unique_user_id();
    let other = unique_user_id();
    let gen = service.create_generation(&owner, None, "image", "to-delete").await.unwrap();

    // Non-owner → NotFound.
    let err = service.delete_generation(gen.id, &other).await.unwrap_err();
    assert!(err.is_not_found());

    // Owner deletes.
    service.delete_generation(gen.id, &owner).await.unwrap();

    // After deletion, get → NotFound.
    let err = service.get_generation_for_user(gen.id, &owner).await.unwrap_err();
    assert!(err.is_not_found());
}

// =============================================================================
// Generation: list_generations pagination
// =============================================================================

#[tokio::test]
async fn test_list_generations_pagination() {
    let _guard = openclaw_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = Arc::new(OpenClawStorage::new(pool.clone()));
    let service = make_service(&storage);

    let user = unique_user_id();
    for i in 0..3 {
        service.create_generation(&user, None, "image", &format!("prompt-{i}")).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    }

    // Page 1: limit=2.
    let (page1, next1) = service.list_generations(&user, Some("image"), 2, None).await.unwrap();
    assert_eq!(page1.len(), 2);
    let cursor1 = next1.expect("first page should yield a cursor");

    // Page 2.
    let (page2, next2) = service.list_generations(&user, Some("image"), 2, Some(cursor1)).await.unwrap();
    assert_eq!(page2.len(), 1);
    assert!(next2.is_none());

    let page1_ids: std::collections::HashSet<i64> = page1.iter().map(|g| g.id).collect();
    for g in &page2 {
        assert!(!page1_ids.contains(&g.id));
    }
}

#[tokio::test]
async fn test_list_generations_invalid_cursor() {
    let _guard = openclaw_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = Arc::new(OpenClawStorage::new(pool.clone()));
    let service = make_service(&storage);

    let user = unique_user_id();
    let err = service.list_generations(&user, None, 10, Some("bad-cursor".to_string())).await.unwrap_err();
    assert!(err.is_bad_request(), "invalid cursor should be BadRequest: {err:?}");
}

// =============================================================================
// Chat Role CRUD: list_chat_roles / create_chat_role
// =============================================================================

#[tokio::test]
async fn test_list_chat_roles_empty() {
    let _guard = openclaw_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = Arc::new(OpenClawStorage::new(pool.clone()));
    let service = make_service(&storage);

    let user = unique_user_id();
    let roles = service.list_chat_roles(&user).await.unwrap();
    assert!(roles.is_empty());
}

#[tokio::test]
async fn test_create_chat_role_basic() {
    let _guard = openclaw_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = Arc::new(OpenClawStorage::new(pool.clone()));
    let service = make_service(&storage);

    let user = unique_user_id();
    let role = service
        .create_chat_role(
            &user,
            "Assistant",
            Some("A helpful assistant"),
            "You are a helpful assistant.",
            Some("gpt-4"),
            Some("mxc://localhost/avatar"),
            Some("general"),
            Some(0.7),
            Some(4096),
            false,
        )
        .await
        .unwrap();

    assert_eq!(role.user_id, user);
    assert_eq!(role.name, "Assistant");
    assert_eq!(role.description.as_deref(), Some("A helpful assistant"));
    assert_eq!(role.system_message, "You are a helpful assistant.");
    assert_eq!(role.model_id.as_deref(), Some("gpt-4"));
    assert!(!role.is_public);
}

#[tokio::test]
async fn test_list_chat_roles_includes_public() {
    let _guard = openclaw_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = Arc::new(OpenClawStorage::new(pool.clone()));
    let service = make_service(&storage);

    let owner = unique_user_id();
    let other = unique_user_id();

    // Owner creates a private role + a public role.
    service
        .create_chat_role(&owner, "private-role", None, "private system msg", None, None, None, None, None, false)
        .await
        .unwrap();
    service
        .create_chat_role(&owner, "public-role", None, "public system msg", None, None, None, None, None, true)
        .await
        .unwrap();

    // Owner sees both.
    let owner_roles = service.list_chat_roles(&owner).await.unwrap();
    assert!(owner_roles.len() >= 2, "owner should see their private + public roles");
    assert!(owner_roles.iter().any(|r| r.name == "private-role"));
    assert!(owner_roles.iter().any(|r| r.name == "public-role"));

    // Other user sees only the public role (plus any from prior tests, but filtered by is_public OR user_id).
    let other_roles = service.list_chat_roles(&other).await.unwrap();
    assert!(other_roles.iter().any(|r| r.name == "public-role"), "other should see public role");
    assert!(!other_roles.iter().any(|r| r.name == "private-role"), "other should NOT see private role");
}

// =============================================================================
// Chat Role CRUD: get_chat_role_for_user
// =============================================================================

#[tokio::test]
async fn test_get_chat_role_public_visible_to_all() {
    let _guard = openclaw_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = Arc::new(OpenClawStorage::new(pool.clone()));
    let service = make_service(&storage);

    let owner = unique_user_id();
    let other = unique_user_id();
    let role = service
        .create_chat_role(&owner, "public", None, "msg", None, None, None, None, None, true)
        .await
        .unwrap();

    // Owner can fetch.
    assert!(service.get_chat_role_for_user(role.id, &owner).await.is_ok());
    // Other user can also fetch (public).
    let fetched = service.get_chat_role_for_user(role.id, &other).await.unwrap();
    assert_eq!(fetched.id, role.id);
    assert!(fetched.is_public);
}

#[tokio::test]
async fn test_get_chat_role_private_owner_only() {
    let _guard = openclaw_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = Arc::new(OpenClawStorage::new(pool.clone()));
    let service = make_service(&storage);

    let owner = unique_user_id();
    let other = unique_user_id();
    let role = service
        .create_chat_role(&owner, "private", None, "msg", None, None, None, None, None, false)
        .await
        .unwrap();

    // Owner can fetch.
    assert!(service.get_chat_role_for_user(role.id, &owner).await.is_ok());
    // Other user → NotFound (private).
    let err = service.get_chat_role_for_user(role.id, &other).await.unwrap_err();
    assert!(err.is_not_found(), "non-owner should not see private role: {err:?}");
}

#[tokio::test]
async fn test_get_chat_role_nonexistent() {
    let _guard = openclaw_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = Arc::new(OpenClawStorage::new(pool.clone()));
    let service = make_service(&storage);

    let user = unique_user_id();
    let err = service.get_chat_role_for_user(99_999_999, &user).await.unwrap_err();
    assert!(err.is_not_found());
}

// =============================================================================
// Chat Role CRUD: update_chat_role
// =============================================================================

#[tokio::test]
async fn test_update_chat_role() {
    let _guard = openclaw_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = Arc::new(OpenClawStorage::new(pool.clone()));
    let service = make_service(&storage);

    let user = unique_user_id();
    let role = service
        .create_chat_role(&user, "orig", None, "orig msg", None, None, None, None, None, false)
        .await
        .unwrap();

    let updated = service
        .update_chat_role(
            role.id,
            &user,
            Some("renamed"),
            Some("new desc"),
            Some("new system msg"),
            Some("claude-3"),
            Some("mxc://localhost/new"),
            Some("coding"),
            Some(0.5),
            Some(8192),
            Some(true),
        )
        .await
        .unwrap();

    assert_eq!(updated.name, "renamed");
    assert_eq!(updated.description.as_deref(), Some("new desc"));
    assert_eq!(updated.system_message, "new system msg");
    assert_eq!(updated.model_id.as_deref(), Some("claude-3"));
    assert!(updated.is_public);
}

#[tokio::test]
async fn test_update_chat_role_wrong_owner() {
    let _guard = openclaw_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = Arc::new(OpenClawStorage::new(pool.clone()));
    let service = make_service(&storage);

    let owner = unique_user_id();
    let other = unique_user_id();
    // Even a public role requires ownership to update.
    let role = service
        .create_chat_role(&owner, "public", None, "msg", None, None, None, None, None, true)
        .await
        .unwrap();

    let err = service
        .update_chat_role(role.id, &other, Some("hijack"), None, None, None, None, None, None, None, None)
        .await
        .unwrap_err();
    assert!(err.is_not_found(), "non-owner update should be NotFound even for public role: {err:?}");
}

// =============================================================================
// Chat Role CRUD: delete_chat_role
// =============================================================================

#[tokio::test]
async fn test_delete_chat_role() {
    let _guard = openclaw_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = Arc::new(OpenClawStorage::new(pool.clone()));
    let service = make_service(&storage);

    let user = unique_user_id();
    let role = service
        .create_chat_role(&user, "to-delete", None, "msg", None, None, None, None, None, false)
        .await
        .unwrap();

    service.delete_chat_role(role.id, &user).await.unwrap();

    // After deletion, get → NotFound.
    let err = service.get_chat_role_for_user(role.id, &user).await.unwrap_err();
    assert!(err.is_not_found());
}

#[tokio::test]
async fn test_delete_chat_role_wrong_owner() {
    let _guard = openclaw_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = Arc::new(OpenClawStorage::new(pool.clone()));
    let service = make_service(&storage);

    let owner = unique_user_id();
    let other = unique_user_id();
    let role = service
        .create_chat_role(&owner, "owned", None, "msg", None, None, None, None, None, true)
        .await
        .unwrap();

    let err = service.delete_chat_role(role.id, &other).await.unwrap_err();
    assert!(err.is_not_found(), "non-owner delete should be NotFound: {err:?}");
}

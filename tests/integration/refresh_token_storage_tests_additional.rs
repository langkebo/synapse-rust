//! Comprehensive integration tests for `RefreshTokenStorage` at
//! `synapse-storage/src/refresh_token/mod.rs`.
//!
//! Covers all 32 public methods (27 storage methods + 5 `RecordUsageRequest` builders).
//! Uses the proven cross-runtime isolation pattern: `warm_up_pool`, a `Mutex` guard for
//! serial execution, and `AtomicU64`-derived unique IDs to prevent interference.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![allow(clippy::await_holding_lock)]

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use synapse_storage::refresh_token::{
    CreateRefreshTokenRequest, RecordUsageRequest, RotateRefreshTokenRequest, RefreshTokenStorage,
};

static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

fn unique_id() -> u64 {
    TEST_COUNTER.fetch_add(1, Ordering::SeqCst)
}

fn refresh_test_guard() -> &'static Mutex<()> {
    static GUARD: OnceLock<Mutex<()>> = OnceLock::new();
    GUARD.get_or_init(|| Mutex::new(()))
}

/// Warm up the shared pool on the current tokio runtime to avoid cross-runtime sqlx isolation issues.
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

/// Prepare schema (idempotent) and clean all refresh-token-related tables in dependency order.
async fn setup_test_database(pool: &Arc<sqlx::PgPool>) {
    warm_up_pool(pool).await;

    // Idempotent table creation (no-op when the migration already created them).
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS users (
            user_id TEXT NOT NULL PRIMARY KEY,
            username TEXT NOT NULL UNIQUE,
            password_hash TEXT,
            is_admin BOOLEAN DEFAULT FALSE,
            is_guest BOOLEAN DEFAULT FALSE,
            creation_ts BIGINT NOT NULL,
            deactivated BOOLEAN DEFAULT FALSE,
            displayname TEXT,
            avatar_url TEXT
        )
        "#,
    )
    .execute(pool.as_ref())
    .await
    .ok();

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS refresh_tokens (
            id BIGSERIAL PRIMARY KEY,
            token_hash TEXT NOT NULL UNIQUE,
            user_id TEXT NOT NULL,
            device_id TEXT,
            access_token_id TEXT,
            scope TEXT,
            created_ts BIGINT NOT NULL,
            expires_at BIGINT,
            last_used_ts BIGINT,
            use_count INTEGER DEFAULT 0,
            is_revoked BOOLEAN DEFAULT FALSE,
            revoked_reason TEXT,
            client_info JSONB,
            ip_address TEXT,
            user_agent TEXT
        )
        "#,
    )
    .execute(pool.as_ref())
    .await
    .ok();

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS refresh_token_usage (
            id BIGSERIAL PRIMARY KEY,
            refresh_token_id BIGINT NOT NULL,
            user_id TEXT NOT NULL,
            old_access_token_id TEXT,
            new_access_token_id TEXT,
            used_ts BIGINT NOT NULL,
            ip_address TEXT,
            user_agent TEXT,
            is_success BOOLEAN DEFAULT TRUE,
            error_message TEXT
        )
        "#,
    )
    .execute(pool.as_ref())
    .await
    .ok();

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS refresh_token_families (
            id BIGSERIAL PRIMARY KEY,
            family_id TEXT NOT NULL UNIQUE,
            user_id TEXT NOT NULL,
            device_id TEXT,
            created_ts BIGINT NOT NULL,
            last_refresh_ts BIGINT,
            refresh_count INTEGER DEFAULT 0,
            is_compromised BOOLEAN DEFAULT FALSE,
            compromised_at BIGINT
        )
        "#,
    )
    .execute(pool.as_ref())
    .await
    .ok();

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS refresh_token_rotations (
            id BIGSERIAL PRIMARY KEY,
            family_id TEXT NOT NULL,
            old_token_hash TEXT,
            new_token_hash TEXT NOT NULL,
            rotated_ts BIGINT NOT NULL,
            rotation_reason TEXT
        )
        "#,
    )
    .execute(pool.as_ref())
    .await
    .ok();

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS token_blacklist (
            id BIGSERIAL PRIMARY KEY,
            token_hash TEXT NOT NULL UNIQUE,
            token TEXT,
            token_type TEXT DEFAULT 'access',
            user_id TEXT,
            is_revoked BOOLEAN DEFAULT TRUE,
            reason TEXT,
            expires_at BIGINT
        )
        "#,
    )
    .execute(pool.as_ref())
    .await
    .ok();

    // Clean child tables first to respect any FK constraints.
    sqlx::query("DELETE FROM refresh_token_rotations").execute(pool.as_ref()).await.ok();
    sqlx::query("DELETE FROM refresh_token_usage").execute(pool.as_ref()).await.ok();
    sqlx::query("DELETE FROM refresh_token_families").execute(pool.as_ref()).await.ok();
    sqlx::query("DELETE FROM refresh_tokens").execute(pool.as_ref()).await.ok();
    sqlx::query("DELETE FROM token_blacklist").execute(pool.as_ref()).await.ok();
}

/// Insert a test user (idempotent) so FK constraints on refresh_tokens are satisfied.
async fn insert_test_user(pool: &Arc<sqlx::PgPool>, user_id: &str) {
    let username = user_id.replace('@', "").replace(':', "_");
    sqlx::query(
        "INSERT INTO users (user_id, username, creation_ts) VALUES ($1, $2, $3) ON CONFLICT (user_id) DO NOTHING",
    )
    .bind(user_id)
    .bind(&username)
    .bind(chrono::Utc::now().timestamp_millis())
    .execute(pool.as_ref())
    .await
    .ok();
}

/// Build a `CreateRefreshTokenRequest` with sensible defaults and a unique token hash.
fn make_create_request(suffix: u64, user_id: &str, expires_at: i64) -> CreateRefreshTokenRequest {
    CreateRefreshTokenRequest {
        token_hash: format!("hash_{suffix}"),
        user_id: user_id.to_string(),
        device_id: Some(format!("device_{suffix}")),
        access_token_id: Some(format!("atid_{suffix}")),
        scope: Some("openid".to_string()),
        expires_at,
        client_info: None,
        ip_address: None,
        user_agent: None,
    }
}

/// Create a token and return it, inserting the user first.
async fn create_token_helper(
    storage: &RefreshTokenStorage,
    pool: &Arc<sqlx::PgPool>,
    suffix: u64,
) -> (String, synapse_storage::refresh_token::RefreshToken) {
    let user_id = format!("@rt_user_{suffix}:localhost");
    insert_test_user(pool, &user_id).await;
    let future_ts = chrono::Utc::now().timestamp_millis() + 3_600_000;
    let request = make_create_request(suffix, &user_id, future_ts);
    let token = storage.create_token(request).await.unwrap();
    (user_id, token)
}

// =============================================================================
// new
// =============================================================================

#[tokio::test]
async fn test_new_creates_storage_with_usable_pool() {
    let _guard = refresh_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = RefreshTokenStorage::new(&pool);
    // A trivial read proves the storage holds a usable pool.
    assert!(storage.get_token("definitely_nonexistent").await.unwrap().is_none());
}

// =============================================================================
// create_token
// =============================================================================

#[tokio::test]
async fn test_create_token_basic_returns_full_record() {
    let _guard = refresh_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = RefreshTokenStorage::new(&pool);

    let suffix = unique_id();
    let (user_id, token) = create_token_helper(&storage, &pool, suffix).await;

    assert!(token.id > 0);
    assert_eq!(token.token_hash, format!("hash_{suffix}"));
    assert_eq!(token.user_id, user_id);
    assert_eq!(token.device_id, Some(format!("device_{suffix}")));
    assert_eq!(token.access_token_id, Some(format!("atid_{suffix}")));
    assert_eq!(token.scope.as_deref(), Some("openid"));
    assert!(token.created_ts > 0);
    assert!(token.expires_at.is_some());
}

#[tokio::test]
async fn test_create_token_sets_defaults() {
    let _guard = refresh_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = RefreshTokenStorage::new(&pool);

    let suffix = unique_id();
    let (_, token) = create_token_helper(&storage, &pool, suffix).await;

    // Defaults from DB / query.
    assert_eq!(token.use_count, 0);
    assert!(!token.is_revoked);
    assert!(token.last_used_ts.is_none());
    assert!(token.revoked_reason.is_none());
    assert!(token.client_info.is_none());
    assert!(token.ip_address.is_none());
    assert!(token.user_agent.is_none());
}

#[tokio::test]
async fn test_create_token_with_all_optional_fields() {
    let _guard = refresh_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = RefreshTokenStorage::new(&pool);

    let suffix = unique_id();
    let user_id = format!("@rt_full_{suffix}:localhost");
    insert_test_user(&pool, &user_id).await;
    let future_ts = chrono::Utc::now().timestamp_millis() + 3_600_000;

    let request = CreateRefreshTokenRequest {
        token_hash: format!("full_{suffix}"),
        user_id: user_id.clone(),
        device_id: Some("dev_x".to_string()),
        access_token_id: Some("atid_x".to_string()),
        scope: Some("profile".to_string()),
        expires_at: future_ts,
        client_info: Some(serde_json::json!({"client": "element"})),
        ip_address: Some("127.0.0.1".to_string()),
        user_agent: Some("Mozilla/5.0".to_string()),
    };
    let token = storage.create_token(request).await.unwrap();

    assert_eq!(token.client_info, Some(serde_json::json!({"client": "element"})));
    assert_eq!(token.ip_address.as_deref(), Some("127.0.0.1"));
    assert_eq!(token.user_agent.as_deref(), Some("Mozilla/5.0"));
}

#[tokio::test]
async fn test_create_token_without_optional_fields() {
    let _guard = refresh_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = RefreshTokenStorage::new(&pool);

    let suffix = unique_id();
    let user_id = format!("@rt_min_{suffix}:localhost");
    insert_test_user(&pool, &user_id).await;
    let future_ts = chrono::Utc::now().timestamp_millis() + 3_600_000;

    let request = CreateRefreshTokenRequest {
        token_hash: format!("min_{suffix}"),
        user_id: user_id.clone(),
        device_id: None,
        access_token_id: None,
        scope: None,
        expires_at: future_ts,
        client_info: None,
        ip_address: None,
        user_agent: None,
    };
    let token = storage.create_token(request).await.unwrap();

    assert!(token.device_id.is_none());
    assert!(token.access_token_id.is_none());
    assert!(token.scope.is_none());
}

// =============================================================================
// get_token
// =============================================================================

#[tokio::test]
async fn test_get_token_existing() {
    let _guard = refresh_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = RefreshTokenStorage::new(&pool);

    let suffix = unique_id();
    let (_, token) = create_token_helper(&storage, &pool, suffix).await;

    let fetched = storage.get_token(&token.token_hash).await.unwrap().expect("token must exist");
    assert_eq!(fetched.id, token.id);
    assert_eq!(fetched.token_hash, token.token_hash);
    assert_eq!(fetched.user_id, token.user_id);
}

#[tokio::test]
async fn test_get_token_nonexistent_returns_none() {
    let _guard = refresh_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = RefreshTokenStorage::new(&pool);

    let result = storage.get_token(&format!("nope_{}", unique_id())).await.unwrap();
    assert!(result.is_none());
}

// =============================================================================
// get_token_by_id
// =============================================================================

#[tokio::test]
async fn test_get_token_by_id_existing() {
    let _guard = refresh_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = RefreshTokenStorage::new(&pool);

    let suffix = unique_id();
    let (_, token) = create_token_helper(&storage, &pool, suffix).await;

    let fetched = storage.get_token_by_id(token.id).await.unwrap().expect("token must exist");
    assert_eq!(fetched.token_hash, token.token_hash);
}

#[tokio::test]
async fn test_get_token_by_id_nonexistent_returns_none() {
    let _guard = refresh_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = RefreshTokenStorage::new(&pool);

    let result = storage.get_token_by_id(99_999_999).await.unwrap();
    assert!(result.is_none());
}

// =============================================================================
// get_user_tokens
// =============================================================================

#[tokio::test]
async fn test_get_user_tokens_returns_all_and_ordered_desc() {
    let _guard = refresh_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = RefreshTokenStorage::new(&pool);

    let suffix = unique_id();
    let user_id = format!("@rt_user_{suffix}:localhost");
    insert_test_user(&pool, &user_id).await;
    let future_ts = chrono::Utc::now().timestamp_millis() + 3_600_000;

    for i in 0..3 {
        let req = CreateRefreshTokenRequest {
            token_hash: format!("hash_{suffix}_{i}"),
            user_id: user_id.clone(),
            device_id: None,
            access_token_id: None,
            scope: None,
            expires_at: future_ts + i, // distinct created_ts via insertion order
            client_info: None,
            ip_address: None,
            user_agent: None,
        };
        storage.create_token(req).await.unwrap();
        // small delay guarantees distinct created_ts ordering for DESC check
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    }

    let tokens = storage.get_user_tokens(&user_id).await.unwrap();
    assert_eq!(tokens.len(), 3);
    assert!(tokens.iter().all(|t| t.user_id == user_id));
    // ORDER BY created_ts DESC.
    assert!(tokens[0].created_ts >= tokens[1].created_ts);
    assert!(tokens[1].created_ts >= tokens[2].created_ts);
}

#[tokio::test]
async fn test_get_user_tokens_empty_for_user_without_tokens() {
    let _guard = refresh_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = RefreshTokenStorage::new(&pool);

    let tokens = storage.get_user_tokens(&format!("@nobody_{}:localhost", unique_id())).await.unwrap();
    assert!(tokens.is_empty());
}

// =============================================================================
// get_active_tokens
// =============================================================================

#[tokio::test]
async fn test_get_active_tokens_excludes_revoked_and_expired() {
    let _guard = refresh_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = RefreshTokenStorage::new(&pool);

    let suffix = unique_id();
    let user_id = format!("@rt_user_{suffix}:localhost");
    insert_test_user(&pool, &user_id).await;
    let now = chrono::Utc::now().timestamp_millis();
    let future_ts = now + 3_600_000;
    let past_ts = now - 3_600_000;

    // Active.
    storage
        .create_token(CreateRefreshTokenRequest {
            token_hash: format!("active_{suffix}"),
            user_id: user_id.clone(),
            device_id: None,
            access_token_id: None,
            scope: None,
            expires_at: future_ts,
            client_info: None,
            ip_address: None,
            user_agent: None,
        })
        .await
        .unwrap();
    // Revoked (still future expiry) — excluded.
    storage
        .create_token(CreateRefreshTokenRequest {
            token_hash: format!("revoked_{suffix}"),
            user_id: user_id.clone(),
            device_id: None,
            access_token_id: None,
            scope: None,
            expires_at: future_ts,
            client_info: None,
            ip_address: None,
            user_agent: None,
        })
        .await
        .unwrap();
    storage.revoke_token(&format!("revoked_{suffix}"), "logout").await.unwrap();
    // Expired (not revoked) — excluded.
    storage
        .create_token(CreateRefreshTokenRequest {
            token_hash: format!("expired_{suffix}"),
            user_id: user_id.clone(),
            device_id: None,
            access_token_id: None,
            scope: None,
            expires_at: past_ts,
            client_info: None,
            ip_address: None,
            user_agent: None,
        })
        .await
        .unwrap();

    let active = storage.get_active_tokens(&user_id).await.unwrap();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].token_hash, format!("active_{suffix}"));
    assert!(!active[0].is_revoked);
}

#[tokio::test]
async fn test_get_active_tokens_empty() {
    let _guard = refresh_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = RefreshTokenStorage::new(&pool);

    let active = storage.get_active_tokens(&format!("@empty_{}:localhost", unique_id())).await.unwrap();
    assert!(active.is_empty());
}

// =============================================================================
// revoke_token
// =============================================================================

#[tokio::test]
async fn test_revoke_token_sets_flag_and_reason() {
    let _guard = refresh_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = RefreshTokenStorage::new(&pool);

    let suffix = unique_id();
    let (_, token) = create_token_helper(&storage, &pool, suffix).await;
    assert!(!token.is_revoked);

    storage.revoke_token(&token.token_hash, "user logout").await.unwrap();

    let revoked = storage.get_token(&token.token_hash).await.unwrap().unwrap();
    assert!(revoked.is_revoked);
    assert_eq!(revoked.revoked_reason.as_deref(), Some("user logout"));
}

#[tokio::test]
async fn test_revoke_token_nonexistent_is_noop() {
    let _guard = refresh_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = RefreshTokenStorage::new(&pool);

    // Revoking a missing token does not error.
    storage.revoke_token(&format!("missing_{}", unique_id()), "n/a").await.unwrap();
}

// =============================================================================
// revoke_token_cas
// =============================================================================

#[tokio::test]
async fn test_revoke_token_cas_first_succeeds_second_fails() {
    let _guard = refresh_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = RefreshTokenStorage::new(&pool);

    let suffix = unique_id();
    let (_, token) = create_token_helper(&storage, &pool, suffix).await;

    let first = storage.revoke_token_cas(&token.token_hash, "first").await.unwrap();
    assert!(first, "first CAS revoke should succeed on a non-revoked token");

    let second = storage.revoke_token_cas(&token.token_hash, "second").await.unwrap();
    assert!(!second, "second CAS revoke should fail on an already-revoked token");

    // Reason unchanged from the winning CAS.
    let fetched = storage.get_token(&token.token_hash).await.unwrap().unwrap();
    assert_eq!(fetched.revoked_reason.as_deref(), Some("first"));
}

#[tokio::test]
async fn test_revoke_token_cas_nonexistent_returns_false() {
    let _guard = refresh_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = RefreshTokenStorage::new(&pool);

    let result = storage.revoke_token_cas(&format!("missing_{}", unique_id()), "x").await.unwrap();
    assert!(!result);
}

// =============================================================================
// revoke_token_by_id
// =============================================================================

#[tokio::test]
async fn test_revoke_token_by_id_sets_flag() {
    let _guard = refresh_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = RefreshTokenStorage::new(&pool);

    let suffix = unique_id();
    let (_, token) = create_token_helper(&storage, &pool, suffix).await;

    storage.revoke_token_by_id(token.id, "security breach").await.unwrap();

    let revoked = storage.get_token_by_id(token.id).await.unwrap().unwrap();
    assert!(revoked.is_revoked);
    assert_eq!(revoked.revoked_reason.as_deref(), Some("security breach"));
}

#[tokio::test]
async fn test_revoke_token_by_id_nonexistent_is_noop() {
    let _guard = refresh_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = RefreshTokenStorage::new(&pool);

    storage.revoke_token_by_id(99_999_999, "n/a").await.unwrap();
}

// =============================================================================
// revoke_all_user_tokens
// =============================================================================

#[tokio::test]
async fn test_revoke_all_user_tokens_revokes_all_active() {
    let _guard = refresh_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = RefreshTokenStorage::new(&pool);

    let suffix = unique_id();
    let user_id = format!("@rt_user_{suffix}:localhost");
    insert_test_user(&pool, &user_id).await;
    let future_ts = chrono::Utc::now().timestamp_millis() + 3_600_000;

    for i in 0..3 {
        storage
            .create_token(CreateRefreshTokenRequest {
                token_hash: format!("hash_{suffix}_{i}"),
                user_id: user_id.clone(),
                device_id: None,
                access_token_id: None,
                scope: None,
                expires_at: future_ts,
                client_info: None,
                ip_address: None,
                user_agent: None,
            })
            .await
            .unwrap();
    }

    let count = storage.revoke_all_user_tokens(&user_id, "bulk revoke").await.unwrap();
    assert_eq!(count, 3);

    let tokens = storage.get_user_tokens(&user_id).await.unwrap();
    assert!(tokens.iter().all(|t| t.is_revoked));
    assert!(tokens.iter().all(|t| t.revoked_reason.as_deref() == Some("bulk revoke")));
}

#[tokio::test]
async fn test_revoke_all_user_tokens_skips_already_revoked() {
    let _guard = refresh_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = RefreshTokenStorage::new(&pool);

    let suffix = unique_id();
    let user_id = format!("@rt_user_{suffix}:localhost");
    insert_test_user(&pool, &user_id).await;
    let future_ts = chrono::Utc::now().timestamp_millis() + 3_600_000;

    // One already revoked, one active.
    for (i, hash) in [(0, format!("rev_{suffix}")), (1, format!("act_{suffix}"))] {
        storage
            .create_token(CreateRefreshTokenRequest {
                token_hash: hash,
                user_id: user_id.clone(),
                device_id: None,
                access_token_id: None,
                scope: None,
                expires_at: future_ts + i,
                client_info: None,
                ip_address: None,
                user_agent: None,
            })
            .await
            .unwrap();
    }
    storage.revoke_token(&format!("rev_{suffix}"), "prior").await.unwrap();

    let count = storage.revoke_all_user_tokens(&user_id, "bulk").await.unwrap();
    assert_eq!(count, 1, "only the non-revoked token should be affected");
}

// =============================================================================
// revoke_all_user_tokens_except_device
// =============================================================================

#[tokio::test]
async fn test_revoke_all_user_tokens_except_device_preserves_device() {
    let _guard = refresh_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = RefreshTokenStorage::new(&pool);

    let suffix = unique_id();
    let user_id = format!("@rt_user_{suffix}:localhost");
    insert_test_user(&pool, &user_id).await;
    let future_ts = chrono::Utc::now().timestamp_millis() + 3_600_000;
    let keep_device = format!("keep_{suffix}");
    let other_device = format!("other_{suffix}");

    // Two tokens on keep_device — must be preserved.
    for i in 0..2 {
        storage
            .create_token(CreateRefreshTokenRequest {
                token_hash: format!("keep_{suffix}_{i}"),
                user_id: user_id.clone(),
                device_id: Some(keep_device.clone()),
                access_token_id: None,
                scope: None,
                expires_at: future_ts + i,
                client_info: None,
                ip_address: None,
                user_agent: None,
            })
            .await
            .unwrap();
    }
    // One token on other_device — must be revoked.
    storage
        .create_token(CreateRefreshTokenRequest {
            token_hash: format!("other_{suffix}"),
            user_id: user_id.clone(),
            device_id: Some(other_device.clone()),
            access_token_id: None,
            scope: None,
            expires_at: future_ts,
            client_info: None,
            ip_address: None,
            user_agent: None,
        })
        .await
        .unwrap();

    let count = storage.revoke_all_user_tokens_except_device(&user_id, &keep_device, "logout").await.unwrap();
    assert_eq!(count, 1, "only the other-device token should be revoked");

    let keep0 = storage.get_token(&format!("keep_{suffix}_0")).await.unwrap().unwrap();
    let keep1 = storage.get_token(&format!("keep_{suffix}_1")).await.unwrap().unwrap();
    assert!(!keep0.is_revoked, "keep_device tokens must stay active");
    assert!(!keep1.is_revoked, "keep_device tokens must stay active");

    let other = storage.get_token(&format!("other_{suffix}")).await.unwrap().unwrap();
    assert!(other.is_revoked, "other-device token must be revoked");
}

#[tokio::test]
async fn test_revoke_all_user_tokens_except_device_skips_already_revoked() {
    let _guard = refresh_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = RefreshTokenStorage::new(&pool);

    let suffix = unique_id();
    let user_id = format!("@rt_user_{suffix}:localhost");
    insert_test_user(&pool, &user_id).await;
    let future_ts = chrono::Utc::now().timestamp_millis() + 3_600_000;

    // Revoked token on another device — must be skipped by the `is_revoked = FALSE` filter.
    storage
        .create_token(CreateRefreshTokenRequest {
            token_hash: format!("already_{suffix}"),
            user_id: user_id.clone(),
            device_id: Some("dev_other".to_string()),
            access_token_id: None,
            scope: None,
            expires_at: future_ts,
            client_info: None,
            ip_address: None,
            user_agent: None,
        })
        .await
        .unwrap();
    storage.revoke_token(&format!("already_{suffix}"), "prior").await.unwrap();

    let count = storage
        .revoke_all_user_tokens_except_device(&user_id, &format!("keep_{suffix}"), "x")
        .await
        .unwrap();
    assert_eq!(count, 0, "already-revoked tokens are skipped");
}

// =============================================================================
// revoke_device_tokens
// =============================================================================

#[tokio::test]
async fn test_revoke_device_tokens_only_affects_target_device() {
    let _guard = refresh_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = RefreshTokenStorage::new(&pool);

    let suffix = unique_id();
    let user_id = format!("@rt_user_{suffix}:localhost");
    insert_test_user(&pool, &user_id).await;
    let future_ts = chrono::Utc::now().timestamp_millis() + 3_600_000;
    let target_device = format!("target_{suffix}");

    for (hash, device) in [
        (format!("t_{suffix}"), target_device.clone()),
        (format!("o_{suffix}"), format!("other_{suffix}")),
    ] {
        storage
            .create_token(CreateRefreshTokenRequest {
                token_hash: hash,
                user_id: user_id.clone(),
                device_id: Some(device),
                access_token_id: None,
                scope: None,
                expires_at: future_ts,
                client_info: None,
                ip_address: None,
                user_agent: None,
            })
            .await
            .unwrap();
    }

    let count = storage.revoke_device_tokens(&user_id, &target_device, "device logout").await.unwrap();
    assert_eq!(count, 1);

    assert!(storage.get_token(&format!("t_{suffix}")).await.unwrap().unwrap().is_revoked);
    assert!(!storage.get_token(&format!("o_{suffix}")).await.unwrap().unwrap().is_revoked);
}

#[tokio::test]
async fn test_revoke_device_tokens_no_matching_device_returns_zero() {
    let _guard = refresh_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = RefreshTokenStorage::new(&pool);

    let suffix = unique_id();
    let user_id = format!("@rt_user_{suffix}:localhost");
    insert_test_user(&pool, &user_id).await;

    let count = storage.revoke_device_tokens(&user_id, "ghost_device", "x").await.unwrap();
    assert_eq!(count, 0);
}

// =============================================================================
// update_token_usage
// =============================================================================

#[tokio::test]
async fn test_update_token_usage_increments_count_and_sets_last_used() {
    let _guard = refresh_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = RefreshTokenStorage::new(&pool);

    let suffix = unique_id();
    let (_, token) = create_token_helper(&storage, &pool, suffix).await;
    assert_eq!(token.use_count, 0);
    assert!(token.last_used_ts.is_none());

    storage.update_token_usage(&token.token_hash, "atid_new").await.unwrap();

    let updated = storage.get_token(&token.token_hash).await.unwrap().unwrap();
    assert_eq!(updated.use_count, 1);
    assert!(updated.last_used_ts.is_some());
    assert_eq!(updated.access_token_id.as_deref(), Some("atid_new"));

    // Second call increments further.
    storage.update_token_usage(&token.token_hash, "atid_newer").await.unwrap();
    let updated2 = storage.get_token(&token.token_hash).await.unwrap().unwrap();
    assert_eq!(updated2.use_count, 2);
    assert_eq!(updated2.access_token_id.as_deref(), Some("atid_newer"));
}

#[tokio::test]
async fn test_update_token_usage_nonexistent_is_noop() {
    let _guard = refresh_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = RefreshTokenStorage::new(&pool);

    // Updating a missing token updates zero rows but does not error.
    storage.update_token_usage(&format!("missing_{}", unique_id()), "atid").await.unwrap();
}

// =============================================================================
// record_usage
// =============================================================================

#[tokio::test]
async fn test_record_usage_success() {
    let _guard = refresh_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = RefreshTokenStorage::new(&pool);

    let suffix = unique_id();
    let (user_id, token) = create_token_helper(&storage, &pool, suffix).await;

    let req = RecordUsageRequest::new(token.id, &user_id, "new_atid", true)
        .old_access_token_id("old_atid");
    storage.record_usage(&req).await.unwrap();

    let history = storage.get_usage_history(&user_id, 10).await.unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].refresh_token_id, token.id);
    assert_eq!(history[0].new_access_token_id.as_deref(), Some("new_atid"));
    assert_eq!(history[0].old_access_token_id.as_deref(), Some("old_atid"));
    assert!(history[0].is_success);
    assert!(history[0].error_message.is_none());
}

#[tokio::test]
async fn test_record_usage_failure_with_error_message() {
    let _guard = refresh_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = RefreshTokenStorage::new(&pool);

    let suffix = unique_id();
    let (user_id, token) = create_token_helper(&storage, &pool, suffix).await;

    let req = RecordUsageRequest::new(token.id, &user_id, "new_atid", false)
        .error_message("invalid token");
    storage.record_usage(&req).await.unwrap();

    let history = storage.get_usage_history(&user_id, 10).await.unwrap();
    assert_eq!(history.len(), 1);
    assert!(!history[0].is_success);
    assert_eq!(history[0].error_message.as_deref(), Some("invalid token"));
}

#[tokio::test]
async fn test_record_usage_with_all_builder_fields() {
    let _guard = refresh_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = RefreshTokenStorage::new(&pool);

    let suffix = unique_id();
    let (user_id, token) = create_token_helper(&storage, &pool, suffix).await;

    let req = RecordUsageRequest::new(token.id, &user_id, "atid_new", true)
        .old_access_token_id("atid_old")
        .ip_address("10.0.0.1")
        .user_agent("UA/1.0")
        .error_message("none");
    storage.record_usage(&req).await.unwrap();

    let row = &storage.get_usage_history(&user_id, 10).await.unwrap()[0];
    assert_eq!(row.ip_address.as_deref(), Some("10.0.0.1"));
    assert_eq!(row.user_agent.as_deref(), Some("UA/1.0"));
    assert_eq!(row.error_message.as_deref(), Some("none"));
}

// =============================================================================
// RecordUsageRequest builder methods
// =============================================================================

#[tokio::test]
async fn test_record_usage_request_new_defaults() {
    let req = RecordUsageRequest::new(42, "@u:localhost", "atid", true);
    assert_eq!(req.refresh_token_id, 42);
    assert_eq!(req.user_id, "@u:localhost");
    assert_eq!(req.new_access_token_id, "atid");
    assert!(req.is_success);
    // Defaults from Default impl.
    assert!(req.old_access_token_id.is_none());
    assert!(req.ip_address.is_none());
    assert!(req.user_agent.is_none());
    assert!(req.error_message.is_none());
}

#[tokio::test]
async fn test_record_usage_request_builder_methods_set_fields() {
    let req = RecordUsageRequest::new(7, "u", "atid", false)
        .old_access_token_id("old")
        .ip_address("1.2.3.4")
        .user_agent("agent")
        .error_message("boom");
    assert_eq!(req.old_access_token_id.as_deref(), Some("old"));
    assert_eq!(req.ip_address.as_deref(), Some("1.2.3.4"));
    assert_eq!(req.user_agent.as_deref(), Some("agent"));
    assert_eq!(req.error_message.as_deref(), Some("boom"));
    assert!(!req.is_success);
}

// =============================================================================
// create_family
// =============================================================================

#[tokio::test]
async fn test_create_family_with_device() {
    let _guard = refresh_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = RefreshTokenStorage::new(&pool);

    let suffix = unique_id();
    let user_id = format!("@rt_user_{suffix}:localhost");
    let family_id = format!("family_{suffix}");
    insert_test_user(&pool, &user_id).await;

    let family = storage.create_family(&family_id, &user_id, Some("dev_1")).await.unwrap();

    assert!(family.id > 0);
    assert_eq!(family.family_id, family_id);
    assert_eq!(family.user_id, user_id);
    assert_eq!(family.device_id.as_deref(), Some("dev_1"));
    assert!(!family.is_compromised);
    assert_eq!(family.refresh_count, 0);
    assert!(family.last_refresh_ts.is_none());
    assert!(family.compromised_ts.is_none());
    assert!(family.created_ts > 0);
}

#[tokio::test]
async fn test_create_family_without_device() {
    let _guard = refresh_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = RefreshTokenStorage::new(&pool);

    let suffix = unique_id();
    let user_id = format!("@rt_user_{suffix}:localhost");
    let family_id = format!("family_{suffix}");
    insert_test_user(&pool, &user_id).await;

    let family = storage.create_family(&family_id, &user_id, None).await.unwrap();
    assert!(family.device_id.is_none());
}

// =============================================================================
// get_family
// =============================================================================

#[tokio::test]
async fn test_get_family_existing() {
    let _guard = refresh_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = RefreshTokenStorage::new(&pool);

    let suffix = unique_id();
    let user_id = format!("@rt_user_{suffix}:localhost");
    let family_id = format!("family_{suffix}");
    insert_test_user(&pool, &user_id).await;
    storage.create_family(&family_id, &user_id, None).await.unwrap();

    let fetched = storage.get_family(&family_id).await.unwrap().expect("family must exist");
    assert_eq!(fetched.family_id, family_id);
    assert_eq!(fetched.user_id, user_id);
}

#[tokio::test]
async fn test_get_family_nonexistent_returns_none() {
    let _guard = refresh_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = RefreshTokenStorage::new(&pool);

    let result = storage.get_family(&format!("nope_{}", unique_id())).await.unwrap();
    assert!(result.is_none());
}

// =============================================================================
// mark_family_compromised
// =============================================================================

#[tokio::test]
async fn test_mark_family_compromised_sets_flag_and_timestamp() {
    let _guard = refresh_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = RefreshTokenStorage::new(&pool);

    let suffix = unique_id();
    let user_id = format!("@rt_user_{suffix}:localhost");
    let family_id = format!("family_{suffix}");
    insert_test_user(&pool, &user_id).await;
    storage.create_family(&family_id, &user_id, None).await.unwrap();

    let before = storage.get_family(&family_id).await.unwrap().unwrap();
    assert!(!before.is_compromised);
    assert!(before.compromised_ts.is_none());

    storage.mark_family_compromised(&family_id).await.unwrap();

    let after = storage.get_family(&family_id).await.unwrap().unwrap();
    assert!(after.is_compromised);
    assert!(after.compromised_ts.is_some());
}

#[tokio::test]
async fn test_mark_family_compromised_nonexistent_is_noop() {
    let _guard = refresh_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = RefreshTokenStorage::new(&pool);

    storage.mark_family_compromised(&format!("ghost_{}", unique_id())).await.unwrap();
}

// =============================================================================
// record_rotation
// =============================================================================

#[tokio::test]
async fn test_record_rotation_with_old_hash_increments_count() {
    let _guard = refresh_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = RefreshTokenStorage::new(&pool);

    let suffix = unique_id();
    let user_id = format!("@rt_user_{suffix}:localhost");
    let family_id = format!("family_{suffix}");
    insert_test_user(&pool, &user_id).await;
    storage.create_family(&family_id, &user_id, None).await.unwrap();

    storage.record_rotation(&family_id, Some("old_hash"), "new_hash", "refresh").await.unwrap();

    let family = storage.get_family(&family_id).await.unwrap().unwrap();
    assert_eq!(family.refresh_count, 1);
    assert!(family.last_refresh_ts.is_some());
}

#[tokio::test]
async fn test_record_rotation_without_old_hash() {
    let _guard = refresh_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = RefreshTokenStorage::new(&pool);

    let suffix = unique_id();
    let user_id = format!("@rt_user_{suffix}:localhost");
    let family_id = format!("family_{suffix}");
    insert_test_user(&pool, &user_id).await;
    storage.create_family(&family_id, &user_id, None).await.unwrap();

    // First rotation has no old token (initial issue).
    storage.record_rotation(&family_id, None, "first_hash", "initial").await.unwrap();

    let rotations = storage.get_rotations(&family_id).await.unwrap();
    assert_eq!(rotations.len(), 1);
    assert!(rotations[0].old_token_hash.is_none());
    assert_eq!(rotations[0].new_token_hash, "first_hash");
    assert_eq!(rotations[0].rotation_reason.as_deref(), Some("initial"));
}

#[tokio::test]
async fn test_record_rotation_multiple_increments_refresh_count() {
    let _guard = refresh_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = RefreshTokenStorage::new(&pool);

    let suffix = unique_id();
    let user_id = format!("@rt_user_{suffix}:localhost");
    let family_id = format!("family_{suffix}");
    insert_test_user(&pool, &user_id).await;
    storage.create_family(&family_id, &user_id, None).await.unwrap();

    for i in 0..3 {
        storage
            .record_rotation(&family_id, Some(&format!("old_{i}")), &format!("new_{i}"), "refresh")
            .await
            .unwrap();
    }

    let family = storage.get_family(&family_id).await.unwrap().unwrap();
    assert_eq!(family.refresh_count, 3);
}

// =============================================================================
// get_rotations
// =============================================================================

#[tokio::test]
async fn test_get_rotations_returns_desc_order() {
    let _guard = refresh_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = RefreshTokenStorage::new(&pool);

    let suffix = unique_id();
    let user_id = format!("@rt_user_{suffix}:localhost");
    let family_id = format!("family_{suffix}");
    insert_test_user(&pool, &user_id).await;
    storage.create_family(&family_id, &user_id, None).await.unwrap();

    for i in 0..3 {
        storage
            .record_rotation(&family_id, Some(&format!("old_{i}")), &format!("new_{i}"), "r")
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    }

    let rotations = storage.get_rotations(&family_id).await.unwrap();
    assert_eq!(rotations.len(), 3);
    // DESC by rotated_ts.
    assert!(rotations[0].rotated_ts >= rotations[1].rotated_ts);
    assert!(rotations[1].rotated_ts >= rotations[2].rotated_ts);
    assert_eq!(rotations[0].new_token_hash, "new_2");
}

#[tokio::test]
async fn test_get_rotations_empty_for_family_without_rotations() {
    let _guard = refresh_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = RefreshTokenStorage::new(&pool);

    let suffix = unique_id();
    let user_id = format!("@rt_user_{suffix}:localhost");
    let family_id = format!("family_{suffix}");
    insert_test_user(&pool, &user_id).await;
    storage.create_family(&family_id, &user_id, None).await.unwrap();

    let rotations = storage.get_rotations(&family_id).await.unwrap();
    assert!(rotations.is_empty());
}

#[tokio::test]
async fn test_get_rotations_filters_by_family() {
    let _guard = refresh_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = RefreshTokenStorage::new(&pool);

    let suffix = unique_id();
    let user_id = format!("@rt_user_{suffix}:localhost");
    let family_a = format!("fam_a_{suffix}");
    let family_b = format!("fam_b_{suffix}");
    insert_test_user(&pool, &user_id).await;
    storage.create_family(&family_a, &user_id, None).await.unwrap();
    storage.create_family(&family_b, &user_id, None).await.unwrap();

    storage.record_rotation(&family_a, Some("old"), "new_a", "r").await.unwrap();
    storage.record_rotation(&family_b, Some("old"), "new_b", "r").await.unwrap();

    let a_rotations = storage.get_rotations(&family_a).await.unwrap();
    assert_eq!(a_rotations.len(), 1);
    assert_eq!(a_rotations[0].new_token_hash, "new_a");
}

// =============================================================================
// add_to_blacklist
// =============================================================================

#[tokio::test]
async fn test_add_to_blacklist_basic() {
    let _guard = refresh_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = RefreshTokenStorage::new(&pool);

    let suffix = unique_id();
    let token_hash = format!("bl_{suffix}");
    let user_id = format!("@rt_user_{suffix}:localhost");
    let future_ts = chrono::Utc::now().timestamp_millis() + 3_600_000;

    assert!(!storage.is_blacklisted(&token_hash).await.unwrap());

    storage.add_to_blacklist(&token_hash, "refresh", &user_id, future_ts, None).await.unwrap();

    assert!(storage.is_blacklisted(&token_hash).await.unwrap());
}

#[tokio::test]
async fn test_add_to_blacklist_with_reason() {
    let _guard = refresh_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = RefreshTokenStorage::new(&pool);

    let suffix = unique_id();
    let token_hash = format!("bl_reason_{suffix}");
    let user_id = format!("@rt_user_{suffix}:localhost");
    let future_ts = chrono::Utc::now().timestamp_millis() + 3_600_000;

    storage
        .add_to_blacklist(&token_hash, "refresh", &user_id, future_ts, Some("compromised"))
        .await
        .unwrap();

    // Verify the row was inserted (is_blacklisted checks expiry > now).
    assert!(storage.is_blacklisted(&token_hash).await.unwrap());
}

#[tokio::test]
async fn test_add_to_blacklist_idempotent_on_conflict() {
    let _guard = refresh_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = RefreshTokenStorage::new(&pool);

    let suffix = unique_id();
    let token_hash = format!("bl_idem_{suffix}");
    let user_id = format!("@rt_user_{suffix}:localhost");
    let future_ts = chrono::Utc::now().timestamp_millis() + 3_600_000;

    // Inserting twice must not error (ON CONFLICT DO NOTHING).
    storage.add_to_blacklist(&token_hash, "refresh", &user_id, future_ts, None).await.unwrap();
    storage.add_to_blacklist(&token_hash, "refresh", &user_id, future_ts, None).await.unwrap();

    // Only one row exists.
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM token_blacklist WHERE token_hash = $1")
        .bind(&token_hash)
        .fetch_one(pool.as_ref())
        .await
        .unwrap();
    assert_eq!(count, 1);
}

// =============================================================================
// is_blacklisted
// =============================================================================

#[tokio::test]
async fn test_is_blacklisted_false_for_missing_token() {
    let _guard = refresh_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = RefreshTokenStorage::new(&pool);

    let result = storage.is_blacklisted(&format!("missing_{}", unique_id())).await.unwrap();
    assert!(!result);
}

#[tokio::test]
async fn test_is_blacklisted_false_for_expired_entry() {
    let _guard = refresh_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = RefreshTokenStorage::new(&pool);

    let suffix = unique_id();
    let token_hash = format!("bl_exp_{suffix}");
    let user_id = format!("@rt_user_{suffix}:localhost");
    let past_ts = chrono::Utc::now().timestamp_millis() - 3_600_000;

    storage.add_to_blacklist(&token_hash, "refresh", &user_id, past_ts, None).await.unwrap();

    // Entry exists but is expired — is_blacklisted checks `expires_at > now`.
    assert!(!storage.is_blacklisted(&token_hash).await.unwrap());
}

// =============================================================================
// cleanup_expired_tokens
// =============================================================================

#[tokio::test]
async fn test_cleanup_expired_tokens_deletes_expired_non_revoked() {
    let _guard = refresh_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = RefreshTokenStorage::new(&pool);

    let suffix = unique_id();
    let user_id = format!("@rt_user_{suffix}:localhost");
    insert_test_user(&pool, &user_id).await;
    let past_ts = chrono::Utc::now().timestamp_millis() - 3_600_000;

    storage
        .create_token(CreateRefreshTokenRequest {
            token_hash: format!("exp_{suffix}"),
            user_id: user_id.clone(),
            device_id: None,
            access_token_id: None,
            scope: None,
            expires_at: past_ts,
            client_info: None,
            ip_address: None,
            user_agent: None,
        })
        .await
        .unwrap();

    let deleted = storage.cleanup_expired_tokens().await.unwrap();
    assert_eq!(deleted, 1);
    assert!(storage.get_token(&format!("exp_{suffix}")).await.unwrap().is_none());
}

#[tokio::test]
async fn test_cleanup_expired_tokens_keeps_revoked_expired() {
    let _guard = refresh_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = RefreshTokenStorage::new(&pool);

    let suffix = unique_id();
    let user_id = format!("@rt_user_{suffix}:localhost");
    insert_test_user(&pool, &user_id).await;
    let past_ts = chrono::Utc::now().timestamp_millis() - 3_600_000;

    storage
        .create_token(CreateRefreshTokenRequest {
            token_hash: format!("rev_exp_{suffix}"),
            user_id: user_id.clone(),
            device_id: None,
            access_token_id: None,
            scope: None,
            expires_at: past_ts,
            client_info: None,
            ip_address: None,
            user_agent: None,
        })
        .await
        .unwrap();
    storage.revoke_token(&format!("rev_exp_{suffix}"), "revoked").await.unwrap();

    let deleted = storage.cleanup_expired_tokens().await.unwrap();
    assert_eq!(deleted, 0, "revoked expired tokens must NOT be deleted");
    assert!(storage.get_token(&format!("rev_exp_{suffix}")).await.unwrap().is_some());
}

#[tokio::test]
async fn test_cleanup_expired_tokens_keeps_active() {
    let _guard = refresh_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = RefreshTokenStorage::new(&pool);

    let suffix = unique_id();
    let user_id = format!("@rt_user_{suffix}:localhost");
    insert_test_user(&pool, &user_id).await;
    let future_ts = chrono::Utc::now().timestamp_millis() + 3_600_000;

    storage
        .create_token(CreateRefreshTokenRequest {
            token_hash: format!("act_{suffix}"),
            user_id: user_id.clone(),
            device_id: None,
            access_token_id: None,
            scope: None,
            expires_at: future_ts,
            client_info: None,
            ip_address: None,
            user_agent: None,
        })
        .await
        .unwrap();

    let deleted = storage.cleanup_expired_tokens().await.unwrap();
    assert_eq!(deleted, 0);
    assert!(storage.get_token(&format!("act_{suffix}")).await.unwrap().is_some());
}

// =============================================================================
// cleanup_blacklist
// =============================================================================

#[tokio::test]
async fn test_cleanup_blacklist_deletes_only_expired() {
    let _guard = refresh_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = RefreshTokenStorage::new(&pool);

    let suffix = unique_id();
    let past_ts = chrono::Utc::now().timestamp_millis() - 3_600_000;
    let future_ts = chrono::Utc::now().timestamp_millis() + 3_600_000;
    let user_id = format!("@rt_user_{suffix}:localhost");

    storage.add_to_blacklist(&format!("exp_{suffix}"), "refresh", &user_id, past_ts, None).await.unwrap();
    storage.add_to_blacklist(&format!("act_{suffix}"), "refresh", &user_id, future_ts, None).await.unwrap();

    let deleted = storage.cleanup_blacklist().await.unwrap();
    assert_eq!(deleted, 1);

    assert!(!storage.is_blacklisted(&format!("exp_{suffix}")).await.unwrap());
    assert!(storage.is_blacklisted(&format!("act_{suffix}")).await.unwrap());
}

#[tokio::test]
async fn test_cleanup_blacklist_no_expired_returns_zero() {
    let _guard = refresh_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = RefreshTokenStorage::new(&pool);

    let suffix = unique_id();
    let future_ts = chrono::Utc::now().timestamp_millis() + 3_600_000;
    let user_id = format!("@rt_user_{suffix}:localhost");

    storage.add_to_blacklist(&format!("act_{suffix}"), "refresh", &user_id, future_ts, None).await.unwrap();

    let deleted = storage.cleanup_blacklist().await.unwrap();
    assert_eq!(deleted, 0);
}

// =============================================================================
// get_user_stats
// =============================================================================

#[tokio::test]
async fn test_get_user_stats_comprehensive() {
    let _guard = refresh_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = RefreshTokenStorage::new(&pool);

    let suffix = unique_id();
    let user_id = format!("@rt_user_{suffix}:localhost");
    insert_test_user(&pool, &user_id).await;
    let now = chrono::Utc::now().timestamp_millis();
    let future_ts = now + 3_600_000;
    let past_ts = now - 3_600_000;

    // 2 active (not revoked, future expiry), 1 revoked, 1 expired.
    for (hash, exp) in [
        (format!("a_{suffix}"), future_ts),
        (format!("b_{suffix}"), future_ts),
        (format!("r_{suffix}"), future_ts),
        (format!("e_{suffix}"), past_ts),
    ] {
        storage
            .create_token(CreateRefreshTokenRequest {
                token_hash: hash,
                user_id: user_id.clone(),
                device_id: None,
                access_token_id: None,
                scope: None,
                expires_at: exp,
                client_info: None,
                ip_address: None,
                user_agent: None,
            })
            .await
            .unwrap();
    }
    storage.revoke_token(&format!("r_{suffix}"), "x").await.unwrap();
    // Bump use_count on one active token.
    storage.update_token_usage(&format!("a_{suffix}"), "atid").await.unwrap();
    storage.update_token_usage(&format!("a_{suffix}"), "atid2").await.unwrap();

    let stats = storage.get_user_stats(&user_id).await.unwrap().expect("stats must exist");
    assert_eq!(stats.user_id, user_id);
    assert_eq!(stats.total_tokens, 4);
    assert_eq!(stats.active_tokens, 2, "only a_ and b_ are active");
    assert_eq!(stats.revoked_tokens, 1);
    assert_eq!(stats.expired_tokens, 1, "e_ is expired");
    assert_eq!(stats.total_uses, 2, "two update_token_usage calls on a_");
}

#[tokio::test]
async fn test_get_user_stats_nonexistent_returns_none() {
    let _guard = refresh_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = RefreshTokenStorage::new(&pool);

    let result = storage.get_user_stats(&format!("@nobody_{}:localhost", unique_id())).await.unwrap();
    assert!(result.is_none(), "GROUP BY yields no rows for a user without tokens");
}

// =============================================================================
// get_usage_history
// =============================================================================

#[tokio::test]
async fn test_get_usage_history_returns_desc_order_and_limit() {
    let _guard = refresh_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = RefreshTokenStorage::new(&pool);

    let suffix = unique_id();
    let (user_id, token) = create_token_helper(&storage, &pool, suffix).await;

    for i in 0..3 {
        let req = RecordUsageRequest::new(token.id, &user_id, &format!("atid_{i}"), true);
        storage.record_usage(&req).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    }

    let limited = storage.get_usage_history(&user_id, 2).await.unwrap();
    assert_eq!(limited.len(), 2, "limit must be respected");
    // DESC by used_ts.
    assert!(limited[0].used_ts >= limited[1].used_ts);

    let all = storage.get_usage_history(&user_id, 100).await.unwrap();
    assert_eq!(all.len(), 3);
    assert!(all[0].used_ts >= all[1].used_ts);
    assert!(all[1].used_ts >= all[2].used_ts);
}

#[tokio::test]
async fn test_get_usage_history_empty_for_user_without_usage() {
    let _guard = refresh_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = RefreshTokenStorage::new(&pool);

    let history = storage.get_usage_history(&format!("@nobody_{}:localhost", unique_id()), 10).await.unwrap();
    assert!(history.is_empty());
}

#[tokio::test]
async fn test_get_usage_history_filters_by_user() {
    let _guard = refresh_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = RefreshTokenStorage::new(&pool);

    let suffix = unique_id();
    let (user_a, token_a) = create_token_helper(&storage, &pool, suffix).await;
    let (user_b, token_b) = create_token_helper(&storage, &pool, suffix + 100).await;

    storage.record_usage(&RecordUsageRequest::new(token_a.id, &user_a, "a", true)).await.unwrap();
    storage.record_usage(&RecordUsageRequest::new(token_b.id, &user_b, "b", true)).await.unwrap();

    let a_history = storage.get_usage_history(&user_a, 10).await.unwrap();
    assert_eq!(a_history.len(), 1);
    assert_eq!(a_history[0].new_access_token_id.as_deref(), Some("a"));
}

// =============================================================================
// delete_token
// =============================================================================

#[tokio::test]
async fn test_delete_token_removes_token() {
    let _guard = refresh_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = RefreshTokenStorage::new(&pool);

    let suffix = unique_id();
    let (_, token) = create_token_helper(&storage, &pool, suffix).await;
    assert!(storage.get_token(&token.token_hash).await.unwrap().is_some());

    storage.delete_token(&token.token_hash).await.unwrap();

    assert!(storage.get_token(&token.token_hash).await.unwrap().is_none());
}

#[tokio::test]
async fn test_delete_token_nonexistent_is_noop() {
    let _guard = refresh_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = RefreshTokenStorage::new(&pool);

    storage.delete_token(&format!("missing_{}", unique_id())).await.unwrap();
}

// =============================================================================
// delete_user_tokens
// =============================================================================

#[tokio::test]
async fn test_delete_user_tokens_returns_count() {
    let _guard = refresh_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = RefreshTokenStorage::new(&pool);

    let suffix = unique_id();
    let user_id = format!("@rt_user_{suffix}:localhost");
    insert_test_user(&pool, &user_id).await;
    let future_ts = chrono::Utc::now().timestamp_millis() + 3_600_000;

    for i in 0..3 {
        storage
            .create_token(CreateRefreshTokenRequest {
                token_hash: format!("hash_{suffix}_{i}"),
                user_id: user_id.clone(),
                device_id: None,
                access_token_id: None,
                scope: None,
                expires_at: future_ts,
                client_info: None,
                ip_address: None,
                user_agent: None,
            })
            .await
            .unwrap();
    }

    let count = storage.delete_user_tokens(&user_id).await.unwrap();
    assert_eq!(count, 3);
    assert!(storage.get_user_tokens(&user_id).await.unwrap().is_empty());
}

#[tokio::test]
async fn test_delete_user_tokens_empty_user_returns_zero() {
    let _guard = refresh_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = RefreshTokenStorage::new(&pool);

    let count = storage.delete_user_tokens(&format!("@nobody_{}:localhost", unique_id())).await.unwrap();
    assert_eq!(count, 0);
}

// =============================================================================
// RotateRefreshTokenRequest (public type construction)
// =============================================================================

#[tokio::test]
async fn test_rotate_refresh_token_request_construction() {
    let request = RotateRefreshTokenRequest {
        old_token_hash: "old_hash".to_string(),
        new_token_hash: "new_hash".to_string(),
        user_id: "@u:localhost".to_string(),
        device_id: Some("dev".to_string()),
        family_id: Some("fam".to_string()),
        expires_at: 1_234_567_890,
        ip_address: Some("1.1.1.1".to_string()),
        user_agent: Some("UA".to_string()),
    };
    assert_eq!(request.old_token_hash, "old_hash");
    assert_eq!(request.new_token_hash, "new_hash");
    assert_eq!(request.user_id, "@u:localhost");
    assert_eq!(request.device_id.as_deref(), Some("dev"));
    assert_eq!(request.family_id.as_deref(), Some("fam"));
    assert_eq!(request.expires_at, 1_234_567_890);
    assert_eq!(request.ip_address.as_deref(), Some("1.1.1.1"));
    assert_eq!(request.user_agent.as_deref(), Some("UA"));
}

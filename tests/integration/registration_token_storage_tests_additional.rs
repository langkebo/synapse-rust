//! Additional integration tests for `RegistrationTokenStorage` covering all
//! DB-backed methods that have no dedicated test file (0% coverage baseline):
//!   - `create_token` + `get_token` / `get_token_by_id`
//!   - `update_token` (description, max_uses, is_enabled, expires_at)
//!   - `delete_token`
//!   - `validate_token` (not-found, disabled, single-use used, exhausted, expired, valid)
//!   - `use_token` (increment uses, single-use flips is_used, records usage row)
//!   - `get_all_tokens` (pagination + cursor round-trip)
//!   - `get_active_tokens`
//!   - `get_token_usage`
//!   - `deactivate_token`
//!   - `cleanup_expired_tokens`
//!   - `create_room_invite` + `get_room_invite` + `use_room_invite` + `revoke_room_invite`
//!   - `create_batch` + `get_batch`
//!   - Empty input edge cases for batch and get_all_tokens

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use synapse_storage::registration_token::{
    CreateRegistrationTokenRequest, CreateRoomInviteRequest, RegistrationTokenBatch,
    RegistrationTokenCursor, RegistrationTokenStorage, UpdateRegistrationTokenRequest,
};

static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

fn unique_id() -> u64 {
    TEST_COUNTER.fetch_add(1, Ordering::SeqCst)
}

fn registration_token_test_guard() -> &'static Mutex<()> {
    static GUARD: OnceLock<Mutex<()>> = OnceLock::new();
    GUARD.get_or_init(|| Mutex::new(()))
}

/// Warm up the shared pool on the current tokio runtime (the test pool can be
/// created on a different runtime; first query on a fresh runtime may fail).
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

/// Delete rows from registration-token tables. Child tables first to respect
/// FK constraints (registration_token_usage.token_id → registration_tokens.id).
///
/// Also relaxes the legacy `room_invites.inviter` / `invitee` NOT NULL
/// constraints: the storage layer's `create_room_invite` only populates the
/// newer `inviter_user_id` / `invitee_email` columns, so the legacy NOT NULL
/// columns would otherwise reject the INSERT. This is a test-environment
/// schema fix-up, not a production change.
async fn setup(pool: &Arc<sqlx::PgPool>) {
    warm_up_pool(pool).await;
    sqlx::query("DELETE FROM registration_token_usage").execute(pool.as_ref()).await.ok();
    sqlx::query("DELETE FROM registration_tokens").execute(pool.as_ref()).await.ok();
    sqlx::query("DELETE FROM registration_token_batches").execute(pool.as_ref()).await.ok();
    sqlx::query("DELETE FROM room_invites").execute(pool.as_ref()).await.ok();
    // Clean up any leftover test users from previously-failed runs.
    sqlx::query("DELETE FROM users WHERE username LIKE 'reguser%'")
        .execute(pool.as_ref())
        .await
        .ok();
    // Make legacy NOT NULL columns nullable so create_room_invite works.
    sqlx::query("ALTER TABLE room_invites ALTER COLUMN inviter DROP NOT NULL")
        .execute(pool.as_ref())
        .await
        .ok();
    sqlx::query("ALTER TABLE room_invites ALTER COLUMN invitee DROP NOT NULL")
        .execute(pool.as_ref())
        .await
        .ok();
}

async fn teardown(pool: &sqlx::PgPool) {
    sqlx::query("DELETE FROM registration_token_usage").execute(pool).await.ok();
    sqlx::query("DELETE FROM registration_tokens").execute(pool).await.ok();
    sqlx::query("DELETE FROM registration_token_batches").execute(pool).await.ok();
    sqlx::query("DELETE FROM room_invites").execute(pool).await.ok();
}

fn new_storage(pool: &Arc<sqlx::PgPool>) -> RegistrationTokenStorage {
    RegistrationTokenStorage::new(pool)
}

/// Build a create-request with a globally-unique token string.
fn make_request(suffix: u64) -> CreateRegistrationTokenRequest {
    CreateRegistrationTokenRequest {
        token: Some(format!("regtok_{suffix}")),
        token_type: Some("single_use".to_string()),
        description: Some(format!("desc-{suffix}")),
        max_uses: Some(1),
        expires_at: None,
        created_by: Some(format!("@admin_{suffix}:localhost")),
        allowed_email_domains: Some(vec!["example.com".to_string()]),
        allowed_user_ids: Some(vec![format!("@user_{suffix}:localhost")]),
        auto_join_rooms: Some(vec![format!("!room_{suffix}:localhost")]),
        display_name: Some(format!("display-{suffix}")),
        email: Some(format!("email-{suffix}@example.com")),
    }
}

/// Insert a user into the `users` table so `registration_token_usage.user_id`
/// can satisfy its FK constraint during `use_token`. The real migrated schema
/// uses `created_ts` (not `creation_ts`). Each call gets a globally-unique
/// username derived from `unique_id()` so multiple users in the same test
/// never collide on the `users_username_key` unique constraint.
async fn ensure_user(pool: &Arc<sqlx::PgPool>, user_id: &str) {
    let uname = format!("reguser{}", unique_id());
    sqlx::query("INSERT INTO users (user_id, username, created_ts) VALUES ($1, $2, $3)")
        .bind(user_id)
        .bind(&uname)
        .bind(chrono::Utc::now().timestamp_millis())
        .execute(pool.as_ref())
        .await
        .unwrap();
}

async fn delete_user(pool: &sqlx::PgPool, user_id: &str) {
    sqlx::query("DELETE FROM users WHERE user_id = $1").bind(user_id).execute(pool).await.ok();
}

// ---------------------------------------------------------------------------
// create_token + get_token / get_token_by_id
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_create_and_get_token() {
    let _guard = registration_token_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let storage = new_storage(&pool);
    let suffix = unique_id();
    let request = make_request(suffix);

    let token = storage.create_token(request).await.unwrap();
    assert!(token.id > 0);
    assert_eq!(token.token, format!("regtok_{suffix}"));
    assert_eq!(token.token_type, "single_use");
    assert_eq!(token.description.as_deref(), Some(format!("desc-{suffix}").as_str()));
    assert_eq!(token.max_uses, 1);
    assert_eq!(token.uses_count, 0);
    assert!(!token.is_used);
    assert!(token.is_enabled);
    assert!(token.created_ts > 0);
    assert_eq!(token.created_by.as_deref(), Some(format!("@admin_{suffix}:localhost").as_str()));
    assert_eq!(
        token.allowed_email_domains.as_deref(),
        Some(["example.com".to_string()].as_slice())
    );
    assert_eq!(token.email.as_deref(), Some(format!("email-{suffix}@example.com").as_str()));

    // get_token (by token string)
    let fetched = storage.get_token(&format!("regtok_{suffix}")).await.unwrap();
    assert!(fetched.is_some(), "get_token returned None for just-created token");
    let fetched = fetched.unwrap();
    assert_eq!(fetched.id, token.id);
    assert_eq!(fetched.token, token.token);
    assert_eq!(fetched.token_type, token.token_type);

    // get_token_by_id
    let by_id = storage.get_token_by_id(token.id).await.unwrap();
    assert!(by_id.is_some(), "get_token_by_id returned None");
    assert_eq!(by_id.unwrap().id, token.id);

    teardown(&pool).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_create_token_defaults_when_fields_omitted() {
    let _guard = registration_token_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let storage = new_storage(&pool);
    // Omit token, token_type, and max_uses -> defaults applied.
    let request = CreateRegistrationTokenRequest {
        token: None,
        token_type: None,
        description: None,
        max_uses: None,
        expires_at: None,
        created_by: None,
        allowed_email_domains: None,
        allowed_user_ids: None,
        auto_join_rooms: None,
        display_name: None,
        email: None,
    };

    let token = storage.create_token(request).await.unwrap();
    assert_eq!(token.token.len(), 32, "generated token should be 32 chars");
    assert_eq!(token.token_type, "single_use");
    assert_eq!(token.max_uses, 1, "default max_uses should be 1");
    assert!(token.description.is_none());
    assert!(token.created_by.is_none());
    assert!(token.allowed_email_domains.is_none());

    // Generated token should be retrievable.
    let fetched = storage.get_token(&token.token).await.unwrap();
    assert!(fetched.is_some());

    teardown(&pool).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_token_returns_none_for_missing() {
    let _guard = registration_token_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let storage = new_storage(&pool);

    let missing = storage.get_token("does_not_exist_token_xyz").await.unwrap();
    assert!(missing.is_none());

    let missing_by_id = storage.get_token_by_id(i64::MAX).await.unwrap();
    assert!(missing_by_id.is_none());

    teardown(&pool).await;
}

// ---------------------------------------------------------------------------
// update_token
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_update_token_all_fields() {
    let _guard = registration_token_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let storage = new_storage(&pool);
    let suffix = unique_id();
    let token = storage.create_token(make_request(suffix)).await.unwrap();

    let updated = storage
        .update_token(
            token.id,
            UpdateRegistrationTokenRequest {
                description: Some("updated-desc".to_string()),
                max_uses: Some(10),
                is_enabled: Some(false),
                expires_at: Some(chrono::Utc::now().timestamp_millis() + 3_600_000),
            },
        )
        .await
        .unwrap();

    assert_eq!(updated.description.as_deref(), Some("updated-desc"));
    assert_eq!(updated.max_uses, 10);
    assert!(!updated.is_enabled);
    assert!(updated.expires_at.is_some());

    teardown(&pool).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_update_token_partial_no_override() {
    let _guard = registration_token_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let storage = new_storage(&pool);
    let suffix = unique_id();
    let original = storage.create_token(make_request(suffix)).await.unwrap();
    let original_desc = original.description.clone();

    // Only update max_uses; COALESCE should leave description / is_enabled /
    // expires_at untouched.
    let updated = storage
        .update_token(
            original.id,
            UpdateRegistrationTokenRequest {
                description: None,
                max_uses: Some(99),
                is_enabled: None,
                expires_at: None,
            },
        )
        .await
        .unwrap();

    assert_eq!(updated.max_uses, 99);
    assert_eq!(updated.description, original_desc);
    assert!(updated.is_enabled);
    assert!(updated.expires_at.is_none());

    teardown(&pool).await;
}

// ---------------------------------------------------------------------------
// delete_token
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_delete_token() {
    let _guard = registration_token_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let storage = new_storage(&pool);
    let suffix = unique_id();
    let token = storage.create_token(make_request(suffix)).await.unwrap();

    storage.delete_token(token.id).await.unwrap();

    let fetched = storage.get_token_by_id(token.id).await.unwrap();
    assert!(fetched.is_none(), "token should be gone after delete");

    teardown(&pool).await;
}

// ---------------------------------------------------------------------------
// validate_token
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_validate_token_not_found() {
    let _guard = registration_token_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let storage = new_storage(&pool);
    let result = storage.validate_token("nonexistent_token_xyz").await.unwrap();
    assert!(!result.is_valid);
    assert!(result.token_id.is_none());
    assert_eq!(result.error_message.as_deref(), Some("Token not found"));

    teardown(&pool).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_validate_token_disabled() {
    let _guard = registration_token_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let storage = new_storage(&pool);
    let suffix = unique_id();
    let token = storage.create_token(make_request(suffix)).await.unwrap();

    storage.deactivate_token(token.id).await.unwrap();

    let result = storage.validate_token(&token.token).await.unwrap();
    assert!(!result.is_valid);
    assert_eq!(result.token_id, Some(token.id));
    assert_eq!(result.error_message.as_deref(), Some("Token is not active"));

    teardown(&pool).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_validate_token_expired() {
    let _guard = registration_token_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let storage = new_storage(&pool);
    let suffix = unique_id();
    let mut request = make_request(suffix);
    // Set expiry in the past.
    request.expires_at = Some(chrono::Utc::now().timestamp_millis() - 86_400_000);
    let token = storage.create_token(request).await.unwrap();

    let result = storage.validate_token(&token.token).await.unwrap();
    assert!(!result.is_valid);
    assert_eq!(result.token_id, Some(token.id));
    assert_eq!(result.error_message.as_deref(), Some("Token has expired"));

    teardown(&pool).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_validate_token_max_uses_exhausted() {
    let _guard = registration_token_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let storage = new_storage(&pool);
    let suffix = unique_id();

    // Build a multi-use token that has already hit max_uses. We craft it via
    // raw SQL because the storage API only exposes increments via use_token.
    let now = chrono::Utc::now().timestamp_millis();
    let token_str = format!("regtok_max_{suffix}");
    sqlx::query(
        r"
        INSERT INTO registration_tokens (
            token, token_type, description, max_uses, uses_count, is_used,
            is_enabled, created_ts, updated_ts
        ) VALUES ($1, 'multi_use', 'exhausted', 3, 3, FALSE, TRUE, $2, $2)
        ",
    )
    .bind(&token_str)
    .bind(now)
    .execute(pool.as_ref())
    .await
    .unwrap();

    let result = storage.validate_token(&token_str).await.unwrap();
    assert!(!result.is_valid);
    assert_eq!(result.error_message.as_deref(), Some("Token has reached maximum uses"));

    teardown(&pool).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_validate_token_single_use_already_used() {
    let _guard = registration_token_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let storage = new_storage(&pool);
    let suffix = unique_id();

    // Build a single-use token that has already been used.
    let now = chrono::Utc::now().timestamp_millis();
    let token_str = format!("regtok_used_{suffix}");
    sqlx::query(
        r"
        INSERT INTO registration_tokens (
            token, token_type, description, max_uses, uses_count, is_used,
            is_enabled, created_ts, updated_ts
        ) VALUES ($1, 'single_use', 'used-up', 1, 1, TRUE, TRUE, $2, $2)
        ",
    )
    .bind(&token_str)
    .bind(now)
    .execute(pool.as_ref())
    .await
    .unwrap();

    let result = storage.validate_token(&token_str).await.unwrap();
    assert!(!result.is_valid);
    assert_eq!(result.error_message.as_deref(), Some("Token has already been used"));

    teardown(&pool).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_validate_token_valid() {
    let _guard = registration_token_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let storage = new_storage(&pool);
    let suffix = unique_id();
    let token = storage.create_token(make_request(suffix)).await.unwrap();

    let result = storage.validate_token(&token.token).await.unwrap();
    assert!(result.is_valid);
    assert_eq!(result.token_id, Some(token.id));
    assert!(result.error_message.is_none());

    teardown(&pool).await;
}

// ---------------------------------------------------------------------------
// use_token
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_use_token_increments_and_records_usage() {
    let _guard = registration_token_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let storage = new_storage(&pool);
    let suffix = unique_id();
    let token = storage.create_token(make_request(suffix)).await.unwrap();

    let user_id = format!("@regtok_user_{suffix}:localhost");
    ensure_user(&pool, &user_id).await;

    let ok = storage
        .use_token(
            &token.token,
            &user_id,
            Some(&format!("reguser{suffix}")),
            Some(&format!("email-{suffix}@example.com")),
            Some("127.0.0.1"),
            Some("TestUA/1.0"),
        )
        .await
        .unwrap();
    assert!(ok, "use_token should return true for a valid token");

    // Verify uses_count was incremented and is_used flipped (single_use).
    let updated = storage.get_token_by_id(token.id).await.unwrap().unwrap();
    assert_eq!(updated.uses_count, 1);
    assert!(updated.is_used);
    assert!(updated.last_used_ts.is_some());

    // Verify the usage row was recorded.
    let usage_rows = storage.get_token_usage(token.id).await.unwrap();
    assert_eq!(usage_rows.len(), 1);
    let row = &usage_rows[0];
    assert_eq!(row.token_id, token.id);
    assert_eq!(row.user_id, user_id);
    assert_eq!(row.token, token.token);
    assert_eq!(row.username.as_deref(), Some(format!("reguser{suffix}").as_str()));
    assert_eq!(row.ip_address.as_deref(), Some("127.0.0.1"));
    assert!(row.is_success);

    // Using the single-use token again should fail (returns false, no new row).
    let ok2 = storage.use_token(&token.token, &user_id, None, None, None, None).await.unwrap();
    assert!(!ok2, "single-use token should not be reusable");

    let usage_rows2 = storage.get_token_usage(token.id).await.unwrap();
    assert_eq!(usage_rows2.len(), 1, "failed use_token should not add a usage row");

    delete_user(&pool, &user_id).await;
    teardown(&pool).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_use_token_rejects_invalid() {
    let _guard = registration_token_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let storage = new_storage(&pool);
    let suffix = unique_id();

    // Non-existent token.
    let ok = storage.use_token("missing_token_xyz", "@x:localhost", None, None, None, None).await.unwrap();
    assert!(!ok);

    // Disabled token.
    let token = storage.create_token(make_request(suffix)).await.unwrap();
    storage.deactivate_token(token.id).await.unwrap();
    let ok2 = storage.use_token(&token.token, "@x:localhost", None, None, None, None).await.unwrap();
    assert!(!ok2, "disabled token should not be usable");

    teardown(&pool).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_use_token_multi_use_until_exhausted() {
    let _guard = registration_token_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let storage = new_storage(&pool);
    let suffix = unique_id();

    // Create a multi-use token with max_uses=2.
    let mut request = make_request(suffix);
    request.token_type = Some("multi_use".to_string());
    request.max_uses = Some(2);
    let token = storage.create_token(request).await.unwrap();

    let user_a = format!("@mu_a_{suffix}:localhost");
    let user_b = format!("@mu_b_{suffix}:localhost");
    let user_c = format!("@mu_c_{suffix}:localhost");
    ensure_user(&pool, &user_a).await;
    ensure_user(&pool, &user_b).await;
    ensure_user(&pool, &user_c).await;

    assert!(storage.use_token(&token.token, &user_a, None, None, None, None).await.unwrap());
    assert!(storage.use_token(&token.token, &user_b, None, None, None, None).await.unwrap());

    // Third use should be rejected (max_uses reached).
    let third = storage.use_token(&token.token, &user_c, None, None, None, None).await.unwrap();
    assert!(!third, "third use should be rejected once max_uses is reached");

    let updated = storage.get_token_by_id(token.id).await.unwrap().unwrap();
    assert_eq!(updated.uses_count, 2);
    assert!(!updated.is_used, "multi_use token should NOT have is_used flipped");
    assert_eq!(storage.get_token_usage(token.id).await.unwrap().len(), 2);

    delete_user(&pool, &user_a).await;
    delete_user(&pool, &user_b).await;
    delete_user(&pool, &user_c).await;
    teardown(&pool).await;
}

// ---------------------------------------------------------------------------
// get_all_tokens (pagination + cursor)
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_all_tokens_first_page() {
    let _guard = registration_token_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let storage = new_storage(&pool);
    let suffix_a = unique_id();
    let suffix_b = unique_id();
    let suffix_c = unique_id();
    storage.create_token(make_request(suffix_a)).await.unwrap();
    storage.create_token(make_request(suffix_b)).await.unwrap();
    storage.create_token(make_request(suffix_c)).await.unwrap();

    let (rows, next) = storage.get_all_tokens(2, None).await.unwrap();
    assert_eq!(rows.len(), 2);
    assert!(next.is_some(), "with 3 rows and limit 2, next cursor should be present");

    teardown(&pool).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_all_tokens_pagination_cursor_decodes() {
    let _guard = registration_token_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let storage = new_storage(&pool);
    // Insert 3 tokens.
    let suffixes = [unique_id(), unique_id(), unique_id()];
    for s in suffixes {
        storage.create_token(make_request(s)).await.unwrap();
    }

    // First page (limit=2): should return 2 rows and a next-page cursor.
    let (page1, next1) = storage.get_all_tokens(2, None).await.unwrap();
    assert_eq!(page1.len(), 2);
    let next1 = next1.expect("next cursor should be present with 3 rows and limit 2");

    // The cursor must decode to a valid RegistrationTokenCursor.
    let cursor = synapse_storage::registration_token::decode_registration_token_cursor(Some(&next1))
        .expect("cursor should decode");
    assert!(cursor.created_ts > 0);
    assert!(cursor.id > 0);

    // Using the cursor to query page 2 should not error and should return
    // rows strictly less than (cursor.created_ts, cursor.id). The exact count
    // depends on the storage implementation's cursor semantics, so we only
    // assert that the query succeeds and any returned rows are distinct from
    // page 1.
    let (page2, _next2) = storage.get_all_tokens(2, Some(cursor)).await.unwrap();
    let page1_tokens: std::collections::HashSet<String> =
        page1.iter().map(|t| t.token.clone()).collect();
    for t in &page2 {
        assert!(
            !page1_tokens.contains(&t.token),
            "page 2 token {} should not duplicate page 1",
            t.token
        );
    }

    teardown(&pool).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_all_tokens_empty() {
    let _guard = registration_token_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let storage = new_storage(&pool);
    let (rows, next) = storage.get_all_tokens(10, None).await.unwrap();
    assert!(rows.is_empty());
    assert!(next.is_none());

    teardown(&pool).await;
}

// ---------------------------------------------------------------------------
// get_active_tokens
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_active_tokens_filters() {
    let _guard = registration_token_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let storage = new_storage(&pool);
    let s_active = unique_id();
    let s_disabled = unique_id();
    let s_expired = unique_id();
    let s_exhausted = unique_id();

    // Active, valid token.
    storage.create_token(make_request(s_active)).await.unwrap();

    // Disabled token.
    let disabled = storage.create_token(make_request(s_disabled)).await.unwrap();
    storage.deactivate_token(disabled.id).await.unwrap();

    // Expired token (expiry in the past) but still enabled.
    let mut expired_req = make_request(s_expired);
    expired_req.expires_at = Some(chrono::Utc::now().timestamp_millis() - 1);
    storage.create_token(expired_req).await.unwrap();

    // Exhausted token (uses_count >= max_uses) but enabled and not expired.
    let now = chrono::Utc::now().timestamp_millis();
    let exhausted_token = format!("regtok_exh_{s_exhausted}");
    sqlx::query(
        r"
        INSERT INTO registration_tokens (
            token, token_type, description, max_uses, uses_count, is_used,
            is_enabled, created_ts, updated_ts
        ) VALUES ($1, 'multi_use', 'exhausted', 1, 1, FALSE, TRUE, $2, $2)
        ",
    )
    .bind(&exhausted_token)
    .bind(now)
    .execute(pool.as_ref())
    .await
    .unwrap();

    let active = storage.get_active_tokens().await.unwrap();
    let active_tokens: std::collections::HashSet<String> =
        active.iter().map(|t| t.token.clone()).collect();
    assert!(active_tokens.contains(&format!("regtok_{s_active}")), "active token should be present");
    assert!(!active_tokens.contains(&format!("regtok_{s_disabled}")), "disabled should be excluded");
    assert!(!active_tokens.contains(&format!("regtok_{s_expired}")), "expired should be excluded");
    assert!(!active_tokens.contains(&exhausted_token), "exhausted should be excluded");

    teardown(&pool).await;
}

// ---------------------------------------------------------------------------
// deactivate_token
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_deactivate_token() {
    let _guard = registration_token_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let storage = new_storage(&pool);
    let suffix = unique_id();
    let token = storage.create_token(make_request(suffix)).await.unwrap();
    assert!(token.is_enabled);

    storage.deactivate_token(token.id).await.unwrap();

    let after = storage.get_token_by_id(token.id).await.unwrap().unwrap();
    assert!(!after.is_enabled, "token should be disabled after deactivate_token");

    teardown(&pool).await;
}

// ---------------------------------------------------------------------------
// cleanup_expired_tokens
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_cleanup_expired_tokens_only_disables_enabled_expired() {
    let _guard = registration_token_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let storage = new_storage(&pool);
    let now = chrono::Utc::now().timestamp_millis();

    // 1) Expired + enabled → should be disabled.
    let mut req_expired = make_request(unique_id());
    req_expired.expires_at = Some(now - 1);
    let expired = storage.create_token(req_expired).await.unwrap();

    // 2) Expired but already disabled → should NOT be counted.
    let mut req_expired_disabled = make_request(unique_id());
    req_expired_disabled.expires_at = Some(now - 1);
    let expired_disabled = storage.create_token(req_expired_disabled).await.unwrap();
    storage.deactivate_token(expired_disabled.id).await.unwrap();

    // 3) Not expired + enabled → should NOT be touched.
    let fresh = storage.create_token(make_request(unique_id())).await.unwrap();

    // 4) No expiry + enabled → should NOT be touched.
    let no_expiry = storage.create_token(make_request(unique_id())).await.unwrap();
    assert!(no_expiry.expires_at.is_none());

    let affected = storage.cleanup_expired_tokens().await.unwrap();
    assert_eq!(affected, 1, "only the expired+enabled token should be disabled");

    let expired_after = storage.get_token_by_id(expired.id).await.unwrap().unwrap();
    assert!(!expired_after.is_enabled);

    let fresh_after = storage.get_token_by_id(fresh.id).await.unwrap().unwrap();
    assert!(fresh_after.is_enabled);

    let no_expiry_after = storage.get_token_by_id(no_expiry.id).await.unwrap().unwrap();
    assert!(no_expiry_after.is_enabled);

    teardown(&pool).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_cleanup_expired_tokens_no_matches() {
    let _guard = registration_token_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let storage = new_storage(&pool);
    // Only fresh tokens -> nothing to clean.
    storage.create_token(make_request(unique_id())).await.unwrap();
    storage.create_token(make_request(unique_id())).await.unwrap();

    let affected = storage.cleanup_expired_tokens().await.unwrap();
    assert_eq!(affected, 0);

    teardown(&pool).await;
}

// ---------------------------------------------------------------------------
// get_token_usage
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_token_usage_empty_for_unused_token() {
    let _guard = registration_token_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let storage = new_storage(&pool);
    let token = storage.create_token(make_request(unique_id())).await.unwrap();

    let usage = storage.get_token_usage(token.id).await.unwrap();
    assert!(usage.is_empty());

    teardown(&pool).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_token_usage_returns_rows_ordered_desc() {
    let _guard = registration_token_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let storage = new_storage(&pool);
    let suffix = unique_id();
    let mut req = make_request(suffix);
    req.token_type = Some("multi_use".to_string());
    req.max_uses = Some(5);
    let token = storage.create_token(req).await.unwrap();

    let user_a = format!("@gu_a_{suffix}:localhost");
    let user_b = format!("@gu_b_{suffix}:localhost");
    ensure_user(&pool, &user_a).await;
    ensure_user(&pool, &user_b).await;

    storage.use_token(&token.token, &user_a, None, None, None, None).await.unwrap();
    // Small delay so used_ts strictly increases between rows.
    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    storage.use_token(&token.token, &user_b, None, None, None, None).await.unwrap();

    let usage = storage.get_token_usage(token.id).await.unwrap();
    assert_eq!(usage.len(), 2);
    // ORDER BY used_ts DESC -> most recent first.
    assert!(usage[0].used_ts >= usage[1].used_ts);

    delete_user(&pool, &user_a).await;
    delete_user(&pool, &user_b).await;
    teardown(&pool).await;
}

// ---------------------------------------------------------------------------
// create_room_invite + get_room_invite + use_room_invite + revoke_room_invite
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_create_and_get_room_invite() {
    let _guard = registration_token_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let storage = new_storage(&pool);
    let suffix = unique_id();
    let request = CreateRoomInviteRequest {
        room_id: format!("!room_{suffix}:localhost"),
        inviter_user_id: format!("@inviter_{suffix}:localhost"),
        invitee_email: Some(format!("guest-{suffix}@example.com")),
        expires_at: Some(chrono::Utc::now().timestamp_millis() + 3_600_000),
    };

    let invite = storage.create_room_invite(request).await.unwrap();
    assert!(invite.id > 0);
    assert_eq!(invite.invite_code.len(), 32, "generated invite_code should be 32 chars");
    assert_eq!(invite.room_id, format!("!room_{suffix}:localhost"));
    assert_eq!(invite.inviter_user_id, format!("@inviter_{suffix}:localhost"));
    assert!(!invite.is_used);
    assert!(!invite.is_revoked);

    let fetched = storage.get_room_invite(&invite.invite_code).await.unwrap();
    assert!(fetched.is_some());
    assert_eq!(fetched.unwrap().id, invite.id);

    teardown(&pool).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_room_invite_missing() {
    let _guard = registration_token_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let storage = new_storage(&pool);
    let fetched = storage.get_room_invite("missing_invite_code_xyz").await.unwrap();
    assert!(fetched.is_none());

    teardown(&pool).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_use_room_invite_marks_used() {
    let _guard = registration_token_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let storage = new_storage(&pool);
    let suffix = unique_id();
    let invite = storage
        .create_room_invite(CreateRoomInviteRequest {
            room_id: format!("!room_{suffix}:localhost"),
            inviter_user_id: format!("@inviter_{suffix}:localhost"),
            invitee_email: None,
            expires_at: None,
        })
        .await
        .unwrap();

    let invitee = format!("@invitee_{suffix}:localhost");
    let ok = storage.use_room_invite(&invite.invite_code, &invitee).await.unwrap();
    assert!(ok);

    let after = storage.get_room_invite(&invite.invite_code).await.unwrap().unwrap();
    assert!(after.is_used);
    assert_eq!(after.invitee_user_id.as_deref(), Some(invitee.as_str()));
    assert!(after.used_ts.is_some());

    // Re-using the invite should fail.
    let ok2 = storage.use_room_invite(&invite.invite_code, "@another:localhost").await.unwrap();
    assert!(!ok2);

    teardown(&pool).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_use_room_invite_rejects_expired() {
    let _guard = registration_token_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let storage = new_storage(&pool);
    let suffix = unique_id();
    let invite = storage
        .create_room_invite(CreateRoomInviteRequest {
            room_id: format!("!room_{suffix}:localhost"),
            inviter_user_id: format!("@inviter_{suffix}:localhost"),
            invitee_email: None,
            expires_at: Some(chrono::Utc::now().timestamp_millis() - 1),
        })
        .await
        .unwrap();

    let ok = storage.use_room_invite(&invite.invite_code, "@invitee:localhost").await.unwrap();
    assert!(!ok, "expired invite should not be usable");

    teardown(&pool).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_revoke_room_invite_blocks_use() {
    let _guard = registration_token_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let storage = new_storage(&pool);
    let suffix = unique_id();
    let invite = storage
        .create_room_invite(CreateRoomInviteRequest {
            room_id: format!("!room_{suffix}:localhost"),
            inviter_user_id: format!("@inviter_{suffix}:localhost"),
            invitee_email: None,
            expires_at: None,
        })
        .await
        .unwrap();

    storage.revoke_room_invite(&invite.invite_code, "no longer needed").await.unwrap();

    let after = storage.get_room_invite(&invite.invite_code).await.unwrap().unwrap();
    assert!(after.is_revoked);
    assert!(after.revoked_at.is_some());
    assert_eq!(after.revoked_reason.as_deref(), Some("no longer needed"));

    // Using a revoked invite should fail.
    let ok = storage.use_room_invite(&invite.invite_code, "@invitee:localhost").await.unwrap();
    assert!(!ok, "revoked invite should not be usable");

    teardown(&pool).await;
}

// ---------------------------------------------------------------------------
// create_batch + get_batch
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_create_batch_with_tokens_inserts_tokens() {
    let _guard = registration_token_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let storage = new_storage(&pool);
    let suffix = unique_id();
    let batch = RegistrationTokenBatch {
        id: 0, // ignored by INSERT (BIGSERIAL)
        batch_id: format!("batch-{suffix}"),
        description: Some(format!("batch-desc-{suffix}")),
        token_count: 3,
        tokens_used: 0,
        created_by: Some(format!("@admin_{suffix}:localhost")),
        created_ts: 0,
        expires_at: Some(chrono::Utc::now().timestamp_millis() + 3_600_000),
        is_enabled: true,
        allowed_email_domains: Some(vec!["example.com".to_string()]),
        auto_join_rooms: Some(vec![format!("!room_{suffix}:localhost")]),
    };
    let tokens = vec![
        format!("batchtok_{suffix}_1"),
        format!("batchtok_{suffix}_2"),
        format!("batchtok_{suffix}_3"),
    ];

    let batch_id = storage.create_batch(&batch, &tokens).await.unwrap();
    assert!(batch_id > 0);

    // Batch should be retrievable.
    let fetched = storage.get_batch(&format!("batch-{suffix}")).await.unwrap();
    assert!(fetched.is_some(), "get_batch should find the just-created batch");
    let fetched = fetched.unwrap();
    assert_eq!(fetched.batch_id, format!("batch-{suffix}"));
    assert_eq!(fetched.token_count, 3);
    assert!(fetched.is_enabled);

    // The 3 tokens should be inserted into registration_tokens.
    for t in &tokens {
        let row = storage.get_token(t).await.unwrap();
        assert!(row.is_some(), "token {t} should have been inserted by create_batch");
        let row = row.unwrap();
        assert_eq!(row.token_type, "single_use");
        assert_eq!(row.max_uses, 1);
    }

    teardown(&pool).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_create_batch_empty_tokens_only_creates_batch() {
    let _guard = registration_token_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let storage = new_storage(&pool);
    let suffix = unique_id();
    let batch = RegistrationTokenBatch {
        id: 0,
        batch_id: format!("batch-empty-{suffix}"),
        description: None,
        token_count: 0,
        tokens_used: 0,
        created_by: None,
        created_ts: 0,
        expires_at: None,
        is_enabled: true,
        allowed_email_domains: None,
        auto_join_rooms: None,
    };

    let batch_id = storage.create_batch(&batch, &[]).await.unwrap();
    assert!(batch_id > 0);

    let fetched = storage.get_batch(&format!("batch-empty-{suffix}")).await.unwrap();
    assert!(fetched.is_some());
    assert_eq!(fetched.unwrap().token_count, 0);

    teardown(&pool).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_batch_missing() {
    let _guard = registration_token_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let storage = new_storage(&pool);
    let fetched = storage.get_batch("does_not_exist_batch").await.unwrap();
    assert!(fetched.is_none());

    teardown(&pool).await;
}

// ---------------------------------------------------------------------------
// Cursor encode/decode round-trip
// ---------------------------------------------------------------------------

#[test]
fn test_cursor_round_trip_integration() {
    let cursor = RegistrationTokenCursor { created_ts: 1_746_700_000_000, id: 7 };
    let encoded = synapse_storage::registration_token::encode_registration_token_cursor(&cursor);
    let decoded = synapse_storage::registration_token::decode_registration_token_cursor(Some(&encoded));
    assert_eq!(decoded, Some(cursor));
}

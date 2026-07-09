use std::sync::Arc;

use sqlx::postgres::PgPoolOptions;
use std::env;

use super::*;

async fn test_pool() -> Arc<sqlx::PgPool> {
    let db_url = env::var("TEST_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://synapse:synapse@localhost:15432/synapse".to_string());
    let pool =
        PgPoolOptions::new().max_connections(2).connect(&db_url).await.expect("Failed to connect to test database");
    Arc::new(pool)
}

async fn cleanup_test_data(pool: &sqlx::PgPool, suffix: &str) {
    let token_pattern = format!("%_{}", suffix);
    let room_pattern = format!("%{}%", suffix);

    sqlx::query("DELETE FROM registration_token_usage WHERE token LIKE $1")
        .bind(&token_pattern)
        .execute(pool)
        .await
        .ok();

    sqlx::query("DELETE FROM registration_tokens WHERE token LIKE $1").bind(&token_pattern).execute(pool).await.ok();

    sqlx::query("DELETE FROM room_invites WHERE inviter_user_id LIKE $1").bind(&room_pattern).execute(pool).await.ok();
}

async fn ensure_test_user(pool: &sqlx::PgPool, user_id: &str) {
    let now = chrono::Utc::now().timestamp_millis();
    let username = user_id.strip_prefix('@').and_then(|u| u.split(':').next()).unwrap_or("testuser");
    sqlx::query(
        "INSERT INTO users (user_id, username, created_ts) VALUES ($1, $2, $3) ON CONFLICT (user_id) DO NOTHING",
    )
    .bind(user_id)
    .bind(username)
    .bind(now)
    .execute(pool)
    .await
    .expect("failed to create test user");
}

fn make_suffix() -> String {
    uuid::Uuid::new_v4().to_string().replace('-', "")
}

fn make_full_token(suffix: &str) -> String {
    format!("regtok_test_{}", suffix)
}

fn empty_token_request() -> CreateRegistrationTokenRequest {
    CreateRegistrationTokenRequest {
        token: Some(String::new()),
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
    }
}

// ——————————————————————————————————————————
// 1. create_token
// ——————————————————————————————————————————

#[tokio::test]
async fn test_create_token_with_all_fields() {
    let pool = test_pool().await;
    let suffix = make_suffix();
    cleanup_test_data(&pool, &suffix).await;

    let storage = RegistrationTokenStorage::new(&pool);
    let token_str = make_full_token(&suffix);
    let request = CreateRegistrationTokenRequest {
        token: Some(token_str.clone()),
        token_type: Some("multi_use".to_string()),
        description: Some("DB test token".to_string()),
        max_uses: Some(10),
        expires_at: None,
        created_by: Some(format!("@admin_{}:test.local", suffix)),
        allowed_email_domains: Some(vec!["test.local".to_string()]),
        allowed_user_ids: Some(vec![format!("@user_{}:test.local", suffix)]),
        auto_join_rooms: Some(vec![format!("!room_{}:test.local", suffix)]),
        display_name: Some("Test Display".to_string()),
        email: Some(format!("test_{}@test.local", suffix)),
    };

    let result = storage.create_token(request).await.expect("create_token should succeed");

    assert_eq!(result.token, token_str);
    assert_eq!(result.token_type, "multi_use");
    assert_eq!(result.description.as_deref(), Some("DB test token"));
    assert_eq!(result.max_uses, 10);
    assert_eq!(result.uses_count, 0);
    assert!(!result.is_used);
    assert!(result.is_enabled);
    assert!(result.created_ts > 0);
    assert!(result.expires_at.is_none());
    assert_eq!(result.created_by.as_deref(), Some(format!("@admin_{}:test.local", suffix).as_str()));
    assert_eq!(result.allowed_email_domains.as_deref(), Some(&vec!["test.local".to_string()][..]));
    assert_eq!(result.allowed_user_ids.as_deref(), Some(&vec![format!("@user_{}:test.local", suffix)][..]));
    assert_eq!(result.auto_join_rooms.as_deref(), Some(&vec![format!("!room_{}:test.local", suffix)][..]));
    assert_eq!(result.display_name.as_deref(), Some("Test Display"));
    assert_eq!(result.email.as_deref(), Some(format!("test_{}@test.local", suffix).as_str()));

    cleanup_test_data(&pool, &suffix).await;
}

// ——————————————————————————————————————————
// 2. get_token (found + not_found)
// ——————————————————————————————————————————

#[tokio::test]
async fn test_get_token_found_and_not_found() {
    let pool = test_pool().await;
    let suffix = make_suffix();
    cleanup_test_data(&pool, &suffix).await;

    let storage = RegistrationTokenStorage::new(&pool);
    let token_str = make_full_token(&suffix);

    // Not found before creation
    let missing = storage.get_token(&token_str).await.expect("get_token should not error");
    assert!(missing.is_none(), "token should not exist yet");

    // Create and find
    let request = CreateRegistrationTokenRequest { token: Some(token_str.clone()), ..empty_token_request() };
    let created = storage.create_token(request).await.expect("create_token should succeed");
    assert_eq!(created.token, token_str);

    let found = storage.get_token(&token_str).await.expect("get_token should not error");
    assert!(found.is_some());
    let found = found.unwrap();
    assert_eq!(found.token, token_str);
    assert_eq!(found.id, created.id);

    cleanup_test_data(&pool, &suffix).await;
}

// ——————————————————————————————————————————
// 3. get_token_by_id (found + not_found)
// ——————————————————————————————————————————

#[tokio::test]
async fn test_get_token_by_id_found_and_not_found() {
    let pool = test_pool().await;
    let suffix = make_suffix();
    cleanup_test_data(&pool, &suffix).await;

    let storage = RegistrationTokenStorage::new(&pool);
    let token_str = make_full_token(&suffix);

    // Not found for non-existent id
    let missing = storage.get_token_by_id(-999).await.expect("get_token_by_id should not error");
    assert!(missing.is_none());

    // Create and find by id
    let request = CreateRegistrationTokenRequest { token: Some(token_str.clone()), ..empty_token_request() };
    let created = storage.create_token(request).await.expect("create_token should succeed");

    let found = storage.get_token_by_id(created.id).await.expect("get_token_by_id should not error");
    assert!(found.is_some());
    assert_eq!(found.unwrap().token, token_str);

    cleanup_test_data(&pool, &suffix).await;
}

// ——————————————————————————————————————————
// 4. update_token (success + not_found)
// ——————————————————————————————————————————

#[tokio::test]
async fn test_update_token_success_and_not_found() {
    let pool = test_pool().await;
    let suffix = make_suffix();
    cleanup_test_data(&pool, &suffix).await;

    let storage = RegistrationTokenStorage::new(&pool);
    let token_str = make_full_token(&suffix);

    let created = storage
        .create_token(CreateRegistrationTokenRequest {
            token: Some(token_str.clone()),
            description: Some("original".to_string()),
            max_uses: Some(5),
            ..empty_token_request()
        })
        .await
        .expect("create_token should succeed");

    // Successful update
    let future_expiry = chrono::Utc::now().timestamp_millis() + 86_400_000;
    let update_req = UpdateRegistrationTokenRequest {
        description: Some("updated".to_string()),
        max_uses: Some(20),
        is_enabled: None,
        expires_at: Some(future_expiry),
    };
    let updated = storage.update_token(created.id, update_req).await.expect("update_token should succeed");
    assert_eq!(updated.description.as_deref(), Some("updated"));
    assert_eq!(updated.max_uses, 20);
    assert_eq!(updated.expires_at, Some(future_expiry));
    assert_eq!(updated.id, created.id);

    // Not found — update on non-existent id should error
    let result = storage.update_token(-999, UpdateRegistrationTokenRequest::default()).await;
    assert!(result.is_err(), "update_token on non-existent id should fail");

    cleanup_test_data(&pool, &suffix).await;
}

// ——————————————————————————————————————————
// 5. delete_token (deletes + idempotent)
// ——————————————————————————————————————————

#[tokio::test]
async fn test_delete_token_and_idempotent() {
    let pool = test_pool().await;
    let suffix = make_suffix();
    cleanup_test_data(&pool, &suffix).await;

    let storage = RegistrationTokenStorage::new(&pool);
    let token_str = make_full_token(&suffix);

    let created = storage
        .create_token(CreateRegistrationTokenRequest { token: Some(token_str.clone()), ..empty_token_request() })
        .await
        .expect("create_token should succeed");

    // Delete
    storage.delete_token(created.id).await.expect("delete_token should succeed");
    let after_delete = storage.get_token_by_id(created.id).await.expect("get_token_by_id should not error");
    assert!(after_delete.is_none(), "token should be deleted");

    // Idempotent — delete again (non-existent id should not error)
    let result = storage.delete_token(created.id).await;
    assert!(result.is_ok(), "delete_token on already-deleted id should not error");

    // Also test with never-existed id
    let result2 = storage.delete_token(-99999).await;
    assert!(result2.is_ok(), "delete_token on never-existed id should not error");

    cleanup_test_data(&pool, &suffix).await;
}

// ——————————————————————————————————————————
// 6. validate_token — valid
// ——————————————————————————————————————————

#[tokio::test]
async fn test_validate_token_valid() {
    let pool = test_pool().await;
    let suffix = make_suffix();
    cleanup_test_data(&pool, &suffix).await;

    let storage = RegistrationTokenStorage::new(&pool);
    let token_str = make_full_token(&suffix);

    storage
        .create_token(CreateRegistrationTokenRequest {
            token: Some(token_str.clone()),
            token_type: Some("multi_use".to_string()),
            max_uses: Some(10),
            ..empty_token_request()
        })
        .await
        .expect("create_token should succeed");

    let result = storage.validate_token(&token_str).await.expect("validate_token should not error");
    assert!(result.is_valid);
    assert!(result.token_id.is_some());
    assert!(result.error_message.is_none());

    cleanup_test_data(&pool, &suffix).await;
}

// ——————————————————————————————————————————
// 7. validate_token — expired
// ——————————————————————————————————————————

#[tokio::test]
async fn test_validate_token_expired() {
    let pool = test_pool().await;
    let suffix = make_suffix();
    cleanup_test_data(&pool, &suffix).await;

    let storage = RegistrationTokenStorage::new(&pool);
    let token_str = make_full_token(&suffix);

    let past = chrono::Utc::now().timestamp_millis() - 86_400_000;
    let created = storage
        .create_token(CreateRegistrationTokenRequest {
            token: Some(token_str.clone()),
            expires_at: Some(past),
            ..empty_token_request()
        })
        .await
        .expect("create_token should succeed");

    let result = storage.validate_token(&token_str).await.expect("validate_token should not error");
    assert!(!result.is_valid);
    assert_eq!(result.token_id, Some(created.id));
    assert_eq!(result.error_message.as_deref(), Some("Token has expired"));

    cleanup_test_data(&pool, &suffix).await;
}

// ——————————————————————————————————————————
// 8. validate_token — exhausted (max_uses reached)
// ——————————————————————————————————————————

#[tokio::test]
async fn test_validate_token_exhausted() {
    let pool = test_pool().await;
    let suffix = make_suffix();
    cleanup_test_data(&pool, &suffix).await;

    let storage = RegistrationTokenStorage::new(&pool);
    let token_str = make_full_token(&suffix);
    let user_id = format!("@exhausted_{}:test.local", suffix);
    ensure_test_user(&pool, &user_id).await;

    // Create multi_use token with max_uses=1
    let created = storage
        .create_token(CreateRegistrationTokenRequest {
            token: Some(token_str.clone()),
            token_type: Some("multi_use".to_string()),
            max_uses: Some(1),
            ..empty_token_request()
        })
        .await
        .expect("create_token should succeed");
    assert_eq!(created.max_uses, 1);
    assert_eq!(created.uses_count, 0);

    // Use the token once — should succeed and exhaust
    let used =
        storage.use_token(&token_str, &user_id, None, None, None, None).await.expect("use_token should not error");
    assert!(used, "first use should succeed");

    // Now validate — should be exhausted
    let result = storage.validate_token(&token_str).await.expect("validate_token should not error");
    assert!(!result.is_valid);
    assert_eq!(result.error_message.as_deref(), Some("Token has reached maximum uses"));

    cleanup_test_data(&pool, &suffix).await;
}

// ——————————————————————————————————————————
// 9. validate_token — disabled (not active)
// ——————————————————————————————————————————

#[tokio::test]
async fn test_validate_token_disabled() {
    let pool = test_pool().await;
    let suffix = make_suffix();
    cleanup_test_data(&pool, &suffix).await;

    let storage = RegistrationTokenStorage::new(&pool);
    let token_str = make_full_token(&suffix);

    let created = storage
        .create_token(CreateRegistrationTokenRequest { token: Some(token_str.clone()), ..empty_token_request() })
        .await
        .expect("create_token should succeed");

    // Deactivate
    storage.deactivate_token(created.id).await.expect("deactivate_token should succeed");

    let result = storage.validate_token(&token_str).await.expect("validate_token should not error");
    assert!(!result.is_valid);
    assert_eq!(result.token_id, Some(created.id));
    assert_eq!(result.error_message.as_deref(), Some("Token is not active"));

    cleanup_test_data(&pool, &suffix).await;
}

// ——————————————————————————————————————————
// 10. validate_token — not found
// ——————————————————————————————————————————

#[tokio::test]
async fn test_validate_token_not_found() {
    let pool = test_pool().await;
    let suffix = make_suffix();
    cleanup_test_data(&pool, &suffix).await;

    let storage = RegistrationTokenStorage::new(&pool);
    let non_existent = make_full_token(&suffix);

    let result = storage.validate_token(&non_existent).await.expect("validate_token should not error");
    assert!(!result.is_valid);
    assert!(result.token_id.is_none());
    assert_eq!(result.error_message.as_deref(), Some("Token not found"));

    cleanup_test_data(&pool, &suffix).await;
}

// ——————————————————————————————————————————
// 11. use_token — increments counter
// ——————————————————————————————————————————

#[tokio::test]
async fn test_use_token_increments_counter() {
    let pool = test_pool().await;
    let suffix = make_suffix();
    cleanup_test_data(&pool, &suffix).await;

    let storage = RegistrationTokenStorage::new(&pool);
    let token_str = make_full_token(&suffix);
    let user_id = format!("@counter_{}:test.local", suffix);
    ensure_test_user(&pool, &user_id).await;

    let created = storage
        .create_token(CreateRegistrationTokenRequest {
            token: Some(token_str.clone()),
            token_type: Some("multi_use".to_string()),
            max_uses: Some(5),
            ..empty_token_request()
        })
        .await
        .expect("create_token should succeed");
    assert_eq!(created.uses_count, 0);

    // First use
    let used = storage
        .use_token(&token_str, &user_id, Some("user1"), None, None, None)
        .await
        .expect("use_token should not error");
    assert!(used);

    let after_first = storage.get_token(&token_str).await.expect("get_token should not error");
    assert_eq!(after_first.unwrap().uses_count, 1);

    // Second use — different user
    let user_id2 = format!("@counter2_{}:test.local", suffix);
    ensure_test_user(&pool, &user_id2).await;
    let used2 = storage
        .use_token(&token_str, &user_id2, Some("user2"), None, None, None)
        .await
        .expect("use_token should not error");
    assert!(used2);

    let after_second = storage.get_token(&token_str).await.expect("get_token should not error");
    assert_eq!(after_second.unwrap().uses_count, 2);

    cleanup_test_data(&pool, &suffix).await;
}

// ——————————————————————————————————————————
// 12. use_token — fails when exhausted
// ——————————————————————————————————————————

#[tokio::test]
async fn test_use_token_fails_when_exhausted() {
    let pool = test_pool().await;
    let suffix = make_suffix();
    cleanup_test_data(&pool, &suffix).await;

    let storage = RegistrationTokenStorage::new(&pool);
    let token_str = make_full_token(&suffix);
    let user_id = format!("@exhaust_{}:test.local", suffix);
    ensure_test_user(&pool, &user_id).await;

    storage
        .create_token(CreateRegistrationTokenRequest {
            token: Some(token_str.clone()),
            token_type: Some("multi_use".to_string()),
            max_uses: Some(1),
            ..empty_token_request()
        })
        .await
        .expect("create_token should succeed");

    // First use — succeeds
    let first =
        storage.use_token(&token_str, &user_id, None, None, None, None).await.expect("use_token should not error");
    assert!(first);

    // Second use — fails (token exhausted)
    let user_id2 = format!("@exhaust2_{}:test.local", suffix);
    ensure_test_user(&pool, &user_id2).await;
    let second =
        storage.use_token(&token_str, &user_id2, None, None, None, None).await.expect("use_token should not error");
    assert!(!second, "second use on exhausted token should return false");

    cleanup_test_data(&pool, &suffix).await;
}

// ——————————————————————————————————————————
// 13. get_all_tokens — cursor pagination
// ——————————————————————————————————————————

#[tokio::test]
async fn test_get_all_tokens_cursor_pagination() {
    let pool = test_pool().await;
    let suffix = make_suffix();
    cleanup_test_data(&pool, &suffix).await;

    let storage = RegistrationTokenStorage::new(&pool);

    // Create 4 tokens with tracked IDs
    let prefix = format!("cursor_{}", suffix);
    let mut created_ids: std::collections::HashSet<i64> = std::collections::HashSet::new();
    for i in 0..4 {
        let token_str = format!("{}_id{}", prefix, i);
        let created = storage
            .create_token(CreateRegistrationTokenRequest { token: Some(token_str), ..empty_token_request() })
            .await
            .expect("create_token should succeed");
        created_ids.insert(created.id);
    }

    // Fetch first page (limit 2) — results are global; just verify pagination works
    let (page1, cursor1) = storage.get_all_tokens(2, None).await.expect("get_all_tokens should succeed");
    assert!(page1.len() <= 2, "should respect limit of 2, got {}", page1.len());
    assert!(cursor1.is_some(), "should have a next cursor (global data)");

    // Fetch second page using cursor
    let decoded = decode_registration_token_cursor(cursor1.as_deref());
    assert!(decoded.is_some(), "cursor should decode");

    let (page2, _cursor2) = storage.get_all_tokens(2, decoded).await.expect("get_all_tokens page 2 should succeed");
    assert!(page2.len() <= 2, "page 2 should respect limit of 2, got {}", page2.len());

    // Verify no overlap between pages
    let page1_ids: std::collections::HashSet<i64> = page1.iter().map(|t| t.id).collect();
    for t in &page2 {
        assert!(!page1_ids.contains(&t.id), "duplicate token id {} between pages", t.id);
    }

    cleanup_test_data(&pool, &suffix).await;
}

// ——————————————————————————————————————————
// 14. get_all_tokens — empty
// ——————————————————————————————————————————

#[tokio::test]
async fn test_get_all_tokens_returns_without_error() {
    let pool = test_pool().await;
    let suffix = make_suffix();
    cleanup_test_data(&pool, &suffix).await;

    let storage = RegistrationTokenStorage::new(&pool);

    let (rows, _cursor) = storage.get_all_tokens(10, None).await.expect("get_all_tokens should succeed");
    // get_all_tokens is global — can't assert empty in shared test DB
    assert!(rows.len() <= 10, "should respect limit");

    cleanup_test_data(&pool, &suffix).await;
}

// ——————————————————————————————————————————
// 15. get_active_tokens (active + empty)
// ——————————————————————————————————————————

#[tokio::test]
async fn test_get_active_tokens_returns_active_only() {
    let pool = test_pool().await;
    let suffix = make_suffix();
    cleanup_test_data(&pool, &suffix).await;

    let storage = RegistrationTokenStorage::new(&pool);

    // Create an active token
    let active_str = format!("active_{}", suffix);
    storage
        .create_token(CreateRegistrationTokenRequest { token: Some(active_str.clone()), ..empty_token_request() })
        .await
        .expect("create_token should succeed");

    // Create a disabled token
    let disabled_str = format!("disabled_{}", suffix);
    let disabled = storage
        .create_token(CreateRegistrationTokenRequest { token: Some(disabled_str.clone()), ..empty_token_request() })
        .await
        .expect("create_token should succeed");
    storage.deactivate_token(disabled.id).await.expect("deactivate should succeed");

    // Create an expired token
    let expired_str = format!("expired_{}", suffix);
    let past = chrono::Utc::now().timestamp_millis() - 86_400_000;
    storage
        .create_token(CreateRegistrationTokenRequest {
            token: Some(expired_str.clone()),
            expires_at: Some(past),
            ..empty_token_request()
        })
        .await
        .expect("create_token should succeed");

    let active = storage.get_active_tokens().await.expect("get_active_tokens should succeed");

    // Our active token must be in the results
    let active_found = active.iter().any(|t| t.token == active_str);
    assert!(active_found, "active token should appear in get_active_tokens results");

    // Our disabled token must NOT be in the results
    let disabled_found = active.iter().any(|t| t.token == disabled_str);
    assert!(!disabled_found, "disabled token should not appear in get_active_tokens results");

    // Our expired token must NOT be in the results
    let expired_found = active.iter().any(|t| t.token == expired_str);
    assert!(!expired_found, "expired token should not appear in get_active_tokens results");

    cleanup_test_data(&pool, &suffix).await;
}

// ——————————————————————————————————————————
// 16. get_token_usage (records + empty)
// ——————————————————————————————————————————

#[tokio::test]
async fn test_get_token_usage_with_records() {
    let pool = test_pool().await;
    let suffix = make_suffix();
    cleanup_test_data(&pool, &suffix).await;

    let storage = RegistrationTokenStorage::new(&pool);
    let token_str = make_full_token(&suffix);
    let user_id = format!("@usage_{}:test.local", suffix);
    ensure_test_user(&pool, &user_id).await;

    let created = storage
        .create_token(CreateRegistrationTokenRequest {
            token: Some(token_str.clone()),
            token_type: Some("multi_use".to_string()),
            max_uses: Some(10),
            ..empty_token_request()
        })
        .await
        .expect("create_token should succeed");

    // Empty before any usage
    let empty = storage.get_token_usage(created.id).await.expect("get_token_usage should succeed");
    assert!(empty.is_empty());

    // Use token
    storage
        .use_token(
            &token_str,
            &user_id,
            Some("testuser"),
            Some("test@test.local"),
            Some("127.0.0.1"),
            Some("TestAgent/1.0"),
        )
        .await
        .expect("use_token should succeed");

    let records = storage.get_token_usage(created.id).await.expect("get_token_usage should succeed");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].token, token_str);
    assert_eq!(records[0].user_id, user_id);
    assert_eq!(records[0].username.as_deref(), Some("testuser"));
    assert!(records[0].is_success);

    cleanup_test_data(&pool, &suffix).await;
}

// ——————————————————————————————————————————
// 17. deactivate_token (deactivates + idempotent)
// ——————————————————————————————————————————

#[tokio::test]
async fn test_deactivate_token_and_idempotent() {
    let pool = test_pool().await;
    let suffix = make_suffix();
    cleanup_test_data(&pool, &suffix).await;

    let storage = RegistrationTokenStorage::new(&pool);
    let token_str = make_full_token(&suffix);

    let created = storage
        .create_token(CreateRegistrationTokenRequest { token: Some(token_str.clone()), ..empty_token_request() })
        .await
        .expect("create_token should succeed");
    assert!(created.is_enabled);

    // Deactivate
    storage.deactivate_token(created.id).await.expect("deactivate should succeed");
    let after = storage.get_token_by_id(created.id).await.expect("get should succeed");
    assert!(!after.unwrap().is_enabled, "token should be disabled");

    // Idempotent — deactivate again
    let result = storage.deactivate_token(created.id).await;
    assert!(result.is_ok(), "second deactivate should not error");

    // Also idempotent on never-existed id
    let result2 = storage.deactivate_token(-99999).await;
    assert!(result2.is_ok(), "deactivate on never-existed id should not error");

    cleanup_test_data(&pool, &suffix).await;
}

// ——————————————————————————————————————————
// 18. cleanup_expired_tokens
// ——————————————————————————————————————————

#[tokio::test]
async fn test_cleanup_expired_tokens() {
    let pool = test_pool().await;
    let suffix = make_suffix();
    cleanup_test_data(&pool, &suffix).await;

    let storage = RegistrationTokenStorage::new(&pool);

    // Create a valid token (no expiry)
    let valid_str = format!("valid_{}", suffix);
    let valid = storage
        .create_token(CreateRegistrationTokenRequest { token: Some(valid_str.clone()), ..empty_token_request() })
        .await
        .expect("create_token should succeed");
    assert!(valid.is_enabled);

    // Create an expired token (expires_at in the past), must be enabled
    let expired_str = format!("expired_{}", suffix);
    let past = chrono::Utc::now().timestamp_millis() - 86_400_000;
    let expired = storage
        .create_token(CreateRegistrationTokenRequest {
            token: Some(expired_str.clone()),
            expires_at: Some(past),
            ..empty_token_request()
        })
        .await
        .expect("create_token should succeed");
    assert!(expired.is_enabled);

    // Run cleanup
    let affected = storage.cleanup_expired_tokens().await.expect("cleanup_expired_tokens should succeed");
    assert!(affected >= 1, "should have affected at least 1 expired token");

    // Valid token should still be enabled
    let valid_after = storage.get_token_by_id(valid.id).await.expect("get should succeed").unwrap();
    assert!(valid_after.is_enabled, "valid token should still be enabled");

    // Expired token should now be disabled
    let expired_after = storage.get_token_by_id(expired.id).await.expect("get should succeed").unwrap();
    assert!(!expired_after.is_enabled, "expired token should be disabled");

    // Second cleanup should return 0 (no more expired + enabled tokens)
    let affected2 = storage.cleanup_expired_tokens().await.expect("cleanup should succeed");
    assert_eq!(affected2, 0, "second cleanup should affect 0 rows");

    cleanup_test_data(&pool, &suffix).await;
}

// ——————————————————————————————————————————
// 19. create_token with auto-generated token
// ——————————————————————————————————————————

#[tokio::test]
async fn test_create_token_auto_generates_token() {
    let pool = test_pool().await;
    let suffix = make_suffix();
    cleanup_test_data(&pool, &suffix).await;

    let storage = RegistrationTokenStorage::new(&pool);

    // Create token without specifying token — it should be auto-generated
    let request = CreateRegistrationTokenRequest {
        token: None,
        token_type: Some("single_use".to_string()),
        created_by: Some(format!("@admin_{}:test.local", suffix)),
        ..empty_token_request()
    };

    let result = storage.create_token(request).await.expect("create_token should succeed");

    assert!(result.id > 0);
    assert!(!result.token.is_empty());
    assert_eq!(result.token.len(), 32, "auto-generated token should be 32 characters");
    assert_eq!(result.token_type, "single_use");
    assert_eq!(result.created_by.as_deref(), Some(format!("@admin_{}:test.local", suffix).as_str()));

    // The token should be findable (no FK dependency)
    let found = storage.get_token(&result.token).await.expect("get_token should succeed");
    assert!(found.is_some());

    // Cleanup with the generated token pattern — it won't match our suffix,
    // so delete directly by the returned id
    storage.delete_token(result.id).await.ok();

    cleanup_test_data(&pool, &suffix).await;
}

// ——————————————————————————————————————————
// 20. get_room_invite (found + not_found)
// ——————————————————————————————————————————

#[tokio::test]
async fn test_get_room_invite_found_and_not_found() {
    let pool = test_pool().await;
    let suffix = make_suffix();
    cleanup_test_data(&pool, &suffix).await;

    let storage = RegistrationTokenStorage::new(&pool);
    let room_id = format!("!room2_{}:test.local", suffix);
    let inviter = format!("@inviter2_{}:test.local", suffix);
    let invite_code = format!("invitecode_{}", suffix);
    let now = chrono::Utc::now().timestamp_millis();

    // Not found before creation
    let missing = storage.get_room_invite("nonexistent_code").await.expect("get_room_invite should not error");
    assert!(missing.is_none());

    // Insert a room invite via raw SQL (create_room_invite is broken due to
    // required inviter/invitee columns that it does not supply — pre-existing bug).
    sqlx::query(
        "INSERT INTO room_invites (invite_code, room_id, inviter_user_id, inviter, invitee, created_ts, is_used, is_revoked) \
         VALUES ($1, $2, $3, $4, $5, $6, FALSE, FALSE)",
    )
    .bind(&invite_code)
    .bind(&room_id)
    .bind(&inviter)
    .bind(&inviter)
    .bind(&inviter)
    .bind(now)
    .execute(&*pool)
    .await
    .expect("failed to insert test room invite");

    // Find by invite_code
    let found = storage.get_room_invite(&invite_code).await.expect("get_room_invite should not error");
    assert!(found.is_some());
    let found = found.unwrap();
    assert_eq!(found.invite_code, invite_code);
    assert_eq!(found.room_id, room_id);
    assert_eq!(found.inviter_user_id, inviter);

    cleanup_test_data(&pool, &suffix).await;
}

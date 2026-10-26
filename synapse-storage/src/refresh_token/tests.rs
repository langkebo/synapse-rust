use super::*;

#[test]
fn test_record_usage_request_new() {
    let req = RecordUsageRequest::new(1, "@alice:example.com", "acc_token_123", true);

    assert_eq!(req.refresh_token_id, 1);
    assert_eq!(req.user_id, "@alice:example.com");
    assert_eq!(req.new_access_token_id, "acc_token_123");
    assert!(req.is_success);
    assert!(req.old_access_token_id.is_none());
    assert!(req.ip_address.is_none());
    assert!(req.user_agent.is_none());
    assert!(req.error_message.is_none());
}

#[test]
fn test_record_usage_request_builder_chain() {
    let req = RecordUsageRequest::new(2, "@bob:example.com", "acc_token_456", false)
        .old_access_token_id("old_token")
        .ip_address("192.168.1.1")
        .user_agent("Mozilla/5.0")
        .error_message("Token expired");

    assert_eq!(req.refresh_token_id, 2);
    assert!(!req.is_success);
    assert_eq!(req.old_access_token_id.as_deref(), Some("old_token"));
    assert_eq!(req.ip_address.as_deref(), Some("192.168.1.1"));
    assert_eq!(req.user_agent.as_deref(), Some("Mozilla/5.0"));
    assert_eq!(req.error_message.as_deref(), Some("Token expired"));
}

#[test]
fn test_refresh_token_struct() {
    let token = RefreshToken {
        id: 1,
        token_hash: "hash123".to_string(),
        user_id: "@alice:example.com".to_string(),
        device_id: Some("DEVICE123".to_string()),
        access_token_id: Some("acc123".to_string()),
        scope: None,
        created_ts: 1700000000000,
        expires_at: None,
        last_used_ts: Some(1700000001000),
        use_count: 5,
        is_revoked: false,
        revoked_reason: None,
        client_info: None,
        ip_address: None,
        user_agent: None,
    };
    assert_eq!(token.id, 1);
    assert_eq!(token.user_id, "@alice:example.com");
    assert!(!token.is_revoked);
    assert_eq!(token.device_id.as_deref(), Some("DEVICE123"));
    assert_eq!(token.use_count, 5);
}

#[test]
fn test_refresh_token_serde() {
    let token = RefreshToken {
        id: 2,
        token_hash: "hash456".to_string(),
        user_id: "@charlie:example.com".to_string(),
        device_id: None,
        access_token_id: None,
        scope: Some("read".to_string()),
        created_ts: 1700000000000,
        expires_at: Some(1800000000000),
        last_used_ts: None,
        use_count: 0,
        is_revoked: true,
        revoked_reason: Some("logout".to_string()),
        client_info: None,
        ip_address: None,
        user_agent: None,
    };

    let json = serde_json::to_string(&token).unwrap();
    let deserialized: RefreshToken = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.id, token.id);
    assert_eq!(deserialized.is_revoked, token.is_revoked);
    assert_eq!(deserialized.use_count, token.use_count);
}

#[test]
fn test_token_blacklist_entry() {
    let entry = TokenBlacklistEntry {
        id: 1,
        token_hash: "hash_blacklisted".to_string(),
        token_type: Some("refresh".to_string()),
        user_id: Some("@dave:example.com".to_string()),
        is_revoked: true,
        expires_at: None,
        reason: Some("security".to_string()),
    };
    assert_eq!(entry.token_hash, "hash_blacklisted");
    assert_eq!(entry.reason.as_deref(), Some("security"));
}

#[test]
fn test_refresh_token_stats() {
    let stats = RefreshTokenStats {
        user_id: "@alice:example.com".to_string(),
        total_tokens: 100,
        active_tokens: 80,
        revoked_tokens: 15,
        expired_tokens: 5,
        total_uses: 200,
    };
    assert_eq!(stats.total_tokens, 100);
    assert_eq!(stats.active_tokens, 80);
}

#[test]
fn test_create_refresh_token_request() {
    let req = CreateRefreshTokenRequest {
        token_hash: "hash_new".to_string(),
        user_id: "@eve:example.com".to_string(),
        device_id: Some("DEVICE999".to_string()),
        access_token_id: None,
        scope: None,
        expires_at: 1800000000000,
        client_info: None,
        ip_address: None,
        user_agent: None,
    };
    assert_eq!(req.token_hash, "hash_new");
    assert_eq!(req.user_id, "@eve:example.com");
    assert_eq!(req.expires_at, 1800000000000);
}

#[test]
fn test_rotate_refresh_token_request() {
    let req = RotateRefreshTokenRequest {
        old_token_hash: "old_hash".to_string(),
        new_token_hash: "new_hash".to_string(),
        user_id: "@frank:example.com".to_string(),
        device_id: None,
        family_id: Some("family_abc".to_string()),
        expires_at: 1800000000000,
        ip_address: None,
        user_agent: None,
    };
    assert_eq!(req.old_token_hash, "old_hash");
    assert_eq!(req.new_token_hash, "new_hash");
    assert_eq!(req.family_id.as_deref(), Some("family_abc"));
}

#[test]
fn test_refresh_token_usage() {
    let usage = RefreshTokenUsage {
        id: 1,
        refresh_token_id: 5,
        user_id: "@grace:example.com".to_string(),
        old_access_token_id: Some("old_acc".to_string()),
        new_access_token_id: Some("new_acc".to_string()),
        used_ts: 1700000000000,
        ip_address: Some("10.0.0.1".to_string()),
        user_agent: Some("App/1.0".to_string()),
        is_success: true,
        error_message: None,
    };
    assert_eq!(usage.refresh_token_id, 5);
    assert!(usage.is_success);
}

#[test]
fn test_refresh_token_family() {
    let family = RefreshTokenFamily {
        id: 1,
        family_id: "family_abc".to_string(),
        user_id: "@henry:example.com".to_string(),
        device_id: Some("DEV123".to_string()),
        created_ts: 1700000000000,
        last_refresh_ts: Some(1700000001000),
        refresh_count: 3,
        is_compromised: false,
        compromised_ts: None,
    };
    assert_eq!(family.family_id, "family_abc");
    assert_eq!(family.refresh_count, 3);
    assert!(!family.is_compromised);
}

#[test]
fn test_refresh_token_rotation() {
    let rotation = RefreshTokenRotation {
        id: 1,
        family_id: "family_abc".to_string(),
        old_token_hash: Some("old_hash".to_string()),
        new_token_hash: "new_hash".to_string(),
        rotated_ts: 1700000000000,
        rotation_reason: Some("refresh".to_string()),
    };
    assert_eq!(rotation.family_id, "family_abc");
    assert_eq!(rotation.new_token_hash, "new_hash");
}

// ===== Database-dependent tests =====

use std::sync::atomic::{AtomicU64, Ordering};

static RT_TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

fn rt_unique_suffix() -> u64 {
    RT_TEST_COUNTER.fetch_add(1, Ordering::SeqCst)
}

async fn setup_refresh_token_db(pool: &Arc<PgPool>) {
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
    .execute(&**pool)
    .await
    .expect("Failed to create refresh_tokens table");

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
    .execute(&**pool)
    .await
    .expect("Failed to create refresh_token_usage table");

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
    .execute(&**pool)
    .await
    .expect("Failed to create refresh_token_families table");

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
    .execute(&**pool)
    .await
    .expect("Failed to create refresh_token_rotations table");

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
    .execute(&**pool)
    .await
    .expect("Failed to create token_blacklist table");
}

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

async fn get_rt_test_pool() -> Option<Arc<PgPool>> {
    match crate::test_utils::prepare_empty_isolated_test_pool().await {
        Ok(guard) => Some(guard.pool()),
        Err(error) => {
            tracing::warn!("Skipping refresh_token DB test because test database is unavailable: {error}");
            None
        }
    }
}

#[tokio::test]
async fn test_db_create_token_success() {
    let pool = match get_rt_test_pool().await {
        Some(p) => p,
        None => return,
    };
    setup_refresh_token_db(&pool).await;

    let storage = RefreshTokenStorage::new(&pool);
    let suffix = rt_unique_suffix();
    let user_id = format!("@user_{suffix}:localhost");
    let future_ts = current_timestamp_millis() + 3_600_000;
    let request = make_create_request(suffix, &user_id, future_ts);

    let token = storage.create_token(request).await.expect("Failed to create token");

    assert!(token.id > 0);
    assert_eq!(token.token_hash, format!("hash_{suffix}"));
    assert_eq!(token.user_id, user_id);
    assert_eq!(token.device_id, Some(format!("device_{suffix}")));
    assert_eq!(token.scope, Some("openid".to_string()));
    assert!(!token.is_revoked);
    assert_eq!(token.use_count, 0);
    assert!(token.created_ts > 0);
    assert_eq!(token.expires_at, Some(future_ts));
}

#[tokio::test]
async fn test_db_get_token_found_and_missing() {
    let pool = match get_rt_test_pool().await {
        Some(p) => p,
        None => return,
    };
    setup_refresh_token_db(&pool).await;

    let storage = RefreshTokenStorage::new(&pool);
    let suffix = rt_unique_suffix();
    let user_id = format!("@user_{suffix}:localhost");
    let future_ts = current_timestamp_millis() + 3_600_000;
    let request = make_create_request(suffix, &user_id, future_ts);

    let created = storage.create_token(request).await.expect("Failed to create token");

    let found = storage.get_token(&format!("hash_{suffix}")).await.expect("Failed to get token");
    assert!(found.is_some());
    let found = found.unwrap();
    assert_eq!(found.id, created.id);
    assert_eq!(found.token_hash, created.token_hash);

    let missing = storage.get_token("nonexistent_hash").await.expect("Failed to query missing token");
    assert!(missing.is_none());
}

#[tokio::test]
async fn test_db_get_token_by_id_found_and_missing() {
    let pool = match get_rt_test_pool().await {
        Some(p) => p,
        None => return,
    };
    setup_refresh_token_db(&pool).await;

    let storage = RefreshTokenStorage::new(&pool);
    let suffix = rt_unique_suffix();
    let user_id = format!("@user_{suffix}:localhost");
    let future_ts = current_timestamp_millis() + 3_600_000;
    let request = make_create_request(suffix, &user_id, future_ts);

    let created = storage.create_token(request).await.expect("Failed to create token");

    let found = storage.get_token_by_id(created.id).await.expect("Failed to get token by id");
    assert!(found.is_some());
    assert_eq!(found.unwrap().token_hash, format!("hash_{suffix}"));

    let missing = storage.get_token_by_id(999_999).await.expect("Failed to query missing token");
    assert!(missing.is_none());
}

#[tokio::test]
async fn test_db_get_user_tokens_multiple() {
    let pool = match get_rt_test_pool().await {
        Some(p) => p,
        None => return,
    };
    setup_refresh_token_db(&pool).await;

    let storage = RefreshTokenStorage::new(&pool);
    let suffix = rt_unique_suffix();
    let user_id = format!("@user_{suffix}:localhost");
    let future_ts = current_timestamp_millis() + 3_600_000;

    for i in 0..3u64 {
        let mut req = make_create_request(suffix * 10 + i, &user_id, future_ts);
        req.token_hash = format!("hash_{suffix}_{i}");
        storage.create_token(req).await.expect("Failed to create token");
    }

    let tokens = storage.get_user_tokens(&user_id).await.expect("Failed to get user tokens");
    assert_eq!(tokens.len(), 3);
    // ORDER BY created_ts DESC — all created within the same millisecond typically,
    // so just verify we got all three.
    for t in &tokens {
        assert_eq!(t.user_id, user_id);
    }
}

#[tokio::test]
async fn test_db_get_user_tokens_empty() {
    let pool = match get_rt_test_pool().await {
        Some(p) => p,
        None => return,
    };
    setup_refresh_token_db(&pool).await;

    let storage = RefreshTokenStorage::new(&pool);
    let tokens = storage.get_user_tokens("@nobody:localhost").await.expect("Failed to get user tokens");
    assert!(tokens.is_empty());
}

#[tokio::test]
async fn test_db_get_active_tokens_filters() {
    let pool = match get_rt_test_pool().await {
        Some(p) => p,
        None => return,
    };
    setup_refresh_token_db(&pool).await;

    let storage = RefreshTokenStorage::new(&pool);
    let suffix = rt_unique_suffix();
    let user_id = format!("@user_{suffix}:localhost");
    let now = current_timestamp_millis();
    let future_ts = now + 3_600_000;
    let past_ts = now - 3_600_000;

    // Active token (not revoked, future expiry)
    let mut req_active = make_create_request(suffix, &user_id, future_ts);
    req_active.token_hash = format!("active_{suffix}");
    storage.create_token(req_active).await.expect("Failed to create active token");

    // Revoked token
    let mut req_revoked = make_create_request(suffix + 1, &user_id, future_ts);
    req_revoked.token_hash = format!("revoked_{suffix}");
    let revoked_token = storage.create_token(req_revoked).await.expect("Failed to create revoked token");
    storage.revoke_token(&revoked_token.token_hash, "logout").await.expect("Failed to revoke");

    // Expired token
    let mut req_expired = make_create_request(suffix + 2, &user_id, past_ts);
    req_expired.token_hash = format!("expired_{suffix}");
    storage.create_token(req_expired).await.expect("Failed to create expired token");

    let active = storage.get_active_tokens(&user_id).await.expect("Failed to get active tokens");
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].token_hash, format!("active_{suffix}"));
}

#[tokio::test]
async fn test_db_revoke_token() {
    let pool = match get_rt_test_pool().await {
        Some(p) => p,
        None => return,
    };
    setup_refresh_token_db(&pool).await;

    let storage = RefreshTokenStorage::new(&pool);
    let suffix = rt_unique_suffix();
    let user_id = format!("@user_{suffix}:localhost");
    let future_ts = current_timestamp_millis() + 3_600_000;
    let request = make_create_request(suffix, &user_id, future_ts);

    let token = storage.create_token(request).await.expect("Failed to create token");
    assert!(!token.is_revoked);

    storage.revoke_token(&token.token_hash, "user_logout").await.expect("Failed to revoke token");

    let after = storage.get_token(&token.token_hash).await.expect("Failed to get token").unwrap();
    assert!(after.is_revoked);
    assert_eq!(after.revoked_reason.as_deref(), Some("user_logout"));
}

#[tokio::test]
async fn test_db_revoke_token_cas() {
    let pool = match get_rt_test_pool().await {
        Some(p) => p,
        None => return,
    };
    setup_refresh_token_db(&pool).await;

    let storage = RefreshTokenStorage::new(&pool);
    let suffix = rt_unique_suffix();
    let user_id = format!("@user_{suffix}:localhost");
    let future_ts = current_timestamp_millis() + 3_600_000;
    let request = make_create_request(suffix, &user_id, future_ts);

    let token = storage.create_token(request).await.expect("Failed to create token");

    // First CAS revoke on active token should succeed
    let first = storage.revoke_token_cas(&token.token_hash, "revoke_1").await.expect("Failed CAS revoke");
    assert!(first);

    // Second CAS revoke on already-revoked token should fail
    let second = storage.revoke_token_cas(&token.token_hash, "revoke_2").await.expect("Failed CAS revoke");
    assert!(!second);

    // CAS revoke on non-existent token should fail
    let missing = storage.revoke_token_cas("nonexistent", "revoke_3").await.expect("Failed CAS revoke");
    assert!(!missing);
}

#[tokio::test]
async fn test_db_revoke_token_by_id() {
    let pool = match get_rt_test_pool().await {
        Some(p) => p,
        None => return,
    };
    setup_refresh_token_db(&pool).await;

    let storage = RefreshTokenStorage::new(&pool);
    let suffix = rt_unique_suffix();
    let user_id = format!("@user_{suffix}:localhost");
    let future_ts = current_timestamp_millis() + 3_600_000;
    let request = make_create_request(suffix, &user_id, future_ts);

    let token = storage.create_token(request).await.expect("Failed to create token");
    storage.revoke_token_by_id(token.id, "breach").await.expect("Failed to revoke by id");

    let after = storage.get_token_by_id(token.id).await.expect("Failed to get token").unwrap();
    assert!(after.is_revoked);
    assert_eq!(after.revoked_reason.as_deref(), Some("breach"));
}

#[tokio::test]
async fn test_db_revoke_all_user_tokens() {
    let pool = match get_rt_test_pool().await {
        Some(p) => p,
        None => return,
    };
    setup_refresh_token_db(&pool).await;

    let storage = RefreshTokenStorage::new(&pool);
    let suffix = rt_unique_suffix();
    let user_id = format!("@user_{suffix}:localhost");
    let future_ts = current_timestamp_millis() + 3_600_000;

    for i in 0..3u64 {
        let mut req = make_create_request(suffix * 10 + i, &user_id, future_ts);
        req.token_hash = format!("hash_{suffix}_{i}");
        storage.create_token(req).await.expect("Failed to create token");
    }

    let count = storage.revoke_all_user_tokens(&user_id, "global_logout").await.expect("Failed to revoke all");
    assert_eq!(count, 3);

    // Second call should revoke 0 (already revoked)
    let count2 = storage.revoke_all_user_tokens(&user_id, "global_logout").await.expect("Failed to revoke all");
    assert_eq!(count2, 0);
}

#[tokio::test]
async fn test_db_revoke_all_user_tokens_except_device() {
    let pool = match get_rt_test_pool().await {
        Some(p) => p,
        None => return,
    };
    setup_refresh_token_db(&pool).await;

    let storage = RefreshTokenStorage::new(&pool);
    let suffix = rt_unique_suffix();
    let user_id = format!("@user_{suffix}:localhost");
    let future_ts = current_timestamp_millis() + 3_600_000;

    // Create tokens on two different devices
    for device in ["keep_device", "revoke_device"] {
        for i in 0..2u64 {
            let mut req = make_create_request(suffix * 10 + i, &user_id, future_ts);
            req.token_hash = format!("hash_{suffix}_{device}_{i}");
            req.device_id = Some(device.to_string());
            storage.create_token(req).await.expect("Failed to create token");
        }
    }

    let count = storage
        .revoke_all_user_tokens_except_device(&user_id, "keep_device", "remote_wipe")
        .await
        .expect("Failed to revoke except device");
    assert_eq!(count, 2);

    // Verify kept device tokens are still active
    let active = storage.get_active_tokens(&user_id).await.expect("Failed to get active tokens");
    assert_eq!(active.len(), 2);
    for t in &active {
        assert_eq!(t.device_id.as_deref(), Some("keep_device"));
    }
}

#[tokio::test]
async fn test_db_revoke_device_tokens() {
    let pool = match get_rt_test_pool().await {
        Some(p) => p,
        None => return,
    };
    setup_refresh_token_db(&pool).await;

    let storage = RefreshTokenStorage::new(&pool);
    let suffix = rt_unique_suffix();
    let user_id = format!("@user_{suffix}:localhost");
    let future_ts = current_timestamp_millis() + 3_600_000;

    for device in ["device_a", "device_b"] {
        let mut req = make_create_request(suffix, &user_id, future_ts);
        req.token_hash = format!("hash_{suffix}_{device}");
        req.device_id = Some(device.to_string());
        storage.create_token(req).await.expect("Failed to create token");
    }

    let count = storage
        .revoke_device_tokens(&user_id, "device_a", "single_logout")
        .await
        .expect("Failed to revoke device tokens");
    assert_eq!(count, 1);

    let active = storage.get_active_tokens(&user_id).await.expect("Failed to get active tokens");
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].device_id.as_deref(), Some("device_b"));
}

#[tokio::test]
async fn test_db_update_token_usage() {
    let pool = match get_rt_test_pool().await {
        Some(p) => p,
        None => return,
    };
    setup_refresh_token_db(&pool).await;

    let storage = RefreshTokenStorage::new(&pool);
    let suffix = rt_unique_suffix();
    let user_id = format!("@user_{suffix}:localhost");
    let future_ts = current_timestamp_millis() + 3_600_000;
    let request = make_create_request(suffix, &user_id, future_ts);

    let token = storage.create_token(request).await.expect("Failed to create token");
    assert_eq!(token.use_count, 0);
    assert!(token.last_used_ts.is_none());

    storage.update_token_usage(&token.token_hash, "new_access_1").await.expect("Failed to update usage");
    storage.update_token_usage(&token.token_hash, "new_access_2").await.expect("Failed to update usage");

    let after = storage.get_token(&token.token_hash).await.expect("Failed to get token").unwrap();
    assert_eq!(after.use_count, 2);
    assert!(after.last_used_ts.is_some());
    assert_eq!(after.access_token_id.as_deref(), Some("new_access_2"));
}

#[tokio::test]
async fn test_db_record_usage_and_history() {
    let pool = match get_rt_test_pool().await {
        Some(p) => p,
        None => return,
    };
    setup_refresh_token_db(&pool).await;

    let storage = RefreshTokenStorage::new(&pool);
    let suffix = rt_unique_suffix();
    let user_id = format!("@user_{suffix}:localhost");
    let future_ts = current_timestamp_millis() + 3_600_000;
    let request = make_create_request(suffix, &user_id, future_ts);

    let token = storage.create_token(request).await.expect("Failed to create token");

    let success_req = RecordUsageRequest::new(token.id, &user_id, "new_at_1", true)
        .old_access_token_id("old_at_1")
        .ip_address("10.0.0.1")
        .user_agent("Agent/1.0");
    storage.record_usage(&success_req).await.expect("Failed to record success usage");

    let fail_req = RecordUsageRequest::new(token.id, &user_id, "new_at_2", false).error_message("invalid_token");
    storage.record_usage(&fail_req).await.expect("Failed to record failure usage");

    let history = storage.get_usage_history(&user_id, 10).await.expect("Failed to get usage history");
    assert_eq!(history.len(), 2);
    // ORDER BY used_ts DESC — most recent first
    assert!(!history[0].is_success);
    assert_eq!(history[0].error_message.as_deref(), Some("invalid_token"));
    assert!(history[1].is_success);
}

#[tokio::test]
async fn test_db_create_and_get_family() {
    let pool = match get_rt_test_pool().await {
        Some(p) => p,
        None => return,
    };
    setup_refresh_token_db(&pool).await;

    let storage = RefreshTokenStorage::new(&pool);
    let suffix = rt_unique_suffix();
    let family_id = format!("family_{suffix}");
    let user_id = format!("@user_{suffix}:localhost");

    let family = storage.create_family(&family_id, &user_id, Some("dev_1")).await.expect("Failed to create family");
    assert!(family.id > 0);
    assert_eq!(family.family_id, family_id);
    assert_eq!(family.user_id, user_id);
    assert_eq!(family.device_id.as_deref(), Some("dev_1"));
    assert_eq!(family.refresh_count, 0);
    assert!(!family.is_compromised);

    let found = storage.get_family(&family_id).await.expect("Failed to get family");
    assert!(found.is_some());
    assert_eq!(found.unwrap().id, family.id);

    let missing = storage.get_family("nonexistent_family").await.expect("Failed to get family");
    assert!(missing.is_none());
}

#[tokio::test]
async fn test_db_mark_family_compromised() {
    let pool = match get_rt_test_pool().await {
        Some(p) => p,
        None => return,
    };
    setup_refresh_token_db(&pool).await;

    let storage = RefreshTokenStorage::new(&pool);
    let suffix = rt_unique_suffix();
    let family_id = format!("family_{suffix}");
    let user_id = format!("@user_{suffix}:localhost");

    storage.create_family(&family_id, &user_id, None).await.expect("Failed to create family");
    storage.mark_family_compromised(&family_id).await.expect("Failed to mark compromised");

    let after = storage.get_family(&family_id).await.expect("Failed to get family").unwrap();
    assert!(after.is_compromised);
    assert!(after.compromised_ts.is_some());
}

#[tokio::test]
async fn test_db_record_rotation_and_get_rotations() {
    let pool = match get_rt_test_pool().await {
        Some(p) => p,
        None => return,
    };
    setup_refresh_token_db(&pool).await;

    let storage = RefreshTokenStorage::new(&pool);
    let suffix = rt_unique_suffix();
    let family_id = format!("family_{suffix}");
    let user_id = format!("@user_{suffix}:localhost");

    storage.create_family(&family_id, &user_id, None).await.expect("Failed to create family");

    storage
        .record_rotation(&family_id, Some("old_hash_1"), "new_hash_1", "refresh")
        .await
        .expect("Failed to record rotation 1");
    storage
        .record_rotation(&family_id, Some("new_hash_1"), "new_hash_2", "refresh")
        .await
        .expect("Failed to record rotation 2");

    let rotations = storage.get_rotations(&family_id).await.expect("Failed to get rotations");
    assert_eq!(rotations.len(), 2);
    // ORDER BY rotated_ts DESC — most recent first
    assert_eq!(rotations[0].new_token_hash, "new_hash_2");
    assert_eq!(rotations[1].new_token_hash, "new_hash_1");

    // Verify family was updated
    let family = storage.get_family(&family_id).await.expect("Failed to get family").unwrap();
    assert_eq!(family.refresh_count, 2);
    assert!(family.last_refresh_ts.is_some());
}

/// `rotated_ts` 是**毫秒**时间戳（`current_timestamp_millis()`），同一毫秒内的两次
/// 轮换会并列。`get_rotations` 的契约是"最近的在前"，所以并列时必须有**确定性**的
/// 决胜键，否则 PostgreSQL 可以按任意顺序返回（小表顺序扫描下通常就是插入顺序 ⇒
/// 恰好把最旧的排在前面）。
///
/// 不是理论问题：2026-09-22 按 CI 口径跑覆盖率时
/// `test_db_record_rotation_and_get_rotations` 实测失败
/// （`left: "new_hash_1" right: "new_hash_2"`），根因就是并列 + 缺决胜键；
/// 修法是 `ORDER BY rotated_ts DESC, id DESC`（`id` 是 BIGSERIAL，单调）。
/// 本测试**刻意构造并列**（两次插入显式写同一个 `rotated_ts`），
/// 因此在"有/无决胜键"之间是确定性的红/绿，而不是靠时序运气。
#[tokio::test]
async fn test_db_get_rotations_breaks_ties_on_the_same_millisecond_by_id() {
    let pool = match get_rt_test_pool().await {
        Some(p) => p,
        None => return,
    };
    setup_refresh_token_db(&pool).await;

    let storage = RefreshTokenStorage::new(&pool);
    let suffix = rt_unique_suffix();
    let family_id = format!("family_{suffix}");
    let user_id = format!("@user_{suffix}:localhost");
    storage.create_family(&family_id, &user_id, None).await.expect("Failed to create family");

    let tied_ts = current_timestamp_millis();
    for (old, new) in [("old_hash_1", "new_hash_1"), ("new_hash_1", "new_hash_2")] {
        // 用**动态**查询而不是 `query!`：夹具要显式写同一个 `rotated_ts` 来构造并列，
        // 而 `#[cfg(test)]` 里的查询宏在 `cargo sqlx prepare --workspace`（默认 target 集）
        // 里不会被收集 —— 那会让 `SQLX_OFFLINE=true ... --all-targets` 直接报
        // `no cached data for this query`（实测）。代价是动态 SQL 棘轮 +1，
        // 已在 `scripts/ci/sqlx_dynamic_ratio_baseline` 逐条登记。
        sqlx::query(
            "INSERT INTO refresh_token_rotations \
                 (family_id, old_token_hash, new_token_hash, rotated_ts, rotation_reason) \
                 VALUES ($1, $2, $3, $4, 'refresh')",
        )
        .bind(&family_id)
        .bind(old)
        .bind(new)
        .bind(tied_ts)
        .execute(&*pool)
        .await
        .expect("insert a tied rotation");
    }

    let rows = storage.get_rotations(&family_id).await.expect("Failed to get rotations");
    assert_eq!(rows.len(), 2, "both tied rotations must come back");
    assert!(
        rows[0].rotated_ts == rows[1].rotated_ts,
        "precondition: the two rows must be tied on rotated_ts ({} vs {})",
        rows[0].rotated_ts,
        rows[1].rotated_ts
    );
    assert_eq!(rows[0].new_token_hash, "new_hash_2", "并列时必须以 id DESC 决胜（后插入的在前），否则返回顺序不确定");
    assert_eq!(rows[1].new_token_hash, "new_hash_1");
    assert!(rows[0].id > rows[1].id, "the tie-break must be monotonic in id");
}

/// CONC-02: `rotate_token_atomic` must revoke the old token, insert the new
/// one, and record the rotation in a single transaction, so the three
/// effects can never be observed partially applied.
#[tokio::test]
async fn test_db_rotate_token_atomic_rotates_and_records() {
    let pool = match get_rt_test_pool().await {
        Some(p) => p,
        None => return,
    };
    setup_refresh_token_db(&pool).await;

    let storage = RefreshTokenStorage::new(&pool);
    let suffix = rt_unique_suffix();
    let user_id = format!("@user_{suffix}:localhost");
    let family_id = format!("family_{suffix}");
    let future_ts = current_timestamp_millis() + 3_600_000;

    storage.create_family(&family_id, &user_id, None).await.expect("Failed to create family");
    let old = storage
        .create_token(make_create_request(suffix, &user_id, future_ts))
        .await
        .expect("Failed to create old token");
    let old_hash = old.token_hash.clone();

    let mut new_req = make_create_request(suffix, &user_id, future_ts);
    new_req.token_hash = format!("rotated_hash_{suffix}");
    let new_hash = new_req.token_hash.clone();

    let rotated = storage
        .rotate_token_atomic(&old_hash, "Rotated", new_req, &family_id, "refresh")
        .await
        .expect("rotate_token_atomic should succeed");

    let new_token = rotated.expect("CAS on an active token must win");
    assert_eq!(new_token.token_hash, new_hash);
    assert!(!new_token.is_revoked);

    // The old token is revoked with the supplied reason.
    let old_after =
        storage.get_token(&old_hash).await.expect("Failed to get old token").expect("old token row must still exist");
    assert!(old_after.is_revoked);
    assert_eq!(old_after.revoked_reason.as_deref(), Some("Rotated"));

    // The rotation log and the family counter advanced together.
    let rotations = storage.get_rotations(&family_id).await.expect("Failed to get rotations");
    assert_eq!(rotations.len(), 1);
    assert_eq!(rotations[0].old_token_hash.as_deref(), Some(old_hash.as_str()));
    assert_eq!(rotations[0].new_token_hash, new_hash);

    let family = storage.get_family(&family_id).await.expect("Failed to get family").unwrap();
    assert_eq!(family.refresh_count, 1, "the family counter must advance with the rotation row");
    assert!(family.last_refresh_ts.is_some());
}

/// CONC-02: when the old token is already revoked (CAS miss) — or does not
/// exist at all — `rotate_token_atomic` writes nothing and returns `None`.
#[tokio::test]
async fn test_db_rotate_token_atomic_cas_miss_writes_nothing() {
    let pool = match get_rt_test_pool().await {
        Some(p) => p,
        None => return,
    };
    setup_refresh_token_db(&pool).await;

    let storage = RefreshTokenStorage::new(&pool);
    let suffix = rt_unique_suffix();
    let user_id = format!("@user_{suffix}:localhost");
    let family_id = format!("family_{suffix}");
    let future_ts = current_timestamp_millis() + 3_600_000;

    storage.create_family(&family_id, &user_id, None).await.expect("Failed to create family");
    let old = storage
        .create_token(make_create_request(suffix, &user_id, future_ts))
        .await
        .expect("Failed to create old token");

    let mut new_req = make_create_request(suffix, &user_id, future_ts);
    new_req.token_hash = format!("rotated_hash_{suffix}");
    let new_hash = new_req.token_hash.clone();

    // Pre-revoke the old token so the CAS (`is_revoked = FALSE`) must lose.
    storage.revoke_token(&old.token_hash, "user_logout").await.expect("Failed to pre-revoke");

    let miss = storage
        .rotate_token_atomic(&old.token_hash, "Rotated", new_req, &family_id, "refresh")
        .await
        .expect("rotate_token_atomic should succeed");
    assert!(miss.is_none(), "CAS on an already-revoked token must lose");

    // Nothing was written: no new token, no rotation row, no family bump,
    // and the original revoke reason is untouched.
    assert!(storage.get_token(&new_hash).await.expect("Failed to get new token").is_none());
    assert!(storage.get_rotations(&family_id).await.expect("Failed to get rotations").is_empty());
    let family = storage.get_family(&family_id).await.expect("Failed to get family").unwrap();
    assert_eq!(family.refresh_count, 0, "a losing CAS must not advance the family counter");
    let old_after = storage.get_token(&old.token_hash).await.expect("Failed to get old token").unwrap();
    assert_eq!(old_after.revoked_reason.as_deref(), Some("user_logout"));

    // A token that never existed is also a no-op.
    let mut unknown_req = make_create_request(suffix, &user_id, future_ts);
    unknown_req.token_hash = format!("unknown_hash_{suffix}");
    let unknown = storage
        .rotate_token_atomic("nonexistent_hash", "Rotated", unknown_req, &family_id, "refresh")
        .await
        .expect("rotate_token_atomic should succeed");
    assert!(unknown.is_none(), "rotating an unknown token must be a no-op");
}

#[tokio::test]
async fn test_db_add_to_blacklist_and_check() {
    let pool = match get_rt_test_pool().await {
        Some(p) => p,
        None => return,
    };
    setup_refresh_token_db(&pool).await;

    let storage = RefreshTokenStorage::new(&pool);
    let future_ts = current_timestamp_millis() + 3_600_000;

    storage
        .add_to_blacklist("bl_hash_1", "refresh", "@user:localhost", future_ts, Some("compromised"))
        .await
        .expect("Failed to add to blacklist");

    let is_blacklisted = storage.is_blacklisted("bl_hash_1").await.expect("Failed to check blacklist");
    assert!(is_blacklisted);

    let not_listed = storage.is_blacklisted("not_blacklisted").await.expect("Failed to check blacklist");
    assert!(!not_listed);
}

#[tokio::test]
async fn test_db_is_blacklisted_expired_entry() {
    let pool = match get_rt_test_pool().await {
        Some(p) => p,
        None => return,
    };
    setup_refresh_token_db(&pool).await;

    let storage = RefreshTokenStorage::new(&pool);
    let past_ts = current_timestamp_millis() - 3_600_000;

    storage
        .add_to_blacklist("expired_bl", "refresh", "@user:localhost", past_ts, Some("old"))
        .await
        .expect("Failed to add expired entry");

    let is_blacklisted = storage.is_blacklisted("expired_bl").await.expect("Failed to check blacklist");
    assert!(!is_blacklisted);
}

#[tokio::test]
async fn test_db_add_to_blacklist_idempotent() {
    let pool = match get_rt_test_pool().await {
        Some(p) => p,
        None => return,
    };
    setup_refresh_token_db(&pool).await;

    let storage = RefreshTokenStorage::new(&pool);
    let future_ts = current_timestamp_millis() + 3_600_000;

    storage
        .add_to_blacklist("dup_hash", "refresh", "@user:localhost", future_ts, Some("first"))
        .await
        .expect("Failed first insert");
    // ON CONFLICT DO NOTHING — second insert should not error
    storage
        .add_to_blacklist("dup_hash", "refresh", "@user:localhost", future_ts, Some("second"))
        .await
        .expect("Failed second insert");

    let is_blacklisted = storage.is_blacklisted("dup_hash").await.expect("Failed to check blacklist");
    assert!(is_blacklisted);
}

#[tokio::test]
async fn test_db_cleanup_expired_tokens() {
    let pool = match get_rt_test_pool().await {
        Some(p) => p,
        None => return,
    };
    setup_refresh_token_db(&pool).await;

    let storage = RefreshTokenStorage::new(&pool);
    let suffix = rt_unique_suffix();
    let user_id = format!("@user_{suffix}:localhost");
    let now = current_timestamp_millis();
    let past_ts = now - 3_600_000;
    let future_ts = now + 3_600_000;

    // Expired + not revoked → should be deleted
    let mut req_expired = make_create_request(suffix, &user_id, past_ts);
    req_expired.token_hash = format!("expired_{suffix}");
    storage.create_token(req_expired).await.expect("Failed to create expired token");

    // Expired + revoked → should NOT be deleted (query has AND is_revoked = FALSE)
    let mut req_revoked = make_create_request(suffix + 1, &user_id, past_ts);
    req_revoked.token_hash = format!("revoked_expired_{suffix}");
    let revoked_token = storage.create_token(req_revoked).await.expect("Failed to create revoked token");
    storage.revoke_token(&revoked_token.token_hash, "revoke").await.expect("Failed to revoke");

    // Active → should NOT be deleted
    let mut req_active = make_create_request(suffix + 2, &user_id, future_ts);
    req_active.token_hash = format!("active_{suffix}");
    storage.create_token(req_active).await.expect("Failed to create active token");

    let deleted = storage.cleanup_expired_tokens().await.expect("Failed to cleanup");
    assert_eq!(deleted, 1);

    let remaining = storage.get_user_tokens(&user_id).await.expect("Failed to get tokens");
    assert_eq!(remaining.len(), 2);
}

#[tokio::test]
async fn test_db_cleanup_blacklist() {
    let pool = match get_rt_test_pool().await {
        Some(p) => p,
        None => return,
    };
    setup_refresh_token_db(&pool).await;

    let storage = RefreshTokenStorage::new(&pool);
    let now = current_timestamp_millis();
    let past_ts = now - 3_600_000;
    let future_ts = now + 3_600_000;

    storage
        .add_to_blacklist("expired_entry", "refresh", "@u:localhost", past_ts, Some("old"))
        .await
        .expect("Failed to add expired");
    storage
        .add_to_blacklist("active_entry", "refresh", "@u:localhost", future_ts, Some("active"))
        .await
        .expect("Failed to add active");

    let deleted = storage.cleanup_blacklist().await.expect("Failed to cleanup blacklist");
    assert_eq!(deleted, 1);

    assert!(!storage.is_blacklisted("expired_entry").await.expect("Failed to check"));
    assert!(storage.is_blacklisted("active_entry").await.expect("Failed to check"));
}

#[tokio::test]
async fn test_db_get_user_stats() {
    let pool = match get_rt_test_pool().await {
        Some(p) => p,
        None => return,
    };
    setup_refresh_token_db(&pool).await;

    let storage = RefreshTokenStorage::new(&pool);
    let suffix = rt_unique_suffix();
    let user_id = format!("@user_{suffix}:localhost");
    let now = current_timestamp_millis();
    let future_ts = now + 3_600_000;
    let past_ts = now - 3_600_000;

    // Active token with use_count 5
    let mut req_active = make_create_request(suffix, &user_id, future_ts);
    req_active.token_hash = format!("active_{suffix}");
    let active = storage.create_token(req_active).await.expect("Failed to create active");
    storage.update_token_usage(&active.token_hash, "at_1").await.expect("Failed to update");
    for _ in 0..4 {
        storage.update_token_usage(&active.token_hash, "at_1").await.expect("Failed to update");
    }

    // Revoked token
    let mut req_revoked = make_create_request(suffix + 1, &user_id, future_ts);
    req_revoked.token_hash = format!("revoked_{suffix}");
    let revoked = storage.create_token(req_revoked).await.expect("Failed to create revoked");
    storage.revoke_token(&revoked.token_hash, "logout").await.expect("Failed to revoke");
    storage.update_token_usage(&revoked.token_hash, "at_2").await.expect("Failed to update");
    for _ in 0..2 {
        storage.update_token_usage(&revoked.token_hash, "at_2").await.expect("Failed to update");
    }

    // Expired token
    let mut req_expired = make_create_request(suffix + 2, &user_id, past_ts);
    req_expired.token_hash = format!("expired_{suffix}");
    let expired = storage.create_token(req_expired).await.expect("Failed to create expired");
    storage.update_token_usage(&expired.token_hash, "at_3").await.expect("Failed to update");
    storage.update_token_usage(&expired.token_hash, "at_3").await.expect("Failed to update");

    let stats = storage.get_user_stats(&user_id).await.expect("Failed to get stats").expect("Stats should exist");
    assert_eq!(stats.user_id, user_id);
    assert_eq!(stats.total_tokens, 3);
    assert_eq!(stats.active_tokens, 1);
    assert_eq!(stats.revoked_tokens, 1);
    assert_eq!(stats.expired_tokens, 1);
    // update_token_usage is unconditional: 5 (active) + 3 (revoked) + 2 (expired) = 10
    assert_eq!(stats.total_uses, 10);
}

#[tokio::test]
async fn test_db_get_user_stats_no_tokens() {
    let pool = match get_rt_test_pool().await {
        Some(p) => p,
        None => return,
    };
    setup_refresh_token_db(&pool).await;

    let storage = RefreshTokenStorage::new(&pool);
    let stats = storage.get_user_stats("@nobody:localhost").await.expect("Failed to get stats");
    assert!(stats.is_none());
}

#[tokio::test]
async fn test_db_delete_token() {
    let pool = match get_rt_test_pool().await {
        Some(p) => p,
        None => return,
    };
    setup_refresh_token_db(&pool).await;

    let storage = RefreshTokenStorage::new(&pool);
    let suffix = rt_unique_suffix();
    let user_id = format!("@user_{suffix}:localhost");
    let future_ts = current_timestamp_millis() + 3_600_000;
    let request = make_create_request(suffix, &user_id, future_ts);

    let token = storage.create_token(request).await.expect("Failed to create token");
    storage.delete_token(&token.token_hash).await.expect("Failed to delete token");

    let after = storage.get_token(&token.token_hash).await.expect("Failed to get token");
    assert!(after.is_none());

    // Deleting non-existent token should not error
    storage.delete_token("nonexistent").await.expect("Deleting nonexistent should not error");
}

#[tokio::test]
async fn test_db_delete_user_tokens() {
    let pool = match get_rt_test_pool().await {
        Some(p) => p,
        None => return,
    };
    setup_refresh_token_db(&pool).await;

    let storage = RefreshTokenStorage::new(&pool);
    let suffix = rt_unique_suffix();
    let user_id = format!("@user_{suffix}:localhost");
    let future_ts = current_timestamp_millis() + 3_600_000;

    for i in 0..3u64 {
        let mut req = make_create_request(suffix * 10 + i, &user_id, future_ts);
        req.token_hash = format!("hash_{suffix}_{i}");
        storage.create_token(req).await.expect("Failed to create token");
    }

    let count = storage.delete_user_tokens(&user_id).await.expect("Failed to delete user tokens");
    assert_eq!(count, 3);

    let tokens = storage.get_user_tokens(&user_id).await.expect("Failed to get tokens");
    assert!(tokens.is_empty());
}

#[tokio::test]
async fn test_db_store_api_trait_impl() {
    let pool = match get_rt_test_pool().await {
        Some(p) => p,
        None => return,
    };
    setup_refresh_token_db(&pool).await;

    let storage = RefreshTokenStorage::new(&pool);
    let suffix = rt_unique_suffix();
    let user_id = format!("@user_{suffix}:localhost");
    let future_ts = current_timestamp_millis() + 3_600_000;
    let request = make_create_request(suffix, &user_id, future_ts);

    let token = storage.create_token(request).await.expect("Failed to create token");

    // Test trait method get_user_tokens
    let tokens: Vec<RefreshToken> =
        RefreshTokenStoreApi::get_user_tokens(&storage, &user_id).await.expect("trait get_user_tokens failed");
    assert_eq!(tokens.len(), 1);

    // Test trait method get_token_by_id
    let found = RefreshTokenStoreApi::get_token_by_id(&storage, token.id).await.expect("trait get_token_by_id failed");
    assert!(found.is_some());

    // Test trait method delete_token
    RefreshTokenStoreApi::delete_token(&storage, &token.token_hash).await.expect("trait delete_token failed");
    let after = storage.get_token(&token.token_hash).await.expect("Failed to get token");
    assert!(after.is_none());
}

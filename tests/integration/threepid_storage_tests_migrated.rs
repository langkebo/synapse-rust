#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use synapse_rust::storage::threepid::{
    CreateThreepidRequest, ThreepidStorage, ThreepidValidationSession, UserThreepid,
};
static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

fn unique_id() -> u64 {
    TEST_COUNTER.fetch_add(1, Ordering::SeqCst)
}

async fn setup_test_database(pool: &Arc<sqlx::PgPool>) {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS users (
            user_id TEXT NOT NULL PRIMARY KEY,
            username TEXT NOT NULL UNIQUE,
            password_hash TEXT,
            is_admin BOOLEAN DEFAULT FALSE,
            is_guest BOOLEAN DEFAULT FALSE,
            is_shadow_banned BOOLEAN DEFAULT FALSE,
            is_deactivated BOOLEAN DEFAULT FALSE,
            created_ts BIGINT NOT NULL,
            updated_ts BIGINT,
            displayname TEXT,
            avatar_url TEXT,
            email TEXT,
            phone TEXT,
            generation BIGINT DEFAULT 0,
            consent_version TEXT,
            appservice_id TEXT,
            user_type TEXT,
            invalid_update_at BIGINT,
            migration_state TEXT,
            password_changed_ts BIGINT,
            is_password_change_required BOOLEAN DEFAULT FALSE,
            must_change_password BOOLEAN DEFAULT FALSE,
            password_expires_at BIGINT,
            failed_login_attempts INTEGER DEFAULT 0,
            locked_until BIGINT
        )
        "#,
    )
    .execute(pool.as_ref())
    .await
    .expect("Failed to create users table");

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS user_threepids (
            id BIGSERIAL PRIMARY KEY,
            user_id TEXT NOT NULL,
            medium TEXT NOT NULL,
            address TEXT NOT NULL,
            validated_at BIGINT,
            added_ts BIGINT NOT NULL,
            is_verified BOOLEAN DEFAULT FALSE,
            verification_token TEXT,
            verification_expires_at BIGINT,
            CONSTRAINT uq_user_threepids_medium_address UNIQUE (medium, address),
            CONSTRAINT fk_user_threepids_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
        )
        "#,
    )
    .execute(pool.as_ref())
    .await
    .expect("Failed to create user_threepids table");

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS threepid_validation_session (
            id BIGSERIAL PRIMARY KEY,
            session_id TEXT NOT NULL UNIQUE,
            medium TEXT NOT NULL,
            address TEXT NOT NULL,
            client_secret TEXT NOT NULL,
            token TEXT NOT NULL,
            send_attempt INT NOT NULL DEFAULT 0,
            next_link TEXT,
            is_validated BOOLEAN NOT NULL DEFAULT FALSE,
            validated_at BIGINT,
            created_ts BIGINT NOT NULL,
            expires_at BIGINT NOT NULL
        )
        "#,
    )
    .execute(pool.as_ref())
    .await
    .expect("Failed to create threepid_validation_session table");
}

fn create_storage(pool: &Arc<sqlx::PgPool>) -> ThreepidStorage {
    ThreepidStorage::new(pool.as_ref())
}

async fn insert_user(pool: &sqlx::PgPool, user_id: &str, username: &str) {
    let now = chrono::Utc::now().timestamp_millis();
    sqlx::query("INSERT INTO users (user_id, username, created_ts) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING")
        .bind(user_id)
        .bind(username)
        .bind(now)
        .execute(pool)
        .await
        .expect("Failed to insert test user");
}

#[tokio::test]
async fn test_user_threepid_struct_fields() {
    let threepid = UserThreepid {
        id: 1,
        user_id: "@test:example.com".to_string(),
        medium: "email".to_string(),
        address: "test@example.com".to_string(),
        validated_at: Some(1234567890000),
        added_ts: 1234567800000,
        is_verified: true,
        verification_token: None,
        verification_expires_at: None,
    };
    assert_eq!(threepid.id, 1);
    assert_eq!(threepid.user_id, "@test:example.com");
    assert_eq!(threepid.medium, "email");
    assert_eq!(threepid.address, "test@example.com");
    assert_eq!(threepid.validated_at, Some(1234567890000));
    assert_eq!(threepid.added_ts, 1234567800000);
    assert!(threepid.is_verified);
    assert!(threepid.verification_token.is_none());
    assert!(threepid.verification_expires_at.is_none());
}

#[tokio::test]
async fn test_user_threepid_struct_unverified() {
    let threepid = UserThreepid {
        id: 2,
        user_id: "@unverified:example.com".to_string(),
        medium: "msisdn".to_string(),
        address: "+1234567890".to_string(),
        validated_at: None,
        added_ts: 1234567800000,
        is_verified: false,
        verification_token: Some("token_abc".to_string()),
        verification_expires_at: Some(1234567999000),
    };
    assert!(!threepid.is_verified);
    assert!(threepid.validated_at.is_none());
    assert_eq!(threepid.verification_token, Some("token_abc".to_string()));
    assert!(threepid.verification_expires_at.is_some());
}

#[tokio::test]
async fn test_create_threepid_request_fields() {
    let request = CreateThreepidRequest {
        user_id: "@test:example.com".to_string(),
        medium: "email".to_string(),
        address: "test@example.com".to_string(),
        verification_token: Some("token123".to_string()),
        verification_expires_at: Some(1234567890000),
    };
    assert_eq!(request.user_id, "@test:example.com");
    assert_eq!(request.medium, "email");
    assert_eq!(request.address, "test@example.com");
    assert_eq!(request.verification_token, Some("token123".to_string()));
    assert_eq!(request.verification_expires_at, Some(1234567890000));
}

#[tokio::test]
async fn test_create_threepid_request_minimal() {
    let request = CreateThreepidRequest {
        user_id: "@minimal:example.com".to_string(),
        medium: "email".to_string(),
        address: "minimal@example.com".to_string(),
        verification_token: None,
        verification_expires_at: None,
    };
    assert!(request.verification_token.is_none());
    assert!(request.verification_expires_at.is_none());
}

#[tokio::test]
async fn test_threepid_validation_session_struct() {
    let session = ThreepidValidationSession {
        id: 1,
        session_id: "sess_123".to_string(),
        medium: "email".to_string(),
        address: "test@example.com".to_string(),
        client_secret: "secret_abc".to_string(),
        token: "verify_token".to_string(),
        send_attempt: 0,
        next_link: Some("https://example.com/next".to_string()),
        is_validated: false,
        validated_at: None,
        created_ts: 1234567800000,
        expires_at: 1234567890000,
    };
    assert_eq!(session.id, 1);
    assert_eq!(session.session_id, "sess_123");
    assert_eq!(session.medium, "email");
    assert_eq!(session.address, "test@example.com");
    assert_eq!(session.client_secret, "secret_abc");
    assert_eq!(session.token, "verify_token");
    assert_eq!(session.send_attempt, 0);
    assert_eq!(session.next_link, Some("https://example.com/next".to_string()));
    assert!(!session.is_validated);
    assert!(session.validated_at.is_none());
    assert_eq!(session.created_ts, 1234567800000);
    assert_eq!(session.expires_at, 1234567890000);
}

#[tokio::test]
async fn test_add_threepid() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_id = format!("@add_user_{suffix}:localhost");

    insert_user(&pool, &user_id, &format!("add_user_{suffix}")).await;

    let request = CreateThreepidRequest {
        user_id: user_id.clone(),
        medium: "email".to_string(),
        address: format!("add_{suffix}@example.com"),
        verification_token: Some("token_add".to_string()),
        verification_expires_at: Some(chrono::Utc::now().timestamp_millis() + 3600000),
    };

    let threepid = storage.add_threepid(request).await.unwrap();
    assert!(threepid.id > 0);
    assert_eq!(threepid.user_id, user_id);
    assert_eq!(threepid.medium, "email");
    assert!(!threepid.is_verified);
    assert_eq!(threepid.verification_token, Some("token_add".to_string()));
    assert!(threepid.verification_expires_at.is_some());
    assert!(threepid.validated_at.is_none());
    assert!(threepid.added_ts > 0);
}

#[tokio::test]
async fn test_add_threepid_without_verification() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_id = format!("@no_verify_{suffix}:localhost");

    insert_user(&pool, &user_id, &format!("no_verify_{suffix}")).await;

    let request = CreateThreepidRequest {
        user_id: user_id.clone(),
        medium: "email".to_string(),
        address: format!("noverify_{suffix}@example.com"),
        verification_token: None,
        verification_expires_at: None,
    };

    let threepid = storage.add_threepid(request).await.unwrap();
    assert!(threepid.verification_token.is_none());
    assert!(threepid.verification_expires_at.is_none());
}

#[tokio::test]
async fn test_add_threepid_duplicate_medium_address() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_id = format!("@dup_user_{suffix}:localhost");

    insert_user(&pool, &user_id, &format!("dup_user_{suffix}")).await;

    let request1 = CreateThreepidRequest {
        user_id: user_id.clone(),
        medium: "email".to_string(),
        address: format!("dup_{suffix}@example.com"),
        verification_token: None,
        verification_expires_at: None,
    };

    storage.add_threepid(request1).await.unwrap();

    let request2 = CreateThreepidRequest {
        user_id: user_id.clone(),
        medium: "email".to_string(),
        address: format!("dup_{suffix}@example.com"),
        verification_token: None,
        verification_expires_at: None,
    };

    let result = storage.add_threepid(request2).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_get_threepid_found() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_id = format!("@get_user_{suffix}:localhost");
    let address = format!("get_{suffix}@example.com");

    insert_user(&pool, &user_id, &format!("get_user_{suffix}")).await;

    let request = CreateThreepidRequest {
        user_id: user_id.clone(),
        medium: "email".to_string(),
        address: address.clone(),
        verification_token: None,
        verification_expires_at: None,
    };

    storage.add_threepid(request).await.unwrap();

    let result = storage.get_threepid(&user_id, "email", &address).await.unwrap();
    assert!(result.is_some());
    let threepid = result.unwrap();
    assert_eq!(threepid.user_id, user_id);
    assert_eq!(threepid.medium, "email");
    assert_eq!(threepid.address, address);
}

#[tokio::test]
async fn test_get_threepid_not_found() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();

    let result =
        storage.get_threepid(&format!("@ghost_{suffix}:localhost"), "email", "nonexistent@example.com").await.unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn test_get_threepids_by_user() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_id = format!("@multi_user_{suffix}:localhost");

    insert_user(&pool, &user_id, &format!("multi_user_{suffix}")).await;

    let request1 = CreateThreepidRequest {
        user_id: user_id.clone(),
        medium: "email".to_string(),
        address: format!("multi_email_{suffix}@example.com"),
        verification_token: None,
        verification_expires_at: None,
    };
    let request2 = CreateThreepidRequest {
        user_id: user_id.clone(),
        medium: "msisdn".to_string(),
        address: format!("+1555{suffix}"),
        verification_token: None,
        verification_expires_at: None,
    };

    storage.add_threepid(request1).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    storage.add_threepid(request2).await.unwrap();

    let threepids = storage.get_threepids_by_user(&user_id).await.unwrap();
    assert_eq!(threepids.len(), 2);
    assert!(threepids[0].added_ts >= threepids[1].added_ts);
}

#[tokio::test]
async fn test_get_threepids_by_user_empty() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();

    let threepids = storage.get_threepids_by_user(&format!("@empty_{suffix}:localhost")).await.unwrap();
    assert!(threepids.is_empty());
}

#[tokio::test]
async fn test_get_threepid_by_address_found() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_id = format!("@addr_user_{suffix}:localhost");
    let address = format!("addr_{suffix}@example.com");

    insert_user(&pool, &user_id, &format!("addr_user_{suffix}")).await;

    let request = CreateThreepidRequest {
        user_id: user_id.clone(),
        medium: "email".to_string(),
        address: address.clone(),
        verification_token: None,
        verification_expires_at: None,
    };

    storage.add_threepid(request).await.unwrap();

    let result = storage.get_threepid_by_address("email", &address).await.unwrap();
    assert!(result.is_some());
    let threepid = result.unwrap();
    assert_eq!(threepid.user_id, user_id);
    assert_eq!(threepid.address, address);
}

#[tokio::test]
async fn test_get_threepid_by_address_not_found() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();

    let result = storage.get_threepid_by_address("email", &format!("noaddr_{suffix}@example.com")).await.unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn test_get_verified_threepid_by_address_found() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_id = format!("@verified_user_{suffix}:localhost");
    let address = format!("verified_{suffix}@example.com");

    insert_user(&pool, &user_id, &format!("verified_user_{suffix}")).await;

    let request = CreateThreepidRequest {
        user_id: user_id.clone(),
        medium: "email".to_string(),
        address: address.clone(),
        verification_token: None,
        verification_expires_at: None,
    };

    storage.add_threepid(request).await.unwrap();
    storage.verify_threepid(&user_id, "email", &address).await.unwrap();

    let result = storage.get_verified_threepid_by_address("email", &address).await.unwrap();
    assert!(result.is_some());
    let threepid = result.unwrap();
    assert!(threepid.is_verified);
    assert!(threepid.validated_at.is_some());
}

#[tokio::test]
async fn test_get_verified_threepid_by_address_unverified() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_id = format!("@unverified_user_{suffix}:localhost");
    let address = format!("unverified_{suffix}@example.com");

    insert_user(&pool, &user_id, &format!("unverified_user_{suffix}")).await;

    let request = CreateThreepidRequest {
        user_id: user_id.clone(),
        medium: "email".to_string(),
        address: address.clone(),
        verification_token: None,
        verification_expires_at: None,
    };

    storage.add_threepid(request).await.unwrap();

    let result = storage.get_verified_threepid_by_address("email", &address).await.unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn test_verify_threepid_success() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_id = format!("@verify_user_{suffix}:localhost");
    let address = format!("verify_{suffix}@example.com");

    insert_user(&pool, &user_id, &format!("verify_user_{suffix}")).await;

    let request = CreateThreepidRequest {
        user_id: user_id.clone(),
        medium: "email".to_string(),
        address: address.clone(),
        verification_token: Some("verify_token".to_string()),
        verification_expires_at: Some(chrono::Utc::now().timestamp_millis() + 3600000),
    };

    storage.add_threepid(request).await.unwrap();

    let verified = storage.verify_threepid(&user_id, "email", &address).await.unwrap();
    assert!(verified);

    let threepid = storage.get_threepid(&user_id, "email", &address).await.unwrap().unwrap();
    assert!(threepid.is_verified);
    assert!(threepid.validated_at.is_some());
    assert!(threepid.verification_token.is_none());
    assert!(threepid.verification_expires_at.is_none());
}

#[tokio::test]
async fn test_verify_threepid_nonexistent() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();

    let verified = storage
        .verify_threepid(&format!("@noverify_{suffix}:localhost"), "email", "nonexistent@example.com")
        .await
        .unwrap();
    assert!(!verified);
}

#[tokio::test]
async fn test_verify_threepid_by_token_success() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_id = format!("@token_user_{suffix}:localhost");
    let address = format!("token_{suffix}@example.com");
    let token = format!("token_{suffix}_abc");

    insert_user(&pool, &user_id, &format!("token_user_{suffix}")).await;

    let request = CreateThreepidRequest {
        user_id: user_id.clone(),
        medium: "email".to_string(),
        address: address.clone(),
        verification_token: Some(token.clone()),
        verification_expires_at: Some(chrono::Utc::now().timestamp_millis() + 3600000),
    };

    storage.add_threepid(request).await.unwrap();

    let result = storage.verify_threepid_by_token(&token).await.unwrap();
    assert!(result.is_some());
    let threepid = result.unwrap();
    assert!(threepid.is_verified);
    assert!(threepid.validated_at.is_some());
    assert!(threepid.verification_token.is_none());
    assert!(threepid.verification_expires_at.is_none());
}

#[tokio::test]
async fn test_verify_threepid_by_token_expired() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_id = format!("@expired_user_{suffix}:localhost");
    let address = format!("expired_{suffix}@example.com");
    let token = format!("expired_token_{suffix}");

    insert_user(&pool, &user_id, &format!("expired_user_{suffix}")).await;

    let request = CreateThreepidRequest {
        user_id: user_id.clone(),
        medium: "email".to_string(),
        address: address.clone(),
        verification_token: Some(token.clone()),
        verification_expires_at: Some(chrono::Utc::now().timestamp_millis() - 1000),
    };

    storage.add_threepid(request).await.unwrap();

    let result = storage.verify_threepid_by_token(&token).await.unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn test_verify_threepid_by_token_invalid() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);

    let result = storage.verify_threepid_by_token("nonexistent_token").await.unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn test_remove_threepid_success() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_id = format!("@remove_user_{suffix}:localhost");
    let address = format!("remove_{suffix}@example.com");

    insert_user(&pool, &user_id, &format!("remove_user_{suffix}")).await;

    let request = CreateThreepidRequest {
        user_id: user_id.clone(),
        medium: "email".to_string(),
        address: address.clone(),
        verification_token: None,
        verification_expires_at: None,
    };

    storage.add_threepid(request).await.unwrap();

    let removed = storage.remove_threepid(&user_id, "email", &address).await.unwrap();
    assert!(removed);

    let result = storage.get_threepid(&user_id, "email", &address).await.unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn test_remove_threepid_nonexistent() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();

    let removed = storage
        .remove_threepid(&format!("@ghost_{suffix}:localhost"), "email", "nonexistent@example.com")
        .await
        .unwrap();
    assert!(!removed);
}

#[tokio::test]
async fn test_remove_threepids_by_user() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_id = format!("@removeall_user_{suffix}:localhost");

    insert_user(&pool, &user_id, &format!("removeall_user_{suffix}")).await;

    let request1 = CreateThreepidRequest {
        user_id: user_id.clone(),
        medium: "email".to_string(),
        address: format!("removeall1_{suffix}@example.com"),
        verification_token: None,
        verification_expires_at: None,
    };
    let request2 = CreateThreepidRequest {
        user_id: user_id.clone(),
        medium: "msisdn".to_string(),
        address: format!("+1555{suffix}"),
        verification_token: None,
        verification_expires_at: None,
    };

    storage.add_threepid(request1).await.unwrap();
    storage.add_threepid(request2).await.unwrap();

    let count = storage.remove_threepids_by_user(&user_id).await.unwrap();
    assert_eq!(count, 2);

    let threepids = storage.get_threepids_by_user(&user_id).await.unwrap();
    assert!(threepids.is_empty());
}

#[tokio::test]
async fn test_remove_threepids_by_user_no_threepids() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();

    let count = storage.remove_threepids_by_user(&format!("@nothreepid_{suffix}:localhost")).await.unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn test_cleanup_expired_verifications() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_id = format!("@cleanup_user_{suffix}:localhost");

    insert_user(&pool, &user_id, &format!("cleanup_user_{suffix}")).await;

    let expired_request = CreateThreepidRequest {
        user_id: user_id.clone(),
        medium: "email".to_string(),
        address: format!("expired_cleanup_{suffix}@example.com"),
        verification_token: Some("expired_token".to_string()),
        verification_expires_at: Some(chrono::Utc::now().timestamp_millis() - 1000),
    };

    storage.add_threepid(expired_request).await.unwrap();

    let valid_request = CreateThreepidRequest {
        user_id: user_id.clone(),
        medium: "msisdn".to_string(),
        address: format!("+1666{suffix}"),
        verification_token: Some("valid_token".to_string()),
        verification_expires_at: Some(chrono::Utc::now().timestamp_millis() + 3600000),
    };

    storage.add_threepid(valid_request).await.unwrap();

    let cleaned = storage.cleanup_expired_verifications().await.unwrap();
    assert_eq!(cleaned, 1);

    let threepids = storage.get_threepids_by_user(&user_id).await.unwrap();
    assert_eq!(threepids.len(), 1);
    assert_eq!(threepids[0].medium, "msisdn");
}

#[tokio::test]
async fn test_cleanup_expired_verifications_none() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);

    let cleaned = storage.cleanup_expired_verifications().await.unwrap();
    assert_eq!(cleaned, 0);
}

#[tokio::test]
async fn test_create_validation_session() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let now = chrono::Utc::now().timestamp_millis();

    let id = storage
        .create_validation_session(
            &format!("session_{suffix}"),
            "email",
            &format!("val_{suffix}@example.com"),
            "client_secret_abc",
            "verify_token_xyz",
            Some("https://example.com/next"),
            now,
            now + 3600000,
        )
        .await
        .unwrap();

    assert!(id > 0);
}

#[tokio::test]
async fn test_create_validation_session_without_next_link() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let now = chrono::Utc::now().timestamp_millis();

    let id = storage
        .create_validation_session(
            &format!("session_no_link_{suffix}"),
            "msisdn",
            &format!("+1777{suffix}"),
            "secret_def",
            "token_456",
            None,
            now,
            now + 3600000,
        )
        .await
        .unwrap();

    assert!(id > 0);
}

#[tokio::test]
async fn test_create_validation_session_duplicate_session_id() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let session_id = format!("dup_session_{suffix}");
    let now = chrono::Utc::now().timestamp_millis();

    storage
        .create_validation_session(
            &session_id,
            "email",
            &format!("dup_{suffix}@example.com"),
            "secret1",
            "token1",
            None,
            now,
            now + 3600000,
        )
        .await
        .unwrap();

    let result = storage
        .create_validation_session(
            &session_id,
            "email",
            &format!("dup2_{suffix}@example.com"),
            "secret2",
            "token2",
            None,
            now,
            now + 3600000,
        )
        .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_get_validation_session() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let now = chrono::Utc::now().timestamp_millis();
    let session_id = format!("get_session_{suffix}");
    let client_secret = format!("secret_{suffix}");
    let token = format!("token_{suffix}");

    storage
        .create_validation_session(
            &session_id,
            "email",
            &format!("getval_{suffix}@example.com"),
            &client_secret,
            &token,
            None,
            now,
            now + 3600000,
        )
        .await
        .unwrap();

    let result = storage.get_validation_session(&session_id, &client_secret, &token).await.unwrap();
    assert!(result.is_some());
    let session = result.unwrap();
    assert_eq!(session.session_id, session_id);
    assert_eq!(session.client_secret, client_secret);
    assert_eq!(session.token, token);
    assert!(!session.is_validated);
}

#[tokio::test]
async fn test_get_validation_session_not_found() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);

    let result = storage.get_validation_session("nonexistent", "secret", "token").await.unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn test_get_validation_session_already_validated() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let now = chrono::Utc::now().timestamp_millis();
    let session_id = format!("validated_session_{suffix}");
    let client_secret = format!("vsecret_{suffix}");
    let token = format!("vtoken_{suffix}");

    let id = storage
        .create_validation_session(
            &session_id,
            "email",
            &format!("already_val_{suffix}@example.com"),
            &client_secret,
            &token,
            None,
            now,
            now + 3600000,
        )
        .await
        .unwrap();

    storage.mark_validation_validated(id).await.unwrap();

    let result = storage.get_validation_session(&session_id, &client_secret, &token).await.unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn test_get_validation_session_expired() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let now = chrono::Utc::now().timestamp_millis();
    let session_id = format!("expired_session_{suffix}");
    let client_secret = format!("esecret_{suffix}");
    let token = format!("etoken_{suffix}");

    storage
        .create_validation_session(
            &session_id,
            "email",
            &format!("expired_val_{suffix}@example.com"),
            &client_secret,
            &token,
            None,
            now,
            now - 1000,
        )
        .await
        .unwrap();

    let result = storage.get_validation_session(&session_id, &client_secret, &token).await.unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn test_get_validation_session_by_token() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let now = chrono::Utc::now().timestamp_millis();
    let token = format!("bytoken_{suffix}");

    storage
        .create_validation_session(
            &format!("bytoken_session_{suffix}"),
            "email",
            &format!("bytoken_{suffix}@example.com"),
            "secret_bytoken",
            &token,
            None,
            now,
            now + 3600000,
        )
        .await
        .unwrap();

    let result = storage.get_validation_session_by_token(&token).await.unwrap();
    assert!(result.is_some());
    let session = result.unwrap();
    assert_eq!(session.token, token);
}

#[tokio::test]
async fn test_get_validation_session_by_token_not_found() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);

    let result = storage.get_validation_session_by_token("nonexistent_token").await.unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn test_mark_validation_validated() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let now = chrono::Utc::now().timestamp_millis();
    let token = format!("mark_token_{suffix}");

    let id = storage
        .create_validation_session(
            &format!("mark_session_{suffix}"),
            "email",
            &format!("mark_{suffix}@example.com"),
            "secret_mark",
            &token,
            None,
            now,
            now + 3600000,
        )
        .await
        .unwrap();

    storage.mark_validation_validated(id).await.unwrap();

    let session = storage.get_validation_session_by_token(&token).await.unwrap().unwrap();
    assert!(session.is_validated);
    assert!(session.validated_at.is_some());
}

#[tokio::test]
async fn test_increment_validation_send_attempt() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let now = chrono::Utc::now().timestamp_millis();
    let token = format!("incr_token_{suffix}");

    let id = storage
        .create_validation_session(
            &format!("incr_session_{suffix}"),
            "email",
            &format!("incr_{suffix}@example.com"),
            "secret_incr",
            &token,
            None,
            now,
            now + 3600000,
        )
        .await
        .unwrap();

    let session_before = storage.get_validation_session_by_token(&token).await.unwrap().unwrap();
    assert_eq!(session_before.send_attempt, 0);

    storage.increment_validation_send_attempt(id).await.unwrap();

    let session_after = storage.get_validation_session_by_token(&token).await.unwrap().unwrap();
    assert_eq!(session_after.send_attempt, 1);

    storage.increment_validation_send_attempt(id).await.unwrap();

    let session_final = storage.get_validation_session_by_token(&token).await.unwrap().unwrap();
    assert_eq!(session_final.send_attempt, 2);
}

#[tokio::test]
async fn test_cleanup_expired_validation_sessions() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let now = chrono::Utc::now().timestamp_millis();

    storage
        .create_validation_session(
            &format!("expired_cleanup_session_{suffix}"),
            "email",
            &format!("expired_cleanup_{suffix}@example.com"),
            "secret_cleanup",
            "token_cleanup_expired",
            None,
            now - 200_000,
            now - 100_000,
        )
        .await
        .unwrap();

    storage
        .create_validation_session(
            &format!("valid_cleanup_session_{suffix}"),
            "email",
            &format!("valid_cleanup_{suffix}@example.com"),
            "secret_cleanup2",
            "token_cleanup_valid",
            None,
            now,
            now + 3600000,
        )
        .await
        .unwrap();

    let cleaned = storage.cleanup_expired_validation_sessions().await.unwrap();
    assert_eq!(cleaned, 1);

    let valid_session = storage.get_validation_session_by_token("token_cleanup_valid").await.unwrap();
    assert!(valid_session.is_some());

    let expired_session = storage.get_validation_session_by_token("token_cleanup_expired").await.unwrap();
    assert!(expired_session.is_none());
}

#[tokio::test]
async fn test_full_threepid_lifecycle() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_id = format!("@lifecycle_user_{suffix}:localhost");
    let address = format!("lifecycle_{suffix}@example.com");
    let token = format!("lifecycle_token_{suffix}");

    insert_user(&pool, &user_id, &format!("lifecycle_user_{suffix}")).await;

    let request = CreateThreepidRequest {
        user_id: user_id.clone(),
        medium: "email".to_string(),
        address: address.clone(),
        verification_token: Some(token.clone()),
        verification_expires_at: Some(chrono::Utc::now().timestamp_millis() + 3600000),
    };

    let threepid = storage.add_threepid(request).await.unwrap();
    assert!(!threepid.is_verified);

    let fetched = storage.get_threepid(&user_id, "email", &address).await.unwrap().unwrap();
    assert!(!fetched.is_verified);

    let verified = storage.verify_threepid(&user_id, "email", &address).await.unwrap();
    assert!(verified);

    let verified_threepid = storage.get_verified_threepid_by_address("email", &address).await.unwrap().unwrap();
    assert!(verified_threepid.is_verified);

    let removed = storage.remove_threepid(&user_id, "email", &address).await.unwrap();
    assert!(removed);

    let gone = storage.get_threepid(&user_id, "email", &address).await.unwrap();
    assert!(gone.is_none());
}

#[tokio::test]
async fn test_full_validation_session_lifecycle() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let now = chrono::Utc::now().timestamp_millis();
    let session_id = format!("lifecycle_sess_{suffix}");
    let client_secret = format!("lc_secret_{suffix}");
    let token = format!("lc_token_{suffix}");

    let id = storage
        .create_validation_session(
            &session_id,
            "email",
            &format!("lc_{suffix}@example.com"),
            &client_secret,
            &token,
            Some("https://example.com/next"),
            now,
            now + 3600000,
        )
        .await
        .unwrap();

    let session = storage.get_validation_session(&session_id, &client_secret, &token).await.unwrap().unwrap();
    assert!(!session.is_validated);
    assert_eq!(session.send_attempt, 0);

    storage.increment_validation_send_attempt(id).await.unwrap();

    storage.mark_validation_validated(id).await.unwrap();

    let validated_session = storage.get_validation_session_by_token(&token).await.unwrap().unwrap();
    assert!(validated_session.is_validated);
    assert!(validated_session.validated_at.is_some());
    assert_eq!(validated_session.send_attempt, 1);

    let not_found = storage.get_validation_session(&session_id, &client_secret, &token).await.unwrap();
    assert!(not_found.is_none());
}

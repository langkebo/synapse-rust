#![cfg(test)]

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use synapse_rust::storage::openid_token::{
    CreateOpenIdTokenRequest, OpenIdToken, OpenIdTokenStorage,
};
use tokio::runtime::Runtime;

static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

fn unique_id() -> u64 {
    TEST_COUNTER.fetch_add(1, Ordering::SeqCst)
}

async fn setup_test_database() -> Option<Arc<sqlx::PgPool>> {
    let pool = match synapse_rust::test_utils::prepare_empty_isolated_test_pool().await {
        Ok(pool) => pool,
        Err(error) => {
            eprintln!(
                "Skipping openid token storage tests because test database is unavailable: {error}"
            );
            return None;
        }
    };

    sqlx::query(
        r#"
        CREATE TABLE users (
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
    .execute(&*pool)
    .await
    .expect("Failed to create users table");

    sqlx::query(
        r#"
        CREATE TABLE openid_tokens (
            id BIGSERIAL PRIMARY KEY,
            token TEXT NOT NULL UNIQUE,
            user_id TEXT NOT NULL,
            device_id TEXT,
            created_ts BIGINT NOT NULL,
            expires_at BIGINT NOT NULL,
            is_valid BOOLEAN DEFAULT TRUE
        )
        "#,
    )
    .execute(&*pool)
    .await
    .expect("Failed to create openid_tokens table");

    Some(pool)
}

fn make_request(suffix: u64, user_id: &str, expires_at: i64) -> CreateOpenIdTokenRequest {
    CreateOpenIdTokenRequest {
        token: format!("openid_token_{suffix}"),
        user_id: user_id.to_string(),
        device_id: Some(format!("device_{suffix}")),
        expires_at,
    }
}

#[test]
fn test_create_openid_token_request_with_none_device_id() {
    let request = CreateOpenIdTokenRequest {
        token: "token_no_device".to_string(),
        user_id: "@nodevice:example.com".to_string(),
        device_id: None,
        expires_at: 9999999999999,
    };
    assert_eq!(request.token, "token_no_device");
    assert_eq!(request.user_id, "@nodevice:example.com");
    assert!(request.device_id.is_none());
    assert_eq!(request.expires_at, 9999999999999);
}

#[test]
fn test_openid_token_struct_without_device_id() {
    let token = OpenIdToken {
        id: 42,
        token: "tok_42".to_string(),
        user_id: "@user:example.com".to_string(),
        device_id: None,
        created_ts: 1000000,
        expires_at: 2000000,
        is_valid: true,
    };
    assert_eq!(token.id, 42);
    assert!(token.device_id.is_none());
    assert!(token.is_valid);
}

#[test]
fn test_openid_token_serialization_roundtrip() {
    let token = OpenIdToken {
        id: 1,
        token: "serialize_test".to_string(),
        user_id: "@serial:example.com".to_string(),
        device_id: Some("DEV1".to_string()),
        created_ts: 1111111111,
        expires_at: 2222222222,
        is_valid: true,
    };
    let json = serde_json::to_string(&token).unwrap();
    let deserialized: OpenIdToken = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.id, token.id);
    assert_eq!(deserialized.token, token.token);
    assert_eq!(deserialized.user_id, token.user_id);
    assert_eq!(deserialized.device_id, token.device_id);
    assert_eq!(deserialized.created_ts, token.created_ts);
    assert_eq!(deserialized.expires_at, token.expires_at);
    assert_eq!(deserialized.is_valid, token.is_valid);
}

#[test]
fn test_create_and_get_token() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };

        let storage = OpenIdTokenStorage::new(&pool);
        let suffix = unique_id();
        let user_id = format!("@oid_user_{suffix}:localhost");

        sqlx::query(
            "INSERT INTO users (user_id, username, creation_ts) VALUES ($1, $2, $3)",
        )
        .bind(&user_id)
        .bind(format!("oiduser{suffix}"))
        .bind(chrono::Utc::now().timestamp_millis())
        .execute(&*pool)
        .await
        .unwrap();

        let future_ts = chrono::Utc::now().timestamp_millis() + 3_600_000;
        let request = make_request(suffix, &user_id, future_ts);
        let token = storage.create_token(request).await.unwrap();

        assert!(token.id > 0);
        assert_eq!(token.token, format!("openid_token_{suffix}"));
        assert_eq!(token.user_id, user_id);
        assert_eq!(token.device_id, Some(format!("device_{suffix}")));
        assert!(token.is_valid);
        assert!(token.created_ts > 0);
        assert_eq!(token.expires_at, future_ts);

        let fetched = storage.get_token(&format!("openid_token_{suffix}")).await.unwrap();
        assert!(fetched.is_some());
        let fetched = fetched.unwrap();
        assert_eq!(fetched.id, token.id);
        assert_eq!(fetched.token, token.token);
        assert_eq!(fetched.user_id, token.user_id);
    });
}

#[test]
fn test_create_token_with_optional_device_id() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };

        let storage = OpenIdTokenStorage::new(&pool);
        let suffix = unique_id();
        let user_id = format!("@oid_user_{suffix}:localhost");

        sqlx::query(
            "INSERT INTO users (user_id, username, creation_ts) VALUES ($1, $2, $3)",
        )
        .bind(&user_id)
        .bind(format!("oiduser{suffix}"))
        .bind(chrono::Utc::now().timestamp_millis())
        .execute(&*pool)
        .await
        .unwrap();

        let future_ts = chrono::Utc::now().timestamp_millis() + 3_600_000;
        let request = CreateOpenIdTokenRequest {
            token: format!("openid_token_{suffix}"),
            user_id: user_id.clone(),
            device_id: None,
            expires_at: future_ts,
        };
        let token = storage.create_token(request).await.unwrap();
        assert!(token.device_id.is_none());
    });
}

#[test]
fn test_get_token_returns_none_for_missing() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };

        let storage = OpenIdTokenStorage::new(&pool);
        let result = storage.get_token("nonexistent_token").await.unwrap();
        assert!(result.is_none());
    });
}

#[test]
fn test_get_token_excludes_invalid() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };

        let storage = OpenIdTokenStorage::new(&pool);
        let suffix = unique_id();
        let user_id = format!("@oid_user_{suffix}:localhost");

        sqlx::query(
            "INSERT INTO users (user_id, username, creation_ts) VALUES ($1, $2, $3)",
        )
        .bind(&user_id)
        .bind(format!("oiduser{suffix}"))
        .bind(chrono::Utc::now().timestamp_millis())
        .execute(&*pool)
        .await
        .unwrap();

        let future_ts = chrono::Utc::now().timestamp_millis() + 3_600_000;
        let request = make_request(suffix, &user_id, future_ts);
        storage.create_token(request).await.unwrap();

        storage.revoke_token(&format!("openid_token_{suffix}")).await.unwrap();

        let result = storage.get_token(&format!("openid_token_{suffix}")).await.unwrap();
        assert!(result.is_none());
    });
}

#[test]
fn test_validate_token_success() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };

        let storage = OpenIdTokenStorage::new(&pool);
        let suffix = unique_id();
        let user_id = format!("@oid_user_{suffix}:localhost");

        sqlx::query(
            "INSERT INTO users (user_id, username, creation_ts) VALUES ($1, $2, $3)",
        )
        .bind(&user_id)
        .bind(format!("oiduser{suffix}"))
        .bind(chrono::Utc::now().timestamp_millis())
        .execute(&*pool)
        .await
        .unwrap();

        let future_ts = chrono::Utc::now().timestamp_millis() + 3_600_000;
        let request = make_request(suffix, &user_id, future_ts);
        storage.create_token(request).await.unwrap();

        let validated = storage.validate_token(&format!("openid_token_{suffix}")).await.unwrap();
        assert!(validated.is_some());
        let validated = validated.unwrap();
        assert_eq!(validated.token, format!("openid_token_{suffix}"));
        assert_eq!(validated.user_id, user_id);
        assert!(validated.is_valid);
    });
}

#[test]
fn test_validate_token_returns_none_for_expired() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };

        let storage = OpenIdTokenStorage::new(&pool);
        let suffix = unique_id();
        let user_id = format!("@oid_user_{suffix}:localhost");

        sqlx::query(
            "INSERT INTO users (user_id, username, creation_ts) VALUES ($1, $2, $3)",
        )
        .bind(&user_id)
        .bind(format!("oiduser{suffix}"))
        .bind(chrono::Utc::now().timestamp_millis())
        .execute(&*pool)
        .await
        .unwrap();

        let past_ts = chrono::Utc::now().timestamp_millis() - 3_600_000;
        let request = make_request(suffix, &user_id, past_ts);
        storage.create_token(request).await.unwrap();

        let validated = storage.validate_token(&format!("openid_token_{suffix}")).await.unwrap();
        assert!(validated.is_none());
    });
}

#[test]
fn test_validate_token_returns_none_for_revoked() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };

        let storage = OpenIdTokenStorage::new(&pool);
        let suffix = unique_id();
        let user_id = format!("@oid_user_{suffix}:localhost");

        sqlx::query(
            "INSERT INTO users (user_id, username, creation_ts) VALUES ($1, $2, $3)",
        )
        .bind(&user_id)
        .bind(format!("oiduser{suffix}"))
        .bind(chrono::Utc::now().timestamp_millis())
        .execute(&*pool)
        .await
        .unwrap();

        let future_ts = chrono::Utc::now().timestamp_millis() + 3_600_000;
        let request = make_request(suffix, &user_id, future_ts);
        storage.create_token(request).await.unwrap();

        storage.revoke_token(&format!("openid_token_{suffix}")).await.unwrap();

        let validated = storage.validate_token(&format!("openid_token_{suffix}")).await.unwrap();
        assert!(validated.is_none());
    });
}

#[test]
fn test_revoke_token_success() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };

        let storage = OpenIdTokenStorage::new(&pool);
        let suffix = unique_id();
        let user_id = format!("@oid_user_{suffix}:localhost");

        sqlx::query(
            "INSERT INTO users (user_id, username, creation_ts) VALUES ($1, $2, $3)",
        )
        .bind(&user_id)
        .bind(format!("oiduser{suffix}"))
        .bind(chrono::Utc::now().timestamp_millis())
        .execute(&*pool)
        .await
        .unwrap();

        let future_ts = chrono::Utc::now().timestamp_millis() + 3_600_000;
        let request = make_request(suffix, &user_id, future_ts);
        let token = storage.create_token(request).await.unwrap();
        assert!(token.is_valid);

        let revoked = storage.revoke_token(&format!("openid_token_{suffix}")).await.unwrap();
        assert!(revoked);

        let all_tokens = storage.get_tokens_by_user(&user_id).await.unwrap();
        let revoked_token = all_tokens.iter().find(|t| t.id == token.id).unwrap();
        assert!(!revoked_token.is_valid);
    });
}

#[test]
fn test_revoke_token_returns_false_for_missing() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };

        let storage = OpenIdTokenStorage::new(&pool);
        let result = storage.revoke_token("nonexistent_token").await.unwrap();
        assert!(!result);
    });
}

#[test]
fn test_revoke_user_tokens() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };

        let storage = OpenIdTokenStorage::new(&pool);
        let suffix = unique_id();
        let user_id = format!("@oid_user_{suffix}:localhost");

        sqlx::query(
            "INSERT INTO users (user_id, username, creation_ts) VALUES ($1, $2, $3)",
        )
        .bind(&user_id)
        .bind(format!("oiduser{suffix}"))
        .bind(chrono::Utc::now().timestamp_millis())
        .execute(&*pool)
        .await
        .unwrap();

        let future_ts = chrono::Utc::now().timestamp_millis() + 3_600_000;

        for i in 0..3 {
            let req = CreateOpenIdTokenRequest {
                token: format!("openid_token_{suffix}_{i}"),
                user_id: user_id.clone(),
                device_id: Some(format!("device_{suffix}_{i}")),
                expires_at: future_ts,
            };
            storage.create_token(req).await.unwrap();
        }

        let count = storage.revoke_user_tokens(&user_id).await.unwrap();
        assert_eq!(count, 3);

        let tokens = storage.get_tokens_by_user(&user_id).await.unwrap();
        assert!(tokens.iter().all(|t| !t.is_valid));
    });
}

#[test]
fn test_revoke_user_tokens_skips_already_revoked() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };

        let storage = OpenIdTokenStorage::new(&pool);
        let suffix = unique_id();
        let user_id = format!("@oid_user_{suffix}:localhost");

        sqlx::query(
            "INSERT INTO users (user_id, username, creation_ts) VALUES ($1, $2, $3)",
        )
        .bind(&user_id)
        .bind(format!("oiduser{suffix}"))
        .bind(chrono::Utc::now().timestamp_millis())
        .execute(&*pool)
        .await
        .unwrap();

        let future_ts = chrono::Utc::now().timestamp_millis() + 3_600_000;

        let req1 = CreateOpenIdTokenRequest {
            token: format!("openid_token_{suffix}_0"),
            user_id: user_id.clone(),
            device_id: None,
            expires_at: future_ts,
        };
        storage.create_token(req1).await.unwrap();

        let req2 = CreateOpenIdTokenRequest {
            token: format!("openid_token_{suffix}_1"),
            user_id: user_id.clone(),
            device_id: None,
            expires_at: future_ts,
        };
        storage.create_token(req2).await.unwrap();
        storage.revoke_token(&format!("openid_token_{suffix}_1")).await.unwrap();

        let count = storage.revoke_user_tokens(&user_id).await.unwrap();
        assert_eq!(count, 1);
    });
}

#[test]
fn test_cleanup_expired_tokens() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };

        let storage = OpenIdTokenStorage::new(&pool);
        let suffix = unique_id();
        let user_id = format!("@oid_user_{suffix}:localhost");

        sqlx::query(
            "INSERT INTO users (user_id, username, creation_ts) VALUES ($1, $2, $3)",
        )
        .bind(&user_id)
        .bind(format!("oiduser{suffix}"))
        .bind(chrono::Utc::now().timestamp_millis())
        .execute(&*pool)
        .await
        .unwrap();

        let past_ts = chrono::Utc::now().timestamp_millis() - 3_600_000;
        let future_ts = chrono::Utc::now().timestamp_millis() + 3_600_000;

        let expired_req = CreateOpenIdTokenRequest {
            token: format!("expired_{suffix}"),
            user_id: user_id.clone(),
            device_id: None,
            expires_at: past_ts,
        };
        storage.create_token(expired_req).await.unwrap();

        let active_req = CreateOpenIdTokenRequest {
            token: format!("active_{suffix}"),
            user_id: user_id.clone(),
            device_id: None,
            expires_at: future_ts,
        };
        storage.create_token(active_req).await.unwrap();

        let deleted = storage.cleanup_expired_tokens().await.unwrap();
        assert_eq!(deleted, 1);

        let tokens = storage.get_tokens_by_user(&user_id).await.unwrap();
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].token, format!("active_{suffix}"));
    });
}

#[test]
fn test_cleanup_also_removes_revoked_tokens() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };

        let storage = OpenIdTokenStorage::new(&pool);
        let suffix = unique_id();
        let user_id = format!("@oid_user_{suffix}:localhost");

        sqlx::query(
            "INSERT INTO users (user_id, username, creation_ts) VALUES ($1, $2, $3)",
        )
        .bind(&user_id)
        .bind(format!("oiduser{suffix}"))
        .bind(chrono::Utc::now().timestamp_millis())
        .execute(&*pool)
        .await
        .unwrap();

        let future_ts = chrono::Utc::now().timestamp_millis() + 3_600_000;

        let req = CreateOpenIdTokenRequest {
            token: format!("revoked_{suffix}"),
            user_id: user_id.clone(),
            device_id: None,
            expires_at: future_ts,
        };
        storage.create_token(req).await.unwrap();

        storage.revoke_token(&format!("revoked_{suffix}")).await.unwrap();

        let deleted = storage.cleanup_expired_tokens().await.unwrap();
        assert_eq!(deleted, 1);

        let tokens = storage.get_tokens_by_user(&user_id).await.unwrap();
        assert!(tokens.is_empty());
    });
}

#[test]
fn test_get_tokens_by_user() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };

        let storage = OpenIdTokenStorage::new(&pool);
        let suffix = unique_id();
        let user_id = format!("@oid_user_{suffix}:localhost");

        sqlx::query(
            "INSERT INTO users (user_id, username, creation_ts) VALUES ($1, $2, $3)",
        )
        .bind(&user_id)
        .bind(format!("oiduser{suffix}"))
        .bind(chrono::Utc::now().timestamp_millis())
        .execute(&*pool)
        .await
        .unwrap();

        let future_ts = chrono::Utc::now().timestamp_millis() + 3_600_000;

        for i in 0..3 {
            let req = CreateOpenIdTokenRequest {
                token: format!("openid_token_{suffix}_{i}"),
                user_id: user_id.clone(),
                device_id: Some(format!("device_{suffix}_{i}")),
                expires_at: future_ts,
            };
            storage.create_token(req).await.unwrap();
        }

        let tokens = storage.get_tokens_by_user(&user_id).await.unwrap();
        assert_eq!(tokens.len(), 3);
        assert!(tokens.iter().all(|t| t.user_id == user_id));
    });
}

#[test]
fn test_get_tokens_by_user_empty() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };

        let storage = OpenIdTokenStorage::new(&pool);
        let tokens = storage.get_tokens_by_user("@nonexistent:localhost").await.unwrap();
        assert!(tokens.is_empty());
    });
}

#![cfg(test)]

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use synapse_rust::storage::refresh_token::{CreateRefreshTokenRequest, RefreshTokenStorage};
use tokio::runtime::Runtime;

static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

fn unique_id() -> u64 {
    TEST_COUNTER.fetch_add(1, Ordering::SeqCst)
}

async fn setup_test_database() -> Option<Arc<sqlx::PgPool>> {
    let pool = match synapse_rust::test_utils::prepare_empty_isolated_test_pool().await {
        Ok(pool) => pool,
        Err(error) => {
            eprintln!("Skipping refresh token storage tests because test database is unavailable: {error}");
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
        CREATE TABLE refresh_tokens (
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
    .execute(&*pool)
    .await
    .expect("Failed to create refresh_tokens table");

    sqlx::query(
        r#"
        CREATE TABLE refresh_token_usage (
            id BIGSERIAL PRIMARY KEY,
            refresh_token_id BIGINT NOT NULL,
            user_id TEXT NOT NULL,
            old_access_token_id TEXT,
            new_access_token_id TEXT,
            used_ts BIGINT NOT NULL,
            ip_address TEXT,
            user_agent TEXT,
            success BOOLEAN DEFAULT TRUE,
            error_message TEXT
        )
        "#,
    )
    .execute(&*pool)
    .await
    .expect("Failed to create refresh_token_usage table");

    sqlx::query(
        r#"
        CREATE TABLE refresh_token_families (
            id BIGSERIAL PRIMARY KEY,
            family_id TEXT NOT NULL UNIQUE,
            user_id TEXT NOT NULL,
            device_id TEXT,
            created_ts BIGINT NOT NULL,
            last_refresh_ts BIGINT,
            refresh_count INTEGER DEFAULT 0,
            is_compromised BOOLEAN DEFAULT FALSE,
            compromised_ts BIGINT
        )
        "#,
    )
    .execute(&*pool)
    .await
    .expect("Failed to create refresh_token_families table");

    sqlx::query(
        r#"
        CREATE TABLE refresh_token_rotations (
            id BIGSERIAL PRIMARY KEY,
            family_id TEXT NOT NULL,
            old_token_hash TEXT,
            new_token_hash TEXT NOT NULL,
            rotated_ts BIGINT NOT NULL,
            rotation_reason TEXT
        )
        "#,
    )
    .execute(&*pool)
    .await
    .expect("Failed to create refresh_token_rotations table");

    sqlx::query(
        r#"
        CREATE TABLE token_blacklist (
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
    .execute(&*pool)
    .await
    .expect("Failed to create token_blacklist table");

    Some(pool)
}

fn make_request(suffix: u64, user_id: &str, expires_at: i64) -> CreateRefreshTokenRequest {
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

#[test]
fn test_create_and_get_token() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };

        let storage = RefreshTokenStorage::new(&pool);
        let suffix = unique_id();
        let user_id = format!("@rt_user_{suffix}:localhost");

        sqlx::query("INSERT INTO users (user_id, username, creation_ts) VALUES ($1, $2, $3)")
            .bind(&user_id)
            .bind(format!("rtuser{suffix}"))
            .bind(chrono::Utc::now().timestamp_millis())
            .execute(&*pool)
            .await
            .unwrap();

        let future_ts = chrono::Utc::now().timestamp_millis() + 3_600_000;
        let request = make_request(suffix, &user_id, future_ts);
        let token = storage.create_token(request).await.unwrap();

        assert!(token.id > 0);
        assert_eq!(token.token_hash, format!("hash_{suffix}"));
        assert_eq!(token.user_id, user_id);
        assert_eq!(token.device_id, Some(format!("device_{suffix}")));
        assert_eq!(token.access_token_id, Some(format!("atid_{suffix}")));
        assert_eq!(token.scope, Some("openid".to_string()));
        assert!(!token.is_revoked);
        assert_eq!(token.use_count, 0);
        assert!(token.created_ts > 0);

        let fetched = storage.get_token(&format!("hash_{suffix}")).await.unwrap();
        assert!(fetched.is_some());
        let fetched = fetched.unwrap();
        assert_eq!(fetched.id, token.id);
        assert_eq!(fetched.token_hash, token.token_hash);
        assert_eq!(fetched.user_id, token.user_id);
    });
}

#[test]
fn test_get_token_by_id() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };

        let storage = RefreshTokenStorage::new(&pool);
        let suffix = unique_id();
        let user_id = format!("@rt_user_{suffix}:localhost");

        sqlx::query("INSERT INTO users (user_id, username, creation_ts) VALUES ($1, $2, $3)")
            .bind(&user_id)
            .bind(format!("rtuser{suffix}"))
            .bind(chrono::Utc::now().timestamp_millis())
            .execute(&*pool)
            .await
            .unwrap();

        let future_ts = chrono::Utc::now().timestamp_millis() + 3_600_000;
        let request = make_request(suffix, &user_id, future_ts);
        let token = storage.create_token(request).await.unwrap();

        let fetched = storage.get_token_by_id(token.id).await.unwrap();
        assert!(fetched.is_some());
        assert_eq!(fetched.unwrap().token_hash, format!("hash_{suffix}"));

        let missing = storage.get_token_by_id(999999).await.unwrap();
        assert!(missing.is_none());
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

        let storage = RefreshTokenStorage::new(&pool);
        let result = storage.get_token("nonexistent_hash").await.unwrap();
        assert!(result.is_none());
    });
}

#[test]
fn test_get_user_tokens() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };

        let storage = RefreshTokenStorage::new(&pool);
        let suffix = unique_id();
        let user_id = format!("@rt_user_{suffix}:localhost");

        sqlx::query("INSERT INTO users (user_id, username, creation_ts) VALUES ($1, $2, $3)")
            .bind(&user_id)
            .bind(format!("rtuser{suffix}"))
            .bind(chrono::Utc::now().timestamp_millis())
            .execute(&*pool)
            .await
            .unwrap();

        let future_ts = chrono::Utc::now().timestamp_millis() + 3_600_000;

        for i in 0..3 {
            let req = CreateRefreshTokenRequest {
                token_hash: format!("hash_{suffix}_{i}"),
                user_id: user_id.clone(),
                device_id: Some(format!("device_{suffix}_{i}")),
                access_token_id: None,
                scope: None,
                expires_at: future_ts,
                client_info: None,
                ip_address: None,
                user_agent: None,
            };
            storage.create_token(req).await.unwrap();
        }

        let tokens = storage.get_user_tokens(&user_id).await.unwrap();
        assert_eq!(tokens.len(), 3);
        assert!(tokens.iter().all(|t| t.user_id == user_id));
    });
}

#[test]
fn test_get_active_tokens_excludes_revoked_and_expired() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };

        let storage = RefreshTokenStorage::new(&pool);
        let suffix = unique_id();
        let user_id = format!("@rt_user_{suffix}:localhost");

        sqlx::query("INSERT INTO users (user_id, username, creation_ts) VALUES ($1, $2, $3)")
            .bind(&user_id)
            .bind(format!("rtuser{suffix}"))
            .bind(chrono::Utc::now().timestamp_millis())
            .execute(&*pool)
            .await
            .unwrap();

        let future_ts = chrono::Utc::now().timestamp_millis() + 3_600_000;
        let past_ts = chrono::Utc::now().timestamp_millis() - 3_600_000;

        let active_req = CreateRefreshTokenRequest {
            token_hash: format!("active_{suffix}"),
            user_id: user_id.clone(),
            device_id: None,
            access_token_id: None,
            scope: None,
            expires_at: future_ts,
            client_info: None,
            ip_address: None,
            user_agent: None,
        };
        storage.create_token(active_req).await.unwrap();

        let revoked_req = CreateRefreshTokenRequest {
            token_hash: format!("revoked_{suffix}"),
            user_id: user_id.clone(),
            device_id: None,
            access_token_id: None,
            scope: None,
            expires_at: future_ts,
            client_info: None,
            ip_address: None,
            user_agent: None,
        };
        storage.create_token(revoked_req).await.unwrap();
        storage.revoke_token(&format!("revoked_{suffix}"), "test revoke").await.unwrap();

        let expired_req = CreateRefreshTokenRequest {
            token_hash: format!("expired_{suffix}"),
            user_id: user_id.clone(),
            device_id: None,
            access_token_id: None,
            scope: None,
            expires_at: past_ts,
            client_info: None,
            ip_address: None,
            user_agent: None,
        };
        storage.create_token(expired_req).await.unwrap();

        let active_tokens = storage.get_active_tokens(&user_id).await.unwrap();
        assert_eq!(active_tokens.len(), 1);
        assert_eq!(active_tokens[0].token_hash, format!("active_{suffix}"));
        assert!(!active_tokens[0].is_revoked);
    });
}

#[test]
fn test_revoke_token() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };

        let storage = RefreshTokenStorage::new(&pool);
        let suffix = unique_id();
        let user_id = format!("@rt_user_{suffix}:localhost");

        sqlx::query("INSERT INTO users (user_id, username, creation_ts) VALUES ($1, $2, $3)")
            .bind(&user_id)
            .bind(format!("rtuser{suffix}"))
            .bind(chrono::Utc::now().timestamp_millis())
            .execute(&*pool)
            .await
            .unwrap();

        let future_ts = chrono::Utc::now().timestamp_millis() + 3_600_000;
        let request = make_request(suffix, &user_id, future_ts);
        let token = storage.create_token(request).await.unwrap();
        assert!(!token.is_revoked);

        storage.revoke_token(&format!("hash_{suffix}"), "user logout").await.unwrap();

        let revoked = storage.get_token(&format!("hash_{suffix}")).await.unwrap().unwrap();
        assert!(revoked.is_revoked);
        assert_eq!(revoked.revoked_reason, Some("user logout".to_string()));
    });
}

#[test]
fn test_revoke_token_by_id() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };

        let storage = RefreshTokenStorage::new(&pool);
        let suffix = unique_id();
        let user_id = format!("@rt_user_{suffix}:localhost");

        sqlx::query("INSERT INTO users (user_id, username, creation_ts) VALUES ($1, $2, $3)")
            .bind(&user_id)
            .bind(format!("rtuser{suffix}"))
            .bind(chrono::Utc::now().timestamp_millis())
            .execute(&*pool)
            .await
            .unwrap();

        let future_ts = chrono::Utc::now().timestamp_millis() + 3_600_000;
        let request = make_request(suffix, &user_id, future_ts);
        let token = storage.create_token(request).await.unwrap();

        storage.revoke_token_by_id(token.id, "security breach").await.unwrap();

        let revoked = storage.get_token_by_id(token.id).await.unwrap().unwrap();
        assert!(revoked.is_revoked);
        assert_eq!(revoked.revoked_reason, Some("security breach".to_string()));
    });
}

#[test]
fn test_revoke_all_user_tokens() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };

        let storage = RefreshTokenStorage::new(&pool);
        let suffix = unique_id();
        let user_id = format!("@rt_user_{suffix}:localhost");

        sqlx::query("INSERT INTO users (user_id, username, creation_ts) VALUES ($1, $2, $3)")
            .bind(&user_id)
            .bind(format!("rtuser{suffix}"))
            .bind(chrono::Utc::now().timestamp_millis())
            .execute(&*pool)
            .await
            .unwrap();

        let future_ts = chrono::Utc::now().timestamp_millis() + 3_600_000;

        for i in 0..3 {
            let req = CreateRefreshTokenRequest {
                token_hash: format!("hash_{suffix}_{i}"),
                user_id: user_id.clone(),
                device_id: None,
                access_token_id: None,
                scope: None,
                expires_at: future_ts,
                client_info: None,
                ip_address: None,
                user_agent: None,
            };
            storage.create_token(req).await.unwrap();
        }

        let count = storage.revoke_all_user_tokens(&user_id, "bulk revoke").await.unwrap();
        assert_eq!(count, 3);

        let tokens = storage.get_user_tokens(&user_id).await.unwrap();
        assert!(tokens.iter().all(|t| t.is_revoked));
        assert!(tokens.iter().all(|t| t.revoked_reason == Some("bulk revoke".to_string())));
    });
}

#[test]
fn test_revoke_all_user_tokens_skips_already_revoked() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };

        let storage = RefreshTokenStorage::new(&pool);
        let suffix = unique_id();
        let user_id = format!("@rt_user_{suffix}:localhost");

        sqlx::query("INSERT INTO users (user_id, username, creation_ts) VALUES ($1, $2, $3)")
            .bind(&user_id)
            .bind(format!("rtuser{suffix}"))
            .bind(chrono::Utc::now().timestamp_millis())
            .execute(&*pool)
            .await
            .unwrap();

        let future_ts = chrono::Utc::now().timestamp_millis() + 3_600_000;

        let req1 = CreateRefreshTokenRequest {
            token_hash: format!("hash_{suffix}_0"),
            user_id: user_id.clone(),
            device_id: None,
            access_token_id: None,
            scope: None,
            expires_at: future_ts,
            client_info: None,
            ip_address: None,
            user_agent: None,
        };
        storage.create_token(req1).await.unwrap();

        let req2 = CreateRefreshTokenRequest {
            token_hash: format!("hash_{suffix}_1"),
            user_id: user_id.clone(),
            device_id: None,
            access_token_id: None,
            scope: None,
            expires_at: future_ts,
            client_info: None,
            ip_address: None,
            user_agent: None,
        };
        storage.create_token(req2).await.unwrap();
        storage.revoke_token(&format!("hash_{suffix}_1"), "already revoked").await.unwrap();

        let count = storage.revoke_all_user_tokens(&user_id, "bulk revoke").await.unwrap();
        assert_eq!(count, 1);
    });
}

#[test]
fn test_update_token_usage() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };

        let storage = RefreshTokenStorage::new(&pool);
        let suffix = unique_id();
        let user_id = format!("@rt_user_{suffix}:localhost");

        sqlx::query("INSERT INTO users (user_id, username, creation_ts) VALUES ($1, $2, $3)")
            .bind(&user_id)
            .bind(format!("rtuser{suffix}"))
            .bind(chrono::Utc::now().timestamp_millis())
            .execute(&*pool)
            .await
            .unwrap();

        let future_ts = chrono::Utc::now().timestamp_millis() + 3_600_000;
        let request = make_request(suffix, &user_id, future_ts);
        let token = storage.create_token(request).await.unwrap();
        assert_eq!(token.use_count, 0);
        assert!(token.last_used_ts.is_none());

        storage.update_token_usage(&format!("hash_{suffix}"), "new_atid_123").await.unwrap();

        let updated = storage.get_token(&format!("hash_{suffix}")).await.unwrap().unwrap();
        assert_eq!(updated.use_count, 1);
        assert!(updated.last_used_ts.is_some());
        assert_eq!(updated.access_token_id, Some("new_atid_123".to_string()));

        storage.update_token_usage(&format!("hash_{suffix}"), "new_atid_456").await.unwrap();

        let updated2 = storage.get_token(&format!("hash_{suffix}")).await.unwrap().unwrap();
        assert_eq!(updated2.use_count, 2);
        assert_eq!(updated2.access_token_id, Some("new_atid_456".to_string()));
    });
}

#[test]
fn test_delete_token() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };

        let storage = RefreshTokenStorage::new(&pool);
        let suffix = unique_id();
        let user_id = format!("@rt_user_{suffix}:localhost");

        sqlx::query("INSERT INTO users (user_id, username, creation_ts) VALUES ($1, $2, $3)")
            .bind(&user_id)
            .bind(format!("rtuser{suffix}"))
            .bind(chrono::Utc::now().timestamp_millis())
            .execute(&*pool)
            .await
            .unwrap();

        let future_ts = chrono::Utc::now().timestamp_millis() + 3_600_000;
        let request = make_request(suffix, &user_id, future_ts);
        storage.create_token(request).await.unwrap();

        let before = storage.get_token(&format!("hash_{suffix}")).await.unwrap();
        assert!(before.is_some());

        storage.delete_token(&format!("hash_{suffix}")).await.unwrap();

        let after = storage.get_token(&format!("hash_{suffix}")).await.unwrap();
        assert!(after.is_none());
    });
}

#[test]
fn test_delete_user_tokens() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };

        let storage = RefreshTokenStorage::new(&pool);
        let suffix = unique_id();
        let user_id = format!("@rt_user_{suffix}:localhost");

        sqlx::query("INSERT INTO users (user_id, username, creation_ts) VALUES ($1, $2, $3)")
            .bind(&user_id)
            .bind(format!("rtuser{suffix}"))
            .bind(chrono::Utc::now().timestamp_millis())
            .execute(&*pool)
            .await
            .unwrap();

        let future_ts = chrono::Utc::now().timestamp_millis() + 3_600_000;

        for i in 0..3 {
            let req = CreateRefreshTokenRequest {
                token_hash: format!("hash_{suffix}_{i}"),
                user_id: user_id.clone(),
                device_id: None,
                access_token_id: None,
                scope: None,
                expires_at: future_ts,
                client_info: None,
                ip_address: None,
                user_agent: None,
            };
            storage.create_token(req).await.unwrap();
        }

        let count = storage.delete_user_tokens(&user_id).await.unwrap();
        assert_eq!(count, 3);

        let tokens = storage.get_user_tokens(&user_id).await.unwrap();
        assert!(tokens.is_empty());
    });
}

#[test]
fn test_add_to_blacklist_and_is_blacklisted() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };

        let storage = RefreshTokenStorage::new(&pool);
        let suffix = unique_id();
        let token_hash = format!("blacklisted_{suffix}");
        let user_id = format!("@rt_user_{suffix}:localhost");
        let future_ts = chrono::Utc::now().timestamp_millis() + 3_600_000;

        let blacklisted = storage.is_blacklisted(&token_hash).await.unwrap();
        assert!(!blacklisted);

        storage.add_to_blacklist(&token_hash, "refresh", &user_id, future_ts, Some("compromised")).await.unwrap();

        let blacklisted = storage.is_blacklisted(&token_hash).await.unwrap();
        assert!(blacklisted);
    });
}

#[test]
fn test_is_blacklisted_returns_false_for_expired_entry() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };

        let storage = RefreshTokenStorage::new(&pool);
        let suffix = unique_id();
        let token_hash = format!("expired_bl_{suffix}");
        let user_id = format!("@rt_user_{suffix}:localhost");
        let past_ts = chrono::Utc::now().timestamp_millis() - 3_600_000;

        storage.add_to_blacklist(&token_hash, "refresh", &user_id, past_ts, None).await.unwrap();

        let blacklisted = storage.is_blacklisted(&token_hash).await.unwrap();
        assert!(!blacklisted);
    });
}

#[test]
fn test_add_to_blacklist_idempotent() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };

        let storage = RefreshTokenStorage::new(&pool);
        let suffix = unique_id();
        let token_hash = format!("idempotent_{suffix}");
        let user_id = format!("@rt_user_{suffix}:localhost");
        let future_ts = chrono::Utc::now().timestamp_millis() + 3_600_000;

        storage.add_to_blacklist(&token_hash, "refresh", &user_id, future_ts, None).await.unwrap();

        storage.add_to_blacklist(&token_hash, "refresh", &user_id, future_ts, None).await.unwrap();

        let blacklisted = storage.is_blacklisted(&token_hash).await.unwrap();
        assert!(blacklisted);
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

        let storage = RefreshTokenStorage::new(&pool);
        let suffix = unique_id();
        let user_id = format!("@rt_user_{suffix}:localhost");

        sqlx::query("INSERT INTO users (user_id, username, creation_ts) VALUES ($1, $2, $3)")
            .bind(&user_id)
            .bind(format!("rtuser{suffix}"))
            .bind(chrono::Utc::now().timestamp_millis())
            .execute(&*pool)
            .await
            .unwrap();

        let past_ts = chrono::Utc::now().timestamp_millis() - 3_600_000;
        let future_ts = chrono::Utc::now().timestamp_millis() + 3_600_000;

        let expired_req = CreateRefreshTokenRequest {
            token_hash: format!("expired_{suffix}"),
            user_id: user_id.clone(),
            device_id: None,
            access_token_id: None,
            scope: None,
            expires_at: past_ts,
            client_info: None,
            ip_address: None,
            user_agent: None,
        };
        storage.create_token(expired_req).await.unwrap();

        let active_req = CreateRefreshTokenRequest {
            token_hash: format!("active_{suffix}"),
            user_id: user_id.clone(),
            device_id: None,
            access_token_id: None,
            scope: None,
            expires_at: future_ts,
            client_info: None,
            ip_address: None,
            user_agent: None,
        };
        storage.create_token(active_req).await.unwrap();

        let deleted = storage.cleanup_expired_tokens().await.unwrap();
        assert_eq!(deleted, 1);

        let tokens = storage.get_user_tokens(&user_id).await.unwrap();
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].token_hash, format!("active_{suffix}"));
    });
}

#[test]
fn test_cleanup_blacklist() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };

        let storage = RefreshTokenStorage::new(&pool);
        let suffix = unique_id();
        let past_ts = chrono::Utc::now().timestamp_millis() - 3_600_000;
        let future_ts = chrono::Utc::now().timestamp_millis() + 3_600_000;

        storage
            .add_to_blacklist(&format!("expired_bl_{suffix}"), "refresh", "@user:localhost", past_ts, None)
            .await
            .unwrap();

        storage
            .add_to_blacklist(&format!("active_bl_{suffix}"), "refresh", "@user:localhost", future_ts, None)
            .await
            .unwrap();

        let deleted = storage.cleanup_blacklist().await.unwrap();
        assert_eq!(deleted, 1);

        assert!(!storage.is_blacklisted(&format!("expired_bl_{suffix}")).await.unwrap());
        assert!(storage.is_blacklisted(&format!("active_bl_{suffix}")).await.unwrap());
    });
}

#[test]
fn test_revoke_token_cas() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };

        let storage = RefreshTokenStorage::new(&pool);
        let suffix = unique_id();
        let user_id = format!("@rt_user_{suffix}:localhost");

        sqlx::query("INSERT INTO users (user_id, username, creation_ts) VALUES ($1, $2, $3)")
            .bind(&user_id)
            .bind(format!("rtuser{suffix}"))
            .bind(chrono::Utc::now().timestamp_millis())
            .execute(&*pool)
            .await
            .unwrap();

        let future_ts = chrono::Utc::now().timestamp_millis() + 3_600_000;
        let request = make_request(suffix, &user_id, future_ts);
        storage.create_token(request).await.unwrap();

        let first = storage.revoke_token_cas(&format!("hash_{suffix}"), "first revoke").await.unwrap();
        assert!(first);

        let second = storage.revoke_token_cas(&format!("hash_{suffix}"), "second revoke").await.unwrap();
        assert!(!second);

        let token = storage.get_token(&format!("hash_{suffix}")).await.unwrap().unwrap();
        assert_eq!(token.revoked_reason, Some("first revoke".to_string()));
    });
}

#[test]
fn test_create_and_get_family() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };

        let storage = RefreshTokenStorage::new(&pool);
        let suffix = unique_id();
        let user_id = format!("@rt_user_{suffix}:localhost");
        let family_id = format!("family_{suffix}");

        sqlx::query("INSERT INTO users (user_id, username, creation_ts) VALUES ($1, $2, $3)")
            .bind(&user_id)
            .bind(format!("rtuser{suffix}"))
            .bind(chrono::Utc::now().timestamp_millis())
            .execute(&*pool)
            .await
            .unwrap();

        let family = storage.create_family(&family_id, &user_id, Some("device_1")).await.unwrap();

        assert!(family.id > 0);
        assert_eq!(family.family_id, family_id);
        assert_eq!(family.user_id, user_id);
        assert_eq!(family.device_id, Some("device_1".to_string()));
        assert!(!family.is_compromised);
        assert_eq!(family.refresh_count, 0);

        let fetched = storage.get_family(&family_id).await.unwrap();
        assert!(fetched.is_some());
        assert_eq!(fetched.unwrap().family_id, family_id);

        let missing = storage.get_family("nonexistent_family").await.unwrap();
        assert!(missing.is_none());
    });
}

#[test]
fn test_mark_family_compromised() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };

        let storage = RefreshTokenStorage::new(&pool);
        let suffix = unique_id();
        let user_id = format!("@rt_user_{suffix}:localhost");
        let family_id = format!("family_{suffix}");

        sqlx::query("INSERT INTO users (user_id, username, creation_ts) VALUES ($1, $2, $3)")
            .bind(&user_id)
            .bind(format!("rtuser{suffix}"))
            .bind(chrono::Utc::now().timestamp_millis())
            .execute(&*pool)
            .await
            .unwrap();

        storage.create_family(&family_id, &user_id, None).await.unwrap();

        storage.mark_family_compromised(&family_id).await.unwrap();

        let family = storage.get_family(&family_id).await.unwrap().unwrap();
        assert!(family.is_compromised);
        assert!(family.compromised_ts.is_some());
    });
}

#[test]
fn test_record_rotation_and_get_rotations() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };

        let storage = RefreshTokenStorage::new(&pool);
        let suffix = unique_id();
        let user_id = format!("@rt_user_{suffix}:localhost");
        let family_id = format!("family_{suffix}");

        sqlx::query("INSERT INTO users (user_id, username, creation_ts) VALUES ($1, $2, $3)")
            .bind(&user_id)
            .bind(format!("rtuser{suffix}"))
            .bind(chrono::Utc::now().timestamp_millis())
            .execute(&*pool)
            .await
            .unwrap();

        storage.create_family(&family_id, &user_id, None).await.unwrap();

        storage.record_rotation(&family_id, Some("old_hash_1"), "new_hash_1", "refresh").await.unwrap();

        let family = storage.get_family(&family_id).await.unwrap().unwrap();
        assert_eq!(family.refresh_count, 1);
        assert!(family.last_refresh_ts.is_some());

        storage.record_rotation(&family_id, Some("old_hash_2"), "new_hash_2", "refresh").await.unwrap();

        let rotations = storage.get_rotations(&family_id).await.unwrap();
        assert_eq!(rotations.len(), 2);
        assert_eq!(rotations[0].new_token_hash, "new_hash_2");
        assert_eq!(rotations[1].new_token_hash, "new_hash_1");
    });
}

#[test]
fn test_record_usage() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };

        let storage = RefreshTokenStorage::new(&pool);
        let suffix = unique_id();
        let user_id = format!("@rt_user_{suffix}:localhost");

        sqlx::query("INSERT INTO users (user_id, username, creation_ts) VALUES ($1, $2, $3)")
            .bind(&user_id)
            .bind(format!("rtuser{suffix}"))
            .bind(chrono::Utc::now().timestamp_millis())
            .execute(&*pool)
            .await
            .unwrap();

        let future_ts = chrono::Utc::now().timestamp_millis() + 3_600_000;
        let request = make_request(suffix, &user_id, future_ts);
        let token = storage.create_token(request).await.unwrap();

        let usage_req =
            synapse_rust::storage::refresh_token::RecordUsageRequest::new(token.id, &user_id, "new_atid_123", true)
                .old_access_token_id("old_atid_abc");

        storage.record_usage(&usage_req).await.unwrap();

        let history = storage.get_usage_history(&user_id, 10).await.unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].refresh_token_id, token.id);
        assert_eq!(history[0].user_id, user_id);
        assert_eq!(history[0].old_access_token_id, Some("old_atid_abc".to_string()));
        assert_eq!(history[0].new_access_token_id, Some("new_atid_123".to_string()));
        assert!(history[0].success);
    });
}

#[test]
fn test_revoke_device_tokens() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };

        let storage = RefreshTokenStorage::new(&pool);
        let suffix = unique_id();
        let user_id = format!("@rt_user_{suffix}:localhost");

        sqlx::query("INSERT INTO users (user_id, username, creation_ts) VALUES ($1, $2, $3)")
            .bind(&user_id)
            .bind(format!("rtuser{suffix}"))
            .bind(chrono::Utc::now().timestamp_millis())
            .execute(&*pool)
            .await
            .unwrap();

        let future_ts = chrono::Utc::now().timestamp_millis() + 3_600_000;

        let req_a = CreateRefreshTokenRequest {
            token_hash: format!("hash_{suffix}_a"),
            user_id: user_id.clone(),
            device_id: Some("device_a".to_string()),
            access_token_id: None,
            scope: None,
            expires_at: future_ts,
            client_info: None,
            ip_address: None,
            user_agent: None,
        };
        storage.create_token(req_a).await.unwrap();

        let req_b = CreateRefreshTokenRequest {
            token_hash: format!("hash_{suffix}_b"),
            user_id: user_id.clone(),
            device_id: Some("device_b".to_string()),
            access_token_id: None,
            scope: None,
            expires_at: future_ts,
            client_info: None,
            ip_address: None,
            user_agent: None,
        };
        storage.create_token(req_b).await.unwrap();

        let count = storage.revoke_device_tokens(&user_id, "device_a", "device logout").await.unwrap();
        assert_eq!(count, 1);

        let token_a = storage.get_token(&format!("hash_{suffix}_a")).await.unwrap().unwrap();
        assert!(token_a.is_revoked);

        let token_b = storage.get_token(&format!("hash_{suffix}_b")).await.unwrap().unwrap();
        assert!(!token_b.is_revoked);
    });
}

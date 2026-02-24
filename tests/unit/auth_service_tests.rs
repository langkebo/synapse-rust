#![cfg(test)]

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use sha2::{Digest, Sha256};
use sqlx::{Pool, Postgres};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::runtime::Runtime;

use synapse_rust::auth::AuthService;
use synapse_rust::cache::{CacheConfig, CacheManager};
use synapse_rust::common::config::SecurityConfig;
use synapse_rust::common::crypto::is_legacy_hash;
use synapse_rust::common::metrics::MetricsCollector;
use synapse_rust::common::ApiError;

static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

fn unique_id() -> u64 {
    TEST_COUNTER.fetch_add(1, Ordering::SeqCst)
}

async fn setup_test_database() -> Option<Pool<Postgres>> {
    let database_url = std::env::var("TEST_DATABASE_URL")
        .or_else(|_| std::env::var("DATABASE_URL"))
        .unwrap_or_else(|_| {
            "postgresql://synapse:secret@localhost:5432/synapse_test".to_string()
        });

    let pool = match sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(std::time::Duration::from_secs(10))
        .connect(&database_url)
        .await
    {
        Ok(pool) => pool,
        Err(error) => {
            eprintln!(
                "Skipping auth service tests because test database is unavailable: {}",
                error
            );
            return None;
        }
    };

    sqlx::query("DROP TABLE IF EXISTS users CASCADE")
        .execute(&pool)
        .await
        .ok();

    sqlx::query(r#"
        CREATE TABLE users (
            user_id VARCHAR(255) PRIMARY KEY,
            username TEXT NOT NULL UNIQUE,
            password_hash TEXT,
            displayname TEXT,
            avatar_url TEXT,
            is_admin BOOLEAN DEFAULT FALSE,
            deactivated BOOLEAN DEFAULT FALSE,
            is_guest BOOLEAN DEFAULT FALSE,
            consent_version TEXT,
            appservice_id TEXT,
            user_type TEXT,
            shadow_banned BOOLEAN DEFAULT FALSE,
            generation BIGINT DEFAULT 0,
            invalid_update_ts BIGINT,
            migration_state TEXT,
            creation_ts BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW())::BIGINT,
            updated_ts BIGINT
        )
    "#)
    .execute(&pool)
    .await
    .expect("Failed to create users table");

    Some(pool)
}

#[test]
fn test_auth_service_register_invalid_username() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => Arc::new(pool),
            None => return,
        };
        let security = SecurityConfig {
            secret: "test_secret".to_string(),
            expiry_time: 3600,
            refresh_token_expiry: 604800,
            argon2_m_cost: 2048,
            argon2_t_cost: 1,
            argon2_p_cost: 1,
            allow_legacy_hashes: false,
        };
        let cache = Arc::new(CacheManager::new(CacheConfig::default()));
        let metrics = Arc::new(MetricsCollector::new());
        let auth = AuthService::new(&pool, cache, metrics, &security, "localhost");

        let id = unique_id();
        let invalid_username = format!("user@{}", id);
        let result = auth.register(&invalid_username, "password", false, None).await;
        assert!(matches!(result, Err(ApiError::BadRequest(_))));

        let result = auth.register("", "password", false, None).await;
        assert!(matches!(result, Err(ApiError::BadRequest(_))));
    });
}

#[test]
fn test_auth_service_login_invalid_credentials() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => Arc::new(pool),
            None => return,
        };
        let security = SecurityConfig {
            secret: "test_secret".to_string(),
            expiry_time: 3600,
            refresh_token_expiry: 604800,
            argon2_m_cost: 2048,
            argon2_t_cost: 1,
            argon2_p_cost: 1,
            allow_legacy_hashes: false,
        };
        let cache = Arc::new(CacheManager::new(CacheConfig::default()));
        let metrics = Arc::new(MetricsCollector::new());
        let auth = AuthService::new(&pool, cache, metrics, &security, "localhost");

        let id = unique_id();
        let nonexistent = format!("non_existent_{}", id);
        let result = auth.login(&nonexistent, "password", None, None).await;
        assert!(result.is_err());
    });
}

#[test]
fn test_password_migration_on_login() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => Arc::new(pool),
            None => return,
        };
        let security = SecurityConfig {
            secret: "test_secret".to_string(),
            expiry_time: 3600,
            refresh_token_expiry: 604800,
            argon2_m_cost: 2048,
            argon2_t_cost: 1,
            argon2_p_cost: 1,
            allow_legacy_hashes: true,
        };
        let cache = Arc::new(CacheManager::new(CacheConfig::default()));
        let metrics = Arc::new(MetricsCollector::new());
        let auth = AuthService::new(&pool, cache.clone(), metrics.clone(), &security, "localhost");

        let id = unique_id();
        let username = format!("migration_user_{}", id);
        let password = "test_password_for_migration";
        let user_id = format!("@{}:localhost", username);

        let legacy_hash = create_legacy_hash(password);

        sqlx::query(
            "INSERT INTO users (user_id, username, password_hash, is_admin, creation_ts, generation) VALUES ($1, $2, $3, $4, $5, $6)"
        )
        .bind(&user_id)
        .bind(&username)
        .bind(&legacy_hash)
        .bind(false)
        .bind(chrono::Utc::now().timestamp())
        .bind(chrono::Utc::now().timestamp() * 1000)
        .execute(&*pool)
        .await
        .expect("Failed to create user with legacy hash");

        assert!(is_legacy_hash(&legacy_hash), "Legacy hash should be detected as legacy");

        let result = auth.login(&username, password, None, None).await;
        assert!(result.is_ok(), "Login should succeed with legacy hash");

        let updated_user = sqlx::query_as::<_, (Option<String>,)>(
            "SELECT password_hash FROM users WHERE user_id = $1"
        )
        .bind(&user_id)
        .fetch_one(&*pool)
        .await
        .expect("Failed to fetch user");

        let new_hash = updated_user.0.expect("Password hash should exist");
        assert!(!is_legacy_hash(&new_hash), "Password should be migrated to Argon2");
        assert!(new_hash.starts_with("$argon2"), "New hash should be Argon2 format");

        let migration_counter = metrics.get_counter("password_migration_success_total");
        assert!(migration_counter.is_some(), "Migration counter should exist");
        assert!(migration_counter.unwrap().get() >= 1, "Migration counter should be incremented");

        let migration_hist = metrics.get_histogram("password_migration_duration_seconds");
        assert!(migration_hist.is_some(), "Migration histogram should exist");
        assert!(migration_hist.unwrap().get_count() >= 1, "Migration histogram should have observations");
    });
}

#[test]
fn test_password_migration_preserves_login_ability() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => Arc::new(pool),
            None => return,
        };
        let security = SecurityConfig {
            secret: "test_secret".to_string(),
            expiry_time: 3600,
            refresh_token_expiry: 604800,
            argon2_m_cost: 2048,
            argon2_t_cost: 1,
            argon2_p_cost: 1,
            allow_legacy_hashes: true,
        };
        let cache = Arc::new(CacheManager::new(CacheConfig::default()));
        let metrics = Arc::new(MetricsCollector::new());
        let auth = AuthService::new(&pool, cache, metrics, &security, "localhost");

        let id = unique_id();
        let username = format!("preserve_user_{}", id);
        let password = "password_to_preserve";
        let user_id = format!("@{}:localhost", username);

        let legacy_hash = create_legacy_hash(password);

        sqlx::query(
            "INSERT INTO users (user_id, username, password_hash, is_admin, creation_ts, generation) VALUES ($1, $2, $3, $4, $5, $6)"
        )
        .bind(&user_id)
        .bind(&username)
        .bind(&legacy_hash)
        .bind(false)
        .bind(chrono::Utc::now().timestamp())
        .bind(chrono::Utc::now().timestamp() * 1000)
        .execute(&*pool)
        .await
        .expect("Failed to create user with legacy hash");

        let result1 = auth.login(&username, password, None, None).await;
        assert!(result1.is_ok(), "First login should succeed");

        let result2 = auth.login(&username, password, None, None).await;
        assert!(result2.is_ok(), "Second login should succeed after migration");

        let wrong_result = auth.login(&username, "wrong_password", None, None).await;
        assert!(wrong_result.is_err(), "Login with wrong password should fail");
    });
}

#[test]
fn test_no_migration_for_argon2_hash() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => Arc::new(pool),
            None => return,
        };
        let security = SecurityConfig {
            secret: "test_secret".to_string(),
            expiry_time: 3600,
            refresh_token_expiry: 604800,
            argon2_m_cost: 2048,
            argon2_t_cost: 1,
            argon2_p_cost: 1,
            allow_legacy_hashes: true,
        };
        let cache = Arc::new(CacheManager::new(CacheConfig::default()));
        let metrics = Arc::new(MetricsCollector::new());
        let auth = AuthService::new(&pool, cache.clone(), metrics.clone(), &security, "localhost");

        let id = unique_id();
        let username = format!("argon2_user_{}", id);
        let password = "argon2_password";
        let user_id = format!("@{}:localhost", username);

        let (user, _, _, _) = auth.register(&username, password, false, None).await
            .expect("Registration should succeed");

        let initial_hash = user.password_hash.clone().expect("Password hash should exist");
        assert!(!is_legacy_hash(&initial_hash), "New user should have Argon2 hash");

        let _ = auth.login(&username, password, None, None).await
            .expect("Login should succeed");

        let updated_user = sqlx::query_as::<_, (Option<String>,)>(
            "SELECT password_hash FROM users WHERE user_id = $1"
        )
        .bind(&user_id)
        .fetch_one(&*pool)
        .await
        .expect("Failed to fetch user");

        let current_hash = updated_user.0.expect("Password hash should exist");
        assert_eq!(initial_hash, current_hash, "Hash should not change for Argon2 users");
    });
}

fn create_legacy_hash(password: &str) -> String {
    let salt = "legacysalt123456";
    let mut hasher = Sha256::new();
    hasher.update(password);
    hasher.update(salt);
    let result = hasher.finalize();
    let encoded = URL_SAFE_NO_PAD.encode(result);
    format!("sha256$v=1$m=32,p=1${}${}", salt, encoded)
}

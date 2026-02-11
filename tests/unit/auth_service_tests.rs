#[cfg(test)]
use sqlx::{Pool, Postgres};
use std::sync::Arc;
use tokio::runtime::Runtime;

use synapse_rust::auth::AuthService;
use synapse_rust::cache::{CacheConfig, CacheManager};
use synapse_rust::common::config::SecurityConfig;
use synapse_rust::common::metrics::MetricsCollector;
use synapse_rust::common::ApiError;

    async fn setup_test_database() -> Option<Pool<Postgres>> {
        let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgresql://synapse:synapse@localhost:5432/synapse_test".to_string()
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

        sqlx::query("DROP TABLE IF EXISTS access_tokens CASCADE")
            .execute(&pool)
            .await
            .ok();
        sqlx::query("DROP TABLE IF EXISTS refresh_tokens CASCADE")
            .execute(&pool)
            .await
            .ok();
        sqlx::query("DROP TABLE IF EXISTS devices CASCADE")
            .execute(&pool)
            .await
            .ok();
        sqlx::query("DROP TABLE IF EXISTS users CASCADE")
            .execute(&pool)
            .await
            .ok();

        sqlx::query(
            r#"
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
                creation_ts BIGINT NOT NULL,
                updated_ts BIGINT
            )
            "#,
        )
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
            };
            let cache = Arc::new(CacheManager::new(CacheConfig::default()));
            let metrics = Arc::new(MetricsCollector::new());
            let auth = AuthService::new(&pool, cache, metrics, &security, "localhost");

            // Test invalid characters
            let result = auth.register("user@name", "password", false, None).await;
            assert!(matches!(result, Err(ApiError::BadRequest(_))));

            // Test too short
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
            };
            let cache = Arc::new(CacheManager::new(CacheConfig::default()));
            let metrics = Arc::new(MetricsCollector::new());
            let auth = AuthService::new(&pool, cache, metrics, &security, "localhost");

            // Login with non-existent user
            let result = auth.login("non_existent", "password", None, None).await;
            assert!(matches!(result, Err(ApiError::Unauthorized(_))));
        });
    }

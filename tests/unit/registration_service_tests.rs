#![cfg(test)]

use sqlx::{Pool, Postgres};
    use std::sync::Arc;
    use synapse_rust::auth::AuthService;
    use synapse_rust::cache::{CacheConfig, CacheManager};
    use synapse_rust::common::config::SecurityConfig;
    use synapse_rust::common::metrics::MetricsCollector;
    use synapse_rust::services::registration_service::RegistrationService;
    use synapse_rust::storage::user::UserStorage;
    use tokio::runtime::Runtime;

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
                    "Skipping registration service tests because test database is unavailable: {}",
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

        sqlx::query(
            r#"
            CREATE TABLE devices (
                device_id VARCHAR(255) PRIMARY KEY,
                user_id VARCHAR(255) NOT NULL,
                display_name TEXT,
                device_key JSONB,
                last_seen_ts BIGINT,
                last_seen_ip TEXT,
                created_at BIGINT NOT NULL,
                first_seen_ts BIGINT NOT NULL,
                created_ts BIGINT,
                appservice_id TEXT,
                ignored_user_list TEXT
            )
            "#,
        )
        .execute(&pool)
        .await
        .expect("Failed to create devices table");

        sqlx::query(
            r#"
            CREATE TABLE access_tokens (
                id BIGSERIAL PRIMARY KEY,
                token VARCHAR(255) UNIQUE NOT NULL,
                user_id VARCHAR(255) NOT NULL,
                device_id VARCHAR(255),
                created_ts BIGINT NOT NULL,
                expires_ts BIGINT NOT NULL,
                invalidated_ts BIGINT
            )
            "#,
        )
        .execute(&pool)
        .await
        .expect("Failed to create access_tokens table");

        sqlx::query(
            r#"
            CREATE TABLE refresh_tokens (
                id BIGSERIAL PRIMARY KEY,
                token VARCHAR(255) UNIQUE NOT NULL,
                user_id VARCHAR(255) NOT NULL,
                device_id VARCHAR(255) NOT NULL,
                created_ts BIGINT NOT NULL,
                expires_ts BIGINT NOT NULL,
                invalidated_ts BIGINT
            )
            "#,
        )
        .execute(&pool)
        .await
        .expect("Failed to create refresh_tokens table");

        Some(pool)
    }

    #[test]
    fn test_register_user_success() {
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
            let auth_service =
                AuthService::new(&pool, cache.clone(), metrics.clone(), &security, "localhost");
            let registration_service = RegistrationService::new(
                UserStorage::new(&pool, cache.clone()),
                auth_service,
                metrics,
                "localhost".to_string(),
                true,
                None,
            );

            let result = registration_service
                .register_user("alice", "Password123!", false, Some("Alice"))
                .await;
            assert!(result.is_ok());
            let val = result.unwrap();
            assert_eq!(val["user_id"], "@alice:localhost");
            assert!(val["access_token"].is_string());
        });
    }

    #[test]
    fn test_login_success() {
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
            let auth_service =
                AuthService::new(&pool, cache.clone(), metrics.clone(), &security, "localhost");
            let registration_service = RegistrationService::new(
                UserStorage::new(&pool, cache.clone()),
                auth_service,
                metrics,
                "localhost".to_string(),
                true,
                None,
            );

            registration_service
                .register_user("alice", "Password123!", false, None)
                .await
                .unwrap();

            let result = registration_service
                .login("alice", "Password123!", None, None)
                .await;
            assert!(result.is_ok());
            let val = result.unwrap();
            assert_eq!(val["user_id"], "@alice:localhost");
        });
    }

    #[test]
    fn test_get_profile_success() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let pool = match setup_test_database().await {
                Some(pool) => Arc::new(pool),
                None => return,
            };
            let cache = Arc::new(CacheManager::new(CacheConfig::default()));
            let user_storage = UserStorage::new(&pool, cache.clone());
            user_storage
                .create_user("@alice:localhost", "alice", None, false)
                .await
                .unwrap();
            user_storage
                .update_displayname("@alice:localhost", Some("Alice"))
                .await
                .unwrap();

            let security = SecurityConfig {
                secret: "test_secret".to_string(),
                expiry_time: 3600,
                refresh_token_expiry: 604800,
                argon2_m_cost: 2048,
                argon2_t_cost: 1,
                argon2_p_cost: 1,
            };
            let metrics = Arc::new(MetricsCollector::new());
            let auth_service =
                AuthService::new(&pool, cache.clone(), metrics.clone(), &security, "localhost");
            let registration_service = RegistrationService::new(
                user_storage,
                auth_service,
                metrics,
                "localhost".to_string(),
                true,
                None,
            );

            let result = registration_service.get_profile("@alice:localhost").await;
            assert!(result.is_ok());
            let val = result.unwrap();
            assert_eq!(val["displayname"], "Alice");
        });
    }

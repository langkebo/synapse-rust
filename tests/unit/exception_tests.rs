#![cfg(test)]

use std::sync::Arc;
    use synapse_rust::auth::AuthService;
    use synapse_rust::cache::{CacheConfig, CacheManager};
    use synapse_rust::common::config::SecurityConfig;
    use synapse_rust::common::metrics::MetricsCollector;
    use synapse_rust::common::ApiError;
    use tokio::runtime::Runtime;

    #[test]
    fn test_invalid_jwt() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
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
                Ok(pool) => Arc::new(pool),
                Err(error) => {
                    eprintln!(
                        "Skipping exception tests because test database is unavailable: {}",
                        error
                    );
                    return;
                }
            };
            let cache = Arc::new(CacheManager::new(CacheConfig::default()));
            let metrics = Arc::new(MetricsCollector::new());
            let security_config = SecurityConfig {
                secret: "test_secret".to_string(),
                expiry_time: 3600,
                refresh_token_expiry: 604800,
                argon2_m_cost: 2048,
                argon2_t_cost: 1,
                argon2_p_cost: 1,
            };

            let auth_service =
                AuthService::new(&pool, cache, metrics, &security_config, "localhost");

            let result = auth_service.validate_token("invalid_token").await;
            assert!(result.is_err());
            match result {
                Err(ApiError::Unauthorized(msg)) => assert!(msg.contains("Invalid token")),
                _ => panic!("Expected Unauthorized error"),
            }
        });
    }

    #[test]
    fn test_database_connection_failure() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let result =
                sqlx::PgPool::connect("postgres://synapse:secret@localhost:5433/synapse_test")
                    .await;
            assert!(result.is_err());
        });
    }

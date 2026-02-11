#![cfg(test)]

use sqlx::{Pool, Postgres};
    use std::sync::Arc;
    use tokio::runtime::Runtime;

    use synapse_rust::storage::user::UserStorage;
    use synapse_rust::cache::{CacheConfig, CacheManager};

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
                    "Skipping storage tests because test database is unavailable: {}",
                    error
                );
                return None;
            }
        };

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
    fn test_create_user_success() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let pool = match setup_test_database().await {
                Some(pool) => pool,
                None => return,
            };
            let cache = Arc::new(CacheManager::new(CacheConfig::default()));
            let storage = UserStorage::new(&Arc::new(pool), cache);

            let user = storage
                .create_user("@alice:localhost", "alice", Some("hash"), false)
                .await
                .unwrap();
            assert_eq!(user.user_id, "@alice:localhost");
            assert_eq!(user.username, "alice");
            assert_eq!(user.password_hash, Some("hash".to_string()));
            assert!(!user.is_admin.unwrap_or(true));
        });
    }

    #[test]
    fn test_get_user_by_id() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let pool = match setup_test_database().await {
                Some(pool) => pool,
                None => return,
            };
            let cache = Arc::new(CacheManager::new(CacheConfig::default()));
            let storage = UserStorage::new(&Arc::new(pool), cache);

            storage
                .create_user("@alice:localhost", "alice", None, false)
                .await
                .unwrap();

            let user = storage.get_user_by_id("@alice:localhost").await.unwrap();
            assert!(user.is_some());
            assert_eq!(user.unwrap().username, "alice");

            let user = storage
                .get_user_by_id("@nonexistent:localhost")
                .await
                .unwrap();
            assert!(user.is_none());
        });
    }

    #[test]
    fn test_user_exists() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let pool = match setup_test_database().await {
                Some(pool) => pool,
                None => return,
            };
            let cache = Arc::new(CacheManager::new(CacheConfig::default()));
            let storage = UserStorage::new(&Arc::new(pool), cache);

            storage
                .create_user("@alice:localhost", "alice", None, false)
                .await
                .unwrap();

            assert!(storage.user_exists("@alice:localhost").await.unwrap());
            assert!(!storage.user_exists("@bob:localhost").await.unwrap());
        });
    }

    #[test]
    fn test_update_displayname() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let pool = match setup_test_database().await {
                Some(pool) => pool,
                None => return,
            };
            let cache = Arc::new(CacheManager::new(CacheConfig::default()));
            let storage = UserStorage::new(&Arc::new(pool), cache);

            storage
                .create_user("@alice:localhost", "alice", None, false)
                .await
                .unwrap();
            storage
                .update_displayname("@alice:localhost", Some("Alice"))
                .await
                .unwrap();

            let user = storage
                .get_user_by_id("@alice:localhost")
                .await
                .unwrap()
                .unwrap();
            assert_eq!(user.displayname, Some("Alice".to_string()));
        });
    }

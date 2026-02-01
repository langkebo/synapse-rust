#[cfg(test)]
mod private_chat_service_tests {
    use sqlx::{Pool, Postgres, Row};
    use std::sync::Arc;
    use tokio::runtime::Runtime;

    use synapse_rust::services::private_chat_service::{PrivateChatService, PrivateChatStorage};
    use synapse_rust::services::ServiceContainer;

    async fn setup_test_database() -> Pool<Postgres> {
        let database_url = std::env::var("TEST_DATABASE_URL").unwrap_or_else(|_| {
            "postgresql://synapse:synapse@localhost:5432/synapse_test".to_string()
        });

        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(5)
            .connect(&database_url)
            .await
            .expect("Failed to connect to test database");

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS users (
                user_id TEXT PRIMARY KEY,
                username TEXT NOT NULL UNIQUE,
                password_hash TEXT,
                creation_ts BIGINT NOT NULL
            )
            "#,
        )
        .execute(&pool)
        .await
        .expect("Failed to create users table");

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS private_sessions (
                id VARCHAR(255) PRIMARY KEY,
                user_id TEXT NOT NULL,
                other_user_id TEXT NOT NULL,
                session_type VARCHAR(50) DEFAULT 'direct',
                created_ts BIGINT NOT NULL,
                last_activity_ts BIGINT NOT NULL,
                updated_ts BIGINT,
                unread_count INT DEFAULT 0
            )
            "#,
        )
        .execute(&pool)
        .await
        .expect("Failed to create private_sessions table");

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS private_messages (
                id BIGSERIAL PRIMARY KEY,
                session_id VARCHAR(255) NOT NULL,
                sender_id TEXT NOT NULL,
                message_type VARCHAR(50) DEFAULT 'text',
                content TEXT,
                encrypted_content TEXT,
                read_by_receiver BOOLEAN DEFAULT FALSE,
                created_ts BIGINT NOT NULL
            )
            "#,
        )
        .execute(&pool)
        .await
        .expect("Failed to create private_messages table");

        pool
    }

    async fn cleanup_test_database(pool: &Pool<Postgres>) {
        sqlx::query("DROP TABLE IF EXISTS private_messages CASCADE")
            .execute(pool)
            .await
            .ok();

        sqlx::query("DROP TABLE IF EXISTS private_sessions CASCADE")
            .execute(pool)
            .await
            .ok();

        sqlx::query("DROP TABLE IF EXISTS users CASCADE")
            .execute(pool)
            .await
            .ok();
    }

    async fn create_test_user(pool: &Pool<Postgres>, user_id: &str, username: &str) {
        sqlx::query(
            r#"
            INSERT INTO users (user_id, username, creation_ts)
            VALUES ($1, $2, $3)
            "#,
        )
        .bind(user_id)
        .bind(username)
        .bind(chrono::Utc::now().timestamp())
        .execute(pool)
        .await
        .expect("Failed to create test user");
    }

    async fn create_test_session(
        pool: &Pool<Postgres>,
        session_id: &str,
        user_id: &str,
        other_user_id: &str,
    ) {
        let now = chrono::Utc::now().timestamp();
        sqlx::query(
            r#"
            INSERT INTO private_sessions (id, user_id, other_user_id, created_ts, last_activity_ts, updated_ts)
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(session_id)
        .bind(user_id)
        .bind(other_user_id)
        .bind(now)
        .bind(now)
        .bind(now)
        .execute(pool)
        .await
        .expect("Failed to create test session");
    }

    async fn create_test_message(
        pool: &Pool<Postgres>,
        session_id: &str,
        sender_id: &str,
        content: &str,
    ) -> i64 {
        let result = sqlx::query(
            r#"
            INSERT INTO private_messages (session_id, sender_id, content, created_ts)
            VALUES ($1, $2, $3, $4)
            RETURNING id
            "#,
        )
        .bind(session_id)
        .bind(sender_id)
        .bind(content)
        .bind(chrono::Utc::now().timestamp())
        .fetch_one(pool)
        .await
        .expect("Failed to create test message");

        result
            .try_get::<i64, _>("id")
            .expect("Failed to get message id")
    }

    #[test]
    fn test_delete_message_success() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let pool = setup_test_database().await;

            create_test_user(&pool, "@alice:example.com", "alice").await;
            create_test_user(&pool, "@bob:example.com", "bob").await;
            create_test_session(&pool, "session_1", "@alice:example.com", "@bob:example.com").await;

            let message_id =
                create_test_message(&pool, "session_1", "@alice:example.com", "Hello Bob!").await;

            let chat_storage = PrivateChatStorage::new(&Arc::new(pool.clone()));
            let result = chat_storage.delete_message(message_id).await;

            assert!(result.is_ok(), "Failed to delete message");

            let message_exists = sqlx::query_as::<_, (bool,)>(
                "SELECT EXISTS(SELECT 1 FROM private_messages WHERE id = $1)",
            )
            .bind(message_id)
            .fetch_one(&pool)
            .await
            .expect("Failed to check message existence");

            assert!(!message_exists.0, "Message still exists after deletion");

            cleanup_test_database(&pool).await;
        });
    }

    #[test]
    fn test_delete_message_nonexistent() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let pool = setup_test_database().await;

            create_test_user(&pool, "@alice:example.com", "alice").await;

            let chat_storage = PrivateChatStorage::new(&Arc::new(pool.clone()));
            let result = chat_storage.delete_message(999999).await;

            assert!(
                result.is_ok(),
                "Deleting non-existent message should succeed"
            );

            cleanup_test_database(&pool).await;
        });
    }

    #[test]
    fn test_delete_message_service_authorization_success() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let pool = setup_test_database().await;

            create_test_user(&pool, "@alice:example.com", "alice").await;
            create_test_user(&pool, "@bob:example.com", "bob").await;
            create_test_session(&pool, "session_1", "@alice:example.com", "@bob:example.com").await;

            let message_id =
                create_test_message(&pool, "session_1", "@alice:example.com", "Hello Bob!").await;

            let chat_storage = PrivateChatStorage::new(&Arc::new(pool.clone()));

            let services = ServiceContainer::new(
                &Arc::new(pool.clone()),
                Arc::new(synapse_rust::cache::CacheManager::new(
                    synapse_rust::cache::CacheConfig::default(),
                )),
                "test_secret",
                "example.com",
            );

            let chat_service = PrivateChatService::new(&services, &Arc::new(pool.clone()));

            let result = chat_service
                .delete_message("@alice:example.com", &message_id.to_string())
                .await;

            assert!(
                result.is_ok(),
                "User should be able to delete their own message"
            );

            let message_exists = sqlx::query_as::<_, (bool,)>(
                "SELECT EXISTS(SELECT 1 FROM private_messages WHERE id = $1)",
            )
            .bind(message_id)
            .fetch_one(&pool)
            .await
            .expect("Failed to check message existence");

            assert!(!message_exists.0, "Message should be deleted");

            cleanup_test_database(&pool).await;
        });
    }

    #[test]
    fn test_delete_message_service_authorization_failure() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let pool = setup_test_database().await;

            create_test_user(&pool, "@alice:example.com", "alice").await;
            create_test_user(&pool, "@bob:example.com", "bob").await;
            create_test_session(&pool, "session_1", "@alice:example.com", "@bob:example.com").await;

            let message_id =
                create_test_message(&pool, "session_1", "@alice:example.com", "Hello Bob!").await;

            let services = ServiceContainer::new(
                &Arc::new(pool.clone()),
                Arc::new(synapse_rust::cache::CacheManager::new(
                    synapse_rust::cache::CacheConfig::default(),
                )),
                "test_secret",
                "example.com",
            );

            let chat_service = PrivateChatService::new(&services, &Arc::new(pool.clone()));

            let result = chat_service
                .delete_message("@bob:example.com", &message_id.to_string())
                .await;

            assert!(
                result.is_err(),
                "User should not be able to delete others' message"
            );

            match result {
                Err(e) => {
                    assert_eq!(e.status_code(), 403, "Should return forbidden status");
                }
                _ => panic!("Expected error"),
            }

            let message_exists = sqlx::query_as::<_, (bool,)>(
                "SELECT EXISTS(SELECT 1 FROM private_messages WHERE id = $1)",
            )
            .bind(message_id)
            .fetch_one(&pool)
            .await
            .expect("Failed to check message existence");

            assert!(message_exists.0, "Message should still exist");

            cleanup_test_database(&pool).await;
        });
    }

    #[test]
    fn test_delete_message_invalid_id() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let pool = setup_test_database().await;

            create_test_user(&pool, "@alice:example.com", "alice").await;

            let services = ServiceContainer::new(
                &Arc::new(pool.clone()),
                Arc::new(synapse_rust::cache::CacheManager::new(
                    synapse_rust::cache::CacheConfig::default(),
                )),
                "test_secret",
                "example.com",
            );

            let chat_service = PrivateChatService::new(&services, &Arc::new(pool.clone()));

            let result = chat_service
                .delete_message("@alice:example.com", "invalid_id")
                .await;

            assert!(
                result.is_err(),
                "Should return error for invalid message ID"
            );

            match result {
                Err(e) => {
                    assert_eq!(e.status_code(), 400, "Should return bad request status");
                }
                _ => panic!("Expected error"),
            }

            cleanup_test_database(&pool).await;
        });
    }

    #[test]
    fn test_delete_message_not_found() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let pool = setup_test_database().await;

            create_test_user(&pool, "@alice:example.com", "alice").await;

            let services = ServiceContainer::new(
                &Arc::new(pool.clone()),
                Arc::new(synapse_rust::cache::CacheManager::new(
                    synapse_rust::cache::CacheConfig::default(),
                )),
                "test_secret",
                "example.com",
            );

            let chat_service = PrivateChatService::new(&services, &Arc::new(pool.clone()));

            let result = chat_service
                .delete_message("@alice:example.com", "999999")
                .await;

            assert!(
                result.is_err(),
                "Should return error for non-existent message"
            );

            match result {
                Err(e) => {
                    assert_eq!(e.status_code(), 404, "Should return not found status");
                }
                _ => panic!("Expected error"),
            }

            cleanup_test_database(&pool).await;
        });
    }

    #[test]
    fn test_delete_multiple_messages() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let pool = setup_test_database().await;

            create_test_user(&pool, "@alice:example.com", "alice").await;
            create_test_user(&pool, "@bob:example.com", "bob").await;
            create_test_session(&pool, "session_1", "@alice:example.com", "@bob:example.com").await;

            let message_id_1 =
                create_test_message(&pool, "session_1", "@alice:example.com", "Message 1").await;

            let message_id_2 =
                create_test_message(&pool, "session_1", "@alice:example.com", "Message 2").await;

            let message_id_3 =
                create_test_message(&pool, "session_1", "@alice:example.com", "Message 3").await;

            let chat_storage = PrivateChatStorage::new(&Arc::new(pool.clone()));

            chat_storage.delete_message(message_id_1).await.unwrap();
            chat_storage.delete_message(message_id_2).await.unwrap();
            chat_storage.delete_message(message_id_3).await.unwrap();

            let message_count: (i64,) =
                sqlx::query_as("SELECT COUNT(*) FROM private_messages WHERE session_id = $1")
                    .bind("session_1")
                    .fetch_one(&pool)
                    .await
                    .expect("Failed to count messages");

            assert_eq!(message_count.0, 0, "All messages should be deleted");

            cleanup_test_database(&pool).await;
        });
    }
}

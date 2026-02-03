#[cfg(test)]
mod friend_service_tests {
    use sqlx::{Pool, Postgres};
    use std::sync::Arc;
    use tokio::runtime::Runtime;

    use synapse_rust::common::ApiError;
    use synapse_rust::services::friend_service::{FriendService, FriendStorage};
    use synapse_rust::services::ServiceContainer;

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
                    "Skipping friend service tests because test database is unavailable: {}",
                    error
                );
                return None;
            }
        };

        // Create required tables
        sqlx::query("DROP TABLE IF EXISTS friends CASCADE")
            .execute(&pool)
            .await
            .ok();
        sqlx::query("DROP TABLE IF EXISTS friend_requests CASCADE")
            .execute(&pool)
            .await
            .ok();
        sqlx::query("DROP TABLE IF EXISTS friend_categories CASCADE")
            .execute(&pool)
            .await
            .ok();
        sqlx::query("DROP TABLE IF EXISTS blocked_users CASCADE")
            .execute(&pool)
            .await
            .ok();
        sqlx::query("DROP TABLE IF EXISTS room_memberships CASCADE")
            .execute(&pool)
            .await
            .ok();
        sqlx::query("DROP TABLE IF EXISTS private_sessions CASCADE")
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
                creation_ts BIGINT NOT NULL
            )
            "#,
        )
        .execute(&pool)
        .await
        .expect("Failed to create users table");

        sqlx::query(
            r#"
            CREATE TABLE room_memberships (
                room_id VARCHAR(255) NOT NULL,
                user_id VARCHAR(255) NOT NULL,
                membership VARCHAR(50) NOT NULL,
                created_ts BIGINT NOT NULL,
                PRIMARY KEY (room_id, user_id)
            )
            "#,
        )
        .execute(&pool)
        .await
        .expect("Failed to create room_memberships table");

        sqlx::query(
            r#"
            CREATE TABLE private_sessions (
                id VARCHAR(255) PRIMARY KEY,
                user_id_1 VARCHAR(255) NOT NULL,
                user_id_2 VARCHAR(255) NOT NULL,
                created_ts BIGINT NOT NULL
            )
            "#,
        )
        .execute(&pool)
        .await
        .expect("Failed to create private_sessions table");

        let storage = FriendStorage::new(&Arc::new(pool.clone()));
        storage
            .create_tables()
            .await
            .expect("Failed to create friend tables");

        Some(pool)
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

    #[test]
    fn test_send_friend_request_success() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let pool = match setup_test_database().await {
                Some(pool) => pool,
                None => return,
            };
            create_test_user(&pool, "@alice:localhost", "alice").await;
            create_test_user(&pool, "@bob:localhost", "bob").await;

            let services = ServiceContainer::new_test();
            let friend_service = FriendService::new(&services, &Arc::new(pool.clone()));

            let result = friend_service
                .send_friend_request("@alice:localhost", "@bob:localhost", Some("Hello!"))
                .await;
            assert!(result.is_ok());
            let val = result.unwrap();
            assert_eq!(val["status"], "pending");
        });
    }

    #[test]
    fn test_send_friend_request_user_not_found() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let pool = match setup_test_database().await {
                Some(pool) => pool,
                None => return,
            };
            create_test_user(&pool, "@alice:localhost", "alice").await;

            let services = ServiceContainer::new_test();
            let friend_service = FriendService::new(&services, &Arc::new(pool.clone()));

            let result = friend_service
                .send_friend_request("@alice:localhost", "@nonexistent:localhost", None)
                .await;
            assert!(result.is_err());
            match result {
                Err(ApiError::NotFound(msg)) => assert_eq!(msg, "User not found"),
                _ => panic!("Expected NotFound error"),
            }
        });
    }

    #[test]
    fn test_send_friend_request_already_friends() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let pool = match setup_test_database().await {
                Some(pool) => pool,
                None => return,
            };
            create_test_user(&pool, "@alice:localhost", "alice").await;
            create_test_user(&pool, "@bob:localhost", "bob").await;

            let storage = FriendStorage::new(&Arc::new(pool.clone()));
            storage
                .add_friend("@alice:localhost", "@bob:localhost")
                .await
                .unwrap();

            let services = ServiceContainer::new_test();
            let friend_service = FriendService::new(&services, &Arc::new(pool.clone()));

            let result = friend_service
                .send_friend_request("@alice:localhost", "@bob:localhost", None)
                .await;
            assert!(result.is_err());
            match result {
                Err(ApiError::BadRequest(msg)) => assert_eq!(msg, "Already friends"),
                _ => panic!("Expected BadRequest error"),
            }
        });
    }

    #[test]
    fn test_block_user_success() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let pool = match setup_test_database().await {
                Some(pool) => pool,
                None => return,
            };
            create_test_user(&pool, "@alice:localhost", "alice").await;
            create_test_user(&pool, "@bob:localhost", "bob").await;

            let services = ServiceContainer::new_test();
            let friend_service = FriendService::new(&services, &Arc::new(pool.clone()));

            let result = friend_service
                .block_user("@alice:localhost", "@bob:localhost", Some("Spam"))
                .await;
            assert!(result.is_ok());

            let blocked = friend_service
                .get_blocked_users("@alice:localhost")
                .await
                .unwrap();
            assert_eq!(blocked["count"], 1);
            assert_eq!(blocked["blocked"][0], "@bob:localhost");
        });
    }

    #[test]
    fn test_accept_friend_request_success() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let pool = match setup_test_database().await {
                Some(pool) => pool,
                None => return,
            };
            create_test_user(&pool, "@alice:localhost", "alice").await;
            create_test_user(&pool, "@bob:localhost", "bob").await;

            let storage = FriendStorage::new(&Arc::new(pool.clone()));
            let request_id = storage
                .create_request("@bob:localhost", "@alice:localhost", None)
                .await
                .unwrap();

            let services = ServiceContainer::new_test();
            let friend_service = FriendService::new(&services, &Arc::new(pool.clone()));

            let result = friend_service
                .accept_request("@alice:localhost", request_id)
                .await;
            assert!(result.is_ok());

            let friends = friend_service
                .get_friends("@alice:localhost")
                .await
                .unwrap();
            assert_eq!(friends["count"], 1);
            assert_eq!(friends["friends"][0]["user_id"], "@bob:localhost");
        });
    }

    #[test]
    fn test_send_friend_request_blocked() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let pool = match setup_test_database().await {
                Some(pool) => pool,
                None => return,
            };
            create_test_user(&pool, "@alice:localhost", "alice").await;
            create_test_user(&pool, "@bob:localhost", "bob").await;

            let storage = FriendStorage::new(&Arc::new(pool.clone()));
            storage
                .block_user("@bob:localhost", "@alice:localhost", None)
                .await
                .unwrap();

            let services = ServiceContainer::new_test();
            let friend_service = FriendService::new(&services, &Arc::new(pool.clone()));

            let result = friend_service
                .send_friend_request("@alice:localhost", "@bob:localhost", None)
                .await;
            assert!(result.is_err());
            match result {
                Err(ApiError::Forbidden(msg)) => {
                    assert_eq!(msg, "Cannot send request to this user")
                }
                _ => panic!("Expected Forbidden error"),
            }
        });
    }

    #[test]
    fn test_get_recommendations() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let pool = match setup_test_database().await {
                Some(pool) => pool,
                None => return,
            };
            create_test_user(&pool, "@alice:localhost", "alice").await;
            create_test_user(&pool, "@bob:localhost", "bob").await;
            create_test_user(&pool, "@charlie:localhost", "charlie").await;

            // Alice and Bob in same room
            sqlx::query("INSERT INTO room_memberships (room_id, user_id, membership, created_ts) VALUES ($1, $2, $3, $4)")
                .bind("room1").bind("@alice:localhost").bind("join").bind(1000i64).execute(&pool).await.unwrap();
            sqlx::query("INSERT INTO room_memberships (room_id, user_id, membership, created_ts) VALUES ($1, $2, $3, $4)")
                .bind("room1").bind("@bob:localhost").bind("join").bind(1001i64).execute(&pool).await.unwrap();

            // Alice and Charlie have a private session
            sqlx::query("INSERT INTO private_sessions (id, user_id_1, user_id_2, created_ts) VALUES ($1, $2, $3, $4)")
                .bind("ps1").bind("@alice:localhost").bind("@charlie:localhost").bind(2000i64).execute(&pool).await.unwrap();

            let storage = FriendStorage::new(&Arc::new(pool.clone()));
            let recommendations = storage.get_recommendations("@alice:localhost", 10).await.unwrap();

            assert_eq!(recommendations.len(), 2);
            assert!(recommendations.contains(&"@bob:localhost".to_string()));
            assert!(recommendations.contains(&"@charlie:localhost".to_string()));
        });
    }
}

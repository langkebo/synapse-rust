#![cfg(test)]

use serde_json::json;
    use sqlx::{Pool, Postgres};
    use std::sync::Arc;
    use tokio::runtime::Runtime;

    use synapse_rust::common::validation::Validator;
    use synapse_rust::services::room_service::{CreateRoomConfig, RoomService};
    use synapse_rust::storage::event::EventStorage;
    use synapse_rust::storage::membership::RoomMemberStorage;
    use synapse_rust::storage::room::RoomStorage;
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
                    "Skipping room service tests because test database is unavailable: {}",
                    error
                );
                return None;
            }
        };

        sqlx::query("DROP TABLE IF EXISTS events CASCADE")
            .execute(&pool)
            .await
            .ok();
        sqlx::query("DROP TABLE IF EXISTS room_memberships CASCADE")
            .execute(&pool)
            .await
            .ok();
        sqlx::query("DROP TABLE IF EXISTS rooms CASCADE")
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
            CREATE TABLE rooms (
                room_id VARCHAR(255) PRIMARY KEY,
                name TEXT,
                topic TEXT,
                avatar_url TEXT,
                canonical_alias TEXT,
                join_rule TEXT,
                creator TEXT NOT NULL,
                version TEXT,
                encryption TEXT,
                is_public BOOLEAN DEFAULT FALSE,
                member_count BIGINT DEFAULT 0,
                history_visibility TEXT,
                visibility TEXT,
                creation_ts BIGINT NOT NULL,
                last_activity_ts BIGINT NOT NULL
            )
            "#,
        )
        .execute(&pool)
        .await
        .expect("Failed to create rooms table");

        sqlx::query(
            r#"
            CREATE TABLE room_memberships (
                room_id VARCHAR(255) NOT NULL,
                user_id VARCHAR(255) NOT NULL,
                sender TEXT,
                membership TEXT NOT NULL,
                event_id TEXT,
                event_type TEXT,
                display_name TEXT,
                avatar_url TEXT,
                is_banned BOOLEAN DEFAULT FALSE,
                invite_token TEXT,
                updated_ts BIGINT,
                joined_ts BIGINT,
                left_ts BIGINT,
                reason TEXT,
                banned_by TEXT,
                ban_reason TEXT,
                ban_ts BIGINT,
                join_reason TEXT,
                PRIMARY KEY (room_id, user_id)
            )
            "#,
        )
        .execute(&pool)
        .await
        .expect("Failed to create room_memberships table");

        sqlx::query(
            r#"
            CREATE TABLE events (
                event_id VARCHAR(255) PRIMARY KEY,
                room_id VARCHAR(255) NOT NULL,
                user_id VARCHAR(255) NOT NULL,
                sender VARCHAR(255) NOT NULL,
                event_type TEXT NOT NULL,
                content JSONB NOT NULL,
                state_key TEXT,
                depth BIGINT,
                origin_server_ts BIGINT NOT NULL,
                processed_ts BIGINT NOT NULL,
                not_before BIGINT,
                status TEXT,
                reference_image TEXT,
                origin TEXT,
                unsigned JSONB
            )
            "#,
        )
        .execute(&pool)
        .await
        .expect("Failed to create events table");

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
    fn test_room_service_creation() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let pool = match setup_test_database().await {
                Some(pool) => Arc::new(pool),
                None => return,
            };
            
            let cache = Arc::new(CacheManager::new(CacheConfig::default()));
            let room_service = RoomService::new(
                RoomStorage::new(&pool),
                RoomMemberStorage::new(&pool, "localhost"),
                EventStorage::new(&pool),
                UserStorage::new(&pool, cache.clone()),
                Arc::new(Validator::default()),
                "localhost".to_string(),
                None,
            );
            
            // Just verify we can create the service and it has the right server name
            assert_eq!(room_service.server_name, "localhost");
        });
    }

    #[test]
    fn test_create_room_success() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let pool = match setup_test_database().await {
                Some(pool) => Arc::new(pool),
                None => return,
            };
            create_test_user(&pool, "@alice:localhost", "alice").await;

            let cache = Arc::new(CacheManager::new(CacheConfig::default()));
            let room_service = RoomService::new(
                RoomStorage::new(&pool),
                RoomMemberStorage::new(&pool, "localhost"),
                EventStorage::new(&pool),
                UserStorage::new(&pool, cache.clone()),
                Arc::new(Validator::default()),
                "localhost".to_string(),
                None,
            );

            let config = CreateRoomConfig {
                name: Some("Test Room".to_string()),
                topic: Some("Test Topic".to_string()),
                visibility: Some("public".to_string()),
                ..Default::default()
            };

            let result = room_service.create_room("@alice:localhost", config).await;
            assert!(result.is_ok());
            let val = result.unwrap();
            assert!(val["room_id"].as_str().unwrap().starts_with('!'));

            let room_id = val["room_id"].as_str().unwrap();
            let room = room_service.get_room(room_id).await.unwrap();
            assert_eq!(room["name"], "Test Room");
            assert_eq!(room["topic"], "Test Topic");
            assert_eq!(room["is_public"], true);
            assert_eq!(room["creator"], "@alice:localhost");
        });
    }

    #[test]
    fn test_join_room_success() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let pool = match setup_test_database().await {
                Some(pool) => Arc::new(pool),
                None => return,
            };
            create_test_user(&pool, "@alice:localhost", "alice").await;
            create_test_user(&pool, "@bob:localhost", "bob").await;

            let cache = Arc::new(CacheManager::new(CacheConfig::default()));
            let room_service = RoomService::new(
                RoomStorage::new(&pool),
                RoomMemberStorage::new(&pool, "localhost"),
                EventStorage::new(&pool),
                UserStorage::new(&pool, cache.clone()),
                Arc::new(Validator::default()),
                "localhost".to_string(),
                None,
            );

            let config = CreateRoomConfig::default();
            let room_val = room_service
                .create_room("@alice:localhost", config)
                .await
                .unwrap();
            let room_id = room_val["room_id"].as_str().unwrap();

            let result = room_service.join_room(room_id, "@bob:localhost").await;
            assert!(result.is_ok());

            let members = room_service
                .get_room_members(room_id, "@alice:localhost")
                .await
                .unwrap();
            let chunk = members["chunk"].as_array().unwrap();
            assert!(chunk.iter().any(|m| m["user_id"] == "@bob:localhost"));
        });
    }

    #[test]
    fn test_send_message_success() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let pool = match setup_test_database().await {
                Some(pool) => Arc::new(pool),
                None => return,
            };
            create_test_user(&pool, "@alice:localhost", "alice").await;

            let cache = Arc::new(CacheManager::new(CacheConfig::default()));
            let room_service = RoomService::new(
                RoomStorage::new(&pool),
                RoomMemberStorage::new(&pool, "localhost"),
                EventStorage::new(&pool),
                UserStorage::new(&pool, cache.clone()),
                Arc::new(Validator::default()),
                "localhost".to_string(),
                None,
            );

            let config = CreateRoomConfig::default();
            let room_val = room_service
                .create_room("@alice:localhost", config)
                .await
                .unwrap();
            let room_id = room_val["room_id"].as_str().unwrap();

            let content = json!({"body": "Hello world"});
            let result = room_service
                .send_message(room_id, "@alice:localhost", "m.text", &content)
                .await;
            assert!(result.is_ok());
            let val = result.unwrap();
            assert!(val["event_id"].as_str().unwrap().starts_with('$'));

            let messages = room_service
                .get_room_messages(room_id, 0, 10, "b")
                .await
                .unwrap();
            let chunk = messages["chunk"].as_array().unwrap();
            assert_eq!(chunk.len(), 1);
            assert_eq!(chunk[0]["content"]["body"]["body"], "Hello world");
            assert_eq!(chunk[0]["sender"], "@alice:localhost");
        });
    }

    #[test]
    fn test_invite_user_success() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let pool = match setup_test_database().await {
                Some(pool) => Arc::new(pool),
                None => return,
            };
            create_test_user(&pool, "@alice:localhost", "alice").await;
            create_test_user(&pool, "@bob:localhost", "bob").await;

            let cache = Arc::new(CacheManager::new(CacheConfig::default()));
            let room_service = RoomService::new(
                RoomStorage::new(&pool),
                RoomMemberStorage::new(&pool, "localhost"),
                EventStorage::new(&pool),
                UserStorage::new(&pool, cache.clone()),
                Arc::new(Validator::default()),
                "localhost".to_string(),
                None,
            );

            let config = CreateRoomConfig::default();
            let room_val = room_service
                .create_room("@alice:localhost", config)
                .await
                .unwrap();
            let room_id = room_val["room_id"].as_str().unwrap();

            let result = room_service
                .invite_user(room_id, "@alice:localhost", "@bob:localhost")
                .await;
            assert!(result.is_ok());

            let member_storage = RoomMemberStorage::new(&pool, "localhost");
            let member = member_storage
                .get_member(room_id, "@bob:localhost")
                .await
                .unwrap()
                .unwrap();
            assert_eq!(member.membership, "invite");
        });
    }

    #[test]
    fn test_ban_user_success() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let pool = match setup_test_database().await {
                Some(pool) => Arc::new(pool),
                None => return,
            };
            create_test_user(&pool, "@alice:localhost", "alice").await;
            create_test_user(&pool, "@bob:localhost", "bob").await;

            let cache = Arc::new(CacheManager::new(CacheConfig::default()));
            let room_service = RoomService::new(
                RoomStorage::new(&pool),
                RoomMemberStorage::new(&pool, "localhost"),
                EventStorage::new(&pool),
                UserStorage::new(&pool, cache.clone()),
                Arc::new(Validator::default()),
                "localhost".to_string(),
                None,
            );

            let config = CreateRoomConfig::default();
            let room_val = room_service
                .create_room("@alice:localhost", config)
                .await
                .unwrap();
            let room_id = room_val["room_id"].as_str().unwrap();

            let result = room_service
                .ban_user(room_id, "@bob:localhost", "@alice:localhost", Some("Spam"))
                .await;
            assert!(result.is_ok());

            let member_storage = RoomMemberStorage::new(&pool, "localhost");
            let member = member_storage
                .get_member(room_id, "@bob:localhost")
                .await
                .unwrap()
                .unwrap();
            assert_eq!(member.membership, "ban");
            assert_eq!(member.banned_by, Some("@alice:localhost".to_string()));
        });
    }

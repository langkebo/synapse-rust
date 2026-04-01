#![cfg(test)]

use serde_json::json;
use sqlx::{Pool, Postgres};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::runtime::Runtime;

use synapse_rust::common::validation::Validator;
use synapse_rust::services::room_service::{CreateRoomConfig, RoomService};
use synapse_rust::services::room_summary_service::RoomSummaryService;
use synapse_rust::storage::event::EventStorage;
use synapse_rust::storage::membership::RoomMemberStorage;
use synapse_rust::storage::room::RoomStorage;
use synapse_rust::storage::room_summary::RoomSummaryStorage;
use synapse_rust::storage::user::UserStorage;
use synapse_rust::cache::{CacheConfig, CacheManager};

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

    sqlx::query(r#"
        CREATE TABLE users (
            user_id VARCHAR(255) PRIMARY KEY,
            username TEXT NOT NULL UNIQUE,
            password_hash TEXT,
            displayname TEXT,
            avatar_url TEXT,
            is_admin BOOLEAN DEFAULT FALSE,
            is_guest BOOLEAN DEFAULT FALSE,
            is_shadow_banned BOOLEAN DEFAULT FALSE,
            is_deactivated BOOLEAN DEFAULT FALSE,
            created_ts BIGINT NOT NULL,
            updated_ts BIGINT,
            generation BIGINT DEFAULT 0
        )
    "#)
    .execute(&pool)
    .await
    .expect("Failed to create users table");

    sqlx::query(r#"
        CREATE TABLE rooms (
            room_id VARCHAR(255) PRIMARY KEY,
            is_public BOOLEAN DEFAULT FALSE,
            room_version TEXT DEFAULT '6',
            created_ts BIGINT NOT NULL,
            last_activity_ts BIGINT,
            join_rules TEXT DEFAULT 'invite',
            history_visibility TEXT DEFAULT 'shared',
            name TEXT,
            topic TEXT,
            avatar_url TEXT,
            canonical_alias TEXT,
            visibility TEXT DEFAULT 'private',
            creator TEXT,
            encryption TEXT,
            member_count BIGINT DEFAULT 0
        )
    "#)
    .execute(&pool)
    .await
    .expect("Failed to create rooms table");

    sqlx::query(r#"
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
            banned_ts BIGINT,
            join_reason TEXT,
            PRIMARY KEY (room_id, user_id)
        )
    "#)
    .execute(&pool)
    .await
    .expect("Failed to create room_memberships table");

    sqlx::query(r#"
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
            processed_ts BIGINT,
            not_before BIGINT,
            is_redacted BOOLEAN DEFAULT FALSE,
            status TEXT,
            reference_image TEXT,
            origin TEXT,
            unsigned JSONB
        )
    "#)
    .execute(&pool)
    .await
    .expect("Failed to create events table");

    Some(pool)
}

async fn create_test_user(pool: &Pool<Postgres>, user_id: &str, username: &str) {
    sqlx::query(
        r#"
        INSERT INTO users (user_id, username, created_ts)
        VALUES ($1, $2, $3)
        ON CONFLICT (user_id) DO NOTHING
        "#,
    )
    .bind(user_id)
    .bind(username)
    .bind(chrono::Utc::now().timestamp_millis())
    .execute(pool)
    .await
    .expect("Failed to create test user");
}

fn create_room_service(pool: &Arc<Pool<Postgres>>, cache: Arc<CacheManager>) -> RoomService {
    let member_storage = RoomMemberStorage::new(pool, "localhost");
    let event_storage = EventStorage::new(pool, "localhost".to_string());
    let room_summary_storage = Arc::new(RoomSummaryStorage::new(pool));
    let room_summary_service = Arc::new(RoomSummaryService::new(
        room_summary_storage,
        Arc::new(event_storage.clone()),
        Some(Arc::new(member_storage.clone())),
    ));

    RoomService::new(synapse_rust::services::room_service::RoomServiceConfig {
        room_storage: RoomStorage::new(pool),
        member_storage,
        event_storage,
        user_storage: UserStorage::new(pool, cache),
        room_summary_service,
        validator: Arc::new(Validator::default()),
        server_name: "localhost".to_string(),
        task_queue: None,
    })
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
        let room_service = create_room_service(&pool, cache.clone());
        
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
        let id = unique_id();
        let alice_id = format!("@alice_{}:localhost", id);
        let alice_name = format!("alice_{}", id);
        create_test_user(&pool, &alice_id, &alice_name).await;

        let cache = Arc::new(CacheManager::new(CacheConfig::default()));
        let room_service = create_room_service(&pool, cache.clone());

        let config = CreateRoomConfig {
            name: Some("Test Room".to_string()),
            topic: Some("Test Topic".to_string()),
            visibility: Some("public".to_string()),
            ..Default::default()
        };

        let result = room_service.create_room(&alice_id, config).await;
        assert!(result.is_ok());
        let val = result.unwrap();
        assert!(val["room_id"].as_str().unwrap().starts_with('!'));

        let room_id = val["room_id"].as_str().unwrap();
        let room = room_service.get_room(room_id).await.unwrap();
        assert_eq!(room["name"], "Test Room");
        assert_eq!(room["topic"], "Test Topic");
        assert_eq!(room["is_public"], true);
        assert_eq!(room["creator"], alice_id);
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
        let id = unique_id();
        let alice_id = format!("@alice_{}:localhost", id);
        let alice_name = format!("alice_{}", id);
        let bob_id = format!("@bob_{}:localhost", id);
        let bob_name = format!("bob_{}", id);
        create_test_user(&pool, &alice_id, &alice_name).await;
        create_test_user(&pool, &bob_id, &bob_name).await;

        let cache = Arc::new(CacheManager::new(CacheConfig::default()));
        let room_service = create_room_service(&pool, cache.clone());

        let config = CreateRoomConfig::default();
        let room_val = room_service
            .create_room(&alice_id, config)
            .await
            .unwrap();
        let room_id = room_val["room_id"].as_str().unwrap();

        let result = room_service.join_room(room_id, &bob_id).await;
        assert!(result.is_ok());

        let members = room_service
            .get_room_members(room_id, &alice_id)
            .await
            .unwrap();
        let chunk = members["chunk"].as_array().unwrap();
        assert!(chunk.iter().any(|m| m["user_id"] == bob_id));
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
        let id = unique_id();
        let alice_id = format!("@alice_{}:localhost", id);
        let alice_name = format!("alice_{}", id);
        create_test_user(&pool, &alice_id, &alice_name).await;

        let cache = Arc::new(CacheManager::new(CacheConfig::default()));
        let room_service = create_room_service(&pool, cache.clone());

        let config = CreateRoomConfig::default();
        let room_val = room_service
            .create_room(&alice_id, config)
            .await
            .unwrap();
        let room_id = room_val["room_id"].as_str().unwrap();

        let content = json!({"msgtype": "m.text", "body": "Hello world"});
        let result = room_service
            .send_message(room_id, &alice_id, "m.room.message", &content)
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
        assert_eq!(chunk[0]["content"]["body"], "Hello world");
        assert_eq!(chunk[0]["sender"], alice_id);
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
        let id = unique_id();
        let alice_id = format!("@alice_{}:localhost", id);
        let alice_name = format!("alice_{}", id);
        let bob_id = format!("@bob_{}:localhost", id);
        let bob_name = format!("bob_{}", id);
        create_test_user(&pool, &alice_id, &alice_name).await;
        create_test_user(&pool, &bob_id, &bob_name).await;

        let cache = Arc::new(CacheManager::new(CacheConfig::default()));
        let room_service = create_room_service(&pool, cache.clone());

        let config = CreateRoomConfig::default();
        let room_val = room_service
            .create_room(&alice_id, config)
            .await
            .unwrap();
        let room_id = room_val["room_id"].as_str().unwrap();

        let result = room_service
            .invite_user(room_id, &alice_id, &bob_id)
            .await;
        assert!(result.is_ok());

        let member_storage = RoomMemberStorage::new(&pool, "localhost");
        let member = member_storage
            .get_member(room_id, &bob_id)
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
        let id = unique_id();
        let alice_id = format!("@alice_{}:localhost", id);
        let alice_name = format!("alice_{}", id);
        let bob_id = format!("@bob_{}:localhost", id);
        let bob_name = format!("bob_{}", id);
        create_test_user(&pool, &alice_id, &alice_name).await;
        create_test_user(&pool, &bob_id, &bob_name).await;

        let cache = Arc::new(CacheManager::new(CacheConfig::default()));
        let room_service = create_room_service(&pool, cache.clone());

        let config = CreateRoomConfig::default();
        let room_val = room_service
            .create_room(&alice_id, config)
            .await
            .unwrap();
        let room_id = room_val["room_id"].as_str().unwrap();

        let result = room_service
            .ban_user(room_id, &bob_id, &alice_id, Some("Spam"))
            .await;
        assert!(result.is_ok());

        let member_storage = RoomMemberStorage::new(&pool, "localhost");
        let member = member_storage
            .get_member(room_id, &bob_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(member.membership, "ban");
        assert_eq!(member.banned_by, Some(alice_id));
    });
}

#[test]
fn test_upgrade_room_success() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => Arc::new(pool),
            None => return,
        };
        let id = unique_id();
        let alice_id = format!("@alice_{}:localhost", id);
        let alice_name = format!("alice_{}", id);
        create_test_user(&pool, &alice_id, &alice_name).await;

        let cache = Arc::new(CacheManager::new(CacheConfig::default()));
        let room_service = create_room_service(&pool, cache.clone());

        let config = CreateRoomConfig::default();
        let room_val = room_service
            .create_room(&alice_id, config)
            .await
            .unwrap();
        let old_room_id = room_val["room_id"].as_str().unwrap();

        let result = room_service
            .upgrade_room(old_room_id, "11", &alice_id)
            .await;
        
        assert!(result.is_ok());
        let new_room_id = result.unwrap();
        assert!(!new_room_id.is_empty());
        assert_ne!(new_room_id, old_room_id);
    });
}

#[test]
fn test_upgrade_room_not_found() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => Arc::new(pool),
            None => return,
        };
        let id = unique_id();
        let alice_id = format!("@alice_{}:localhost", id);
        let alice_name = format!("alice_{}", id);
        create_test_user(&pool, &alice_id, &alice_name).await;

        let cache = Arc::new(CacheManager::new(CacheConfig::default()));
        let room_service = create_room_service(&pool, cache.clone());

        let result = room_service
            .upgrade_room("!nonexistent:localhost", "11", &alice_id)
            .await;
        
        assert!(result.is_err());
    });
}

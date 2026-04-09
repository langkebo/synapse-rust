#![cfg(test)]

use serde_json::json;
use sqlx::{Pool, Postgres};
use std::sync::Arc;
use tokio::runtime::Runtime;

use synapse_rust::cache::{CacheConfig, CacheManager};
use synapse_rust::common::validation::Validator;
use synapse_rust::services::room_service::{CreateRoomConfig, RoomService};
use synapse_rust::services::room_summary_service::RoomSummaryService;
use synapse_rust::services::sync_service::SyncService;
use synapse_rust::services::PresenceStorage;
use synapse_rust::storage::device::DeviceStorage;
use synapse_rust::storage::event::EventStorage;
use synapse_rust::storage::membership::RoomMemberStorage;
use synapse_rust::storage::room::RoomStorage;
use synapse_rust::storage::room_summary::RoomSummaryStorage;
use synapse_rust::storage::user::UserStorage;

async fn setup_test_database() -> Option<Arc<Pool<Postgres>>> {
    let pool = match synapse_rust::test_utils::prepare_empty_isolated_test_pool().await {
        Ok(pool) => pool,
        Err(error) => {
            eprintln!(
                "Skipping sync service tests because test database is unavailable: {}",
                error
            );
            return None;
        }
    };

    sqlx::query(
        r#"
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
            "#,
    )
    .execute(&*pool)
    .await
    .expect("Failed to create users table");

    sqlx::query(
        r#"
            CREATE TABLE presence (
                user_id VARCHAR(255) PRIMARY KEY,
                presence TEXT,
                status_msg TEXT,
                last_active_ts BIGINT,
                created_ts BIGINT,
                updated_ts BIGINT
            )
            "#,
    )
    .execute(&*pool)
    .await
    .expect("Failed to create presence table");

    sqlx::query(
        r#"
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
            "#,
    )
    .execute(&*pool)
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
                banned_ts BIGINT,
                join_reason TEXT,
                PRIMARY KEY (room_id, user_id)
            )
            "#,
    )
    .execute(&*pool)
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
                processed_ts BIGINT,
                not_before BIGINT,
                is_redacted BOOLEAN DEFAULT FALSE,
                status TEXT,
                reference_image TEXT,
                origin TEXT,
                unsigned JSONB
            )
            "#,
    )
    .execute(&*pool)
    .await
    .expect("Failed to create events table");

    Some(pool)
}

async fn create_test_user(pool: &Pool<Postgres>, user_id: &str, username: &str) {
    sqlx::query(
        r#"
            INSERT INTO users (user_id, username, created_ts)
            VALUES ($1, $2, $3)
            "#,
    )
    .bind(user_id)
    .bind(username)
    .bind(chrono::Utc::now().timestamp_millis())
    .execute(pool)
    .await
    .expect("Failed to create test user");
}

fn create_room_service(
    pool: &Arc<Pool<Postgres>>,
    room_storage: RoomStorage,
    member_storage: RoomMemberStorage,
    event_storage: EventStorage,
    user_storage: UserStorage,
) -> RoomService {
    let room_summary_storage = Arc::new(RoomSummaryStorage::new(pool));
    let room_summary_service = Arc::new(RoomSummaryService::new(
        room_summary_storage,
        Arc::new(event_storage.clone()),
        Some(Arc::new(member_storage.clone())),
    ));

    RoomService::new(synapse_rust::services::room_service::RoomServiceConfig {
        room_storage,
        member_storage,
        event_storage,
        user_storage,
        room_summary_service,
        validator: Arc::new(Validator::default()),
        server_name: "localhost".to_string(),
        task_queue: None,
        beacon_service: None,
    })
}

#[test]
fn test_sync_success() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        create_test_user(&*pool, "@alice:localhost", "alice").await;

        let cache = Arc::new(CacheManager::new(CacheConfig::default()));
        let presence_storage = PresenceStorage::new(pool.clone(), cache.clone());
        let member_storage = RoomMemberStorage::new(&pool, "localhost");
        let event_storage = EventStorage::new(&pool, "localhost".to_string());
        let room_storage = RoomStorage::new(&pool);
        let user_storage = UserStorage::new(&pool, cache.clone());

        let room_service = create_room_service(
            &pool,
            room_storage.clone(),
            member_storage.clone(),
            event_storage.clone(),
            user_storage.clone(),
        );

        let sync_service = SyncService::new(
            presence_storage,
            member_storage,
            event_storage,
            room_storage,
            DeviceStorage::new(&pool),
        );

        // Create a room and send a message
        let config = CreateRoomConfig {
            name: Some("Test Room".to_string()),
            ..Default::default()
        };
        let room_val = room_service
            .create_room("@alice:localhost", config)
            .await
            .unwrap();
        let room_id = room_val["room_id"].as_str().unwrap();

        let content = json!({"msgtype": "m.text", "body": "Hello"});
        room_service
            .send_message(room_id, "@alice:localhost", "m.room.message", &content)
            .await
            .unwrap();

        let result = sync_service
            .sync("@alice:localhost", None, 0, false, "online", None)
            .await;
        assert!(result.is_ok());
        let val = result.unwrap();
        assert!(val["rooms"]["join"].is_object());

        assert!(val["rooms"]["join"]
            .as_object()
            .unwrap()
            .contains_key(room_id));
        let room_data = &val["rooms"]["join"][room_id];
        let events = room_data["timeline"]["events"].as_array().unwrap();
        assert!(events.iter().any(|event| {
            event["type"] == "m.room.message" && event["content"]["body"] == "Hello"
        }));
    });
}

#[test]
fn test_incremental_sync_does_not_replay_old_timeline() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        create_test_user(&*pool, "@alice:localhost", "alice").await;

        let cache = Arc::new(CacheManager::new(CacheConfig::default()));
        let presence_storage = PresenceStorage::new(pool.clone(), cache.clone());
        let member_storage = RoomMemberStorage::new(&pool, "localhost");
        let event_storage = EventStorage::new(&pool, "localhost".to_string());
        let room_storage = RoomStorage::new(&pool);
        let user_storage = UserStorage::new(&pool, cache.clone());

        let room_service = create_room_service(
            &pool,
            room_storage.clone(),
            member_storage.clone(),
            event_storage.clone(),
            user_storage.clone(),
        );

        let sync_service = SyncService::new(
            presence_storage,
            member_storage,
            event_storage,
            room_storage,
            DeviceStorage::new(&pool),
        );

        let config = CreateRoomConfig {
            name: Some("Incremental Room".to_string()),
            ..Default::default()
        };
        let room_val = room_service
            .create_room("@alice:localhost", config)
            .await
            .unwrap();
        let room_id = room_val["room_id"].as_str().unwrap().to_string();

        let content = json!({"msgtype": "m.text", "body": "Hello once"});
        room_service
            .send_message(&room_id, "@alice:localhost", "m.room.message", &content)
            .await
            .unwrap();

        let first_sync = sync_service
            .sync("@alice:localhost", None, 0, false, "offline", None)
            .await
            .unwrap();
        let since = first_sync["next_batch"].as_str().unwrap().to_string();

        let second_sync = sync_service
            .sync(
                "@alice:localhost",
                None,
                0,
                false,
                "offline",
                Some(since.as_str()),
            )
            .await
            .unwrap();

        let joined_rooms = second_sync["rooms"]["join"].as_object().unwrap();
        assert!(
            joined_rooms.is_empty(),
            "incremental sync should not replay unchanged rooms"
        );
    });
}

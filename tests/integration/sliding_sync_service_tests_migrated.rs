#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use synapse_rust::cache::{CacheConfig, CacheManager};
use synapse_rust::services::sliding_sync_service::SlidingSyncService;
use synapse_rust::services::typing_service::TypingService;
use synapse_rust::services::PresenceStorage;
use synapse_rust::storage::event::EventStorage;
use synapse_rust::storage::membership::RoomMemberStorage;
use synapse_rust::storage::sliding_sync::{
    SlidingSyncFilters, SlidingSyncListData, SlidingSyncRequest, SlidingSyncStorage,
};

static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

fn unique_id() -> u64 {
    TEST_COUNTER.fetch_add(1, Ordering::SeqCst)
}

async fn setup_test_database(pool: &Arc<sqlx::PgPool>) {
    sqlx::query("CREATE SEQUENCE IF NOT EXISTS sliding_sync_pos_seq")
        .execute(pool.as_ref())
        .await
        .expect("Failed to create sliding_sync_pos_seq");

    sqlx::query(
        r#"
        CREATE TABLE sliding_sync_tokens (
            id BIGSERIAL PRIMARY KEY,
            user_id TEXT NOT NULL,
            device_id TEXT NOT NULL,
            conn_id TEXT,
            token TEXT NOT NULL,
            pos BIGINT NOT NULL,
            created_ts BIGINT NOT NULL,
            expires_at BIGINT
        )
        "#,
    )
    .execute(pool.as_ref())
    .await
    .expect("Failed to create sliding_sync_tokens table");

    sqlx::query(
        r#"
        CREATE UNIQUE INDEX idx_sliding_sync_tokens_unique ON sliding_sync_tokens(user_id, device_id, COALESCE(conn_id, ''))
        "#,
    )
    .execute(pool.as_ref())
    .await
    .expect("Failed to create sliding_sync_tokens unique index");

    sqlx::query(
        r#"
        CREATE TABLE sliding_sync_lists (
            id BIGSERIAL PRIMARY KEY,
            user_id TEXT NOT NULL,
            device_id TEXT NOT NULL,
            conn_id TEXT,
            list_key TEXT NOT NULL,
            sort JSONB DEFAULT '[]',
            filters JSONB DEFAULT '{}',
            room_subscription JSONB DEFAULT '{}',
            ranges JSONB DEFAULT '[]',
            created_ts BIGINT NOT NULL,
            updated_ts BIGINT NOT NULL
        )
        "#,
    )
    .execute(pool.as_ref())
    .await
    .expect("Failed to create sliding_sync_lists table");

    sqlx::query(
        r#"
        CREATE UNIQUE INDEX idx_sliding_sync_lists_unique ON sliding_sync_lists(user_id, device_id, COALESCE(conn_id, ''), list_key)
        "#,
    )
    .execute(pool.as_ref())
    .await
    .expect("Failed to create sliding_sync_lists unique index");

    sqlx::query(
        r#"
        CREATE TABLE sliding_sync_rooms (
            id BIGSERIAL PRIMARY KEY,
            user_id TEXT NOT NULL,
            device_id TEXT NOT NULL,
            room_id TEXT NOT NULL,
            conn_id TEXT,
            list_key TEXT,
            bump_stamp BIGINT DEFAULT 0,
            highlight_count INTEGER DEFAULT 0,
            notification_count INTEGER DEFAULT 0,
            is_dm BOOLEAN DEFAULT FALSE,
            is_encrypted BOOLEAN DEFAULT FALSE,
            is_tombstoned BOOLEAN DEFAULT FALSE,
            invited BOOLEAN DEFAULT FALSE,
            name TEXT,
            avatar TEXT,
            timestamp BIGINT DEFAULT 0,
            created_ts BIGINT NOT NULL,
            updated_ts BIGINT NOT NULL
        )
        "#,
    )
    .execute(pool.as_ref())
    .await
    .expect("Failed to create sliding_sync_rooms table");

    sqlx::query(
        r#"
        CREATE UNIQUE INDEX idx_sliding_sync_rooms_unique ON sliding_sync_rooms(user_id, device_id, room_id, COALESCE(conn_id, ''))
        "#,
    )
    .execute(pool.as_ref())
    .await
    .expect("Failed to create sliding_sync_rooms unique index");

    sqlx::query(
        r#"
        CREATE INDEX idx_sliding_sync_rooms_room_id ON sliding_sync_rooms(room_id, updated_ts DESC)
        "#,
    )
    .execute(pool.as_ref())
    .await
    .expect("Failed to create sliding_sync_rooms room_id index");

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS presence (
            user_id VARCHAR(255) PRIMARY KEY,
            presence TEXT,
            status_msg TEXT,
            last_active_ts BIGINT,
            created_ts BIGINT,
            updated_ts BIGINT
        )
        "#,
    )
    .execute(pool.as_ref())
    .await
    .expect("Failed to create presence table");

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS room_memberships (
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
    .execute(pool.as_ref())
    .await
    .expect("Failed to create room_memberships table");

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS events (
            event_id VARCHAR(255) PRIMARY KEY,
            room_id VARCHAR(255) NOT NULL,
            user_id VARCHAR(255) NOT NULL,
            sender VARCHAR(255) NOT NULL,
            event_type TEXT NOT NULL,
            content JSONB NOT NULL,
            state_key TEXT,
            depth BIGINT,
            stream_ordering BIGSERIAL,
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
    .execute(pool.as_ref())
    .await
    .expect("Failed to create events table");

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS rooms (
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
    .execute(pool.as_ref())
    .await
    .expect("Failed to create rooms table");

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS device_lists_stream (
            stream_id BIGSERIAL PRIMARY KEY,
            user_id VARCHAR(255) NOT NULL,
            device_id VARCHAR(255),
            created_ts BIGINT NOT NULL
        )
        "#,
    )
    .execute(pool.as_ref())
    .await
    .expect("Failed to create device_lists_stream table");

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS to_device_messages (
            stream_id BIGSERIAL PRIMARY KEY,
            sender_user_id VARCHAR(255) NOT NULL,
            sender_device_id VARCHAR(255) NOT NULL,
            recipient_user_id VARCHAR(255) NOT NULL,
            recipient_device_id VARCHAR(255) NOT NULL,
            event_type TEXT NOT NULL,
            content JSONB NOT NULL,
            message_id TEXT
        )
        "#,
    )
    .execute(pool.as_ref())
    .await
    .expect("Failed to create to_device_messages table");

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS account_data (
            user_id TEXT NOT NULL,
            data_type TEXT NOT NULL,
            content JSONB NOT NULL,
            PRIMARY KEY (user_id, data_type)
        )
        "#,
    )
    .execute(pool.as_ref())
    .await
    .expect("Failed to create account_data table");

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS room_account_data (
            user_id TEXT NOT NULL,
            room_id TEXT NOT NULL,
            data_type TEXT NOT NULL,
            data JSONB NOT NULL,
            PRIMARY KEY (user_id, room_id, data_type)
        )
        "#,
    )
    .execute(pool.as_ref())
    .await
    .expect("Failed to create room_account_data table");

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS event_receipts (
            room_id TEXT NOT NULL,
            event_id TEXT NOT NULL,
            user_id TEXT NOT NULL,
            receipt_type TEXT NOT NULL,
            ts BIGINT NOT NULL,
            data JSONB DEFAULT '{}'
        )
        "#,
    )
    .execute(pool.as_ref())
    .await
    .expect("Failed to create event_receipts table");

}

fn create_service(pool: &Arc<sqlx::PgPool>) -> SlidingSyncService {
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let storage = SlidingSyncStorage::new(pool.clone());
    let event_storage = EventStorage::new(pool, "localhost".to_string());
    let typing_service = Arc::new(TypingService::default());
    let presence_storage = PresenceStorage::new(pool.clone(), cache.clone());
    let member_storage = RoomMemberStorage::new(pool, "localhost");

    SlidingSyncService::new(storage, cache, event_storage, typing_service, presence_storage, member_storage)
}

#[tokio::test]
async fn test_initial_sync_returns_pos_and_empty_rooms() {

        let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
        let service = create_service(&pool);
        let suffix = unique_id();
        let user_id = format!("@init_{suffix}:localhost");

        let mut lists = HashMap::new();
        lists.insert(
            "main".to_string(),
            SlidingSyncListData {
                ranges: vec![vec![0, 20]],
                sort: vec!["by_recency".to_string()],
                filters: None,
                timeline_limit: None,
                required_state: None,
                slow_by: None,
                bump_event_types: None,
            },
        );

        let request = SlidingSyncRequest {
            conn_id: None,
            lists,
            room_subscriptions: None,
            unsubscribe_rooms: None,
            extensions: None,
            pos: None,
            timeout: None,
            client_timeout: None,
        };

        let response = service.sync(&user_id, "DEV1", request).await.unwrap();

        assert!(!response.pos.is_empty());
        assert!(response.conn_id.is_none());
        assert!(response.rooms.is_object());
}

#[tokio::test]
async fn test_sync_with_conn_id() {

        let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
        let service = create_service(&pool);
        let suffix = unique_id();
        let user_id = format!("@conn_{suffix}:localhost");

        let mut lists = HashMap::new();
        lists.insert(
            "main".to_string(),
            SlidingSyncListData {
                ranges: vec![vec![0, 10]],
                sort: vec!["by_recency".to_string()],
                filters: None,
                timeline_limit: None,
                required_state: None,
                slow_by: None,
                bump_event_types: None,
            },
        );

        let request = SlidingSyncRequest {
            conn_id: Some("test_conn".to_string()),
            lists,
            room_subscriptions: None,
            unsubscribe_rooms: None,
            extensions: None,
            pos: None,
            timeout: None,
            client_timeout: None,
        };

        let response = service.sync(&user_id, "DEV1", request).await.unwrap();

        assert_eq!(response.conn_id, Some("test_conn".to_string()));
}

#[tokio::test]
async fn test_incremental_sync_with_valid_pos() {

        let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
        let service = create_service(&pool);
        let suffix = unique_id();
        let user_id = format!("@incr_{suffix}:localhost");

        let mut lists = HashMap::new();
        lists.insert(
            "main".to_string(),
            SlidingSyncListData {
                ranges: vec![vec![0, 20]],
                sort: vec!["by_recency".to_string()],
                filters: None,
                timeline_limit: None,
                required_state: None,
                slow_by: None,
                bump_event_types: None,
            },
        );

        let request = SlidingSyncRequest {
            conn_id: None,
            lists: lists.clone(),
            room_subscriptions: None,
            unsubscribe_rooms: None,
            extensions: None,
            pos: None,
            timeout: None,
            client_timeout: None,
        };

        let first = service.sync(&user_id, "DEV1", request).await.unwrap();

        let incremental = SlidingSyncRequest {
            conn_id: None,
            lists,
            room_subscriptions: None,
            unsubscribe_rooms: None,
            extensions: None,
            pos: Some(first.pos.clone()),
            timeout: None,
            client_timeout: None,
        };

        let second = service.sync(&user_id, "DEV1", incremental).await.unwrap();
        assert_ne!(second.pos, first.pos);
}

#[tokio::test]
async fn test_incremental_sync_with_invalid_pos_returns_error() {

        let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
        let service = create_service(&pool);
        let suffix = unique_id();
        let user_id = format!("@badpos_{suffix}:localhost");

        let mut lists = HashMap::new();
        lists.insert(
            "main".to_string(),
            SlidingSyncListData {
                ranges: vec![vec![0, 20]],
                sort: vec!["by_recency".to_string()],
                filters: None,
                timeline_limit: None,
                required_state: None,
                slow_by: None,
                bump_event_types: None,
            },
        );

        let request = SlidingSyncRequest {
            conn_id: None,
            lists,
            room_subscriptions: None,
            unsubscribe_rooms: None,
            extensions: None,
            pos: Some("999999".to_string()),
            timeout: None,
            client_timeout: None,
        };

        let result = service.sync(&user_id, "DEV1", request).await;
        assert!(result.is_err());
}

#[tokio::test]
async fn test_update_room_state() {

        let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
        let service = create_service(&pool);
        let suffix = unique_id();
        let user_id = format!("@update_{suffix}:localhost");
        let room_id = format!("!room_{suffix}:localhost");

        service
            .update_room_state(
                &user_id,
                "DEV1",
                &room_id,
                None,
                1000,
                2,
                5,
                true,
                false,
                Some("Test Room"),
                Some("mxc://avatar"),
            )
            .await
            .unwrap();

        let storage = SlidingSyncStorage::new(pool.clone());
        let room = storage.get_room(&user_id, "DEV1", &room_id, None).await.unwrap().unwrap();

        assert_eq!(room.bump_stamp, 1000);
        assert_eq!(room.highlight_count, 2);
        assert_eq!(room.notification_count, 5);
        assert!(room.is_dm);
        assert!(!room.is_encrypted);
        assert_eq!(room.name, Some("Test Room".to_string()));
}

#[tokio::test]
async fn test_bump_room() {

        let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
        let service = create_service(&pool);
        let suffix = unique_id();
        let user_id = format!("@bump_{suffix}:localhost");
        let room_id = format!("!room_{suffix}:localhost");

        service
            .update_room_state(&user_id, "DEV1", &room_id, None, 1000, 0, 0, false, false, None, None)
            .await
            .unwrap();

        service.bump_room(&user_id, "DEV1", &room_id, None, 3000).await.unwrap();

        let storage = SlidingSyncStorage::new(pool.clone());
        let room = storage.get_room(&user_id, "DEV1", &room_id, None).await.unwrap().unwrap();
        assert_eq!(room.bump_stamp, 3000);

        service.bump_room(&user_id, "DEV1", &room_id, None, 2000).await.unwrap();

        let room = storage.get_room(&user_id, "DEV1", &room_id, None).await.unwrap().unwrap();
        assert_eq!(room.bump_stamp, 3000);
}

#[tokio::test]
async fn test_update_notification_counts() {

        let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
        let service = create_service(&pool);
        let suffix = unique_id();
        let user_id = format!("@notif_{suffix}:localhost");
        let room_id = format!("!room_{suffix}:localhost");

        service
            .update_room_state(&user_id, "DEV1", &room_id, None, 1000, 0, 0, false, false, None, None)
            .await
            .unwrap();

        service.update_notification_counts(&user_id, "DEV1", &room_id, None, 7, 15).await.unwrap();

        let storage = SlidingSyncStorage::new(pool.clone());
        let room = storage.get_room(&user_id, "DEV1", &room_id, None).await.unwrap().unwrap();
        assert_eq!(room.highlight_count, 7);
        assert_eq!(room.notification_count, 15);
}

#[tokio::test]
async fn test_remove_room() {

        let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
        let service = create_service(&pool);
        let suffix = unique_id();
        let user_id = format!("@remove_{suffix}:localhost");
        let room_id = format!("!room_{suffix}:localhost");

        service
            .update_room_state(&user_id, "DEV1", &room_id, None, 1000, 0, 0, false, false, None, None)
            .await
            .unwrap();

        service.remove_room(&user_id, "DEV1", &room_id, None).await.unwrap();

        let storage = SlidingSyncStorage::new(pool.clone());
        let room = storage.get_room(&user_id, "DEV1", &room_id, None).await.unwrap();
        assert!(room.is_none());
}

#[tokio::test]
async fn test_cleanup_expired_tokens() {

        let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
        let service = create_service(&pool);
        let suffix = unique_id();
        let user_id = format!("@cleanup_{suffix}:localhost");

        let storage = SlidingSyncStorage::new(pool.clone());
        let token = storage.create_or_update_token(&user_id, "DEV1", None).await.unwrap();

        let past_expiry = chrono::Utc::now().timestamp_millis() - 1000;
        sqlx::query("UPDATE sliding_sync_tokens SET expires_at = $1 WHERE id = $2")
            .bind(past_expiry)
            .bind(token.id)
            .execute(pool.as_ref())
            .await
            .unwrap();

        let deleted = service.cleanup_expired_tokens().await.unwrap();
        assert_eq!(deleted, 1);
}

#[tokio::test]
async fn test_get_room_token_sync() {

        let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
        let service = create_service(&pool);
        let suffix = unique_id();
        let user_id = format!("@token_sync_{suffix}:localhost");
        let room_id = format!("!room_{suffix}:localhost");

        let storage = SlidingSyncStorage::new(pool.clone());
        storage.create_or_update_token(&user_id, "DEV1", None).await.unwrap();

        service
            .update_room_state(&user_id, "DEV1", &room_id, None, 1000, 1, 3, false, false, Some("Sync Room"), None)
            .await
            .unwrap();

        let (entries, total) = service.get_room_token_sync(&room_id, 10, None).await.unwrap();
        assert_eq!(total, 1);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, Some("Sync Room".to_string()));
}

#[tokio::test]
async fn test_sync_with_room_subscriptions() {

        let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
        let service = create_service(&pool);
        let suffix = unique_id();
        let user_id = format!("@sub_{suffix}:localhost");
        let room_id = format!("!room_{suffix}:localhost");

        service
            .update_room_state(&user_id, "DEV1", &room_id, None, 1000, 0, 0, false, false, Some("Sub Room"), None)
            .await
            .unwrap();

        let mut lists = HashMap::new();
        lists.insert(
            "main".to_string(),
            SlidingSyncListData {
                ranges: vec![vec![0, 20]],
                sort: vec!["by_recency".to_string()],
                filters: None,
                timeline_limit: None,
                required_state: None,
                slow_by: None,
                bump_event_types: None,
            },
        );

        let request = SlidingSyncRequest {
            conn_id: None,
            lists,
            room_subscriptions: Some(serde_json::json!({
                &room_id: {
                    "timeline_limit": 10
                }
            })),
            unsubscribe_rooms: None,
            extensions: None,
            pos: None,
            timeout: None,
            client_timeout: None,
        };

        let response = service.sync(&user_id, "DEV1", request).await.unwrap();
        let rooms = response.rooms.as_object().unwrap();
        assert!(rooms.contains_key(&room_id));
}

#[tokio::test]
async fn test_sync_with_unsubscribe_rooms() {

        let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
        let service = create_service(&pool);
        let suffix = unique_id();
        let user_id = format!("@unsub_{suffix}:localhost");
        let room_id = format!("!room_{suffix}:localhost");

        service
            .update_room_state(&user_id, "DEV1", &room_id, None, 1000, 0, 0, false, false, None, None)
            .await
            .unwrap();

        let mut lists = HashMap::new();
        lists.insert(
            "main".to_string(),
            SlidingSyncListData {
                ranges: vec![vec![0, 20]],
                sort: vec!["by_recency".to_string()],
                filters: None,
                timeline_limit: None,
                required_state: None,
                slow_by: None,
                bump_event_types: None,
            },
        );

        let request = SlidingSyncRequest {
            conn_id: None,
            lists,
            room_subscriptions: None,
            unsubscribe_rooms: Some(vec![room_id.clone()]),
            extensions: None,
            pos: None,
            timeout: None,
            client_timeout: None,
        };

        let response = service.sync(&user_id, "DEV1", request).await.unwrap();
        assert!(!response.pos.is_empty());

        let storage = SlidingSyncStorage::new(pool.clone());
        let room = storage.get_room(&user_id, "DEV1", &room_id, None).await.unwrap();
        assert!(room.is_none());
}

#[tokio::test]
async fn test_sync_with_filters() {

        let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
        let service = create_service(&pool);
        let suffix = unique_id();
        let user_id = format!("@filter_{suffix}:localhost");

        service
            .update_room_state(
                &user_id,
                "DEV1",
                &format!("!dm_{suffix}:localhost"),
                None,
                100,
                0,
                0,
                true,
                false,
                Some("DM Room"),
                None,
            )
            .await
            .unwrap();
        service
            .update_room_state(
                &user_id,
                "DEV1",
                &format!("!group_{suffix}:localhost"),
                None,
                200,
                0,
                0,
                false,
                false,
                Some("Group Room"),
                None,
            )
            .await
            .unwrap();

        let mut lists = HashMap::new();
        lists.insert(
            "main".to_string(),
            SlidingSyncListData {
                ranges: vec![vec![0, 20]],
                sort: vec!["by_recency".to_string()],
                filters: Some(SlidingSyncFilters { is_dm: Some(true), ..Default::default() }),
                timeline_limit: None,
                required_state: None,
                slow_by: None,
                bump_event_types: None,
            },
        );

        let request = SlidingSyncRequest {
            conn_id: None,
            lists,
            room_subscriptions: None,
            unsubscribe_rooms: None,
            extensions: None,
            pos: None,
            timeout: None,
            client_timeout: None,
        };

        let response = service.sync(&user_id, "DEV1", request).await.unwrap();
        let rooms = response.rooms.as_object().unwrap();
        assert_eq!(rooms.len(), 1);
        assert!(rooms.contains_key(&format!("!dm_{suffix}:localhost")));
}

#[tokio::test]
async fn test_sync_multiple_lists() {

        let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
        let service = create_service(&pool);
        let suffix = unique_id();
        let user_id = format!("@multi_{suffix}:localhost");

        service
            .update_room_state(
                &user_id,
                "DEV1",
                &format!("!room1_{suffix}:localhost"),
                None,
                100,
                0,
                0,
                false,
                false,
                None,
                None,
            )
            .await
            .unwrap();

        let mut lists = HashMap::new();
        lists.insert(
            "main".to_string(),
            SlidingSyncListData {
                ranges: vec![vec![0, 10]],
                sort: vec!["by_recency".to_string()],
                filters: None,
                timeline_limit: None,
                required_state: None,
                slow_by: None,
                bump_event_types: None,
            },
        );
        lists.insert(
            "invites".to_string(),
            SlidingSyncListData {
                ranges: vec![vec![0, 10]],
                sort: vec!["by_recency".to_string()],
                filters: Some(SlidingSyncFilters { is_invite: Some(true), ..Default::default() }),
                timeline_limit: None,
                required_state: None,
                slow_by: None,
                bump_event_types: None,
            },
        );

        let request = SlidingSyncRequest {
            conn_id: None,
            lists,
            room_subscriptions: None,
            unsubscribe_rooms: None,
            extensions: None,
            pos: None,
            timeout: None,
            client_timeout: None,
        };

        let response = service.sync(&user_id, "DEV1", request).await.unwrap();
        let lists_obj = response.lists.as_object().unwrap();
        assert!(lists_obj.contains_key("main"));
        assert!(lists_obj.contains_key("invites"));
}

#[tokio::test]
async fn test_sync_with_empty_lists() {

        let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
        let service = create_service(&pool);
        let suffix = unique_id();
        let user_id = format!("@empty_{suffix}:localhost");

        let request = SlidingSyncRequest {
            conn_id: None,
            lists: HashMap::new(),
            room_subscriptions: None,
            unsubscribe_rooms: None,
            extensions: None,
            pos: None,
            timeout: None,
            client_timeout: None,
        };

        let response = service.sync(&user_id, "DEV1", request).await.unwrap();
        assert!(!response.pos.is_empty());
}

#[tokio::test]
async fn test_update_room_state_with_conn_id_isolation() {

        let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
        let service = create_service(&pool);
        let suffix = unique_id();
        let user_id = format!("@conn_iso_{suffix}:localhost");
        let room_id = format!("!room_{suffix}:localhost");

        service
            .update_room_state(&user_id, "DEV1", &room_id, None, 1000, 1, 2, false, false, Some("No Conn"), None)
            .await
            .unwrap();
        service
            .update_room_state(
                &user_id,
                "DEV1",
                &room_id,
                Some("conn1"),
                1000,
                3,
                4,
                false,
                false,
                Some("With Conn"),
                None,
            )
            .await
            .unwrap();

        let storage = SlidingSyncStorage::new(pool.clone());
        let room_none = storage.get_room(&user_id, "DEV1", &room_id, None).await.unwrap().unwrap();
        let room_conn = storage.get_room(&user_id, "DEV1", &room_id, Some("conn1")).await.unwrap().unwrap();

        assert_ne!(room_none.id, room_conn.id);
        assert_eq!(room_none.name, Some("No Conn".to_string()));
        assert_eq!(room_conn.name, Some("With Conn".to_string()));
}

#[tokio::test]
async fn test_remove_room_different_conn_id_no_cross_delete() {

        let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
        let service = create_service(&pool);
        let suffix = unique_id();
        let user_id = format!("@cross_del_{suffix}:localhost");
        let room_id = format!("!room_{suffix}:localhost");

        service
            .update_room_state(&user_id, "DEV1", &room_id, None, 1000, 0, 0, false, false, None, None)
            .await
            .unwrap();
        service
            .update_room_state(&user_id, "DEV1", &room_id, Some("conn1"), 1000, 0, 0, false, false, None, None)
            .await
            .unwrap();

        service.remove_room(&user_id, "DEV1", &room_id, None).await.unwrap();

        let storage = SlidingSyncStorage::new(pool.clone());
        let room_none = storage.get_room(&user_id, "DEV1", &room_id, None).await.unwrap();
        assert!(room_none.is_none());

        let room_conn = storage.get_room(&user_id, "DEV1", &room_id, Some("conn1")).await.unwrap();
        assert!(room_conn.is_some());
}

#[tokio::test]
async fn test_sync_pos_advances_on_each_request() {

        let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
        let service = create_service(&pool);
        let suffix = unique_id();
        let user_id = format!("@advance_{suffix}:localhost");

        let mut lists = HashMap::new();
        lists.insert(
            "main".to_string(),
            SlidingSyncListData {
                ranges: vec![vec![0, 20]],
                sort: vec!["by_recency".to_string()],
                filters: None,
                timeline_limit: None,
                required_state: None,
                slow_by: None,
                bump_event_types: None,
            },
        );

        let mut positions = Vec::new();
        for _ in 0..3 {
            let request = SlidingSyncRequest {
                conn_id: None,
                lists: lists.clone(),
                room_subscriptions: None,
                unsubscribe_rooms: None,
                extensions: None,
                pos: positions.last().cloned(),
                timeout: None,
                client_timeout: None,
            };

            let response = service.sync(&user_id, "DEV1", request).await.unwrap();
            positions.push(response.pos);
        }

        let pos_values: Vec<i64> = positions.iter().map(|p| p.parse::<i64>().unwrap()).collect();
        assert!(pos_values.windows(2).all(|w| w[1] > w[0]));
}

#[tokio::test]
async fn test_sync_with_account_data_extension() {

        let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
        let service = create_service(&pool);
        let suffix = unique_id();
        let user_id = format!("@ext_ad_{suffix}:localhost");

        let mut lists = HashMap::new();
        lists.insert(
            "main".to_string(),
            SlidingSyncListData {
                ranges: vec![vec![0, 20]],
                sort: vec!["by_recency".to_string()],
                filters: None,
                timeline_limit: None,
                required_state: None,
                slow_by: None,
                bump_event_types: None,
            },
        );

        let request = SlidingSyncRequest {
            conn_id: None,
            lists,
            room_subscriptions: None,
            unsubscribe_rooms: None,
            extensions: Some(serde_json::json!({
                "account_data": true
            })),
            pos: None,
            timeout: None,
            client_timeout: None,
        };

        let response = service.sync(&user_id, "DEV1", request).await.unwrap();
        assert!(response.extensions.is_some());
        let ext = response.extensions.unwrap();
        assert!(ext.get("account_data").is_some());
}

#[tokio::test]
async fn test_sync_without_extensions_returns_none() {

        let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
        let service = create_service(&pool);
        let suffix = unique_id();
        let user_id = format!("@no_ext_{suffix}:localhost");

        let mut lists = HashMap::new();
        lists.insert(
            "main".to_string(),
            SlidingSyncListData {
                ranges: vec![vec![0, 20]],
                sort: vec!["by_recency".to_string()],
                filters: None,
                timeline_limit: None,
                required_state: None,
                slow_by: None,
                bump_event_types: None,
            },
        );

        let request = SlidingSyncRequest {
            conn_id: None,
            lists,
            room_subscriptions: None,
            unsubscribe_rooms: None,
            extensions: None,
            pos: None,
            timeout: None,
            client_timeout: None,
        };

        let response = service.sync(&user_id, "DEV1", request).await.unwrap();
        assert!(response.extensions.is_none());
}

#[tokio::test]
async fn test_update_room_state_preserves_higher_bump_stamp() {

        let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
        let service = create_service(&pool);
        let suffix = unique_id();
        let user_id = format!("@bump_preserve_{suffix}:localhost");
        let room_id = format!("!room_{suffix}:localhost");

        service
            .update_room_state(&user_id, "DEV1", &room_id, None, 5000, 0, 0, false, false, None, None)
            .await
            .unwrap();

        service
            .update_room_state(&user_id, "DEV1", &room_id, None, 3000, 1, 1, false, false, None, None)
            .await
            .unwrap();

        let storage = SlidingSyncStorage::new(pool.clone());
        let room = storage.get_room(&user_id, "DEV1", &room_id, None).await.unwrap().unwrap();
        assert_eq!(room.bump_stamp, 5000);
}

#[tokio::test]
async fn test_update_room_state_preserves_name_when_null() {

        let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
        let service = create_service(&pool);
        let suffix = unique_id();
        let user_id = format!("@name_preserve_{suffix}:localhost");
        let room_id = format!("!room_{suffix}:localhost");

        service
            .update_room_state(
                &user_id,
                "DEV1",
                &room_id,
                None,
                1000,
                0,
                0,
                false,
                false,
                Some("Original Name"),
                Some("mxc://orig"),
            )
            .await
            .unwrap();

        service
            .update_room_state(&user_id, "DEV1", &room_id, None, 2000, 1, 1, false, false, None, None)
            .await
            .unwrap();

        let storage = SlidingSyncStorage::new(pool.clone());
        let room = storage.get_room(&user_id, "DEV1", &room_id, None).await.unwrap().unwrap();
        assert_eq!(room.name, Some("Original Name".to_string()));
        assert_eq!(room.avatar, Some("mxc://orig".to_string()));
}

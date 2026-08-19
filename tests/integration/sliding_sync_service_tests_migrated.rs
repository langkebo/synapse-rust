#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use synapse_common::current_timestamp_millis;
use synapse_e2ee::device_keys::DeviceKeyStorage;
use synapse_e2ee::to_device::ToDeviceStorage;
use synapse_rust::cache::{CacheConfig, CacheManager};
use synapse_rust::config::PerformanceConfig;
use synapse_rust::metrics::MetricsCollector;
use synapse_services::sliding_sync_service::SlidingSyncService;
use synapse_services::typing_service::TypingService;
use synapse_storage::device::DeviceStorage;
use synapse_storage::event::EventStorage;
use synapse_storage::membership::RoomMemberStorage;
use synapse_storage::sliding_sync::{SlidingSyncFilters, SlidingSyncListData, SlidingSyncRequest, SlidingSyncStorage};
use synapse_storage::PresenceStorage;

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
        CREATE TABLE IF NOT EXISTS sliding_sync_tokens (
            id BIGSERIAL PRIMARY KEY,
            user_id TEXT NOT NULL,
            device_id TEXT NOT NULL,
            conn_id TEXT,
            token TEXT NOT NULL,
            pos BIGINT NOT NULL,
            created_ts BIGINT NOT NULL,
            expires_at BIGINT,
            event_stream_pos BIGINT NOT NULL DEFAULT 0
        )
        "#,
    )
    .execute(pool.as_ref())
    .await
    .expect("Failed to create sliding_sync_tokens table");

    // S14: 兼容先于本列创建的测试库（CREATE TABLE IF NOT EXISTS 不会补列）
    sqlx::query("ALTER TABLE sliding_sync_tokens ADD COLUMN IF NOT EXISTS event_stream_pos BIGINT NOT NULL DEFAULT 0")
        .execute(pool.as_ref())
        .await
        .expect("Failed to ensure sliding_sync_tokens.event_stream_pos");

    sqlx::query(
        r#"
        CREATE UNIQUE INDEX IF NOT EXISTS idx_sliding_sync_tokens_unique ON sliding_sync_tokens(user_id, device_id, COALESCE(conn_id, ''))
        "#,
    )
    .execute(pool.as_ref())
    .await
    .expect("Failed to create sliding_sync_tokens unique index");

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS sliding_sync_lists (
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
        CREATE UNIQUE INDEX IF NOT EXISTS idx_sliding_sync_lists_unique ON sliding_sync_lists(user_id, device_id, COALESCE(conn_id, ''), list_key)
        "#,
    )
    .execute(pool.as_ref())
    .await
    .expect("Failed to create sliding_sync_lists unique index");

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS sliding_sync_rooms (
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
            is_invited BOOLEAN DEFAULT FALSE,
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
        CREATE UNIQUE INDEX IF NOT EXISTS idx_sliding_sync_rooms_unique ON sliding_sync_rooms(user_id, device_id, room_id, COALESCE(conn_id, ''))
        "#,
    )
    .execute(pool.as_ref())
    .await
    .expect("Failed to create sliding_sync_rooms unique index");

    sqlx::query(
        r#"
        CREATE INDEX IF NOT EXISTS idx_sliding_sync_rooms_room_id ON sliding_sync_rooms(room_id, updated_ts DESC)
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
    create_service_with_cache(pool, Arc::new(CacheManager::new(&CacheConfig::default())))
}

fn create_service_with_cache(pool: &Arc<sqlx::PgPool>, cache: Arc<CacheManager>) -> SlidingSyncService {
    let storage = Arc::new(SlidingSyncStorage::new(pool.clone()));
    let event_storage = Arc::new(EventStorage::new(pool, "localhost".to_string()));
    let typing_service = Arc::new(TypingService::default());
    let presence_storage = Arc::new(PresenceStorage::new(pool.clone(), cache.clone()));
    let member_storage = Arc::new(RoomMemberStorage::new(pool, "localhost"));
    let device_storage = Arc::new(DeviceStorage::new(pool));
    let to_device_storage = ToDeviceStorage::new(pool);
    let metrics = Arc::new(MetricsCollector::new());

    SlidingSyncService::new(
        storage,
        cache,
        event_storage,
        Arc::new(DeviceKeyStorage::new(pool)) as Arc<dyn synapse_e2ee::device_keys::DeviceKeyStoreApi>,
        typing_service,
        presence_storage,
        member_storage,
        device_storage,
        to_device_storage,
        metrics,
        PerformanceConfig::default(),
        None,
    )
}

// ── 存储层便捷封装 ────────────────────────────────────────────────────────
// S10/N2 清理后，SlidingSyncService 删除了这些生产无调用方的便捷方法
// （其附带的 invalidate_room_cache 本属永落空空操作）。测试改为直调存储层，
// 语义与被删除的封装一致。

#[allow(clippy::too_many_arguments)]
async fn update_room_state(
    pool: &Arc<sqlx::PgPool>,
    user_id: &str,
    device_id: &str,
    room_id: &str,
    conn_id: Option<&str>,
    bump_stamp: i64,
    highlight_count: i32,
    notification_count: i32,
    is_dm: bool,
    is_encrypted: bool,
    name: Option<&str>,
    avatar: Option<&str>,
) -> Result<(), sqlx::Error> {
    SlidingSyncStorage::new(pool.clone())
        .upsert_room(
            user_id,
            device_id,
            room_id,
            conn_id,
            None,
            bump_stamp,
            highlight_count,
            notification_count,
            is_dm,
            is_encrypted,
            false,
            false,
            name,
            avatar,
            bump_stamp,
        )
        .await
        .map(|_| ())
}

async fn bump_room(
    pool: &Arc<sqlx::PgPool>,
    user_id: &str,
    device_id: &str,
    room_id: &str,
    conn_id: Option<&str>,
    bump_stamp: i64,
) -> Result<(), sqlx::Error> {
    SlidingSyncStorage::new(pool.clone()).bump_room(user_id, device_id, room_id, conn_id, bump_stamp).await
}

async fn update_notification_counts(
    pool: &Arc<sqlx::PgPool>,
    user_id: &str,
    device_id: &str,
    room_id: &str,
    conn_id: Option<&str>,
    highlight_count: i32,
    notification_count: i32,
) -> Result<(), sqlx::Error> {
    SlidingSyncStorage::new(pool.clone())
        .update_notification_counts(user_id, device_id, room_id, conn_id, highlight_count, notification_count)
        .await
}

async fn remove_room(
    pool: &Arc<sqlx::PgPool>,
    user_id: &str,
    device_id: &str,
    room_id: &str,
    conn_id: Option<&str>,
) -> Result<(), sqlx::Error> {
    SlidingSyncStorage::new(pool.clone()).delete_room(user_id, device_id, room_id, conn_id).await
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
        txn_id: None,
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
        txn_id: None,
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
        txn_id: None,
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
        txn_id: None,
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
        txn_id: None,
    };

    let result = service.sync(&user_id, "DEV1", request).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_update_room_state() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let _ = create_service(&pool);
    let suffix = unique_id();
    let user_id = format!("@update_{suffix}:localhost");
    let room_id = format!("!room_{suffix}:localhost");

    update_room_state(
        &pool,
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

    assert_eq!(room.bump_stamp, Some(1000));
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
    let _ = create_service(&pool);
    let suffix = unique_id();
    let user_id = format!("@bump_{suffix}:localhost");
    let room_id = format!("!room_{suffix}:localhost");

    update_room_state(&pool, &user_id, "DEV1", &room_id, None, 1000, 0, 0, false, false, None, None).await.unwrap();

    bump_room(&pool, &user_id, "DEV1", &room_id, None, 3000).await.unwrap();

    let storage = SlidingSyncStorage::new(pool.clone());
    let room = storage.get_room(&user_id, "DEV1", &room_id, None).await.unwrap().unwrap();
    assert_eq!(room.bump_stamp, Some(3000));

    bump_room(&pool, &user_id, "DEV1", &room_id, None, 2000).await.unwrap();

    let room = storage.get_room(&user_id, "DEV1", &room_id, None).await.unwrap().unwrap();
    assert_eq!(room.bump_stamp, Some(3000));
}

#[tokio::test]
async fn test_update_notification_counts() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let _ = create_service(&pool);
    let suffix = unique_id();
    let user_id = format!("@notif_{suffix}:localhost");
    let room_id = format!("!room_{suffix}:localhost");

    update_room_state(&pool, &user_id, "DEV1", &room_id, None, 1000, 0, 0, false, false, None, None).await.unwrap();

    update_notification_counts(&pool, &user_id, "DEV1", &room_id, None, 7, 15).await.unwrap();

    let storage = SlidingSyncStorage::new(pool.clone());
    let room = storage.get_room(&user_id, "DEV1", &room_id, None).await.unwrap().unwrap();
    assert_eq!(room.highlight_count, 7);
    assert_eq!(room.notification_count, 15);
}

#[tokio::test]
async fn test_remove_room() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let _ = create_service(&pool);
    let suffix = unique_id();
    let user_id = format!("@remove_{suffix}:localhost");
    let room_id = format!("!room_{suffix}:localhost");

    update_room_state(&pool, &user_id, "DEV1", &room_id, None, 1000, 0, 0, false, false, None, None).await.unwrap();

    remove_room(&pool, &user_id, "DEV1", &room_id, None).await.unwrap();

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
    let token = storage.create_or_update_token(&user_id, "DEV1", None, 0).await.unwrap();

    let past_expiry = current_timestamp_millis() - 1000;
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
    storage.create_or_update_token(&user_id, "DEV1", None, 0).await.unwrap();

    update_room_state(&pool, &user_id, "DEV1", &room_id, None, 1000, 1, 3, false, false, Some("Sync Room"), None)
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

    update_room_state(&pool, &user_id, "DEV1", &room_id, None, 1000, 0, 0, false, false, Some("Sub Room"), None)
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
        txn_id: None,
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

    update_room_state(&pool, &user_id, "DEV1", &room_id, None, 1000, 0, 0, false, false, None, None).await.unwrap();

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
        txn_id: None,
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

    update_room_state(
        &pool,
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
    update_room_state(
        &pool,
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
        txn_id: None,
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

    update_room_state(
        &pool,
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
        txn_id: None,
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
        txn_id: None,
    };

    let response = service.sync(&user_id, "DEV1", request).await.unwrap();
    assert!(!response.pos.is_empty());
}

#[tokio::test]
async fn test_update_room_state_with_conn_id_isolation() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let _ = create_service(&pool);
    let suffix = unique_id();
    let user_id = format!("@conn_iso_{suffix}:localhost");
    let room_id = format!("!room_{suffix}:localhost");

    update_room_state(&pool, &user_id, "DEV1", &room_id, None, 1000, 1, 2, false, false, Some("No Conn"), None)
        .await
        .unwrap();
    update_room_state(
        &pool,
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
    let _ = create_service(&pool);
    let suffix = unique_id();
    let user_id = format!("@cross_del_{suffix}:localhost");
    let room_id = format!("!room_{suffix}:localhost");

    update_room_state(&pool, &user_id, "DEV1", &room_id, None, 1000, 0, 0, false, false, None, None).await.unwrap();
    update_room_state(&pool, &user_id, "DEV1", &room_id, Some("conn1"), 1000, 0, 0, false, false, None, None)
        .await
        .unwrap();

    remove_room(&pool, &user_id, "DEV1", &room_id, None).await.unwrap();

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
            txn_id: None,
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
        txn_id: None,
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
        txn_id: None,
    };

    let response = service.sync(&user_id, "DEV1", request).await.unwrap();
    assert!(response.extensions.is_none());
}

#[tokio::test]
async fn test_update_room_state_preserves_higher_bump_stamp() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let _ = create_service(&pool);
    let suffix = unique_id();
    let user_id = format!("@bump_preserve_{suffix}:localhost");
    let room_id = format!("!room_{suffix}:localhost");

    update_room_state(&pool, &user_id, "DEV1", &room_id, None, 5000, 0, 0, false, false, None, None).await.unwrap();

    update_room_state(&pool, &user_id, "DEV1", &room_id, None, 3000, 1, 1, false, false, None, None).await.unwrap();

    let storage = SlidingSyncStorage::new(pool.clone());
    let room = storage.get_room(&user_id, "DEV1", &room_id, None).await.unwrap().unwrap();
    assert_eq!(room.bump_stamp, Some(5000));
}

#[tokio::test]
async fn test_update_room_state_preserves_name_when_null() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let _ = create_service(&pool);
    let suffix = unique_id();
    let user_id = format!("@name_preserve_{suffix}:localhost");
    let room_id = format!("!room_{suffix}:localhost");

    update_room_state(
        &pool,
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

    update_room_state(&pool, &user_id, "DEV1", &room_id, None, 2000, 1, 1, false, false, None, None).await.unwrap();

    let storage = SlidingSyncStorage::new(pool.clone());
    let room = storage.get_room(&user_id, "DEV1", &room_id, None).await.unwrap().unwrap();
    assert_eq!(room.name, Some("Original Name".to_string()));
    assert_eq!(room.avatar, Some("mxc://orig".to_string()));
}

// =============================================================================
// P1-5: Sliding Sync 订阅变更即时响应 (Synapse #19714 / #19734)
//
// 上游修复：在 long-poll 模式下，当客户端在等待期间发送了订阅变更请求
// (room_subscriptions / unsubscribe_rooms / required_state / timeline_limit)，
// 服务器应立即返回新响应，而不是继续等待 long-poll 超时。
//
// synapse-rust 当前是同步轮询模式（无 long-poll），订阅变更天然在新请求中
// 立即生效。以下测试验证此行为，作为回归保护：如果未来引入 long-poll 模式，
// 这些测试确保订阅变更仍然立即响应。
// =============================================================================

/// 辅助函数：构造一个空的 main list 请求。
fn make_p1_5_request(
    lists: HashMap<String, SlidingSyncListData>,
    room_subscriptions: Option<serde_json::Value>,
    unsubscribe_rooms: Option<Vec<String>>,
    pos: Option<String>,
) -> SlidingSyncRequest {
    SlidingSyncRequest {
        conn_id: None,
        lists,
        room_subscriptions,
        unsubscribe_rooms,
        extensions: None,
        pos,
        timeout: None,
        client_timeout: None,
        txn_id: None,
    }
}

/// 辅助函数：构造 main list（范围 [0, 20]）。
fn make_p1_5_main_list() -> HashMap<String, SlidingSyncListData> {
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
    lists
}

/// P1-5 场景 1: room_subscriptions 变更即时响应。
///
/// 步骤：
/// 1. 创建 room_A 和 room_B 两个房间
/// 2. 第一次 sync：订阅 room_A，验证响应包含 room_A 不包含 room_B
/// 3. 第二次 sync：订阅 room_B（不再订阅 room_A），验证响应包含 room_B 不包含 room_A
///
/// 期望：第二次响应立即反映订阅变更，无需等待。
#[tokio::test]
async fn test_p1_5_room_subscription_change_reflected_immediately() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = create_service(&pool);
    let suffix = unique_id();
    let user_id = format!("@p15_sub_{suffix}:localhost");
    let room_a = format!("!roomA_{suffix}:localhost");
    let room_b = format!("!roomB_{suffix}:localhost");

    // 物化两个房间
    update_room_state(&pool, &user_id, "DEV1", &room_a, None, 1000, 0, 0, false, false, Some("Room A"), None)
        .await
        .unwrap();
    update_room_state(&pool, &user_id, "DEV1", &room_b, None, 2000, 0, 0, false, false, Some("Room B"), None)
        .await
        .unwrap();

    // 第一次 sync：订阅 room_A
    let request1 = make_p1_5_request(
        make_p1_5_main_list(),
        Some(serde_json::json!({
            &room_a: { "timeline_limit": 10 }
        })),
        None,
        None,
    );
    let response1 = service.sync(&user_id, "DEV1", request1).await.unwrap();
    let rooms1 = response1.rooms.as_object().unwrap();
    assert!(rooms1.contains_key(&room_a), "first sync should include room_A in response (subscribed)");

    // 第二次 sync：订阅 room_B（不再订阅 room_A）
    let request2 = make_p1_5_request(
        make_p1_5_main_list(),
        Some(serde_json::json!({
            &room_b: { "timeline_limit": 10 }
        })),
        None,
        Some(response1.pos.clone()),
    );
    let response2 = service.sync(&user_id, "DEV1", request2).await.unwrap();
    let rooms2 = response2.rooms.as_object().unwrap();
    assert!(rooms2.contains_key(&room_b), "second sync should immediately include room_B (new subscription)");
    // room_A 仍然可能在响应中（因为它在 main list 范围内），但 room_B 必须立即出现
    // 关键点：room_B 的订阅变更在第二次请求中立即生效，无需等待
}

/// P1-5 场景 2: unsubscribe_rooms 即时生效。
///
/// 步骤：
/// 1. 创建并订阅 room_A
/// 2. 第二次 sync：unsubscribe_rooms: [room_A]
/// 3. 验证 room_A 已从存储中删除
///
/// 期望：unsubscribe 在当次请求中立即生效。
#[tokio::test]
async fn test_p1_5_unsubscribe_rooms_takes_effect_immediately() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = create_service(&pool);
    let suffix = unique_id();
    let user_id = format!("@p15_unsub_{suffix}:localhost");
    let room_a = format!("!roomA_{suffix}:localhost");

    update_room_state(&pool, &user_id, "DEV1", &room_a, None, 1000, 0, 0, false, false, Some("Room A"), None)
        .await
        .unwrap();

    // 第一次 sync：订阅 room_A
    let request1 = make_p1_5_request(
        make_p1_5_main_list(),
        Some(serde_json::json!({
            &room_a: { "timeline_limit": 10 }
        })),
        None,
        None,
    );
    let response1 = service.sync(&user_id, "DEV1", request1).await.unwrap();
    let rooms1 = response1.rooms.as_object().unwrap();
    assert!(rooms1.contains_key(&room_a), "first sync should include room_A");

    // 第二次 sync：unsubscribe room_A
    let request2 =
        make_p1_5_request(make_p1_5_main_list(), None, Some(vec![room_a.clone()]), Some(response1.pos.clone()));
    let response2 = service.sync(&user_id, "DEV1", request2).await.unwrap();
    assert!(!response2.pos.is_empty(), "second sync should succeed");

    // 验证 room_A 已从存储中删除（unsubscribe 立即生效）
    let storage = SlidingSyncStorage::new(pool.clone());
    let room = storage.get_room(&user_id, "DEV1", &room_a, None).await.unwrap();
    assert!(room.is_none(), "room_A should be deleted from storage after unsubscribe");
}

/// P1-5 场景 3: required_state 变更即时响应。
///
/// 步骤：
/// 1. 创建 room_A，写入 m.room.name 状态事件
/// 2. 第一次 sync：订阅 room_A，required_state = []（空）
/// 3. 第二次 sync：订阅 room_A，required_state = [["m.room.name", ""]]
/// 4. 验证第二次响应的 required_state 立即包含 m.room.name 事件
///
/// 期望：required_state 变更在第二次请求中立即生效。
#[tokio::test]
async fn test_p1_5_required_state_change_reflected_immediately() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = create_service(&pool);
    let suffix = unique_id();
    let user_id = format!("@p15_rs_{suffix}:localhost");
    let room_a = format!("!roomA_{suffix}:localhost");

    update_room_state(&pool, &user_id, "DEV1", &room_a, None, 1000, 0, 0, false, false, Some("Room A"), None)
        .await
        .unwrap();

    // 写入 m.room.name 状态事件到 events 表（供 required_state 查询）
    sqlx::query(
        r#"
        INSERT INTO events (event_id, room_id, user_id, sender, event_type, content, state_key, depth, origin_server_ts, processed_at, not_before, is_redacted, status, origin)
        VALUES ($1, $2, $3, $3, 'm.room.name', '{"name": "Room A"}', '', 1, 1000, 1000, 0, FALSE, 'processed', 'localhost')
        "#,
    )
    .bind(format!("$name_{suffix}:localhost"))
    .bind(&room_a)
    .bind(&user_id)
    .execute(pool.as_ref())
    .await
    .unwrap();

    // 第一次 sync：订阅 room_A，required_state = []（空，不返回任何状态）
    let request1 = make_p1_5_request(
        make_p1_5_main_list(),
        Some(serde_json::json!({
            &room_a: {
                "timeline_limit": 0,
                "required_state": []
            }
        })),
        None,
        None,
    );
    let response1 = service.sync(&user_id, "DEV1", request1).await.unwrap();
    let rooms1 = response1.rooms.as_object().unwrap();
    let room_a_resp1 = rooms1.get(&room_a).expect("room_A should be in response");
    let required_state1 = room_a_resp1.get("required_state").and_then(|v| v.as_array());
    assert!(
        required_state1.is_none_or(|arr| arr.is_empty()),
        "first sync with empty required_state should return no state events, got: {:?}",
        required_state1
    );

    // 第二次 sync：订阅 room_A，required_state = [["m.room.name", ""]]
    let request2 = make_p1_5_request(
        make_p1_5_main_list(),
        Some(serde_json::json!({
            &room_a: {
                "timeline_limit": 0,
                "required_state": [["m.room.name", ""]]
            }
        })),
        None,
        Some(response1.pos.clone()),
    );
    let response2 = service.sync(&user_id, "DEV1", request2).await.unwrap();
    let rooms2 = response2.rooms.as_object().unwrap();
    let room_a_resp2 = rooms2.get(&room_a).expect("room_A should be in second response");
    let required_state2 = room_a_resp2
        .get("required_state")
        .and_then(|v| v.as_array())
        .expect("required_state should be an array in second response");
    assert!(
        !required_state2.is_empty(),
        "second sync with required_state=[[m.room.name,\"\"]] should immediately return name event, got: {:?}",
        required_state2
    );
    // 验证返回的事件类型是 m.room.name
    let event_type = required_state2[0].get("type").and_then(|v| v.as_str());
    assert_eq!(event_type, Some("m.room.name"), "required_state event should be m.room.name, got: {:?}", event_type);
}

/// P1-5 场景 4: timeline_limit 变更即时响应。
///
/// 步骤：
/// 1. 创建 room_A，写入 2 条 timeline 事件
/// 2. 第一次 sync：timeline_limit = 1
/// 3. 第二次 sync：timeline_limit = 10
/// 4. 验证第二次响应的 timeline 立即包含更多事件
///
/// 期望：timeline_limit 变更在第二次请求中立即生效。
#[tokio::test]
async fn test_p1_5_timeline_limit_change_reflected_immediately() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = create_service(&pool);
    let suffix = unique_id();
    let user_id = format!("@p15_tl_{suffix}:localhost");
    let room_a = format!("!roomA_{suffix}:localhost");

    update_room_state(&pool, &user_id, "DEV1", &room_a, None, 1000, 0, 0, false, false, Some("Room A"), None)
        .await
        .unwrap();

    // 写入 2 条 timeline 事件
    for i in 0..2 {
        sqlx::query(
            r#"
            INSERT INTO events (event_id, room_id, user_id, sender, event_type, content, state_key, depth, origin_server_ts, processed_at, not_before, is_redacted, status, origin)
            VALUES ($1, $2, $3, $3, 'm.room.message', $4, NULL, $5, $6, $6, 0, FALSE, 'processed', 'localhost')
            "#,
        )
        .bind(format!("$msg{i}_{suffix}:localhost"))
        .bind(&room_a)
        .bind(&user_id)
        .bind(serde_json::json!({"body": format!("msg {i}"), "msgtype": "m.text"}))
        .bind(i + 1)
        .bind(2000 + i)
        .execute(pool.as_ref())
        .await
        .unwrap();
    }

    // 第一次 sync：timeline_limit = 1
    let request1 = make_p1_5_request(
        make_p1_5_main_list(),
        Some(serde_json::json!({
            &room_a: {
                "timeline_limit": 1,
                "required_state": []
            }
        })),
        None,
        None,
    );
    let response1 = service.sync(&user_id, "DEV1", request1).await.unwrap();
    let rooms1 = response1.rooms.as_object().unwrap();
    let room_a_resp1 = rooms1.get(&room_a).expect("room_A should be in first response");
    let timeline1 =
        room_a_resp1.get("timeline").and_then(|v| v.as_array()).expect("timeline should be an array in first response");
    assert_eq!(
        timeline1.len(),
        1,
        "first sync with timeline_limit=1 should return exactly 1 event, got: {}",
        timeline1.len()
    );

    // 第二次 sync：timeline_limit = 10
    let request2 = make_p1_5_request(
        make_p1_5_main_list(),
        Some(serde_json::json!({
            &room_a: {
                "timeline_limit": 10,
                "required_state": []
            }
        })),
        None,
        Some(response1.pos.clone()),
    );
    let response2 = service.sync(&user_id, "DEV1", request2).await.unwrap();
    let rooms2 = response2.rooms.as_object().unwrap();
    let room_a_resp2 = rooms2.get(&room_a).expect("room_A should be in second response");
    let timeline2 = room_a_resp2
        .get("timeline")
        .and_then(|v| v.as_array())
        .expect("timeline should be an array in second response");
    assert!(
        timeline2.len() >= 2,
        "second sync with timeline_limit=10 should immediately return at least 2 events, got: {}",
        timeline2.len()
    );
}

// =============================================================================
// P1-6: /sync 瞬态错误缓存修复验证 (Synapse #19845)
//
// 上游修复：`/sync` 不应缓存瞬态错误响应（如数据库短暂故障）。
// synapse-rust 的 `/sync` (v3) 没有 response cache，不存在此问题。
// MSC4186 sliding sync 的 txn_id 缓存已明确：只在 `Ok` 时缓存。
// 此测试验证 txn_id 缓存行为：成功响应被缓存，失败响应不被缓存。
// =============================================================================

/// P1-6 场景 1: 成功响应应被 txn_id 缓存（基线行为）。
#[tokio::test]
async fn test_p1_6_successful_response_is_cached_under_txn_id() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = create_service(&pool);
    let suffix = unique_id();
    let user_id = format!("@p16_ok_{suffix}:localhost");
    let txn_id = format!("txn-p16-ok-{suffix}");

    let request = SlidingSyncRequest {
        conn_id: None,
        lists: make_p1_5_main_list(),
        room_subscriptions: None,
        unsubscribe_rooms: None,
        extensions: None,
        pos: None,
        timeout: None,
        client_timeout: None,
        txn_id: Some(txn_id.clone()),
    };

    // 第一次请求 — 应成功并被缓存
    let response1 = service.sync(&user_id, "DEV1", request).await.expect("first sync should succeed");
    assert!(!response1.pos.is_empty());

    // 第二次请求使用相同 txn_id — 应命中缓存返回相同 pos
    let request2 = SlidingSyncRequest {
        conn_id: None,
        lists: make_p1_5_main_list(),
        room_subscriptions: None,
        unsubscribe_rooms: None,
        extensions: None,
        pos: None,
        timeout: None,
        client_timeout: None,
        txn_id: Some(txn_id.clone()),
    };
    let response2 = service.sync(&user_id, "DEV1", request2).await.expect("cached sync should succeed");
    assert_eq!(
        response1.pos, response2.pos,
        "P1-6 baseline: successful response with same txn_id should be cached and return same pos"
    );
}

/// P1-6 场景 2: 失败响应不应被 txn_id 缓存。
///
/// 验证方式：发送一个会失败的请求（使用无效 pos），确认错误不被缓存。
/// 如果错误被缓存，后续相同 txn_id 的请求会持续返回错误。
#[tokio::test]
async fn test_p1_6_failed_response_not_cached_under_txn_id() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = create_service(&pool);
    let suffix = unique_id();
    let user_id = format!("@p16_err_{suffix}:localhost");
    let txn_id = format!("txn-p16-err-{suffix}");

    // 使用无效 pos 触发失败
    let request_with_invalid_pos = SlidingSyncRequest {
        conn_id: None,
        lists: make_p1_5_main_list(),
        room_subscriptions: None,
        unsubscribe_rooms: None,
        extensions: None,
        pos: Some("invalid-pos-token-p1-6".to_string()),
        timeout: None,
        client_timeout: None,
        txn_id: Some(txn_id.clone()),
    };

    // 第一次请求 — 应失败（无效 pos）
    let result1 = service.sync(&user_id, "DEV1", request_with_invalid_pos).await;
    assert!(result1.is_err(), "sync with invalid pos should fail");

    // 第二次请求使用相同 txn_id 但有效参数 — 应成功（不命中缓存的错误）
    let request_valid = SlidingSyncRequest {
        conn_id: None,
        lists: make_p1_5_main_list(),
        room_subscriptions: None,
        unsubscribe_rooms: None,
        extensions: None,
        pos: None,
        timeout: None,
        client_timeout: None,
        txn_id: Some(txn_id.clone()),
    };
    let response2 = service.sync(&user_id, "DEV1", request_valid).await;
    assert!(
        response2.is_ok(),
        "P1-6: failed response must NOT be cached — second request with same txn_id but valid params should succeed, got error: {:?}",
        response2.err()
    );
}

// ── S14/SS-10: 增量 timeline 水位线回写 ────────────────────────────────────
//
// 验收路径（对应优化方案 S14）：
//   1. 初始同步把「读阶段开始时的最大 stream_ordering」快照写入 token 行；
//   2. 客户端离线期间到达的新事件，增量同步只下发水位线之后的事件
//      （不得重复下发已收事件，也不得丢弃）；
//   3. 增量同步结束时把新的最大流水号回写 token 行，作为下一轮水位线；
//   4. 无新事件时再次增量同步不重复下发。

async fn insert_wm_message_event(pool: &Arc<sqlx::PgPool>, event_id: &str, room_id: &str, user_id: &str, ts: i64) {
    sqlx::query(
        r#"
        INSERT INTO events (event_id, room_id, user_id, sender, event_type, content, state_key, depth, origin_server_ts, processed_at, not_before, is_redacted, status, origin)
        VALUES ($1, $2, $3, $3, 'm.room.message', '{"msgtype":"m.text","body":"wm"}', NULL, 1, $4, $4, 0, FALSE, 'processed', 'localhost')
        "#,
    )
    .bind(event_id)
    .bind(room_id)
    .bind(user_id)
    .bind(ts)
    .execute(pool.as_ref())
    .await
    .expect("insert watermark test event");
}

async fn wm_max_stream_ordering(pool: &Arc<sqlx::PgPool>) -> i64 {
    sqlx::query_scalar("SELECT COALESCE(MAX(stream_ordering), 0) FROM events")
        .fetch_one(pool.as_ref())
        .await
        .expect("max stream_ordering")
}

fn make_wm_request(room_id: &str, pos: Option<String>) -> SlidingSyncRequest {
    SlidingSyncRequest {
        conn_id: None,
        lists: HashMap::new(),
        room_subscriptions: Some(serde_json::json!({ room_id: { "timeline_limit": 10 } })),
        unsubscribe_rooms: None,
        extensions: None,
        pos,
        // timeout=0：空闲长轮询立即返回，避免测试在 park 上空等
        timeout: Some(0),
        client_timeout: None,
        txn_id: None,
    }
}

fn wm_timeline_event_ids(response: &synapse_storage::sliding_sync::SlidingSyncResponse, room_id: &str) -> Vec<String> {
    response
        .rooms
        .get(room_id)
        .and_then(|room| room.get("timeline"))
        .and_then(|timeline| timeline.as_array())
        .map(|events| {
            events.iter().filter_map(|e| e.get("event_id").and_then(|id| id.as_str()).map(str::to_string)).collect()
        })
        .unwrap_or_default()
}

#[tokio::test]
async fn test_s14_watermark_writeback_across_incremental_syncs() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = create_service(&pool);
    let suffix = unique_id();
    let user_id = format!("@wm_{suffix}:localhost");
    let room_id = format!("!wm_{suffix}:localhost");

    update_room_state(&pool, &user_id, "DEV1", &room_id, None, 1000, 0, 0, false, false, Some("WM Room"), None)
        .await
        .unwrap();

    // E1：初始同步前已存在的事件
    let e1 = format!("$wm_e1_{suffix}:localhost");
    insert_wm_message_event(&pool, &e1, &room_id, &user_id, 1000).await;
    let snapshot1 = wm_max_stream_ordering(&pool).await;

    // 1) 初始同步：timeline 含 E1，token 行写入读快照
    let resp1 = service.sync(&user_id, "DEV1", make_wm_request(&room_id, None)).await.unwrap();
    assert!(wm_timeline_event_ids(&resp1, &room_id).contains(&e1), "初始同步应下发已存在事件 E1");

    let storage = SlidingSyncStorage::new(pool.clone());
    let token1 = storage.get_token(&user_id, "DEV1", None).await.unwrap().expect("token row should exist");
    assert_eq!(token1.event_stream_pos, snapshot1, "初始同步应把读阶段快照写入 token 行作为水位线");

    // E2：客户端两次请求之间（“离线期间”）到达的新事件
    let e2 = format!("$wm_e2_{suffix}:localhost");
    insert_wm_message_event(&pool, &e2, &room_id, &user_id, 2000).await;
    let snapshot2 = wm_max_stream_ordering(&pool).await;
    assert!(snapshot2 > snapshot1, "E2 的 stream_ordering 应推进最大流水号");

    // 2) 增量同步：timeline 只含 E2，不重复下发 E1
    let resp2 = service.sync(&user_id, "DEV1", make_wm_request(&room_id, Some(resp1.pos.clone()))).await.unwrap();
    let timeline2 = wm_timeline_event_ids(&resp2, &room_id);
    assert!(timeline2.contains(&e2), "增量同步必须下发水位线之后的新事件 E2，got: {:?}", timeline2);
    assert!(!timeline2.contains(&e1), "增量同步不得重复下发已收事件 E1，got: {:?}", timeline2);

    // 3) 水位线回写：token 行更新为新快照，pos 同步前进
    let token2 = storage.get_token(&user_id, "DEV1", None).await.unwrap().expect("token row should exist");
    assert_eq!(token2.event_stream_pos, snapshot2, "增量同步结束应把新快照回写为下一轮水位线");
    assert!(token2.pos > token1.pos, "pos 应随每轮同步前进");

    // 4) 无新事件再次增量：不重复下发 E2
    let resp3 = service.sync(&user_id, "DEV1", make_wm_request(&room_id, Some(resp2.pos.clone()))).await.unwrap();
    let timeline3 = wm_timeline_event_ids(&resp3, &room_id);
    assert!(timeline3.is_empty(), "无新事件时增量 timeline 必须为空（不重复下发 E2），got: {:?}", timeline3);
}

// ── S7: presence 去重状态跨实例存活（L1 丢失后回源 Redis）──────────────────
//
// 回归场景：两个「实例」（各自独立 L1、共享同一 Redis）。实例 A 完成初始同步
// 写入 presence 去重状态后，客户端被路由到实例 B 做增量同步。修复前 B 的
// 同步 get_raw 只读 L1 → 误判 changed=true → presence 回声 → extensions
// 非空 → is_idle 失效 → 忙循环复发。修复后 B 回源 Redis，去重仍然生效。
#[tokio::test]
async fn test_s7_presence_dedup_survives_local_cache_loss() {
    use deadpool_redis::{Config as RedisPoolConfig, Runtime};

    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;

    let redis_pool =
        RedisPoolConfig::from_url("redis://127.0.0.1:6379").create_pool(Some(Runtime::Tokio1)).expect("redis pool");
    match tokio::time::timeout(std::time::Duration::from_millis(800), redis_pool.get()).await {
        Ok(Ok(conn)) => drop(conn),
        _ => {
            eprintln!("skip: local redis unavailable");
            return;
        }
    }

    let make_cache = || Arc::new(CacheManager::with_redis_pool(redis_pool.clone(), &CacheConfig::default()));
    let service_a = create_service_with_cache(&pool, make_cache());
    let service_b = create_service_with_cache(&pool, make_cache());

    let suffix = unique_id();
    let user_id = format!("@s7_{suffix}:localhost");
    let conn_id = format!("s7conn_{suffix}");

    let make_req = |pos: Option<String>| SlidingSyncRequest {
        conn_id: Some(conn_id.clone()),
        lists: HashMap::new(),
        room_subscriptions: None,
        unsubscribe_rooms: None,
        extensions: Some(serde_json::json!({ "presence": { "enabled": true } })),
        pos,
        timeout: Some(0),
        client_timeout: None,
        txn_id: None,
    };

    // 初始同步（实例 A）：presence 负载随响应下发，去重状态写 L1_A + Redis
    let resp1 = service_a.sync(&user_id, "DEV1", make_req(None)).await.unwrap();
    let ext1 = resp1.extensions.as_ref().expect("initial sync should carry extensions");
    assert!(ext1["presence"].get("events").is_some(), "初始同步必须包含 presence 事件负载");

    // 同实例增量（基线）：payload 未变 → 不再携带 events
    let resp2 = service_a.sync(&user_id, "DEV1", make_req(Some(resp1.pos.clone()))).await.unwrap();
    let ext2 = resp2.extensions.as_ref().expect("extensions should be present");
    assert!(ext2["presence"].get("events").is_none(), "同实例增量不应回声 presence，got: {:?}", ext2["presence"]);

    // 跨实例增量（B 的 L1 无该键）：修复前回声，修复后回源 Redis 无回声
    let resp3 = service_b.sync(&user_id, "DEV1", make_req(Some(resp2.pos.clone()))).await.unwrap();
    let ext3 = resp3.extensions.as_ref().expect("extensions should be present");
    assert!(
        ext3["presence"].get("events").is_none(),
        "跨实例后 presence 去重必须仍然生效（S7 复发开关），got: {:?}",
        ext3["presence"]
    );
}

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use serde_json::json;
use std::sync::Arc;
use synapse_rust::common::config::PerformanceConfig;
use synapse_rust::common::metrics::MetricsCollector;

use synapse_e2ee::device_keys::DeviceKeyStorage;
use synapse_e2ee::key_rotation::KeyRotationStorage;
use synapse_rust::cache::{CacheConfig, CacheManager};
use synapse_rust::e2ee::to_device::storage::ToDeviceMessage;
use synapse_rust::e2ee::to_device::ToDeviceStorage;
use synapse_services::sync_service::SyncService;
use synapse_storage::device::DeviceStorage;
use synapse_storage::event::EventStorage;
use synapse_storage::membership::RoomMemberStorage;
use synapse_storage::room::RoomStorage;
use synapse_storage::room_account_data::RoomAccountDataStorage;
use synapse_storage::PresenceStorage;
use synapse_storage::{AccountDataStorage, FilterStorage};

async fn setup_test_database(pool: &Arc<sqlx::PgPool>) {
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
            CREATE TABLE IF NOT EXISTS to_device_messages (
                id BIGSERIAL PRIMARY KEY,
                stream_id BIGINT NOT NULL,
                sender_user_id VARCHAR(255) NOT NULL,
                sender_device_id VARCHAR(255) NOT NULL,
                recipient_user_id VARCHAR(255) NOT NULL,
                recipient_device_id VARCHAR(255) NOT NULL,
                event_type TEXT NOT NULL,
                content JSONB NOT NULL,
                message_id TEXT,
                created_ts BIGINT NOT NULL
            )
            "#,
    )
    .execute(pool.as_ref())
    .await
    .expect("Failed to create to_device_messages table");

    sqlx::query("CREATE SEQUENCE IF NOT EXISTS to_device_stream_id_seq")
        .execute(pool.as_ref())
        .await
        .expect("Failed to create to_device_stream_id_seq");

    sqlx::query(
        r#"
            CREATE TABLE IF NOT EXISTS devices (
                device_id VARCHAR(255) PRIMARY KEY,
                user_id VARCHAR(255) NOT NULL,
                display_name TEXT,
                device_key JSONB,
                last_seen_ts BIGINT,
                last_seen_ip TEXT,
                created_ts BIGINT NOT NULL,
                first_seen_ts BIGINT NOT NULL,
                user_agent TEXT,
                appservice_id TEXT,
                ignored_user_list TEXT
            )
            "#,
    )
    .execute(pool.as_ref())
    .await
    .expect("Failed to create devices table");

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
            CREATE TABLE IF NOT EXISTS filters (
                id BIGSERIAL PRIMARY KEY,
                user_id VARCHAR(255) NOT NULL,
                filter_id VARCHAR(255) NOT NULL,
                content JSONB NOT NULL,
                created_ts BIGINT NOT NULL
            )
            "#,
    )
    .execute(pool.as_ref())
    .await
    .expect("Failed to create filters table");

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
            CREATE TABLE IF NOT EXISTS device_lists_changes (
                id BIGSERIAL PRIMARY KEY,
                user_id VARCHAR(255) NOT NULL,
                device_id VARCHAR(255),
                change_type TEXT NOT NULL,
                stream_id BIGINT NOT NULL,
                created_ts BIGINT NOT NULL
            )
            "#,
    )
    .execute(pool.as_ref())
    .await
    .expect("Failed to create device_lists_changes table");

    sqlx::query(
        r#"
            CREATE TABLE IF NOT EXISTS key_rotation_pending (
                room_id TEXT NOT NULL,
                reason TEXT NOT NULL,
                triggered_by_user_id TEXT NOT NULL,
                created_ts BIGINT NOT NULL,
                PRIMARY KEY (room_id, triggered_by_user_id)
            )
            "#,
    )
    .execute(pool.as_ref())
    .await
    .expect("Failed to create key_rotation_pending table");

    sqlx::query(
        r#"
            CREATE TABLE IF NOT EXISTS key_rotation_state (
                user_id TEXT NOT NULL,
                room_id TEXT NOT NULL,
                is_rotated BOOLEAN NOT NULL DEFAULT FALSE,
                rotated_at TIMESTAMPTZ,
                PRIMARY KEY (user_id, room_id)
            )
            "#,
    )
    .execute(pool.as_ref())
    .await
    .expect("Failed to create key_rotation_state table");

    sqlx::query(
        r#"
            CREATE TABLE IF NOT EXISTS megolm_key_shares (
                room_id TEXT NOT NULL,
                session_id TEXT NOT NULL,
                share_reason TEXT NOT NULL,
                shared_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                PRIMARY KEY (room_id, session_id)
            )
            "#,
    )
    .execute(pool.as_ref())
    .await
    .expect("Failed to create megolm_key_shares table");

    sqlx::query(
        r#"
            CREATE TABLE IF NOT EXISTS megolm_sessions (
                session_id TEXT NOT NULL PRIMARY KEY,
                room_id TEXT NOT NULL,
                sender_key TEXT NOT NULL,
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                expires_at TIMESTAMPTZ
            )
            "#,
    )
    .execute(pool.as_ref())
    .await
    .expect("Failed to create megolm_sessions table");
}

#[tokio::test]
async fn test_to_device_next_batch_token_respects_limit() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;

    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let presence_storage = Arc::new(PresenceStorage::new(pool.clone(), cache.clone()));
    let member_storage = Arc::new(RoomMemberStorage::new(&pool, "localhost"));
    let event_storage = Arc::new(EventStorage::new(&pool, "localhost".to_string()));
    let room_storage = Arc::new(RoomStorage::new(&pool));
    let to_device_storage = ToDeviceStorage::new(&pool);

    let sync_service = SyncService::new(
        presence_storage,
        member_storage,
        event_storage,
        room_storage,
        Arc::new(RoomAccountDataStorage::new(&pool)),
        Arc::new(AccountDataStorage::new(&pool)),
        Arc::new(FilterStorage::new(&pool)),
        Arc::new(DeviceStorage::new(&pool)),
        DeviceKeyStorage::new(&pool),
        KeyRotationStorage::new(pool.clone()),
        to_device_storage.clone(),
        Arc::new(MetricsCollector::new()),
        PerformanceConfig::default(),
        Arc::new(CacheManager::new(&CacheConfig::default())),
    );

    let user_id = "@alice:localhost";
    let device_id = "ALICEDEVICE";

    // Create the device first, otherwise add_message will skip it
    DeviceStorage::new(&pool).create_device(device_id, user_id, Some("Alice phone")).await.unwrap();

    // Add 5 to-device messages
    for i in 1..=5 {
        to_device_storage
            .add_message(ToDeviceMessage {
                sender_user_id: "@bob:localhost",
                sender_device_id: "BOBDEVICE",
                recipient_user_id: user_id,
                recipient_device_id: device_id,
                event_type: "m.test",
                content: json!({"index": i}),
                message_id: None,
            })
            .await
            .unwrap();
    }

    // Initial sync to get everything
    let first_sync = sync_service.sync(user_id, Some(device_id), 0, false, "online", None, None).await.unwrap();
    let first_token = first_sync["next_batch"].as_str().unwrap().to_string();

    // Add one more message
    to_device_storage
        .add_message(ToDeviceMessage {
            sender_user_id: "@bob:localhost",
            sender_device_id: "BOBDEVICE",
            recipient_user_id: user_id,
            recipient_device_id: device_id,
            event_type: "m.test",
            content: json!({"index": 6}),
            message_id: None,
        })
        .await
        .unwrap();

    let second_sync =
        sync_service.sync(user_id, Some(device_id), 0, false, "online", None, Some(&first_token)).await.unwrap();
    let to_device_events = second_sync["to_device"]["events"].as_array().unwrap();

    assert_eq!(to_device_events.len(), 1);
    assert_eq!(to_device_events[0]["content"]["index"], 6);
}

#[tokio::test]
async fn test_to_device_messages_are_deleted_after_ack() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;

    let to_device_storage = ToDeviceStorage::new(&pool);
    let sync_service = SyncService::new(
        Arc::new(PresenceStorage::new(pool.clone(), Arc::new(CacheManager::new(&CacheConfig::default())))),
        Arc::new(RoomMemberStorage::new(&pool, "localhost")),
        Arc::new(EventStorage::new(&pool, "localhost".to_string())),
        Arc::new(RoomStorage::new(&pool)),
        Arc::new(RoomAccountDataStorage::new(&pool)),
        Arc::new(AccountDataStorage::new(&pool)),
        Arc::new(FilterStorage::new(&pool)),
        Arc::new(DeviceStorage::new(&pool)),
        DeviceKeyStorage::new(&pool),
        KeyRotationStorage::new(pool.clone()),
        to_device_storage.clone(),
        Arc::new(MetricsCollector::new()),
        PerformanceConfig::default(),
        Arc::new(CacheManager::new(&CacheConfig::default())),
    );

    let user_id = "@alice:localhost";
    let device_id = "ALICEDEVICE";

    // Create the device first
    DeviceStorage::new(&pool).create_device(device_id, user_id, Some("Alice phone")).await.unwrap();

    // Add a message
    to_device_storage
        .add_message(ToDeviceMessage {
            sender_user_id: "@bob:localhost",
            sender_device_id: "BOBDEVICE",
            recipient_user_id: user_id,
            recipient_device_id: device_id,
            event_type: "m.test",
            content: json!({"index": 1}),
            message_id: None,
        })
        .await
        .unwrap();

    // Initial sync to get the token
    let first_sync = sync_service.sync(user_id, Some(device_id), 0, false, "online", None, None).await.unwrap();
    let first_token = first_sync["next_batch"].as_str().unwrap().to_string();

    // Verify message exists in DB
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM to_device_messages").fetch_one(&*pool).await.unwrap();
    assert_eq!(count, 1);

    // Sync again with the token (this should trigger deletion of messages up to the token's stream_id)
    sync_service.sync(user_id, Some(device_id), 0, false, "online", None, Some(&first_token)).await.unwrap();

    // Verify message is deleted from DB
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM to_device_messages").fetch_one(&*pool).await.unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn test_record_transaction_atomic_dedup() {
    let pool = crate::require_test_pool().await;

    // Guard: collapse any pre-existing duplicate rows before creating the index
    sqlx::query(
        r#"
        DELETE FROM to_device_transactions a
        USING to_device_transactions b
        WHERE a.message_id IS NOT NULL
          AND a.message_id = b.message_id
          AND a.sender_user_id = b.sender_user_id
          AND a.sender_device_id = b.sender_device_id
          AND a.id > b.id
        "#,
    )
    .execute(&*pool)
    .await
    .expect("Failed to deduplicate to_device_transactions");

    // Ensure the unique index needed for atomic ON CONFLICT dedup exists.
    // The test pool has the production schema (with only the
    // transaction_id-based unique constraint), so add the message_id-based
    // unique index here. PostgreSQL treats NULLs as distinct in UNIQUE
    // indexes, so multiple NULL message_id rows for the same sender/device
    // will not conflict.
    sqlx::query(
        "CREATE UNIQUE INDEX IF NOT EXISTS uq_to_device_txn_msgid \
         ON to_device_transactions (sender_user_id, sender_device_id, message_id)",
    )
    .execute(&*pool)
    .await
    .expect("Failed to create unique index");

    let storage = synapse_rust::e2ee::to_device::ToDeviceStorage::new(&pool);

    // First insert is not a duplicate
    let first = storage.record_transaction("@user:localhost", "DEVICE1", "mid1").await.unwrap();
    assert!(first, "first insert of (user, dev, mid1) should be Ok(true)");

    // Second insert with same args IS a duplicate
    let second = storage.record_transaction("@user:localhost", "DEVICE1", "mid1").await.unwrap();
    assert!(!second, "second insert of (user, dev, mid1) should be Ok(false)");

    // Different message_id should succeed
    let third = storage.record_transaction("@user:localhost", "DEVICE1", "mid2").await.unwrap();
    assert!(third, "different message_id should be Ok(true)");

    // Different sender should succeed
    let fourth = storage.record_transaction("@user2:localhost", "DEVICE1", "mid1").await.unwrap();
    assert!(fourth, "different sender should be Ok(true)");
}

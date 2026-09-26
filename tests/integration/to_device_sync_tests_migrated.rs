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
use synapse_storage::room_account_data::RoomAccountDataStorage;
use synapse_storage::PresenceStorage;
use synapse_storage::{AccountDataStorage, FilterStorage};

#[tokio::test]
async fn test_to_device_next_batch_token_respects_limit() {
    let pool = crate::require_test_pool().await;

    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let presence_storage = Arc::new(PresenceStorage::new(pool.clone(), cache.clone()));
    let member_storage = Arc::new(RoomMemberStorage::new(&pool, "localhost"));
    let event_storage = Arc::new(EventStorage::new(&pool, "localhost".to_string()));
    let to_device_storage = ToDeviceStorage::new(&pool);

    let sync_service = SyncService::new(
        presence_storage,
        member_storage,
        event_storage,
        Arc::new(RoomAccountDataStorage::new(&pool)),
        Arc::new(AccountDataStorage::new(&pool)),
        Arc::new(FilterStorage::new(&pool)),
        Arc::new(DeviceStorage::new(&pool)),
        Arc::new(DeviceKeyStorage::new(&pool)) as Arc<dyn synapse_e2ee::device_keys::DeviceKeyStoreApi>,
        KeyRotationStorage::new(pool.clone()),
        to_device_storage.clone(),
        Arc::new(MetricsCollector::new()),
        PerformanceConfig::default(),
        Arc::new(CacheManager::new(&CacheConfig::default())),
        None,
    );

    let user_id = "@alice:localhost";
    let device_id = "ALICEDEVICE";

    // `devices` is FK-bound to `users`, so the owning user must exist first.
    crate::ensure_test_user(&pool, user_id).await;

    // Create the device first, otherwise add_message will skip it
    crate::ensure_test_user(&pool, user_id).await;
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
    let first_sync = sync_service.sync(user_id, Some(device_id), 0, false, "online", None, None, None).await.unwrap();
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
        sync_service.sync(user_id, Some(device_id), 0, false, "online", None, Some(&first_token), None).await.unwrap();
    let to_device_events = second_sync["to_device"]["events"].as_array().unwrap();

    assert_eq!(to_device_events.len(), 1);
    assert_eq!(to_device_events[0]["content"]["index"], 6);
}

#[tokio::test]
async fn test_to_device_messages_are_deleted_after_ack() {
    let pool = crate::require_test_pool().await;

    let to_device_storage = ToDeviceStorage::new(&pool);
    let sync_service = SyncService::new(
        Arc::new(PresenceStorage::new(pool.clone(), Arc::new(CacheManager::new(&CacheConfig::default())))),
        Arc::new(RoomMemberStorage::new(&pool, "localhost")),
        Arc::new(EventStorage::new(&pool, "localhost".to_string())),
        Arc::new(RoomAccountDataStorage::new(&pool)),
        Arc::new(AccountDataStorage::new(&pool)),
        Arc::new(FilterStorage::new(&pool)),
        Arc::new(DeviceStorage::new(&pool)),
        Arc::new(DeviceKeyStorage::new(&pool)) as Arc<dyn synapse_e2ee::device_keys::DeviceKeyStoreApi>,
        KeyRotationStorage::new(pool.clone()),
        to_device_storage.clone(),
        Arc::new(MetricsCollector::new()),
        PerformanceConfig::default(),
        Arc::new(CacheManager::new(&CacheConfig::default())),
        None,
    );

    let user_id = "@alice:localhost";
    let device_id = "ALICEDEVICE";

    // `devices` is FK-bound to `users`, so the owning user must exist first.
    crate::ensure_test_user(&pool, user_id).await;

    // Create the device first
    crate::ensure_test_user(&pool, user_id).await;
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
    let first_sync = sync_service.sync(user_id, Some(device_id), 0, false, "online", None, None, None).await.unwrap();
    let first_token = first_sync["next_batch"].as_str().unwrap().to_string();

    // Verify message exists in DB
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM to_device_messages").fetch_one(&*pool).await.unwrap();
    assert_eq!(count, 1);

    // Sync again with the token (this should trigger deletion of messages up to the token's stream_id)
    sync_service.sync(user_id, Some(device_id), 0, false, "online", None, Some(&first_token), None).await.unwrap();

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

/// E2EE-10: To-device messages must be delivered in stream_id order.
///
/// This test inserts messages with out-of-order stream_ids (bypassing
/// nextval) to simulate a scenario where physical insertion order does
/// not match stream_id order. Without `ORDER BY stream_id ASC` in
/// `get_and_delete_messages`, PostgreSQL's `DELETE ... RETURNING`
/// returns rows in unspecified order — potentially delivering messages
/// out of sequence, which causes race conditions in key exchange.
#[tokio::test]
async fn test_to_device_messages_ordered_by_stream_id() {
    let pool = crate::require_test_pool().await;

    let to_device_storage = ToDeviceStorage::new(&pool);

    let user_id = "@alice:localhost";
    let device_id = "ALICEDEVICE";

    // Create the device first, otherwise add_message would skip it
    crate::ensure_test_user(&pool, user_id).await;
    DeviceStorage::new(&pool).create_device(device_id, user_id, Some("Alice phone")).await.unwrap();

    // Insert messages with out-of-order stream_ids to simulate concurrent
    // or reordered insertion. Direct SQL bypasses nextval() to create a
    // scenario where physical insertion order does not match stream_id order.
    let now = chrono::Utc::now().timestamp_millis();
    for (stream_id, seq) in [(30i64, 3i64), (10, 1), (20, 2)] {
        sqlx::query(
            r#"
            INSERT INTO to_device_messages (
                sender_user_id, sender_device_id,
                recipient_user_id, recipient_device_id,
                event_type, content, message_id, stream_id, created_ts
            )
            VALUES ($1, $2, $3, $4, $5, $6, NULL, $7, $8)
            "#,
        )
        .bind("@bob:localhost")
        .bind("BOBDEVICE")
        .bind(user_id)
        .bind(device_id)
        .bind("m.room_key")
        .bind(serde_json::json!({"seq": seq}))
        .bind(stream_id)
        .bind(now)
        .execute(&*pool)
        .await
        .unwrap();
    }

    // Fetch and delete messages — they must be returned in stream_id order
    let messages = to_device_storage.get_and_delete_messages(user_id, device_id).await.unwrap();

    // Messages must be returned in stream_id order: seq 1, 2, 3
    assert_eq!(messages.len(), 3, "expected 3 to-device messages");
    assert_eq!(messages[0]["content"]["seq"], 1, "first message must have seq=1 (stream_id=10)");
    assert_eq!(messages[1]["content"]["seq"], 2, "second message must have seq=2 (stream_id=20)");
    assert_eq!(messages[2]["content"]["seq"], 3, "third message must have seq=3 (stream_id=30)");
}

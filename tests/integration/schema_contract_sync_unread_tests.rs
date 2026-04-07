#![cfg(test)]

#[path = "../common/mod.rs"]
mod common;

use serde_json::json;
use std::sync::Arc;
use synapse_rust::cache::{CacheConfig, CacheManager};
use synapse_rust::services::sync_service::SyncService;
use synapse_rust::storage::device::DeviceStorage;
use synapse_rust::storage::event::EventStorage;
use synapse_rust::storage::membership::RoomMemberStorage;
use synapse_rust::storage::presence::PresenceStorage;
use synapse_rust::storage::room::RoomStorage;

async fn connect_pool() -> Option<Arc<sqlx::PgPool>> {
    match common::get_test_pool_async().await {
        Ok(pool) => Some(pool),
        Err(error) => {
            eprintln!(
                "Skipping sync unread schema contract integration tests because test database is unavailable: {}",
                error
            );
            None
        }
    }
}

async fn seed_fixtures(pool: &sqlx::PgPool, suffix: &str) -> (String, String, String, Vec<String>) {
    let reader_user_id = format!("@schema-sync-unread-reader-{suffix}:localhost");
    let sender_user_id = format!("@schema-sync-unread-sender-{suffix}:localhost");
    let room_id = format!("!schema-sync-unread-room-{suffix}:localhost");

    for (user_id, username) in [
        (
            &reader_user_id,
            format!("schema_sync_unread_reader_{suffix}"),
        ),
        (
            &sender_user_id,
            format!("schema_sync_unread_sender_{suffix}"),
        ),
    ] {
        sqlx::query(
            "INSERT INTO users (user_id, username, created_ts) VALUES ($1, $2, $3) ON CONFLICT (user_id) DO NOTHING",
        )
        .bind(user_id)
        .bind(username)
        .bind(0_i64)
        .execute(pool)
        .await
        .expect("Failed to seed user fixture");
    }

    sqlx::query(
        "INSERT INTO rooms (room_id, creator, created_ts) VALUES ($1, $2, $3) ON CONFLICT (room_id) DO NOTHING",
    )
    .bind(&room_id)
    .bind(&reader_user_id)
    .bind(0_i64)
    .execute(pool)
    .await
    .expect("Failed to seed room fixture");

    let read_event_id = format!("$schema-sync-unread-read-{suffix}");
    let unread_event_mention_id = format!("$schema-sync-unread-mention-{suffix}");
    let unread_event_room_id = format!("$schema-sync-unread-room-{suffix}");
    let self_event_id = format!("$schema-sync-unread-self-{suffix}");

    let sender_events = [
        (
            &read_event_id,
            100_i64,
            json!({ "msgtype": "m.text", "body": "baseline" }),
        ),
        (
            &unread_event_mention_id,
            200_i64,
            json!({ "msgtype": "m.text", "body": format!("hello {}", reader_user_id) }),
        ),
        (
            &unread_event_room_id,
            300_i64,
            json!({ "msgtype": "m.text", "body": "hello @room" }),
        ),
    ];

    for (event_id, ts, content) in sender_events {
        sqlx::query(
            r#"
            INSERT INTO events (event_id, room_id, sender, user_id, event_type, content, origin_server_ts)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (event_id) DO NOTHING
            "#,
        )
        .bind(event_id)
        .bind(&room_id)
        .bind(&sender_user_id)
        .bind(&sender_user_id)
        .bind("m.room.message")
        .bind(content)
        .bind(ts)
        .execute(pool)
        .await
        .expect("Failed to seed sender event fixture");
    }

    sqlx::query(
        r#"
        INSERT INTO events (event_id, room_id, sender, user_id, event_type, content, origin_server_ts)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        ON CONFLICT (event_id) DO NOTHING
        "#,
    )
    .bind(&self_event_id)
    .bind(&room_id)
    .bind(&reader_user_id)
    .bind(&reader_user_id)
    .bind("m.room.message")
    .bind(json!({ "msgtype": "m.text", "body": "self message @room" }))
    .bind(400_i64)
    .execute(pool)
    .await
    .expect("Failed to seed self event fixture");

    (
        reader_user_id,
        sender_user_id,
        room_id,
        vec![
            read_event_id,
            unread_event_mention_id,
            unread_event_room_id,
            self_event_id,
        ],
    )
}

async fn cleanup_fixtures(
    pool: &sqlx::PgPool,
    room_id: &str,
    user_ids: [&str; 2],
    event_ids: &[String],
) {
    sqlx::query("DELETE FROM read_markers WHERE room_id = $1")
        .bind(room_id)
        .execute(pool)
        .await
        .ok();

    for event_id in event_ids {
        sqlx::query("DELETE FROM events WHERE event_id = $1")
            .bind(event_id)
            .execute(pool)
            .await
            .ok();
    }

    sqlx::query("DELETE FROM rooms WHERE room_id = $1")
        .bind(room_id)
        .execute(pool)
        .await
        .ok();

    for user_id in user_ids {
        sqlx::query("DELETE FROM users WHERE user_id = $1")
            .bind(user_id)
            .execute(pool)
            .await
            .ok();
    }
}

#[tokio::test]
async fn test_schema_contract_sync_unread_counts_uses_read_markers_and_excludes_self_events() {
    let pool = match connect_pool().await {
        Some(pool) => pool,
        None => return,
    };

    let suffix = uuid::Uuid::new_v4().to_string();
    let (reader_user_id, sender_user_id, room_id, event_ids) = seed_fixtures(&pool, &suffix).await;

    let cache = Arc::new(CacheManager::new(CacheConfig::default()));
    let presence_storage = PresenceStorage::new(pool.clone(), cache);
    let member_storage = RoomMemberStorage::new(&pool, "localhost");
    let event_storage = EventStorage::new(&pool, "localhost".to_string());
    let room_storage = RoomStorage::new(&pool);
    let device_storage = DeviceStorage::new(&pool);

    room_storage
        .update_read_marker(&room_id, &reader_user_id, &event_ids[0])
        .await
        .expect("Failed to write read marker fixture");

    let service = SyncService::new(
        presence_storage,
        member_storage,
        event_storage,
        room_storage,
        device_storage,
    );

    let (notification_count, highlight_count) = service
        .room_unread_counts(&room_id, &reader_user_id)
        .await
        .expect("Failed to compute unread counts");

    assert_eq!(notification_count, 2);
    assert_eq!(highlight_count, 2);

    cleanup_fixtures(
        &pool,
        &room_id,
        [reader_user_id.as_str(), sender_user_id.as_str()],
        &event_ids,
    )
    .await;
}

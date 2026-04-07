#![cfg(test)]

#[path = "../common/mod.rs"]
mod common;

use serde_json::json;
use std::sync::Arc;
use synapse_rust::cache::{CacheConfig, CacheManager};
use synapse_rust::services::sync_service::SyncService;
use synapse_rust::storage::device::DeviceStorage;
use synapse_rust::storage::event::{CreateEventParams, EventStorage};
use synapse_rust::storage::membership::RoomMemberStorage;
use synapse_rust::storage::presence::PresenceStorage;
use synapse_rust::storage::room::RoomStorage;

async fn connect_pool() -> Option<Arc<sqlx::PgPool>> {
    match common::get_test_pool_async().await {
        Ok(pool) => Some(pool),
        Err(error) => {
            eprintln!(
                "Skipping sync ephemeral receipts schema contract integration tests because test database is unavailable: {}",
                error
            );
            None
        }
    }
}

async fn seed_fixtures(pool: &sqlx::PgPool, suffix: &str) -> (String, String, String, String, i64) {
    let user_id = format!("@schema-sync-ephemeral-receipts-user-{suffix}:localhost");
    let room_id = format!("!schema-sync-ephemeral-receipts-room-{suffix}:localhost");
    let receipt_event_id = format!("$schema-sync-ephemeral-receipts-receipt-{suffix}");
    let typing_user_id = format!("@schema-sync-ephemeral-receipts-typing-{suffix}:localhost");

    for (uid, username) in [
        (
            &user_id,
            format!("schema_sync_ephemeral_receipts_user_{suffix}"),
        ),
        (
            &typing_user_id,
            format!("schema_sync_ephemeral_receipts_typing_{suffix}"),
        ),
    ] {
        sqlx::query(
            "INSERT INTO users (user_id, username, created_ts) VALUES ($1, $2, $3) ON CONFLICT (user_id) DO NOTHING",
        )
        .bind(uid)
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
    .bind(&user_id)
    .bind(0_i64)
    .execute(pool)
    .await
    .expect("Failed to seed room fixture");

    let stream_id = chrono::Utc::now().timestamp_millis();
    sqlx::query(
        r#"
        INSERT INTO room_ephemeral (room_id, event_type, user_id, content, stream_id, created_ts, expires_ts)
        VALUES ($1, $2, $3, $4, $5, $5, NULL)
        "#,
    )
    .bind(&room_id)
    .bind("m.typing")
    .bind(&typing_user_id)
    .bind(json!({ "user_ids": [typing_user_id.clone()] }))
    .bind(stream_id)
    .execute(pool)
    .await
    .expect("Failed to seed room_ephemeral fixture");

    (
        user_id,
        typing_user_id,
        room_id,
        receipt_event_id,
        stream_id,
    )
}

async fn cleanup_fixtures(
    pool: &sqlx::PgPool,
    room_id: &str,
    user_ids: [&str; 2],
    receipt_event_id: &str,
) {
    sqlx::query("DELETE FROM room_ephemeral WHERE room_id = $1")
        .bind(room_id)
        .execute(pool)
        .await
        .ok();

    sqlx::query("DELETE FROM events WHERE room_id = $1 OR event_id = $2")
        .bind(room_id)
        .bind(receipt_event_id)
        .execute(pool)
        .await
        .ok();

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
async fn test_schema_contract_sync_room_sync_includes_ephemeral_from_room_ephemeral_and_receipt_as_state_event(
) {
    let pool = match connect_pool().await {
        Some(pool) => pool,
        None => return,
    };

    let suffix = uuid::Uuid::new_v4().to_string();
    let (user_id, typing_user_id, room_id, receipt_event_id, _stream_id) =
        seed_fixtures(&pool, &suffix).await;

    let cache = Arc::new(CacheManager::new(CacheConfig::default()));
    let presence_storage = PresenceStorage::new(pool.clone(), cache);
    let member_storage = RoomMemberStorage::new(&pool, "localhost");
    let event_storage = EventStorage::new(&pool, "localhost".to_string());
    let room_storage = RoomStorage::new(&pool);
    let device_storage = DeviceStorage::new(&pool);
    let service = SyncService::new(
        presence_storage,
        member_storage,
        event_storage.clone(),
        room_storage,
        device_storage,
    );

    event_storage
        .create_event(
            CreateEventParams {
                event_id: receipt_event_id.clone(),
                room_id: room_id.clone(),
                user_id: user_id.clone(),
                event_type: "m.receipt".to_string(),
                content: json!({ "dummy": true }),
                state_key: Some(user_id.clone()),
                origin_server_ts: chrono::Utc::now().timestamp_millis(),
            },
            None,
        )
        .await
        .expect("Failed to seed m.receipt state event");

    let response = service
        .room_sync(&user_id, &room_id, 0, false, None)
        .await
        .expect("Failed to build room sync response");

    let state_events = response
        .get("state")
        .and_then(|v| v.get("events"))
        .and_then(|v| v.as_array())
        .expect("Expected state.events array");
    assert!(state_events.iter().any(|e| {
        e.get("type").and_then(|v| v.as_str()) == Some("m.receipt")
            && e.get("state_key").and_then(|v| v.as_str()) == Some(user_id.as_str())
    }));

    let ephemeral_events = response
        .get("ephemeral")
        .and_then(|v| v.get("events"))
        .and_then(|v| v.as_array())
        .expect("Expected ephemeral.events array");
    assert!(ephemeral_events.iter().any(|e| {
        e.get("type").and_then(|v| v.as_str()) == Some("m.typing")
            && e.get("content")
                .and_then(|c| c.get("user_ids"))
                .and_then(|u| u.as_array())
                .is_some_and(|arr| {
                    arr.iter()
                        .any(|x| x.as_str() == Some(typing_user_id.as_str()))
                })
    }));
    assert!(!ephemeral_events
        .iter()
        .any(|e| e.get("type").and_then(|v| v.as_str()) == Some("m.receipt")));

    cleanup_fixtures(
        &pool,
        &room_id,
        [user_id.as_str(), typing_user_id.as_str()],
        &receipt_event_id,
    )
    .await;
}

#![cfg(test)]

#[path = "../common/mod.rs"]
mod common;

use serde_json::json;
use std::sync::Arc;
use synapse_rust::storage::room::RoomStorage;
use synapse_rust::web::routes::handlers::room::write_read_markers_from_body;

async fn connect_pool() -> Option<Arc<sqlx::PgPool>> {
    match common::get_test_pool_async().await {
        Ok(pool) => Some(pool),
        Err(error) => {
            eprintln!(
                "Skipping read markers handler schema contract integration tests because test database is unavailable: {}",
                error
            );
            None
        }
    }
}

async fn seed_user_room_events(
    pool: &sqlx::PgPool,
    suffix: &str,
) -> (String, String, String, String) {
    let user_id = format!("@schema-read-markers-user-{suffix}:localhost");
    let room_id = format!("!schema-read-markers-room-{suffix}:localhost");
    let event_id_1 = format!("$schema-read-markers-event-1-{suffix}");
    let event_id_2 = format!("$schema-read-markers-event-2-{suffix}");

    sqlx::query(
        "INSERT INTO users (user_id, username, created_ts) VALUES ($1, $2, $3) ON CONFLICT (user_id) DO NOTHING",
    )
    .bind(&user_id)
    .bind(format!("schema_read_markers_user_{suffix}"))
    .bind(0_i64)
    .execute(pool)
    .await
    .expect("Failed to seed user fixture");

    sqlx::query(
        "INSERT INTO rooms (room_id, creator, created_ts) VALUES ($1, $2, $3) ON CONFLICT (room_id) DO NOTHING",
    )
    .bind(&room_id)
    .bind(&user_id)
    .bind(0_i64)
    .execute(pool)
    .await
    .expect("Failed to seed room fixture");

    for event_id in [&event_id_1, &event_id_2] {
        sqlx::query(
            r#"
            INSERT INTO events (event_id, room_id, sender, event_type, content, origin_server_ts, user_id)
            VALUES ($1, $2, $3, 'm.room.message', '{}'::jsonb, $4, $3)
            ON CONFLICT (event_id) DO NOTHING
            "#,
        )
        .bind(event_id)
        .bind(&room_id)
        .bind(&user_id)
        .bind(0_i64)
        .execute(pool)
        .await
        .expect("Failed to seed event fixture");
    }

    (user_id, room_id, event_id_1, event_id_2)
}

async fn cleanup_fixtures(pool: &sqlx::PgPool, user_id: &str, room_id: &str, event_ids: &[&str]) {
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

    sqlx::query("DELETE FROM users WHERE user_id = $1")
        .bind(user_id)
        .execute(pool)
        .await
        .ok();
}

#[tokio::test]
async fn test_schema_contract_read_markers_handler_m_read_updates_m_fully_read_marker() {
    let pool = match connect_pool().await {
        Some(pool) => pool,
        None => return,
    };

    let suffix = uuid::Uuid::new_v4().to_string();
    let (user_id, room_id, event_id_1, event_id_2) = seed_user_room_events(&pool, &suffix).await;

    let storage = RoomStorage::new(&pool);

    write_read_markers_from_body(
        &storage,
        &room_id,
        &user_id,
        &json!({
            "m.read": event_id_1,
            "m.fully_read": event_id_2,
            "m.private_read": "not-an-event-id",
            "m.marked_unread": { "events": ["not-an-event-id", event_id_2] }
        }),
    )
    .await
    .expect("Failed to write read markers");

    let fully_read = storage
        .get_read_marker(&room_id, &user_id, "m.fully_read")
        .await
        .expect("Failed to fetch m.fully_read marker");
    assert_eq!(fully_read, Some(event_id_1.clone()));

    let read = storage
        .get_read_marker(&room_id, &user_id, "m.read")
        .await
        .expect("Failed to fetch m.read marker");
    assert_eq!(read, None);

    let private_read = storage
        .get_read_marker(&room_id, &user_id, "m.private_read")
        .await
        .expect("Failed to fetch m.private_read marker");
    assert_eq!(private_read, None);

    let marked_unread = storage
        .get_read_marker(&room_id, &user_id, "m.marked_unread")
        .await
        .expect("Failed to fetch m.marked_unread marker");
    assert_eq!(marked_unread, Some(event_id_2.clone()));

    cleanup_fixtures(&pool, &user_id, &room_id, &[&event_id_1, &event_id_2]).await;
}

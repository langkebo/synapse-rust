#![cfg(test)]

#[path = "../common/mod.rs"]
mod common;

use serde_json::{json, Value};
use std::collections::HashSet;
use std::sync::Arc;
use synapse_rust::storage::room::RoomStorage;
use synapse_rust::web::routes::handlers::room::build_receipts_chunk;

async fn connect_pool() -> Option<Arc<sqlx::PgPool>> {
    match common::get_test_pool_async().await {
        Ok(pool) => Some(pool),
        Err(error) => {
            eprintln!(
                "Skipping receipts aggregation schema contract integration tests because test database is unavailable: {}",
                error
            );
            None
        }
    }
}

async fn seed_receipts_fixtures(
    pool: &sqlx::PgPool,
    suffix: &str,
) -> (String, String, String, String, String, Vec<String>) {
    let user_id_1 = format!("@schema-receipts-agg-user-1-{suffix}:localhost");
    let user_id_2 = format!("@schema-receipts-agg-user-2-{suffix}:localhost");
    let room_id = format!("!schema-receipts-agg-room-{suffix}:localhost");
    let event_id = format!("$schema-receipts-agg-event-{suffix}");

    for (user_id, username) in [
        (&user_id_1, format!("schema_receipts_agg_user_1_{suffix}")),
        (&user_id_2, format!("schema_receipts_agg_user_2_{suffix}")),
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
    .bind(&user_id_1)
    .bind(0_i64)
    .execute(pool)
    .await
    .expect("Failed to seed room fixture");

    sqlx::query(
        r#"
        INSERT INTO events (event_id, room_id, sender, event_type, content, origin_server_ts, user_id)
        VALUES ($1, $2, $3, 'm.room.message', '{}'::jsonb, $4, $3)
        ON CONFLICT (event_id) DO NOTHING
        "#,
    )
    .bind(&event_id)
    .bind(&room_id)
    .bind(&user_id_1)
    .bind(0_i64)
    .execute(pool)
    .await
    .expect("Failed to seed event fixture");

    let user_ids = vec![user_id_1.clone(), user_id_2.clone()];
    (
        user_id_1.clone(),
        user_id_2.clone(),
        room_id,
        event_id,
        "m.read".to_string(),
        user_ids,
    )
}

async fn cleanup_receipts_agg_fixtures(
    pool: &sqlx::PgPool,
    room_id: &str,
    event_id: &str,
    user_ids: &[String],
) {
    sqlx::query("DELETE FROM event_receipts WHERE room_id = $1")
        .bind(room_id)
        .execute(pool)
        .await
        .ok();

    sqlx::query("DELETE FROM events WHERE event_id = $1")
        .bind(event_id)
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
async fn test_schema_contract_receipts_get_receipts_chunk_aggregation_contract() {
    let Some(pool) = connect_pool().await else {
        return;
    };

    let suffix = uuid::Uuid::new_v4().to_string();
    let (user_id_1, user_id_2, room_id, event_id, receipt_type, user_ids) =
        seed_receipts_fixtures(&pool, &suffix).await;

    let storage = RoomStorage::new(&pool);
    storage
        .add_receipt(&user_id_1, &user_id_1, &room_id, &event_id, &receipt_type)
        .await
        .expect("Failed to seed receipt for user 1");
    storage
        .add_receipt(&user_id_2, &user_id_2, &room_id, &event_id, &receipt_type)
        .await
        .expect("Failed to seed receipt for user 2");

    let receipts = storage
        .get_receipts(&room_id, &receipt_type, &event_id)
        .await
        .expect("Failed to get seeded receipts");
    let json = build_receipts_chunk(receipts);
    let chunk = json
        .get("chunk")
        .and_then(|v| v.as_array())
        .expect("Expected response to include chunk array");
    assert_eq!(chunk.len(), 2);

    let mut seen_users = HashSet::new();
    for item in chunk {
        assert_eq!(item.get("event_id"), Some(&Value::String(event_id.clone())));
        assert_eq!(
            item.get("receipt_type"),
            Some(&Value::String(receipt_type.clone()))
        );
        assert_eq!(item.get("data"), Some(&json!({})));
        let ts = item.get("ts").and_then(|v| v.as_i64()).unwrap_or(0);
        assert!(ts > 0);
        let user_id = item.get("user_id").and_then(|v| v.as_str()).unwrap_or("");
        seen_users.insert(user_id.to_string());
    }
    assert_eq!(
        seen_users,
        HashSet::from_iter([user_id_1.clone(), user_id_2.clone()])
    );

    cleanup_receipts_agg_fixtures(&pool, &room_id, &event_id, &user_ids).await;
}

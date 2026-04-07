#![cfg(test)]

#[path = "../common/mod.rs"]
mod common;

use sqlx::Row;
use std::sync::Arc;

async fn connect_pool() -> Option<Arc<sqlx::PgPool>> {
    match common::get_test_pool_async().await {
        Ok(pool) => Some(pool),
        Err(error) => {
            eprintln!(
                "Skipping room core schema contract integration tests because test database is unavailable: {}",
                error
            );
            None
        }
    }
}

async fn has_unique_constraint_on(pool: &sqlx::PgPool, table_name: &str, columns: &[&str]) -> bool {
    let rows = sqlx::query(
        r#"
        SELECT tc.constraint_name, kcu.column_name, kcu.ordinal_position
        FROM information_schema.table_constraints tc
        JOIN information_schema.key_column_usage kcu
          ON tc.constraint_name = kcu.constraint_name
         AND tc.table_schema = kcu.table_schema
        WHERE tc.table_schema = 'public'
          AND tc.table_name = $1
          AND tc.constraint_type = 'UNIQUE'
        ORDER BY tc.constraint_name, kcu.ordinal_position
        "#,
    )
    .bind(table_name)
    .fetch_all(pool)
    .await
    .expect("Failed to query unique constraints");

    let mut current_name: Option<String> = None;
    let mut current_columns: Vec<String> = Vec::new();
    for row in rows {
        let name = row.get::<String, _>("constraint_name");
        let column = row.get::<String, _>("column_name");
        if current_name.as_deref() != Some(name.as_str()) {
            if current_columns == columns.iter().map(|c| (*c).to_string()).collect::<Vec<_>>() {
                return true;
            }
            current_name = Some(name);
            current_columns.clear();
        }
        current_columns.push(column);
    }

    current_columns == columns.iter().map(|c| (*c).to_string()).collect::<Vec<_>>()
}

async fn has_index_named(pool: &sqlx::PgPool, index_name: &str) -> bool {
    sqlx::query_scalar::<_, bool>(
        r#"
        SELECT EXISTS (
            SELECT 1
            FROM pg_indexes
            WHERE schemaname = 'public' AND indexname = $1
        )
        "#,
    )
    .bind(index_name)
    .fetch_one(pool)
    .await
    .expect("Failed to query pg_indexes")
}

async fn assert_column(
    pool: &sqlx::PgPool,
    table_name: &str,
    column_name: &str,
    expected_types: &[&str],
    expected_nullable: bool,
    expected_default_contains: Option<&str>,
) {
    let row = sqlx::query(
        r#"
        SELECT data_type, is_nullable, column_default
        FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = $1 AND column_name = $2
        "#,
    )
    .bind(table_name)
    .bind(column_name)
    .fetch_one(pool)
    .await
    .unwrap_or_else(|_| panic!("Expected column {}.{} to exist", table_name, column_name));

    let data_type = row.get::<String, _>("data_type");
    assert!(
        expected_types
            .iter()
            .any(|ty| data_type.eq_ignore_ascii_case(ty)),
        "Expected {}.{} type in {:?}, got {}",
        table_name,
        column_name,
        expected_types,
        data_type
    );

    let is_nullable = row.get::<String, _>("is_nullable");
    assert_eq!(
        is_nullable.eq_ignore_ascii_case("YES"),
        expected_nullable,
        "Unexpected nullable flag for {}.{}",
        table_name,
        column_name
    );

    if let Some(expected_default_fragment) = expected_default_contains {
        let column_default = row
            .get::<Option<String>, _>("column_default")
            .unwrap_or_default();
        assert!(
            column_default.contains(expected_default_fragment),
            "Expected {}.{} default to contain {:?}, got {:?}",
            table_name,
            column_name,
            expected_default_fragment,
            column_default
        );
    }
}

#[tokio::test]
async fn test_schema_contract_room_core_tables_shape() {
    let pool = match connect_pool().await {
        Some(pool) => pool,
        None => return,
    };

    assert_column(
        &pool,
        "rooms",
        "room_id",
        &["text", "character varying"],
        false,
        None,
    )
    .await;
    assert_column(
        &pool,
        "rooms",
        "creator",
        &["text", "character varying"],
        true,
        None,
    )
    .await;
    assert_column(&pool, "rooms", "created_ts", &["bigint"], false, None).await;
    assert_column(
        &pool,
        "rooms",
        "is_public",
        &["boolean"],
        true,
        Some("false"),
    )
    .await;
    assert_column(
        &pool,
        "rooms",
        "room_version",
        &["text", "character varying"],
        true,
        Some("6"),
    )
    .await;
    assert_column(
        &pool,
        "rooms",
        "join_rules",
        &["text", "character varying"],
        true,
        Some("invite"),
    )
    .await;
    assert_column(
        &pool,
        "rooms",
        "history_visibility",
        &["text", "character varying"],
        true,
        Some("shared"),
    )
    .await;

    assert!(
        has_index_named(&pool, "idx_rooms_creator").await,
        "Expected rooms index idx_rooms_creator"
    );
    assert!(
        has_index_named(&pool, "idx_rooms_is_public").await,
        "Expected rooms index idx_rooms_is_public"
    );

    for column in ["room_id", "user_id", "membership"] {
        assert_column(
            &pool,
            "room_memberships",
            column,
            &["text", "character varying"],
            false,
            None,
        )
        .await;
    }
    assert_column(
        &pool,
        "room_memberships",
        "updated_ts",
        &["bigint"],
        true,
        None,
    )
    .await;
    assert_column(
        &pool,
        "room_memberships",
        "is_banned",
        &["boolean"],
        true,
        Some("false"),
    )
    .await;
    assert!(
        has_unique_constraint_on(&pool, "room_memberships", &["room_id", "user_id"]).await,
        "Expected room_memberships UNIQUE(room_id,user_id)"
    );
    assert!(
        has_index_named(&pool, "idx_room_memberships_room").await,
        "Expected room_memberships index idx_room_memberships_room"
    );
    assert!(
        has_index_named(&pool, "idx_room_memberships_user_membership").await,
        "Expected room_memberships index idx_room_memberships_user_membership"
    );
    assert!(
        has_index_named(&pool, "idx_room_memberships_room_membership").await,
        "Expected room_memberships index idx_room_memberships_room_membership"
    );

    for column in ["event_id", "room_id", "sender", "event_type"] {
        assert_column(
            &pool,
            "events",
            column,
            &["text", "character varying"],
            false,
            None,
        )
        .await;
    }
    assert_column(&pool, "events", "content", &["jsonb"], false, None).await;
    assert_column(
        &pool,
        "events",
        "origin_server_ts",
        &["bigint"],
        false,
        None,
    )
    .await;
    assert_column(
        &pool,
        "events",
        "state_key",
        &["text", "character varying"],
        true,
        None,
    )
    .await;
    assert_column(
        &pool,
        "events",
        "is_redacted",
        &["boolean"],
        true,
        Some("false"),
    )
    .await;
    assert_column(&pool, "events", "unsigned", &["jsonb"], true, Some("{}")).await;

    assert!(
        has_index_named(&pool, "idx_events_room_id").await,
        "Expected events index idx_events_room_id"
    );
    assert!(
        has_index_named(&pool, "idx_events_origin_server_ts").await,
        "Expected events index idx_events_origin_server_ts"
    );
    assert!(
        has_index_named(&pool, "idx_events_not_redacted").await,
        "Expected events partial index idx_events_not_redacted"
    );
}

#[tokio::test]
async fn test_schema_contract_room_core_query_and_write_read_closure() {
    let pool = match connect_pool().await {
        Some(pool) => pool,
        None => return,
    };

    let now = chrono::Utc::now().timestamp_millis();
    let user_id = format!("@schema-room-core-{}:localhost", uuid::Uuid::new_v4());
    let username = format!("schema_room_core_{}", uuid::Uuid::new_v4());
    let room_id = format!("!schema-room-core-{}:localhost", uuid::Uuid::new_v4());

    sqlx::query("INSERT INTO users (user_id, username, created_ts) VALUES ($1, $2, $3)")
        .bind(&user_id)
        .bind(&username)
        .bind(now)
        .execute(&*pool)
        .await
        .expect("Failed to insert users fixture");

    sqlx::query("INSERT INTO rooms (room_id, creator, created_ts) VALUES ($1, $2, $3)")
        .bind(&room_id)
        .bind(&user_id)
        .bind(now)
        .execute(&*pool)
        .await
        .expect("Failed to insert rooms fixture");

    sqlx::query(
        r#"
        INSERT INTO room_memberships (room_id, user_id, membership, joined_ts)
        VALUES ($1, $2, 'join', $3)
        "#,
    )
    .bind(&room_id)
    .bind(&user_id)
    .bind(now)
    .execute(&*pool)
    .await
    .expect("Failed to insert room_memberships fixture");

    let duplicate_membership = sqlx::query(
        r#"
        INSERT INTO room_memberships (room_id, user_id, membership, joined_ts)
        VALUES ($1, $2, 'join', $3)
        "#,
    )
    .bind(&room_id)
    .bind(&user_id)
    .bind(now + 1)
    .execute(&*pool)
    .await;
    assert!(
        duplicate_membership.is_err(),
        "Expected UNIQUE(room_id,user_id) to reject duplicates"
    );

    let membership_row =
        sqlx::query("SELECT membership FROM room_memberships WHERE room_id = $1 AND user_id = $2")
            .bind(&room_id)
            .bind(&user_id)
            .fetch_one(&*pool)
            .await
            .expect("Failed to query room_memberships by room_id+user_id");
    assert_eq!(
        membership_row
            .get::<String, _>("membership")
            .to_ascii_lowercase(),
        "join"
    );

    let event_id_old = format!("$schema-room-core-old-{}", uuid::Uuid::new_v4());
    let event_id_new = format!("$schema-room-core-new-{}", uuid::Uuid::new_v4());

    sqlx::query(
        r#"
        INSERT INTO events (event_id, room_id, sender, event_type, content, origin_server_ts)
        VALUES ($1, $2, $3, 'm.room.message', $4, $5)
        "#,
    )
    .bind(&event_id_old)
    .bind(&room_id)
    .bind(&user_id)
    .bind(serde_json::json!({"body": "old"}))
    .bind(now)
    .execute(&*pool)
    .await
    .expect("Failed to insert old event fixture");

    sqlx::query(
        r#"
        INSERT INTO events (event_id, room_id, sender, event_type, content, origin_server_ts)
        VALUES ($1, $2, $3, 'm.room.message', $4, $5)
        "#,
    )
    .bind(&event_id_new)
    .bind(&room_id)
    .bind(&user_id)
    .bind(serde_json::json!({"body": "new"}))
    .bind(now + 10)
    .execute(&*pool)
    .await
    .expect("Failed to insert new event fixture");

    let latest_event = sqlx::query(
        "SELECT event_id FROM events WHERE room_id = $1 ORDER BY origin_server_ts DESC LIMIT 1",
    )
    .bind(&room_id)
    .fetch_one(&*pool)
    .await
    .expect("Failed to query latest event by origin_server_ts");
    assert_eq!(latest_event.get::<String, _>("event_id"), event_id_new);

    sqlx::query("DELETE FROM events WHERE room_id = $1")
        .bind(&room_id)
        .execute(&*pool)
        .await
        .expect("Failed to cleanup events fixture");
    sqlx::query("DELETE FROM room_memberships WHERE room_id = $1")
        .bind(&room_id)
        .execute(&*pool)
        .await
        .expect("Failed to cleanup room_memberships fixture");
    sqlx::query("DELETE FROM rooms WHERE room_id = $1")
        .bind(&room_id)
        .execute(&*pool)
        .await
        .expect("Failed to cleanup rooms fixture");
    sqlx::query("DELETE FROM users WHERE user_id = $1")
        .bind(&user_id)
        .execute(&*pool)
        .await
        .expect("Failed to cleanup users fixture");
}

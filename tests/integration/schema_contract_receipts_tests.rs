#![cfg(test)]

#[path = "../common/mod.rs"]
mod common;

use sqlx::Row;
use std::sync::Arc;
use synapse_rust::storage::room::RoomStorage;

async fn connect_pool() -> Option<Arc<sqlx::PgPool>> {
    match common::get_test_pool_async().await {
        Ok(pool) => Some(pool),
        Err(error) => {
            eprintln!(
                "Skipping receipts schema contract integration tests because test database is unavailable: {}",
                error
            );
            None
        }
    }
}

async fn primary_key_columns(pool: &sqlx::PgPool, table_name: &str) -> Vec<String> {
    sqlx::query_scalar::<_, String>(
        r#"
        SELECT a.attname
        FROM pg_index i
        JOIN pg_class c ON c.oid = i.indrelid
        JOIN pg_namespace n ON n.oid = c.relnamespace
        JOIN pg_attribute a ON a.attrelid = c.oid AND a.attnum = ANY(i.indkey)
        WHERE i.indisprimary
          AND n.nspname = 'public'
          AND c.relname = $1
        ORDER BY array_position(i.indkey, a.attnum)
        "#,
    )
    .bind(table_name)
    .fetch_all(pool)
    .await
    .expect("Failed to query primary key columns")
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

    let expected = columns.iter().map(|c| (*c).to_string()).collect::<Vec<_>>();
    let mut current_name: Option<String> = None;
    let mut current_columns: Vec<String> = Vec::new();
    for row in rows {
        let name = row.get::<String, _>("constraint_name");
        let column = row.get::<String, _>("column_name");
        if current_name.as_deref() != Some(name.as_str()) {
            if current_columns == expected {
                return true;
            }
            current_name = Some(name);
            current_columns.clear();
        }
        current_columns.push(column);
    }

    current_columns == expected
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

async fn seed_user_room_events(
    pool: &sqlx::PgPool,
    suffix: &str,
) -> (String, String, String, String) {
    let user_id = format!("@schema-receipts-user-{suffix}:localhost");
    let room_id = format!("!schema-receipts-room-{suffix}:localhost");
    let event_id_1 = format!("$schema-receipts-event-1-{suffix}");
    let event_id_2 = format!("$schema-receipts-event-2-{suffix}");

    sqlx::query(
        "INSERT INTO users (user_id, username, created_ts) VALUES ($1, $2, $3) ON CONFLICT (user_id) DO NOTHING",
    )
    .bind(&user_id)
    .bind(format!("schema_receipts_user_{suffix}"))
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

async fn cleanup_receipts_fixtures(
    pool: &sqlx::PgPool,
    user_id: &str,
    room_id: &str,
    event_ids: &[&str],
) {
    sqlx::query("DELETE FROM event_receipts WHERE room_id = $1 OR user_id = $2")
        .bind(room_id)
        .bind(user_id)
        .execute(pool)
        .await
        .expect("Failed to cleanup event_receipts");

    sqlx::query("DELETE FROM read_markers WHERE room_id = $1 OR user_id = $2")
        .bind(room_id)
        .bind(user_id)
        .execute(pool)
        .await
        .expect("Failed to cleanup read_markers");

    for event_id in event_ids {
        sqlx::query("DELETE FROM events WHERE event_id = $1")
            .bind(event_id)
            .execute(pool)
            .await
            .expect("Failed to cleanup events");
    }

    sqlx::query("DELETE FROM rooms WHERE room_id = $1")
        .bind(room_id)
        .execute(pool)
        .await
        .expect("Failed to cleanup rooms");

    sqlx::query("DELETE FROM users WHERE user_id = $1")
        .bind(user_id)
        .execute(pool)
        .await
        .expect("Failed to cleanup users");
}

#[tokio::test]
async fn test_schema_contract_receipts_tables_shape() {
    let pool = match connect_pool().await {
        Some(pool) => pool,
        None => return,
    };

    for table_name in ["read_markers", "event_receipts"] {
        assert_eq!(
            primary_key_columns(&pool, table_name).await,
            vec!["id".to_string()],
            "Expected {table_name} PRIMARY KEY(id)"
        );
    }

    assert!(
        has_unique_constraint_on(
            &pool,
            "read_markers",
            &["room_id", "user_id", "marker_type"]
        )
        .await,
        "Expected read_markers UNIQUE(room_id, user_id, marker_type)"
    );
    assert!(
        has_unique_constraint_on(
            &pool,
            "event_receipts",
            &["event_id", "room_id", "user_id", "receipt_type"],
        )
        .await,
        "Expected event_receipts UNIQUE(event_id, room_id, user_id, receipt_type)"
    );

    assert_column(
        &pool,
        "read_markers",
        "marker_type",
        &["text", "character varying"],
        false,
        None,
    )
    .await;
    assert_column(
        &pool,
        "read_markers",
        "created_ts",
        &["bigint"],
        false,
        None,
    )
    .await;
    assert_column(
        &pool,
        "read_markers",
        "updated_ts",
        &["bigint"],
        false,
        None,
    )
    .await;
    assert_column(
        &pool,
        "event_receipts",
        "receipt_type",
        &["text", "character varying"],
        false,
        None,
    )
    .await;
    assert_column(&pool, "event_receipts", "ts", &["bigint"], false, None).await;
    assert_column(
        &pool,
        "event_receipts",
        "data",
        &["jsonb"],
        true,
        Some("{}"),
    )
    .await;

    for index_name in [
        "idx_read_markers_room_user",
        "idx_event_receipts_event",
        "idx_event_receipts_room",
        "idx_event_receipts_room_type",
    ] {
        assert!(
            has_index_named(&pool, index_name).await,
            "Expected index {} to exist",
            index_name
        );
    }
}

#[tokio::test]
async fn test_schema_contract_receipts_query_and_write_read_closure() {
    let pool = match connect_pool().await {
        Some(pool) => pool,
        None => return,
    };

    let storage = RoomStorage::new(&pool);
    let suffix = uuid::Uuid::new_v4().to_string();
    let (user_id, room_id, event_id_1, event_id_2) = seed_user_room_events(&pool, &suffix).await;

    storage
        .update_read_marker(&room_id, &user_id, &event_id_1)
        .await
        .expect("Failed to write default read marker");
    assert_eq!(
        storage
            .get_read_marker(&room_id, &user_id, "m.fully_read")
            .await
            .expect("Failed to fetch default read marker"),
        Some(event_id_1.clone())
    );

    storage
        .update_read_marker_with_type(&room_id, &user_id, &event_id_2, "m.read")
        .await
        .expect("Failed to write m.read marker");
    storage
        .update_read_marker_with_type(&room_id, &user_id, &event_id_2, "m.fully_read")
        .await
        .expect("Failed to upsert m.fully_read marker");

    let markers = storage
        .get_all_read_markers(&room_id, &user_id)
        .await
        .expect("Failed to list read markers");
    assert_eq!(markers.get("m.fully_read"), Some(&event_id_2));
    assert_eq!(markers.get("m.read"), Some(&event_id_2));

    storage
        .add_receipt(&user_id, &user_id, &room_id, &event_id_2, "m.read")
        .await
        .expect("Failed to write read receipt");
    storage
        .add_receipt(&user_id, &user_id, &room_id, &event_id_2, "m.read")
        .await
        .expect("Failed to upsert read receipt");
    storage
        .add_receipt(&user_id, &user_id, &room_id, &event_id_2, "m.read.private")
        .await
        .expect("Failed to write private read receipt");

    let read_receipts = storage
        .get_receipts(&room_id, "m.read", &event_id_2)
        .await
        .expect("Failed to get m.read receipts");
    assert_eq!(read_receipts.len(), 1);
    assert_eq!(read_receipts[0].user_id, user_id);
    assert_eq!(read_receipts[0].event_id, event_id_2);
    assert_eq!(read_receipts[0].receipt_type, "m.read");
    assert_eq!(read_receipts[0].data, serde_json::json!({}));
    assert!(read_receipts[0].ts > 0);

    let private_receipts = storage
        .get_receipts(&room_id, "m.read.private", &event_id_2)
        .await
        .expect("Failed to get m.read.private receipts");
    assert_eq!(private_receipts.len(), 1);
    assert_eq!(private_receipts[0].receipt_type, "m.read.private");

    let read_receipt_rows: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*)
        FROM event_receipts
        WHERE room_id = $1 AND user_id = $2 AND event_id = $3 AND receipt_type = 'm.read'
        "#,
    )
    .bind(&room_id)
    .bind(&user_id)
    .bind(&event_id_2)
    .fetch_one(&*pool)
    .await
    .expect("Failed to count stored m.read receipts");
    assert_eq!(read_receipt_rows, 1);

    let read_marker_rows: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*)
        FROM read_markers
        WHERE room_id = $1 AND user_id = $2 AND marker_type IN ('m.read', 'm.fully_read')
        "#,
    )
    .bind(&room_id)
    .bind(&user_id)
    .fetch_one(&*pool)
    .await
    .expect("Failed to count stored read markers");
    assert_eq!(read_marker_rows, 2);

    cleanup_receipts_fixtures(&pool, &user_id, &room_id, &[&event_id_1, &event_id_2]).await;
}

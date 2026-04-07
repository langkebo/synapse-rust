#![cfg(test)]

#[path = "../common/mod.rs"]
mod common;

use sqlx::Row;
use std::sync::Arc;
use synapse_rust::storage::retention::{
    CreateRoomRetentionPolicyRequest, RetentionStorage, UpdateRoomRetentionPolicyRequest,
};

async fn connect_pool() -> Option<Arc<sqlx::PgPool>> {
    match common::get_test_pool_async().await {
        Ok(pool) => Some(pool),
        Err(error) => {
            eprintln!(
                "Skipping retention schema contract integration tests because test database is unavailable: {}",
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

async fn seed_user_and_room(pool: &sqlx::PgPool, suffix: &str) -> (String, String) {
    let creator = format!("@schema-retention-creator-{suffix}:localhost");
    let room_id = format!("!schema-retention-room-{suffix}:localhost");

    sqlx::query(
        "INSERT INTO users (user_id, username, created_ts) VALUES ($1, $2, $3) ON CONFLICT (user_id) DO NOTHING",
    )
    .bind(&creator)
    .bind(format!("schema_retention_creator_{suffix}"))
    .bind(0_i64)
    .execute(pool)
    .await
    .expect("Failed to seed user fixture");

    sqlx::query(
        "INSERT INTO rooms (room_id, creator, created_ts) VALUES ($1, $2, $3) ON CONFLICT (room_id) DO NOTHING",
    )
    .bind(&room_id)
    .bind(&creator)
    .bind(0_i64)
    .execute(pool)
    .await
    .expect("Failed to seed room fixture");

    (creator, room_id)
}

async fn cleanup_retention_fixtures(pool: &sqlx::PgPool, room_id: &str, user_id: &str) {
    sqlx::query("DELETE FROM deleted_events_index WHERE room_id = $1")
        .bind(room_id)
        .execute(pool)
        .await
        .expect("Failed to cleanup deleted_events_index");

    sqlx::query("DELETE FROM retention_cleanup_logs WHERE room_id = $1")
        .bind(room_id)
        .execute(pool)
        .await
        .expect("Failed to cleanup retention_cleanup_logs");

    sqlx::query("DELETE FROM retention_cleanup_queue WHERE room_id = $1")
        .bind(room_id)
        .execute(pool)
        .await
        .expect("Failed to cleanup retention_cleanup_queue");

    sqlx::query("DELETE FROM retention_stats WHERE room_id = $1")
        .bind(room_id)
        .execute(pool)
        .await
        .expect("Failed to cleanup retention_stats");

    sqlx::query("DELETE FROM room_retention_policies WHERE room_id = $1")
        .bind(room_id)
        .execute(pool)
        .await
        .expect("Failed to cleanup room_retention_policies");

    sqlx::query("DELETE FROM rooms WHERE room_id = $1")
        .bind(room_id)
        .execute(pool)
        .await
        .expect("Failed to cleanup room fixture");

    sqlx::query("DELETE FROM users WHERE user_id = $1")
        .bind(user_id)
        .execute(pool)
        .await
        .expect("Failed to cleanup user fixture");
}

#[tokio::test]
async fn test_schema_contract_retention_tables_shape() {
    let pool = match connect_pool().await {
        Some(pool) => pool,
        None => return,
    };

    assert_eq!(
        primary_key_columns(&pool, "server_retention_policy").await,
        vec!["id".to_string()],
        "Expected server_retention_policy PRIMARY KEY(id)"
    );
    assert!(
        has_unique_constraint_on(&pool, "room_retention_policies", &["room_id"]).await,
        "Expected room_retention_policies UNIQUE(room_id)"
    );
    assert!(
        has_unique_constraint_on(&pool, "retention_cleanup_queue", &["room_id", "event_id"]).await,
        "Expected retention_cleanup_queue UNIQUE(room_id,event_id)"
    );
    assert!(
        has_unique_constraint_on(&pool, "deleted_events_index", &["room_id", "event_id"]).await,
        "Expected deleted_events_index UNIQUE(room_id,event_id)"
    );

    assert_column(
        &pool,
        "server_retention_policy",
        "min_lifetime",
        &["bigint"],
        false,
        Some("0"),
    )
    .await;
    assert_column(
        &pool,
        "server_retention_policy",
        "expire_on_clients",
        &["boolean"],
        false,
        Some("false"),
    )
    .await;
    assert_column(
        &pool,
        "room_retention_policies",
        "min_lifetime",
        &["bigint"],
        false,
        Some("0"),
    )
    .await;
    assert_column(
        &pool,
        "retention_cleanup_queue",
        "status",
        &["text", "character varying"],
        false,
        Some("pending"),
    )
    .await;
    assert_column(
        &pool,
        "retention_cleanup_queue",
        "retry_count",
        &["integer"],
        false,
        Some("0"),
    )
    .await;

    for index_name in [
        "idx_room_retention_policies_server_default",
        "idx_retention_cleanup_queue_status_origin",
        "idx_retention_cleanup_logs_room_started",
        "idx_deleted_events_index_room_ts",
    ] {
        assert!(
            has_index_named(&pool, index_name).await,
            "Expected index {} to exist",
            index_name
        );
    }
}

#[tokio::test]
async fn test_schema_contract_retention_query_and_write_read_closure() {
    let pool = match connect_pool().await {
        Some(pool) => pool,
        None => return,
    };

    let storage = RetentionStorage::new(&pool);
    let suffix = uuid::Uuid::new_v4().to_string();
    let (creator, room_id) = seed_user_and_room(&pool, &suffix).await;

    storage
        .create_room_policy(CreateRoomRetentionPolicyRequest {
            room_id: room_id.clone(),
            max_lifetime: Some(1000),
            min_lifetime: Some(10),
            expire_on_clients: Some(true),
        })
        .await
        .expect("Failed to create room retention policy fixture");

    let fetched = storage
        .get_room_policy(&room_id)
        .await
        .expect("Failed to get room retention policy")
        .expect("Expected room retention policy");
    assert_eq!(fetched.room_id, room_id);
    assert_eq!(fetched.max_lifetime, Some(1000));
    assert_eq!(fetched.min_lifetime, 10);
    assert!(fetched.expire_on_clients);

    let effective = storage
        .get_effective_policy(&room_id)
        .await
        .expect("Failed to get effective retention policy");
    assert_eq!(effective.max_lifetime, Some(1000));
    assert_eq!(effective.min_lifetime, 10);
    assert!(effective.expire_on_clients);

    storage
        .update_room_policy(
            &room_id,
            UpdateRoomRetentionPolicyRequest {
                max_lifetime: Some(2000),
                min_lifetime: Some(20),
                expire_on_clients: Some(false),
            },
        )
        .await
        .expect("Failed to update room retention policy");

    let updated = storage
        .get_room_policy(&room_id)
        .await
        .expect("Failed to reload room retention policy")
        .expect("Expected updated room retention policy");
    assert_eq!(updated.max_lifetime, Some(2000));
    assert_eq!(updated.min_lifetime, 20);
    assert!(!updated.expire_on_clients);

    let event_id = format!("$schema-retention-event-{suffix}");
    storage
        .queue_cleanup(&room_id, &event_id, "m.room.message", 123)
        .await
        .expect("Failed to queue retention cleanup");
    storage
        .queue_cleanup(&room_id, &event_id, "m.room.message", 123)
        .await
        .expect("Failed to queue retention cleanup twice");

    let pending = storage
        .get_pending_cleanups(10)
        .await
        .expect("Failed to fetch pending cleanups");
    let pending_for_room: Vec<_> = pending
        .into_iter()
        .filter(|p| p.room_id == room_id)
        .collect();
    assert_eq!(pending_for_room.len(), 1);

    storage
        .mark_cleanup_failed(pending_for_room[0].id, "forced failure")
        .await
        .expect("Failed to mark cleanup failed");

    let failed_row = sqlx::query(
        "SELECT status, retry_count, error_message FROM retention_cleanup_queue WHERE id = $1",
    )
    .bind(pending_for_room[0].id)
    .fetch_one(&*pool)
    .await
    .expect("Failed to query retention_cleanup_queue row");
    assert_eq!(failed_row.get::<String, _>("status"), "failed");
    assert_eq!(failed_row.get::<i32, _>("retry_count"), 1);
    assert_eq!(
        failed_row
            .get::<Option<String>, _>("error_message")
            .as_deref(),
        Some("forced failure")
    );

    storage
        .record_deleted_event(&room_id, &event_id, "retention")
        .await
        .expect("Failed to record deleted event");
    let deleted_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM deleted_events_index WHERE room_id = $1 AND event_id = $2",
    )
    .bind(&room_id)
    .bind(&event_id)
    .fetch_one(&*pool)
    .await
    .expect("Failed to count deleted_events_index");
    assert_eq!(deleted_count, 1);

    cleanup_retention_fixtures(&pool, &room_id, &creator).await;
}

#![cfg(test)]

#[path = "../common/mod.rs"]
mod common;

use sqlx::Row;
use std::sync::Arc;
use synapse_rust::storage::search_index::{SearchIndexEntry, SearchIndexStorage, SearchQuery};

async fn connect_pool() -> Option<Arc<sqlx::PgPool>> {
    match common::get_test_pool_async().await {
        Ok(pool) => Some(pool),
        Err(error) => {
            eprintln!(
                "Skipping search schema contract integration tests because test database is unavailable: {}",
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
    expected_char_length: Option<i32>,
) {
    let row = sqlx::query(
        r#"
        SELECT data_type, is_nullable, column_default, character_maximum_length
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

    if let Some(expected_length) = expected_char_length {
        assert_eq!(
            row.get::<Option<i32>, _>("character_maximum_length"),
            Some(expected_length),
            "Unexpected varchar length for {}.{}",
            table_name,
            column_name
        );
    }
}

#[tokio::test]
async fn test_schema_contract_search_index_shape() {
    let pool = match connect_pool().await {
        Some(pool) => pool,
        None => return,
    };

    assert_column(
        &pool,
        "search_index",
        "event_id",
        &["character varying"],
        false,
        None,
        Some(255),
    )
    .await;
    for column in ["room_id", "user_id", "event_type"] {
        assert_column(
            &pool,
            "search_index",
            column,
            &["character varying"],
            false,
            None,
            Some(255),
        )
        .await;
    }
    assert_column(
        &pool,
        "search_index",
        "type",
        &["character varying"],
        false,
        None,
        Some(255),
    )
    .await;
    assert_column(
        &pool,
        "search_index",
        "content",
        &["text"],
        false,
        None,
        None,
    )
    .await;
    assert_column(
        &pool,
        "search_index",
        "created_ts",
        &["bigint"],
        false,
        None,
        None,
    )
    .await;
    assert_column(
        &pool,
        "search_index",
        "updated_ts",
        &["bigint"],
        true,
        None,
        None,
    )
    .await;

    assert!(
        has_unique_constraint_on(&pool, "search_index", &["event_id"]).await,
        "Expected search_index UNIQUE(event_id)"
    );
    for index_name in [
        "idx_search_index_room",
        "idx_search_index_user",
        "idx_search_index_type",
    ] {
        assert!(
            has_index_named(&pool, index_name).await,
            "Expected index {} to exist",
            index_name
        );
    }
}

#[tokio::test]
async fn test_schema_contract_search_index_query_and_write_read_closure() {
    let pool = match connect_pool().await {
        Some(pool) => pool,
        None => return,
    };

    let storage = SearchIndexStorage::new(pool.as_ref().clone());
    let suffix = uuid::Uuid::new_v4().to_string();
    let room_id = format!("!schema-search-room-{suffix}:localhost");
    let user_id = format!("@schema-search-user-{suffix}:localhost");
    let event_id_old = format!("$schema-search-old-{suffix}");
    let event_id_new = format!("$schema-search-new-{suffix}");
    let created_ts = chrono::Utc::now().timestamp_millis();

    storage
        .index_event(&SearchIndexEntry {
            id: 0,
            event_id: event_id_old.clone(),
            room_id: room_id.clone(),
            user_id: user_id.clone(),
            event_type: "m.room.message".to_string(),
            content_type: "m.room.message".to_string(),
            content: "older schema contract hit".to_string(),
            created_ts,
            updated_ts: None,
        })
        .await
        .expect("Failed to index first search entry");
    storage
        .index_event(&SearchIndexEntry {
            id: 0,
            event_id: event_id_new.clone(),
            room_id: room_id.clone(),
            user_id: user_id.clone(),
            event_type: "m.room.message".to_string(),
            content_type: "m.room.message".to_string(),
            content: "newer schema contract hit".to_string(),
            created_ts: created_ts + 1000,
            updated_ts: Some(created_ts + 1000),
        })
        .await
        .expect("Failed to index second search entry");

    let results = storage
        .search_events(&SearchQuery {
            search_term: "schema contract".to_string(),
            room_ids: None,
            not_room_ids: None,
            sender: None,
            event_types: None,
            limit: Some(10),
            offset: Some(0),
        })
        .await
        .expect("Failed to search indexed events");

    assert_eq!(results.len(), 2);
    assert_eq!(results[0].event_id, event_id_new);
    assert_eq!(results[1].event_id, event_id_old);

    storage
        .index_event(&SearchIndexEntry {
            id: 0,
            event_id: event_id_new.clone(),
            room_id: room_id.clone(),
            user_id: user_id.clone(),
            event_type: "m.room.message".to_string(),
            content_type: "m.room.message".to_string(),
            content: "newest schema contract hit".to_string(),
            created_ts: created_ts + 1000,
            updated_ts: Some(created_ts + 2000),
        })
        .await
        .expect("Failed to upsert search entry");

    let updated_row =
        sqlx::query("SELECT content, updated_ts FROM search_index WHERE event_id = $1")
            .bind(&event_id_new)
            .fetch_one(&*pool)
            .await
            .expect("Failed to fetch updated search_index row");
    assert_eq!(
        updated_row.get::<String, _>("content"),
        "newest schema contract hit"
    );
    assert_eq!(
        updated_row.get::<Option<i64>, _>("updated_ts"),
        Some(created_ts + 2000)
    );

    let stats = storage
        .get_stats()
        .await
        .expect("Failed to query search stats");
    assert!(stats.total_count >= 2);
    assert!(stats.by_event_type.contains_key("m.room.message"));

    storage
        .delete_event(&event_id_old)
        .await
        .expect("Failed to delete first search entry");
    storage
        .delete_event(&event_id_new)
        .await
        .expect("Failed to delete second search entry");
}

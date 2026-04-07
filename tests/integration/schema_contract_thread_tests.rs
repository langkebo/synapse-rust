#![cfg(test)]

#[path = "../common/mod.rs"]
mod common;

use sqlx::Row;
use std::sync::Arc;
use synapse_rust::storage::thread::{
    CreateThreadReplyParams, CreateThreadRootParams, ThreadListParams, ThreadStorage,
};

async fn connect_pool() -> Option<Arc<sqlx::PgPool>> {
    match common::get_test_pool_async().await {
        Ok(pool) => Some(pool),
        Err(error) => {
            eprintln!(
                "Skipping thread schema contract integration tests because test database is unavailable: {}",
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

async fn seed_users_and_room(pool: &sqlx::PgPool, suffix: &str) -> (String, String, String) {
    let creator = format!("@schema-thread-creator-{suffix}:localhost");
    let replier = format!("@schema-thread-replier-{suffix}:localhost");
    let room_id = format!("!schema-thread-room-{suffix}:localhost");

    for (user_id, username) in [
        (&creator, format!("schema_thread_creator_{suffix}")),
        (&replier, format!("schema_thread_replier_{suffix}")),
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
    .bind(&creator)
    .bind(0_i64)
    .execute(pool)
    .await
    .expect("Failed to seed room fixture");

    (creator, replier, room_id)
}

async fn cleanup_thread_fixtures(pool: &sqlx::PgPool, room_id: &str, user_ids: &[String]) {
    sqlx::query("DELETE FROM thread_relations WHERE room_id = $1")
        .bind(room_id)
        .execute(pool)
        .await
        .expect("Failed to cleanup thread_relations");

    sqlx::query("DELETE FROM thread_replies WHERE room_id = $1")
        .bind(room_id)
        .execute(pool)
        .await
        .expect("Failed to cleanup thread_replies");

    sqlx::query("DELETE FROM thread_roots WHERE room_id = $1")
        .bind(room_id)
        .execute(pool)
        .await
        .expect("Failed to cleanup thread_roots");

    sqlx::query("DELETE FROM rooms WHERE room_id = $1")
        .bind(room_id)
        .execute(pool)
        .await
        .expect("Failed to cleanup room fixture");

    for user_id in user_ids {
        sqlx::query("DELETE FROM users WHERE user_id = $1")
            .bind(user_id)
            .execute(pool)
            .await
            .expect("Failed to cleanup user fixture");
    }
}

#[tokio::test]
async fn test_schema_contract_thread_tables_shape() {
    let pool = match connect_pool().await {
        Some(pool) => pool,
        None => return,
    };

    assert_eq!(
        primary_key_columns(&pool, "thread_roots").await,
        vec!["id".to_string()],
        "Expected thread_roots PRIMARY KEY(id)"
    );
    assert_eq!(
        primary_key_columns(&pool, "thread_replies").await,
        vec!["id".to_string()],
        "Expected thread_replies PRIMARY KEY(id)"
    );
    assert_eq!(
        primary_key_columns(&pool, "thread_relations").await,
        vec!["id".to_string()],
        "Expected thread_relations PRIMARY KEY(id)"
    );

    assert!(
        has_unique_constraint_on(&pool, "thread_roots", &["room_id", "root_event_id"]).await,
        "Expected thread_roots UNIQUE(room_id,root_event_id)"
    );
    assert!(
        has_unique_constraint_on(&pool, "thread_replies", &["room_id", "event_id"]).await,
        "Expected thread_replies UNIQUE(room_id,event_id)"
    );
    assert!(
        has_unique_constraint_on(
            &pool,
            "thread_relations",
            &["room_id", "event_id", "relation_type"]
        )
        .await,
        "Expected thread_relations UNIQUE(room_id,event_id,relation_type)"
    );

    assert_column(
        &pool,
        "thread_roots",
        "participants",
        &["jsonb"],
        true,
        Some("'[]'"),
    )
    .await;
    assert_column(
        &pool,
        "thread_roots",
        "is_fetched",
        &["boolean"],
        true,
        Some("false"),
    )
    .await;
    assert_column(
        &pool,
        "thread_replies",
        "content",
        &["jsonb"],
        false,
        Some("'{}'"),
    )
    .await;
    assert_column(
        &pool,
        "thread_replies",
        "is_edited",
        &["boolean"],
        false,
        Some("false"),
    )
    .await;

    for index_name in [
        "idx_thread_roots_room",
        "idx_thread_roots_root_event",
        "idx_thread_roots_thread",
        "idx_thread_roots_room_thread_unique",
        "idx_thread_roots_room_last_reply_created",
        "idx_thread_roots_last_reply",
        "idx_thread_replies_room_thread_ts",
        "idx_thread_replies_room_event",
        "idx_thread_replies_room_thread_event",
        "idx_thread_relations_room_event",
        "idx_thread_relations_room_relates_to",
        "idx_thread_relations_room_thread",
    ] {
        assert!(
            has_index_named(&pool, index_name).await,
            "Expected index {} to exist",
            index_name
        );
    }
}

#[tokio::test]
async fn test_schema_contract_thread_query_and_write_read_closure() {
    let pool = match connect_pool().await {
        Some(pool) => pool,
        None => return,
    };

    let storage = ThreadStorage::new(&pool);
    let suffix = uuid::Uuid::new_v4().to_string();
    let (creator, replier, room_id) = seed_users_and_room(&pool, &suffix).await;
    let thread_id_with_replies = format!("$schema-thread-{suffix}");
    let thread_id_without_replies = format!("$schema-thread-empty-{suffix}");

    storage
        .create_thread_root(CreateThreadRootParams {
            room_id: room_id.clone(),
            root_event_id: format!("$schema-thread-root-{suffix}"),
            sender: creator.clone(),
            thread_id: Some(thread_id_with_replies.clone()),
        })
        .await
        .expect("Failed to create thread root fixture");

    storage
        .create_thread_root(CreateThreadRootParams {
            room_id: room_id.clone(),
            root_event_id: format!("$schema-thread-root-empty-{suffix}"),
            sender: creator.clone(),
            thread_id: Some(thread_id_without_replies.clone()),
        })
        .await
        .expect("Failed to create second thread root fixture");

    storage
        .create_thread_reply(CreateThreadReplyParams {
            room_id: room_id.clone(),
            thread_id: thread_id_with_replies.clone(),
            event_id: format!("$schema-thread-reply-1-{suffix}"),
            root_event_id: format!("$schema-thread-root-{suffix}"),
            sender: replier.clone(),
            in_reply_to_event_id: None,
            content: serde_json::json!({ "body": "first" }),
            origin_server_ts: 100,
        })
        .await
        .expect("Failed to create first thread reply fixture");

    let last_reply_id = format!("$schema-thread-reply-2-{suffix}");
    storage
        .create_thread_reply(CreateThreadReplyParams {
            room_id: room_id.clone(),
            thread_id: thread_id_with_replies.clone(),
            event_id: last_reply_id.clone(),
            root_event_id: format!("$schema-thread-root-{suffix}"),
            sender: creator.clone(),
            in_reply_to_event_id: None,
            content: serde_json::json!({ "body": "second" }),
            origin_server_ts: 200,
        })
        .await
        .expect("Failed to create second thread reply fixture");

    let fetched_root = storage
        .get_thread_root(&room_id, &thread_id_with_replies)
        .await
        .expect("Failed to get thread root by id")
        .expect("Expected thread root to exist");
    assert_eq!(fetched_root.reply_count, 2);
    assert_eq!(
        fetched_root.last_reply_event_id.as_deref(),
        Some(last_reply_id.as_str())
    );
    assert_eq!(
        fetched_root.last_reply_sender.as_deref(),
        Some(creator.as_str())
    );
    assert_eq!(fetched_root.last_reply_ts, Some(200));

    let listed = storage
        .list_thread_roots(ThreadListParams {
            room_id: room_id.clone(),
            limit: Some(10),
            from: None,
            include_all: true,
        })
        .await
        .expect("Failed to list thread roots");
    assert_eq!(listed.len(), 2);
    assert_eq!(
        listed[0].thread_id.as_deref(),
        Some(thread_id_with_replies.as_str())
    );
    assert_eq!(
        listed[1].thread_id.as_deref(),
        Some(thread_id_without_replies.as_str())
    );

    let replies = storage
        .get_thread_replies(&room_id, &thread_id_with_replies, Some(10), None)
        .await
        .expect("Failed to list thread replies");
    assert_eq!(replies.len(), 2);
    assert!(replies[0].origin_server_ts <= replies[1].origin_server_ts);

    let participants = storage
        .get_thread_participants(&room_id, &thread_id_with_replies)
        .await
        .expect("Failed to get thread participants");
    assert_eq!(participants.len(), 2);

    let mut expected_participants = vec![creator.clone(), replier.clone()];
    expected_participants.sort();
    let mut actual_participants = participants;
    actual_participants.sort();
    assert_eq!(actual_participants, expected_participants);

    cleanup_thread_fixtures(&pool, &room_id, &[creator, replier]).await;
}

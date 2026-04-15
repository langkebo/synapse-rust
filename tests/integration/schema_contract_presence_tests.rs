#![cfg(test)]

#[path = "../common/mod.rs"]
mod common;

use sqlx::Row;
use std::sync::Arc;
use synapse_rust::cache::{CacheConfig, CacheManager};
use synapse_rust::storage::presence::PresenceStorage;

async fn connect_pool() -> Option<Arc<sqlx::PgPool>> {
    match common::get_test_pool_async().await {
        Ok(pool) => Some(pool),
        Err(error) => {
            eprintln!(
                "Skipping presence schema contract integration tests because test database is unavailable: {}",
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

async fn seed_users(pool: &sqlx::PgPool, suffix: &str) -> (String, String) {
    let subscriber = format!("@schema-presence-subscriber-{suffix}:localhost");
    let target = format!("@schema-presence-target-{suffix}:localhost");

    for (user_id, username) in [
        (&subscriber, format!("schema_presence_subscriber_{suffix}")),
        (&target, format!("schema_presence_target_{suffix}")),
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

    (subscriber, target)
}

async fn cleanup_presence_fixtures(pool: &sqlx::PgPool, user_ids: &[String]) {
    sqlx::query(
        r#"
        DELETE FROM presence_subscriptions
        WHERE subscriber_id = ANY($1) OR target_id = ANY($1)
        "#,
    )
    .bind(user_ids)
    .execute(pool)
    .await
    .expect("Failed to cleanup presence_subscriptions");

    sqlx::query("DELETE FROM presence WHERE user_id = ANY($1)")
        .bind(user_ids)
        .execute(pool)
        .await
        .expect("Failed to cleanup presence");

    for user_id in user_ids {
        sqlx::query("DELETE FROM users WHERE user_id = $1")
            .bind(user_id)
            .execute(pool)
            .await
            .expect("Failed to cleanup user fixture");
    }
}

#[tokio::test]
async fn test_schema_contract_presence_tables_shape() {
    let pool = match connect_pool().await {
        Some(pool) => pool,
        None => return,
    };

    assert_eq!(
        primary_key_columns(&pool, "presence").await,
        vec!["user_id".to_string()],
        "Expected presence PRIMARY KEY(user_id)"
    );
    assert!(
        has_unique_constraint_on(
            &pool,
            "presence_subscriptions",
            &["subscriber_id", "target_id"]
        )
        .await,
        "Expected presence_subscriptions UNIQUE(subscriber_id,target_id)"
    );

    assert_column(
        &pool,
        "presence",
        "presence",
        &["text", "character varying"],
        false,
        Some("offline"),
    )
    .await;
    assert_column(
        &pool,
        "presence",
        "last_active_ts",
        &["bigint"],
        false,
        Some("0"),
    )
    .await;

    for index_name in [
        "idx_presence_subscriptions_subscriber",
        "idx_presence_subscriptions_target",
        "idx_presence_user_status",
    ] {
        assert!(
            has_index_named(&pool, index_name).await,
            "Expected index {} to exist",
            index_name
        );
    }
}

#[tokio::test]
async fn test_schema_contract_presence_query_and_write_read_closure() {
    let pool = match connect_pool().await {
        Some(pool) => pool,
        None => return,
    };

    let cache = Arc::new(CacheManager::new(CacheConfig::default()));
    let storage = PresenceStorage::new(pool.clone(), cache);

    let suffix = uuid::Uuid::new_v4().to_string();
    let (subscriber, target) = seed_users(&pool, &suffix).await;

    storage
        .set_presence(&subscriber, "online", Some("hi"))
        .await
        .expect("Failed to set subscriber presence");
    storage
        .set_presence(&target, "offline", None)
        .await
        .expect("Failed to set target presence");

    let presence = storage
        .get_presence(&subscriber)
        .await
        .expect("Failed to get presence")
        .expect("Expected presence row");
    assert_eq!(presence.0, "online");
    assert_eq!(presence.1.as_deref(), Some("hi"));

    storage
        .add_subscription(&subscriber, &target)
        .await
        .expect("Failed to add presence subscription");
    let subs = storage
        .get_subscriptions(&subscriber)
        .await
        .expect("Failed to get subscriptions");
    assert_eq!(subs, vec![target.clone()]);

    let subscribers = storage
        .get_subscribers(&target)
        .await
        .expect("Failed to get subscribers");
    assert_eq!(subscribers, vec![subscriber.clone()]);

    let batch = storage
        .get_presence_batch(&[subscriber.clone(), target.clone()])
        .await
        .expect("Failed to get presence batch");
    assert_eq!(batch.len(), 2);

    storage
        .remove_subscription(&subscriber, &target)
        .await
        .expect("Failed to remove subscription");
    let subs_after = storage
        .get_subscriptions(&subscriber)
        .await
        .expect("Failed to get subscriptions after removal");
    assert!(subs_after.is_empty());

    cleanup_presence_fixtures(&pool, &[subscriber, target]).await;
}

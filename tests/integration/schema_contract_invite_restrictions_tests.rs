#![cfg(test)]

#[path = "../common/mod.rs"]
mod common;

use sqlx::Row;
use std::sync::Arc;
use synapse_rust::storage::invite_blocklist::InviteBlocklistStorage;

async fn connect_pool() -> Option<Arc<sqlx::PgPool>> {
    match common::get_test_pool_async().await {
        Ok(pool) => Some(pool),
        Err(error) => {
            eprintln!(
                "Skipping invite restriction schema contract integration tests because test database is unavailable: {}",
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

async fn seed_users_and_room(
    pool: &sqlx::PgPool,
    suffix: &str,
) -> (String, String, String, String) {
    let creator = format!("@schema-invite-creator-{suffix}:localhost");
    let user_a = format!("@schema-invite-a-{suffix}:localhost");
    let user_b = format!("@schema-invite-b-{suffix}:localhost");
    let room_id = format!("!schema-invite-room-{suffix}:localhost");

    for (user_id, username) in [
        (&creator, format!("schema_invite_creator_{suffix}")),
        (&user_a, format!("schema_invite_a_{suffix}")),
        (&user_b, format!("schema_invite_b_{suffix}")),
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

    (creator, user_a, user_b, room_id)
}

async fn cleanup_invite_fixtures(pool: &sqlx::PgPool, room_id: &str, user_ids: &[String]) {
    sqlx::query("DELETE FROM room_invite_blocklist WHERE room_id = $1")
        .bind(room_id)
        .execute(pool)
        .await
        .expect("Failed to cleanup room_invite_blocklist");

    sqlx::query("DELETE FROM room_invite_allowlist WHERE room_id = $1")
        .bind(room_id)
        .execute(pool)
        .await
        .expect("Failed to cleanup room_invite_allowlist");

    sqlx::query("DELETE FROM room_invites WHERE room_id = $1")
        .bind(room_id)
        .execute(pool)
        .await
        .expect("Failed to cleanup room_invites");

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
async fn test_schema_contract_invite_restriction_tables_shape() {
    let pool = match connect_pool().await {
        Some(pool) => pool,
        None => return,
    };

    assert_eq!(
        primary_key_columns(&pool, "room_invite_blocklist").await,
        vec!["id".to_string()],
        "Expected room_invite_blocklist PRIMARY KEY(id)"
    );
    assert_eq!(
        primary_key_columns(&pool, "room_invite_allowlist").await,
        vec!["id".to_string()],
        "Expected room_invite_allowlist PRIMARY KEY(id)"
    );
    assert!(
        has_unique_constraint_on(&pool, "room_invite_blocklist", &["room_id", "user_id"]).await,
        "Expected room_invite_blocklist UNIQUE(room_id,user_id)"
    );
    assert!(
        has_unique_constraint_on(&pool, "room_invite_allowlist", &["room_id", "user_id"]).await,
        "Expected room_invite_allowlist UNIQUE(room_id,user_id)"
    );

    assert_column(
        &pool,
        "room_invite_blocklist",
        "created_ts",
        &["bigint"],
        false,
        None,
    )
    .await;
    assert_column(
        &pool,
        "room_invite_allowlist",
        "created_ts",
        &["bigint"],
        false,
        None,
    )
    .await;

    for index_name in [
        "idx_room_invite_blocklist_room",
        "idx_room_invite_blocklist_user",
        "idx_room_invite_allowlist_room",
        "idx_room_invite_allowlist_user",
    ] {
        assert!(
            has_index_named(&pool, index_name).await,
            "Expected index {} to exist",
            index_name
        );
    }
}

#[tokio::test]
async fn test_schema_contract_invite_restriction_query_and_write_read_closure() {
    let pool = match connect_pool().await {
        Some(pool) => pool,
        None => return,
    };

    let suffix = uuid::Uuid::new_v4().to_string();
    let (creator, user_a, user_b, room_id) = seed_users_and_room(&pool, &suffix).await;
    let storage = InviteBlocklistStorage::new(pool.clone());

    storage
        .set_invite_blocklist(&room_id, vec![user_a.clone()])
        .await
        .expect("Failed to set invite blocklist");

    let blocklist = storage
        .get_invite_blocklist(&room_id)
        .await
        .expect("Failed to get invite blocklist");
    assert_eq!(blocklist, vec![user_a.clone()]);

    assert!(
        storage
            .is_user_blocked(&room_id, &user_a)
            .await
            .expect("Failed to check blocked user"),
        "Expected user_a to be blocked"
    );
    assert!(
        !storage
            .is_user_blocked(&room_id, &user_b)
            .await
            .expect("Failed to check non-blocked user"),
        "Expected user_b to not be blocked"
    );
    assert!(
        storage
            .has_any_invite_restriction(&room_id)
            .await
            .expect("Failed to check invite restriction"),
        "Expected restrictions to exist after blocklist set"
    );

    storage
        .set_invite_allowlist(&room_id, vec![user_b.clone()])
        .await
        .expect("Failed to set invite allowlist");

    let allowlist = storage
        .get_invite_allowlist(&room_id)
        .await
        .expect("Failed to get invite allowlist");
    assert_eq!(allowlist, vec![user_b.clone()]);
    assert!(
        storage
            .is_user_allowed(&room_id, &user_b)
            .await
            .expect("Failed to check allowed user"),
        "Expected user_b to be allowed"
    );
    assert!(
        !storage
            .is_user_allowed(&room_id, &user_a)
            .await
            .expect("Failed to check non-allowed user"),
        "Expected user_a to not be allowed when allowlist set"
    );

    storage
        .set_invite_blocklist(&room_id, vec![])
        .await
        .expect("Failed to clear invite blocklist");
    storage
        .set_invite_allowlist(&room_id, vec![])
        .await
        .expect("Failed to clear invite allowlist");
    assert!(
        !storage
            .has_any_invite_restriction(&room_id)
            .await
            .expect("Failed to check invite restriction after clear"),
        "Expected restrictions to be cleared"
    );

    cleanup_invite_fixtures(&pool, &room_id, &[creator, user_a, user_b]).await;
}

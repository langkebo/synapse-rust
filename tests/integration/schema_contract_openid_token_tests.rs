#![cfg(test)]

#[path = "../common/mod.rs"]
mod common;

use sqlx::Row;
use std::sync::Arc;
use synapse_rust::storage::openid_token::{CreateOpenIdTokenRequest, OpenIdTokenStorage};

async fn connect_pool() -> Option<Arc<sqlx::PgPool>> {
    match common::get_test_pool_async().await {
        Ok(pool) => Some(pool),
        Err(error) => {
            eprintln!(
                "Skipping openid token schema contract integration tests because test database is unavailable: {}",
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

async fn seed_user(pool: &sqlx::PgPool, suffix: &str) -> String {
    let user_id = format!("@schema-openid-user-{suffix}:localhost");
    sqlx::query(
        "INSERT INTO users (user_id, username, created_ts) VALUES ($1, $2, $3) ON CONFLICT (user_id) DO NOTHING",
    )
    .bind(&user_id)
    .bind(format!("schema_openid_user_{suffix}"))
    .bind(0_i64)
    .execute(pool)
    .await
    .expect("Failed to seed user fixture");
    user_id
}

async fn cleanup_openid_fixtures(pool: &sqlx::PgPool, user_id: &str) {
    sqlx::query("DELETE FROM openid_tokens WHERE user_id = $1")
        .bind(user_id)
        .execute(pool)
        .await
        .expect("Failed to cleanup openid_tokens");

    sqlx::query("DELETE FROM users WHERE user_id = $1")
        .bind(user_id)
        .execute(pool)
        .await
        .expect("Failed to cleanup user fixture");
}

#[tokio::test]
async fn test_schema_contract_openid_token_tables_shape() {
    let pool = match connect_pool().await {
        Some(pool) => pool,
        None => return,
    };

    assert_eq!(
        primary_key_columns(&pool, "openid_tokens").await,
        vec!["id".to_string()],
        "Expected openid_tokens PRIMARY KEY(id)"
    );
    assert!(
        has_unique_constraint_on(&pool, "openid_tokens", &["token"]).await,
        "Expected openid_tokens UNIQUE(token)"
    );

    assert_column(
        &pool,
        "openid_tokens",
        "is_valid",
        &["boolean"],
        true,
        Some("true"),
    )
    .await;
    assert_column(
        &pool,
        "openid_tokens",
        "expires_at",
        &["bigint"],
        false,
        None,
    )
    .await;

    assert!(
        has_index_named(&pool, "idx_openid_tokens_user").await,
        "Expected index idx_openid_tokens_user to exist"
    );
}

#[tokio::test]
async fn test_schema_contract_openid_token_query_and_write_read_closure() {
    let pool = match connect_pool().await {
        Some(pool) => pool,
        None => return,
    };

    let storage = OpenIdTokenStorage::new(&pool);
    let suffix = uuid::Uuid::new_v4().to_string();
    let user_id = seed_user(&pool, &suffix).await;

    let token_value = format!("openid-token-{suffix}");
    let expires_at = chrono::Utc::now().timestamp_millis() + 60_000;
    let created = storage
        .create_token(CreateOpenIdTokenRequest {
            token: token_value.clone(),
            user_id: user_id.clone(),
            device_id: None,
            expires_at,
        })
        .await
        .expect("Failed to create openid token fixture");
    assert_eq!(created.user_id, user_id);
    assert!(created.is_valid);

    let validated = storage
        .validate_token(&token_value)
        .await
        .expect("Failed to validate openid token")
        .expect("Expected token to validate");
    assert_eq!(validated.token, token_value);

    let revoked = storage
        .revoke_token(&token_value)
        .await
        .expect("Failed to revoke openid token");
    assert!(revoked);

    assert!(
        storage
            .validate_token(&token_value)
            .await
            .expect("Failed to validate revoked token")
            .is_none(),
        "Revoked token must not validate"
    );

    cleanup_openid_fixtures(&pool, &user_id).await;
}

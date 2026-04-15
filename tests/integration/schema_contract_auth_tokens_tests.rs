#![cfg(test)]

#[path = "../common/mod.rs"]
mod common;

use sqlx::Row;
use std::sync::Arc;
use synapse_rust::storage::refresh_token::{CreateRefreshTokenRequest, RefreshTokenStorage};
use synapse_rust::storage::token::AccessTokenStorage;

async fn connect_pool() -> Option<Arc<sqlx::PgPool>> {
    match common::get_test_pool_async().await {
        Ok(pool) => Some(pool),
        Err(error) => {
            eprintln!(
                "Skipping auth token schema contract integration tests because test database is unavailable: {}",
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

async fn seed_user_and_device(pool: &sqlx::PgPool, suffix: &str) -> (String, String) {
    let user_id = format!("@schema-auth-user-{suffix}:localhost");
    let device_id = format!("SCHEMAAUTHDEVICE-{suffix}");

    sqlx::query(
        "INSERT INTO users (user_id, username, created_ts) VALUES ($1, $2, $3) ON CONFLICT (user_id) DO NOTHING",
    )
    .bind(&user_id)
    .bind(format!("schema_auth_user_{suffix}"))
    .bind(0_i64)
    .execute(pool)
    .await
    .expect("Failed to seed user fixture");

    sqlx::query(
        "INSERT INTO devices (device_id, user_id, created_ts, first_seen_ts) VALUES ($1, $2, $3, $4) ON CONFLICT (device_id) DO NOTHING",
    )
    .bind(&device_id)
    .bind(&user_id)
    .bind(0_i64)
    .bind(0_i64)
    .execute(pool)
    .await
    .expect("Failed to seed device fixture");

    (user_id, device_id)
}

async fn cleanup_auth_fixtures(pool: &sqlx::PgPool, user_id: &str, device_id: &str) {
    sqlx::query("DELETE FROM token_blacklist WHERE user_id = $1")
        .bind(user_id)
        .execute(pool)
        .await
        .expect("Failed to cleanup token_blacklist");

    sqlx::query("DELETE FROM refresh_token_usage WHERE user_id = $1")
        .bind(user_id)
        .execute(pool)
        .await
        .expect("Failed to cleanup refresh_token_usage");

    sqlx::query("DELETE FROM refresh_tokens WHERE user_id = $1")
        .bind(user_id)
        .execute(pool)
        .await
        .expect("Failed to cleanup refresh_tokens");

    sqlx::query("DELETE FROM access_tokens WHERE user_id = $1")
        .bind(user_id)
        .execute(pool)
        .await
        .expect("Failed to cleanup access_tokens");

    sqlx::query("DELETE FROM devices WHERE device_id = $1")
        .bind(device_id)
        .execute(pool)
        .await
        .expect("Failed to cleanup devices");

    sqlx::query("DELETE FROM users WHERE user_id = $1")
        .bind(user_id)
        .execute(pool)
        .await
        .expect("Failed to cleanup users");
}

#[tokio::test]
async fn test_schema_contract_auth_token_tables_shape() {
    let pool = match connect_pool().await {
        Some(pool) => pool,
        None => return,
    };

    assert_eq!(
        primary_key_columns(&pool, "access_tokens").await,
        vec!["id".to_string()],
        "Expected access_tokens PRIMARY KEY(id)"
    );
    assert_eq!(
        primary_key_columns(&pool, "refresh_tokens").await,
        vec!["id".to_string()],
        "Expected refresh_tokens PRIMARY KEY(id)"
    );
    assert_eq!(
        primary_key_columns(&pool, "token_blacklist").await,
        vec!["id".to_string()],
        "Expected token_blacklist PRIMARY KEY(id)"
    );

    assert!(
        has_unique_constraint_on(&pool, "access_tokens", &["token_hash"]).await,
        "Expected access_tokens UNIQUE(token_hash)"
    );
    assert!(
        has_unique_constraint_on(&pool, "refresh_tokens", &["token_hash"]).await,
        "Expected refresh_tokens UNIQUE(token_hash)"
    );
    assert!(
        has_unique_constraint_on(&pool, "token_blacklist", &["token_hash"]).await,
        "Expected token_blacklist UNIQUE(token_hash)"
    );

    assert_column(
        &pool,
        "access_tokens",
        "token_hash",
        &["text", "character varying"],
        true,
        None,
    )
    .await;
    assert_column(
        &pool,
        "access_tokens",
        "is_revoked",
        &["boolean"],
        true,
        Some("false"),
    )
    .await;
    assert_column(
        &pool,
        "refresh_tokens",
        "use_count",
        &["integer"],
        true,
        Some("0"),
    )
    .await;
    assert_column(
        &pool,
        "refresh_tokens",
        "is_revoked",
        &["boolean"],
        true,
        Some("false"),
    )
    .await;
    assert_column(
        &pool,
        "token_blacklist",
        "token_type",
        &["text", "character varying"],
        true,
        Some("access"),
    )
    .await;
    assert_column(
        &pool,
        "token_blacklist",
        "is_revoked",
        &["boolean"],
        true,
        Some("true"),
    )
    .await;

    for index_name in [
        "idx_access_tokens_user_id",
        "idx_access_tokens_token_hash",
        "idx_access_tokens_valid",
        "idx_refresh_tokens_user_id",
        "idx_refresh_tokens_revoked",
        "idx_token_blacklist_hash",
    ] {
        assert!(
            has_index_named(&pool, index_name).await,
            "Expected index {} to exist",
            index_name
        );
    }
}

#[tokio::test]
async fn test_schema_contract_auth_token_query_and_write_read_closure() {
    let pool = match connect_pool().await {
        Some(pool) => pool,
        None => return,
    };

    let suffix = uuid::Uuid::new_v4().to_string();
    let (user_id, device_id) = seed_user_and_device(&pool, &suffix).await;

    let token_storage = AccessTokenStorage::new(&pool);
    let refresh_storage = RefreshTokenStorage::new(&pool);

    let access_token_value = format!("access-token-{suffix}");
    let created_access = token_storage
        .create_token(&access_token_value, &user_id, Some(&device_id), None)
        .await
        .expect("Failed to create access token fixture");
    assert_eq!(created_access.user_id, user_id);
    assert_eq!(
        created_access.device_id.as_deref(),
        Some(device_id.as_str())
    );
    assert!(!created_access.is_revoked);

    assert!(
        token_storage
            .token_exists(&access_token_value)
            .await
            .expect("Failed to check token exists"),
        "Expected token to exist"
    );

    let fetched_access = token_storage
        .get_token(&access_token_value)
        .await
        .expect("Failed to fetch access token")
        .expect("Expected access token row");
    assert_eq!(fetched_access.token_hash.len(), 43);

    let raw_access_row = sqlx::query("SELECT token, token_hash FROM access_tokens WHERE id = $1")
        .bind(created_access.id)
        .fetch_one(&*pool)
        .await
        .expect("Failed to fetch raw access token row");
    assert!(
        raw_access_row.get::<Option<String>, _>("token").is_none(),
        "Expected access token plaintext to be cleared in storage"
    );
    assert_eq!(
        raw_access_row.get::<String, _>("token_hash"),
        fetched_access.token_hash
    );

    token_storage
        .add_to_blacklist(&access_token_value, &user_id, Some("logout"))
        .await
        .expect("Failed to add token to blacklist");
    assert!(
        token_storage
            .is_in_blacklist(&access_token_value)
            .await
            .expect("Failed to check token blacklist"),
        "Expected token to be blacklisted"
    );

    token_storage
        .delete_token(&access_token_value)
        .await
        .expect("Failed to revoke access token");
    assert!(
        token_storage
            .is_token_revoked(&access_token_value)
            .await
            .expect("Failed to check revoked token"),
        "Expected token to be revoked"
    );
    assert!(
        token_storage
            .get_token(&access_token_value)
            .await
            .expect("Failed to fetch revoked token")
            .is_none(),
        "Revoked token should not be returned by get_token"
    );

    let refresh_hash = format!("refresh-hash-{suffix}");
    let refresh = refresh_storage
        .create_token(CreateRefreshTokenRequest {
            token_hash: refresh_hash.clone(),
            user_id: user_id.clone(),
            device_id: Some(device_id.clone()),
            access_token_id: Some(created_access.id.to_string()),
            scope: Some("refresh".to_string()),
            expires_at: chrono::Utc::now().timestamp_millis() + 3_600_000,
            client_info: None,
            ip_address: None,
            user_agent: None,
        })
        .await
        .expect("Failed to create refresh token fixture");
    assert_eq!(refresh.token_hash, refresh_hash);
    assert_eq!(refresh.user_id, user_id);
    assert!(!refresh.is_revoked);

    let active = refresh_storage
        .get_active_tokens(&user_id)
        .await
        .expect("Failed to query active refresh tokens");
    assert_eq!(active.len(), 1);

    refresh_storage
        .update_token_usage(&refresh_hash, &created_access.id.to_string())
        .await
        .expect("Failed to update refresh token usage");
    let after_usage = refresh_storage
        .get_token(&refresh_hash)
        .await
        .expect("Failed to fetch refresh token")
        .expect("Expected refresh token");
    assert!(after_usage.use_count >= 1);
    assert!(after_usage.last_used_ts.is_some());

    refresh_storage
        .revoke_token(&refresh_hash, "rotate")
        .await
        .expect("Failed to revoke refresh token");
    let revoked = refresh_storage
        .get_token(&refresh_hash)
        .await
        .expect("Failed to fetch revoked refresh token")
        .expect("Expected revoked refresh token");
    assert!(revoked.is_revoked);
    assert_eq!(revoked.revoked_reason.as_deref(), Some("rotate"));

    cleanup_auth_fixtures(&pool, &user_id, &device_id).await;
}

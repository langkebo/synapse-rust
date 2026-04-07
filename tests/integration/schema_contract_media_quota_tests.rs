#![cfg(test)]

#[path = "../common/mod.rs"]
mod common;

use sqlx::Row;
use std::sync::Arc;
use synapse_rust::storage::media_quota::{
    CreateQuotaConfigRequest, MediaQuotaStorage, SetUserQuotaRequest, UpdateUsageRequest,
};

async fn connect_pool() -> Option<Arc<sqlx::PgPool>> {
    match common::get_test_pool_async().await {
        Ok(pool) => Some(pool),
        Err(error) => {
            eprintln!(
                "Skipping media quota schema contract integration tests because test database is unavailable: {}",
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

async fn seed_user(pool: &sqlx::PgPool, suffix: &str) -> String {
    let user_id = format!("@schema-media-user-{suffix}:localhost");
    sqlx::query(
        "INSERT INTO users (user_id, username, created_ts) VALUES ($1, $2, $3) ON CONFLICT (user_id) DO NOTHING",
    )
    .bind(&user_id)
    .bind(format!("schema_media_user_{suffix}"))
    .bind(0_i64)
    .execute(pool)
    .await
    .expect("Failed to seed user fixture");
    user_id
}

async fn cleanup_media_quota_fixtures(pool: &sqlx::PgPool, user_id: &str, config_name: &str) {
    sqlx::query("DELETE FROM media_quota_alerts WHERE user_id = $1")
        .bind(user_id)
        .execute(pool)
        .await
        .expect("Failed to cleanup media_quota_alerts");

    sqlx::query("DELETE FROM media_usage_log WHERE user_id = $1")
        .bind(user_id)
        .execute(pool)
        .await
        .expect("Failed to cleanup media_usage_log");

    sqlx::query("DELETE FROM user_media_quota WHERE user_id = $1")
        .bind(user_id)
        .execute(pool)
        .await
        .expect("Failed to cleanup user_media_quota");

    sqlx::query("DELETE FROM media_quota_config WHERE name = $1")
        .bind(config_name)
        .execute(pool)
        .await
        .expect("Failed to cleanup media_quota_config");

    sqlx::query("DELETE FROM users WHERE user_id = $1")
        .bind(user_id)
        .execute(pool)
        .await
        .expect("Failed to cleanup users");
}

#[tokio::test]
async fn test_schema_contract_media_quota_tables_shape() {
    let pool = match connect_pool().await {
        Some(pool) => pool,
        None => return,
    };

    for table_name in [
        "media_quota_config",
        "user_media_quota",
        "media_usage_log",
        "media_quota_alerts",
        "server_media_quota",
    ] {
        assert_eq!(
            primary_key_columns(&pool, table_name).await,
            vec!["id".to_string()],
            "Expected {table_name} PRIMARY KEY(id)"
        );
    }

    assert!(
        has_unique_constraint_on(&pool, "user_media_quota", &["user_id"]).await,
        "Expected user_media_quota UNIQUE(user_id)"
    );

    assert_column(
        &pool,
        "media_quota_config",
        "name",
        &["text", "character varying"],
        false,
        Some("default"),
    )
    .await;
    assert_column(
        &pool,
        "media_quota_config",
        "max_storage_bytes",
        &["bigint"],
        false,
        Some("10737418240"),
    )
    .await;
    assert_column(
        &pool,
        "media_quota_config",
        "allowed_mime_types",
        &["jsonb"],
        false,
        Some("[]"),
    )
    .await;
    assert_column(
        &pool,
        "user_media_quota",
        "current_storage_bytes",
        &["bigint"],
        false,
        Some("0"),
    )
    .await;
    assert_column(
        &pool,
        "user_media_quota",
        "current_files_count",
        &["integer"],
        false,
        Some("0"),
    )
    .await;
    assert_column(
        &pool,
        "media_quota_alerts",
        "is_read",
        &["boolean"],
        false,
        Some("false"),
    )
    .await;
    assert_column(
        &pool,
        "server_media_quota",
        "alert_threshold_percent",
        &["integer"],
        false,
        Some("80"),
    )
    .await;

    for index_name in [
        "idx_user_media_quota_used",
        "idx_media_usage_log_user",
        "idx_media_usage_log_timestamp",
        "idx_media_quota_alerts_user",
    ] {
        assert!(
            has_index_named(&pool, index_name).await,
            "Expected index {} to exist",
            index_name
        );
    }
}

#[tokio::test]
async fn test_schema_contract_media_quota_query_and_write_read_closure() {
    let pool = match connect_pool().await {
        Some(pool) => pool,
        None => return,
    };

    let storage = MediaQuotaStorage::new(&pool);
    let suffix = uuid::Uuid::new_v4().to_string();
    let config_name = format!("schema-media-config-{suffix}");
    let user_id = seed_user(&pool, &suffix).await;

    let config = storage
        .create_config(CreateQuotaConfigRequest {
            name: config_name.clone(),
            description: Some("schema contract media quota".to_string()),
            max_storage_bytes: 2048,
            max_file_size_bytes: 1024,
            max_files_count: 10,
            allowed_mime_types: Some(vec!["image/png".to_string()]),
            blocked_mime_types: Some(vec!["application/x-msdownload".to_string()]),
            is_default: Some(true),
        })
        .await
        .expect("Failed to create media quota config");
    assert_eq!(config.name, config_name);
    assert_eq!(config.max_storage_bytes, 2048);
    assert!(config.is_default);

    let default_config = storage
        .get_default_config()
        .await
        .expect("Failed to get default config")
        .expect("Expected default config");
    assert_eq!(default_config.id, config.id);

    let created_quota = storage
        .get_or_create_user_quota(&user_id)
        .await
        .expect("Failed to get or create user quota");
    assert_eq!(created_quota.quota_config_id, Some(config.id));
    assert_eq!(created_quota.current_storage_bytes, 0);

    let updated_quota = storage
        .set_user_quota(SetUserQuotaRequest {
            user_id: user_id.clone(),
            quota_config_id: Some(config.id),
            custom_max_storage_bytes: Some(2048),
            custom_max_file_size_bytes: Some(1024),
            custom_max_files_count: Some(10),
        })
        .await
        .expect("Failed to set user quota");
    assert_eq!(updated_quota.custom_max_storage_bytes, Some(2048));

    let initial_check = storage
        .check_quota(&user_id, 1024)
        .await
        .expect("Failed to check initial quota");
    assert!(initial_check.allowed);

    storage
        .update_usage(UpdateUsageRequest {
            user_id: user_id.clone(),
            media_id: format!("media-{suffix}"),
            file_size_bytes: 1024,
            mime_type: Some("image/png".to_string()),
            operation: "upload".to_string(),
        })
        .await
        .expect("Failed to record upload usage");

    let quota_after_upload = storage
        .get_user_quota(&user_id)
        .await
        .expect("Failed to fetch quota after upload")
        .expect("Expected user quota after upload");
    assert_eq!(quota_after_upload.current_storage_bytes, 1024);
    assert_eq!(quota_after_upload.current_files_count, 1);

    let post_upload_check = storage
        .check_quota(&user_id, 1200)
        .await
        .expect("Failed to check quota after upload");
    assert!(!post_upload_check.allowed);

    let stats = storage
        .get_usage_stats(&user_id)
        .await
        .expect("Failed to get usage stats");
    assert_eq!(stats["current_storage_bytes"].as_i64(), Some(1024));
    assert_eq!(stats["current_files_count"].as_i64(), Some(1));
    assert_eq!(stats["recent_uploads_bytes"].as_i64(), Some(1024));

    let alert = storage
        .create_alert(&user_id, "warning", 80, 1024, 2048, Some("quota warning"))
        .await
        .expect("Failed to create quota alert");
    assert!(!alert.is_read);

    let unread_alerts = storage
        .get_user_alerts(&user_id, true)
        .await
        .expect("Failed to query unread alerts");
    assert_eq!(unread_alerts.len(), 1);
    assert_eq!(unread_alerts[0].id, alert.id);

    assert!(
        storage
            .mark_alert_read(alert.id)
            .await
            .expect("Failed to mark alert read"),
        "Expected mark_alert_read to update row"
    );
    let unread_after_mark = storage
        .get_user_alerts(&user_id, true)
        .await
        .expect("Failed to query unread alerts after mark");
    assert!(unread_after_mark.is_empty());

    let server_quota = storage
        .update_server_quota(Some(4096), Some(2048), Some(20), Some(95))
        .await
        .expect("Failed to update server quota");
    assert_eq!(server_quota.max_storage_bytes, Some(4096));
    assert_eq!(server_quota.max_file_size_bytes, Some(2048));
    assert_eq!(server_quota.alert_threshold_percent, 95);

    storage
        .update_usage(UpdateUsageRequest {
            user_id: user_id.clone(),
            media_id: format!("media-{suffix}"),
            file_size_bytes: 1024,
            mime_type: Some("image/png".to_string()),
            operation: "delete".to_string(),
        })
        .await
        .expect("Failed to record delete usage");

    let quota_after_delete = storage
        .get_user_quota(&user_id)
        .await
        .expect("Failed to fetch quota after delete")
        .expect("Expected user quota after delete");
    assert_eq!(quota_after_delete.current_storage_bytes, 0);
    assert_eq!(quota_after_delete.current_files_count, 0);

    cleanup_media_quota_fixtures(&pool, &user_id, &config_name).await;
}

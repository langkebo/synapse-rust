use std::sync::Arc;

use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

use super::*;

async fn test_pool() -> Arc<PgPool> {
    let db_url = std::env::var("TEST_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://synapse:synapse@localhost:15432/synapse".to_string());
    let pool =
        PgPoolOptions::new().max_connections(2).connect(&db_url).await.expect("Failed to connect to test database");
    Arc::new(pool)
}

async fn ensure_test_user(pool: &PgPool, user_id: &str) {
    let username = user_id.strip_prefix('@').and_then(|u| u.split(':').next()).unwrap_or("testuser");
    sqlx::query(
        "INSERT INTO users (user_id, username, created_ts) VALUES ($1, $2, EXTRACT(EPOCH FROM NOW()) * 1000) ON CONFLICT (user_id) DO NOTHING",
    )
    .bind(user_id)
    .bind(username)
    .execute(pool)
    .await
    .ok();
}

async fn ensure_server_quota_row(pool: &PgPool) {
    let now = chrono::Utc::now().timestamp_millis();
    sqlx::query(
        "INSERT INTO server_media_quota (id, current_storage_bytes, current_files_count, alert_threshold_percent, updated_ts) VALUES (1, 0, 0, 80, $1) ON CONFLICT (id) DO NOTHING",
    )
    .bind(now)
    .execute(pool)
    .await
    .ok();
}

async fn cleanup_test_data(pool: &PgPool, suffix: &str) {
    let pattern = format!("%{suffix}%");
    sqlx::query("DELETE FROM media_quota_alerts WHERE user_id LIKE $1").bind(&pattern).execute(pool).await.ok();
    sqlx::query("DELETE FROM media_usage_log WHERE user_id LIKE $1").bind(&pattern).execute(pool).await.ok();
    sqlx::query("DELETE FROM user_media_quota WHERE user_id LIKE $1").bind(&pattern).execute(pool).await.ok();
    sqlx::query("DELETE FROM media_quota_config WHERE config_name LIKE $1").bind(&pattern).execute(pool).await.ok();
}

// —— get_default_config ——

#[tokio::test]
async fn test_get_default_config_found() {
    let pool = test_pool().await;
    let storage = MediaQuotaStorage::new(&pool);
    let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
    let config_name = format!("mq_default_config_{suffix}");

    cleanup_test_data(&pool, &suffix).await;
    // Directly insert a default enabled config so get_default_config can find it.
    let now = chrono::Utc::now().timestamp_millis();
    sqlx::query(
        "INSERT INTO media_quota_config (config_name, name, max_storage_bytes, max_file_size_bytes, max_files_count, allowed_mime_types, blocked_mime_types, is_default, is_enabled, created_ts) VALUES ($1, $1, 1073741824, 10485760, 1000, '[]'::jsonb, '[]'::jsonb, TRUE, TRUE, $2)",
    )
    .bind(&config_name)
    .bind(now)
    .execute(pool.as_ref())
    .await
    .expect("should insert default config");

    let result = storage.get_default_config().await.expect("should succeed");
    assert!(result.is_some(), "default config should be found");
    let config = result.unwrap();
    assert_eq!(config.name, config_name);
    assert!(config.is_default);
    assert!(config.is_enabled);

    cleanup_test_data(&pool, &suffix).await;
}

#[tokio::test]
async fn test_get_default_config_not_found() {
    let pool = test_pool().await;
    let storage = MediaQuotaStorage::new(&pool);
    let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
    cleanup_test_data(&pool, &suffix).await;

    let result = storage.get_default_config().await.expect("should succeed");
    assert!(result.is_none(), "should be None when no default config exists");

    cleanup_test_data(&pool, &suffix).await;
}

// —— CRUD: create_config / get_config / list_configs / delete_config ——

#[tokio::test]
async fn test_create_config() {
    let pool = test_pool().await;
    let storage = MediaQuotaStorage::new(&pool);
    let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
    let config_name = format!("mq_crud_{suffix}");

    cleanup_test_data(&pool, &suffix).await;

    let request = CreateQuotaConfigRequest {
        name: config_name.clone(),
        description: Some("CRUD test config".to_string()),
        max_storage_bytes: 5_000_000,
        max_file_size_bytes: 1_000_000,
        max_files_count: 500,
        allowed_mime_types: Some(vec!["image/png".to_string()]),
        blocked_mime_types: Some(vec!["application/exe".to_string()]),
        is_default: Some(false),
    };

    let config = storage.create_config(request).await.expect("should create config");

    assert_eq!(config.name, config_name);
    assert_eq!(config.max_storage_bytes, 5_000_000);
    assert_eq!(config.max_file_size_bytes, 1_000_000);
    assert_eq!(config.max_files_count, 500);
    assert!(!config.is_default);
    assert!(config.is_enabled);
    assert!(config.id > 0);

    cleanup_test_data(&pool, &suffix).await;
}

#[tokio::test]
async fn test_get_config() {
    let pool = test_pool().await;
    let storage = MediaQuotaStorage::new(&pool);
    let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
    let config_name = format!("mq_get_{suffix}");

    cleanup_test_data(&pool, &suffix).await;

    let request = CreateQuotaConfigRequest {
        name: config_name.clone(),
        description: None,
        max_storage_bytes: 10_000_000,
        max_file_size_bytes: 2_000_000,
        max_files_count: 200,
        allowed_mime_types: None,
        blocked_mime_types: None,
        is_default: Some(false),
    };
    let created = storage.create_config(request).await.expect("should create config");

    let fetched = storage.get_config(created.id).await.expect("should succeed");
    assert!(fetched.is_some());
    let fetched = fetched.unwrap();
    assert_eq!(fetched.id, created.id);
    assert_eq!(fetched.name, config_name);
    assert_eq!(fetched.max_storage_bytes, 10_000_000);

    // get_config for non-existent id
    let missing = storage.get_config(99999999).await.expect("should succeed");
    assert!(missing.is_none());

    cleanup_test_data(&pool, &suffix).await;
}

#[tokio::test]
async fn test_list_configs() {
    let pool = test_pool().await;
    let storage = MediaQuotaStorage::new(&pool);
    let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
    let name_a = format!("mq_list_a_{suffix}");
    let name_b = format!("mq_list_b_{suffix}");

    cleanup_test_data(&pool, &suffix).await;

    storage
        .create_config(CreateQuotaConfigRequest {
            name: name_a.clone(),
            description: None,
            max_storage_bytes: 1_000_000,
            max_file_size_bytes: 100_000,
            max_files_count: 10,
            allowed_mime_types: None,
            blocked_mime_types: None,
            is_default: Some(false),
        })
        .await
        .expect("should create config A");

    storage
        .create_config(CreateQuotaConfigRequest {
            name: name_b.clone(),
            description: None,
            max_storage_bytes: 2_000_000,
            max_file_size_bytes: 200_000,
            max_files_count: 20,
            allowed_mime_types: None,
            blocked_mime_types: None,
            is_default: Some(false),
        })
        .await
        .expect("should create config B");

    let configs = storage.list_configs().await.expect("should list configs");
    assert!(configs.iter().any(|c| c.name == name_a), "should contain config A");
    assert!(configs.iter().any(|c| c.name == name_b), "should contain config B");

    cleanup_test_data(&pool, &suffix).await;
}

#[tokio::test]
async fn test_delete_config() {
    let pool = test_pool().await;
    let storage = MediaQuotaStorage::new(&pool);
    let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
    let config_name = format!("mq_delete_{suffix}");

    cleanup_test_data(&pool, &suffix).await;

    let created = storage
        .create_config(CreateQuotaConfigRequest {
            name: config_name.clone(),
            description: None,
            max_storage_bytes: 1_000_000,
            max_file_size_bytes: 100_000,
            max_files_count: 10,
            allowed_mime_types: None,
            blocked_mime_types: None,
            is_default: Some(false),
        })
        .await
        .expect("should create config");

    let deleted = storage.delete_config(created.id).await.expect("should succeed");
    assert!(deleted, "delete should return true for existing config");

    // Double-delete should return false (already disabled).
    let deleted_again = storage.delete_config(created.id).await.expect("should succeed");
    assert!(!deleted_again, "second delete should return false");

    // get_config still returns the row (it only filters by id, not by is_enabled).
    let fetched = storage.get_config(created.id).await.expect("should succeed");
    assert!(fetched.is_some(), "row still exists but is_enabled=false");
    assert!(!fetched.unwrap().is_enabled);

    cleanup_test_data(&pool, &suffix).await;
}

// —— get_user_quota ——

#[tokio::test]
async fn test_get_user_quota_found() {
    let pool = test_pool().await;
    let storage = MediaQuotaStorage::new(&pool);
    let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
    let user_id = format!("@mq_uq_{suffix}:localhost");

    cleanup_test_data(&pool, &suffix).await;
    ensure_test_user(&pool, &user_id).await;

    // Populate a user_media_quota row via set_user_quota (UPSERT).
    storage
        .set_user_quota(SetUserQuotaRequest {
            user_id: user_id.clone(),
            quota_config_id: None,
            custom_max_storage_bytes: Some(50_000_000),
            custom_max_file_size_bytes: None,
            custom_max_files_count: None,
        })
        .await
        .expect("should set user quota");

    let quota = storage.get_user_quota(&user_id).await.expect("should succeed");
    assert!(quota.is_some(), "user quota should be found");
    let quota = quota.unwrap();
    assert_eq!(quota.user_id, user_id);
    assert_eq!(quota.custom_max_storage_bytes, Some(50_000_000));

    cleanup_test_data(&pool, &suffix).await;
}

#[tokio::test]
async fn test_get_user_quota_not_found() {
    let pool = test_pool().await;
    let storage = MediaQuotaStorage::new(&pool);
    let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
    let user_id = format!("@mq_nf_{suffix}:localhost");

    cleanup_test_data(&pool, &suffix).await;

    let result = storage.get_user_quota(&user_id).await.expect("should succeed");
    assert!(result.is_none(), "should be None for unknown user");

    cleanup_test_data(&pool, &suffix).await;
}

// —— get_or_create_user_quota ——

#[tokio::test]
async fn test_get_or_create_user_quota_creates() {
    let pool = test_pool().await;
    let storage = MediaQuotaStorage::new(&pool);
    let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
    let user_id = format!("@mq_goc_{suffix}:localhost");

    cleanup_test_data(&pool, &suffix).await;
    ensure_test_user(&pool, &user_id).await;

    let quota = storage.get_or_create_user_quota(&user_id).await.expect("should succeed");
    assert_eq!(quota.user_id, user_id);
    assert!(quota.id > 0);
    // No default config exists, so quota_config_id should be None.
    assert_eq!(quota.quota_config_id, None);

    cleanup_test_data(&pool, &suffix).await;
}

#[tokio::test]
async fn test_get_or_create_user_quota_returns_existing() {
    let pool = test_pool().await;
    let storage = MediaQuotaStorage::new(&pool);
    let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
    let user_id = format!("@mq_goe_{suffix}:localhost");

    cleanup_test_data(&pool, &suffix).await;
    ensure_test_user(&pool, &user_id).await;

    // First call creates.
    let first = storage.get_or_create_user_quota(&user_id).await.expect("should succeed");
    let first_id = first.id;

    // Second call returns existing.
    let second = storage.get_or_create_user_quota(&user_id).await.expect("should succeed");
    assert_eq!(second.id, first_id, "should return the same row");
    assert_eq!(second.user_id, first.user_id);

    cleanup_test_data(&pool, &suffix).await;
}

// —— set_user_quota ——

#[tokio::test]
async fn test_set_user_quota_sets_custom_limit() {
    let pool = test_pool().await;
    let storage = MediaQuotaStorage::new(&pool);
    let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
    let user_id = format!("@mq_sql_{suffix}:localhost");

    cleanup_test_data(&pool, &suffix).await;
    ensure_test_user(&pool, &user_id).await;

    let quota = storage
        .set_user_quota(SetUserQuotaRequest {
            user_id: user_id.clone(),
            quota_config_id: Some(42),
            custom_max_storage_bytes: Some(100_000_000),
            custom_max_file_size_bytes: Some(10_000_000),
            custom_max_files_count: Some(1000),
        })
        .await
        .expect("should set quota");

    assert_eq!(quota.user_id, user_id);
    assert_eq!(quota.quota_config_id, Some(42));
    assert_eq!(quota.custom_max_storage_bytes, Some(100_000_000));
    assert_eq!(quota.custom_max_file_size_bytes, Some(10_000_000));
    assert_eq!(quota.custom_max_files_count, Some(1000));

    cleanup_test_data(&pool, &suffix).await;
}

#[tokio::test]
async fn test_set_user_quota_updates_defaults() {
    let pool = test_pool().await;
    let storage = MediaQuotaStorage::new(&pool);
    let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
    let user_id = format!("@mq_sqd_{suffix}:localhost");

    cleanup_test_data(&pool, &suffix).await;
    ensure_test_user(&pool, &user_id).await;

    // Set full custom limits first.
    storage
        .set_user_quota(SetUserQuotaRequest {
            user_id: user_id.clone(),
            quota_config_id: Some(10),
            custom_max_storage_bytes: Some(50_000_000),
            custom_max_file_size_bytes: Some(5_000_000),
            custom_max_files_count: Some(500),
        })
        .await
        .expect("should set initial quota");

    // Update with only partial fields — unset fields become None.
    let updated = storage
        .set_user_quota(SetUserQuotaRequest {
            user_id: user_id.clone(),
            quota_config_id: Some(20),
            custom_max_storage_bytes: None,
            custom_max_file_size_bytes: None,
            custom_max_files_count: Some(200),
        })
        .await
        .expect("should update quota");

    assert_eq!(updated.quota_config_id, Some(20));
    assert_eq!(updated.custom_max_storage_bytes, None);
    assert_eq!(updated.custom_max_file_size_bytes, None);
    assert_eq!(updated.custom_max_files_count, Some(200));

    cleanup_test_data(&pool, &suffix).await;
}

// —— update_usage ——

#[tokio::test]
async fn test_update_usage_upload_increments() {
    let pool = test_pool().await;
    let storage = MediaQuotaStorage::new(&pool);
    let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
    let user_id = format!("@mq_up_{suffix}:localhost");
    let media_id = format!("media_up_{suffix}");

    cleanup_test_data(&pool, &suffix).await;
    ensure_test_user(&pool, &user_id).await;
    ensure_server_quota_row(&pool).await;

    storage
        .update_usage(UpdateUsageRequest {
            user_id: user_id.clone(),
            media_id: media_id.clone(),
            file_size_bytes: 500_000,
            mime_type: Some("image/png".to_string()),
            operation: "upload".to_string(),
        })
        .await
        .expect("should log upload");

    let quota = storage.get_user_quota(&user_id).await.expect("should succeed").expect("user quota should exist");
    assert_eq!(quota.current_storage_bytes, 500_000);
    assert_eq!(quota.current_files_count, 1);

    cleanup_test_data(&pool, &suffix).await;
}

#[tokio::test]
async fn test_update_usage_multiple_accumulates() {
    let pool = test_pool().await;
    let storage = MediaQuotaStorage::new(&pool);
    let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
    let user_id = format!("@mq_ma_{suffix}:localhost");
    let media_a = format!("media_ma_a_{suffix}");
    let media_b = format!("media_ma_b_{suffix}");

    cleanup_test_data(&pool, &suffix).await;
    ensure_test_user(&pool, &user_id).await;
    ensure_server_quota_row(&pool).await;

    // First upload creates the user_media_quota row.
    storage
        .update_usage(UpdateUsageRequest {
            user_id: user_id.clone(),
            media_id: media_a,
            file_size_bytes: 200_000,
            mime_type: Some("image/png".to_string()),
            operation: "upload".to_string(),
        })
        .await
        .expect("should log first upload");

    // Second upload accumulates on top.
    storage
        .update_usage(UpdateUsageRequest {
            user_id: user_id.clone(),
            media_id: media_b,
            file_size_bytes: 300_000,
            mime_type: Some("image/jpeg".to_string()),
            operation: "upload".to_string(),
        })
        .await
        .expect("should log second upload");

    let quota = storage.get_user_quota(&user_id).await.expect("should succeed").expect("user quota should exist");
    assert_eq!(quota.current_storage_bytes, 500_000);
    assert_eq!(quota.current_files_count, 2);

    cleanup_test_data(&pool, &suffix).await;
}

#[tokio::test]
async fn test_update_usage_delete_decrements() {
    let pool = test_pool().await;
    let storage = MediaQuotaStorage::new(&pool);
    let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
    let user_id = format!("@mq_del_{suffix}:localhost");
    let media_id = format!("media_del_{suffix}");

    cleanup_test_data(&pool, &suffix).await;
    ensure_test_user(&pool, &user_id).await;
    ensure_server_quota_row(&pool).await;

    // Upload first.
    storage
        .update_usage(UpdateUsageRequest {
            user_id: user_id.clone(),
            media_id: media_id.clone(),
            file_size_bytes: 1_000_000,
            mime_type: Some("video/mp4".to_string()),
            operation: "upload".to_string(),
        })
        .await
        .expect("should log upload");

    // Delete.
    storage
        .update_usage(UpdateUsageRequest {
            user_id: user_id.clone(),
            media_id: format!("{media_id}_del"),
            file_size_bytes: 600_000,
            mime_type: Some("video/mp4".to_string()),
            operation: "delete".to_string(),
        })
        .await
        .expect("should log delete");

    let quota = storage.get_user_quota(&user_id).await.expect("should succeed").expect("user quota should exist");
    assert_eq!(quota.current_storage_bytes, 400_000);
    assert_eq!(quota.current_files_count, 0);

    cleanup_test_data(&pool, &suffix).await;
}

// —— check_quota ——

#[tokio::test]
async fn test_check_quota_allowed() {
    let pool = test_pool().await;
    let storage = MediaQuotaStorage::new(&pool);
    let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
    let user_id = format!("@mq_ca_{suffix}:localhost");

    cleanup_test_data(&pool, &suffix).await;
    ensure_test_user(&pool, &user_id).await;

    // Set a custom storage limit of 100_000.
    storage
        .set_user_quota(SetUserQuotaRequest {
            user_id: user_id.clone(),
            quota_config_id: None,
            custom_max_storage_bytes: Some(100_000),
            custom_max_file_size_bytes: None,
            custom_max_files_count: None,
        })
        .await
        .expect("should set quota");

    // Current usage is 0 (default), check a 50_000-byte file.
    let result = storage.check_quota(&user_id, 50_000).await.expect("should check quota");
    assert!(result.is_allowed, "should be allowed under limit");
    assert_eq!(result.quota_limit, 100_000);
    assert_eq!(result.current_usage, 0);

    cleanup_test_data(&pool, &suffix).await;
}

#[tokio::test]
async fn test_check_quota_exceeded() {
    let pool = test_pool().await;
    let storage = MediaQuotaStorage::new(&pool);
    let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
    let user_id = format!("@mq_ce_{suffix}:localhost");

    cleanup_test_data(&pool, &suffix).await;
    ensure_test_user(&pool, &user_id).await;
    ensure_server_quota_row(&pool).await;

    // Set a low custom storage limit.
    storage
        .set_user_quota(SetUserQuotaRequest {
            user_id: user_id.clone(),
            quota_config_id: None,
            custom_max_storage_bytes: Some(1_000),
            custom_max_file_size_bytes: None,
            custom_max_files_count: None,
        })
        .await
        .expect("should set quota");

    // Upload 900 bytes.
    storage
        .update_usage(UpdateUsageRequest {
            user_id: user_id.clone(),
            media_id: format!("media_ce_{suffix}"),
            file_size_bytes: 900,
            mime_type: None,
            operation: "upload".to_string(),
        })
        .await
        .expect("should log upload");

    // Check with 200 more bytes -> 1100 > 1000.
    let result = storage.check_quota(&user_id, 200).await.expect("should check quota");
    assert!(!result.is_allowed, "should be exceeded");
    assert!(result.reason.is_some());
    assert_eq!(result.quota_limit, 1_000);
    assert_eq!(result.current_usage, 900);

    cleanup_test_data(&pool, &suffix).await;
}

#[tokio::test]
async fn test_check_quota_no_limit_always_allowed() {
    let pool = test_pool().await;
    let storage = MediaQuotaStorage::new(&pool);
    let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
    let user_id = format!("@mq_cz_{suffix}:localhost");

    cleanup_test_data(&pool, &suffix).await;
    ensure_test_user(&pool, &user_id).await;

    // No quota config, no custom limits — max_storage = 0 — always allowed.
    let result = storage.check_quota(&user_id, 9_999_999_999).await.expect("should check quota");
    assert!(result.is_allowed, "should always be allowed when limit is 0");
    assert_eq!(result.quota_limit, 0);
    assert!(result.reason.is_none());

    cleanup_test_data(&pool, &suffix).await;
}

// —— server_quota ——

#[tokio::test]
async fn test_get_server_quota() {
    let pool = test_pool().await;
    let storage = MediaQuotaStorage::new(&pool);

    ensure_server_quota_row(&pool).await;

    let quota = storage.get_server_quota().await.expect("should succeed");
    assert_eq!(quota.id, 1);
    // alert_threshold_percent may differ from the insert default if a
    // pre-existing row was already present (ON CONFLICT DO NOTHING).
    assert!(quota.alert_threshold_percent > 0, "should have a threshold");
}

#[tokio::test]
async fn test_update_server_quota() {
    let pool = test_pool().await;
    let storage = MediaQuotaStorage::new(&pool);

    ensure_server_quota_row(&pool).await;

    let updated = storage
        .update_server_quota(Some(500_000_000_000_i64), Some(100_000_000_i64), Some(50000_i32), Some(95_i32))
        .await
        .expect("should update server quota");

    assert_eq!(updated.max_storage_bytes, Some(500_000_000_000_i64));
    assert_eq!(updated.max_file_size_bytes, Some(100_000_000_i64));
    assert_eq!(updated.max_files_count, Some(50000_i32));
    assert_eq!(updated.alert_threshold_percent, 95);

    // Verify persisted.
    let fetched = storage.get_server_quota().await.expect("should succeed");
    assert_eq!(fetched.max_storage_bytes, Some(500_000_000_000_i64));
    assert_eq!(fetched.alert_threshold_percent, 95);
}

// —— create_alert / get_user_alerts ——

#[tokio::test]
async fn test_create_alert_and_get_user_alerts() {
    let pool = test_pool().await;
    let storage = MediaQuotaStorage::new(&pool);
    let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
    let user_id = format!("@mq_alert_{suffix}:localhost");

    cleanup_test_data(&pool, &suffix).await;
    ensure_test_user(&pool, &user_id).await;

    let alert = storage
        .create_alert(&user_id, "warning", 80, 800_000, 1_000_000, Some("Storage at 80%"))
        .await
        .expect("should create alert");

    assert_eq!(alert.user_id, user_id);
    assert_eq!(alert.alert_type, "warning");
    assert_eq!(alert.threshold_percent, 80);
    assert!(!alert.is_read);
    assert!(alert.id > 0);

    // Retrieve all alerts for the user.
    let alerts = storage.get_user_alerts(&user_id, false).await.expect("should get alerts");
    assert!(!alerts.is_empty());
    assert!(alerts.iter().any(|a| a.id == alert.id));

    cleanup_test_data(&pool, &suffix).await;
}

#[tokio::test]
async fn test_get_user_alerts_unread_only() {
    let pool = test_pool().await;
    let storage = MediaQuotaStorage::new(&pool);
    let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
    let user_id = format!("@mq_unread_{suffix}:localhost");

    cleanup_test_data(&pool, &suffix).await;
    ensure_test_user(&pool, &user_id).await;

    // Create two alerts.
    let alert1 =
        storage.create_alert(&user_id, "warning", 50, 500_000, 1_000_000, None).await.expect("should create alert1");

    let alert2 =
        storage.create_alert(&user_id, "critical", 90, 900_000, 1_000_000, None).await.expect("should create alert2");

    // Mark alert2 as read.
    let marked = storage.mark_alert_read(alert2.id).await.expect("should mark alert read");
    assert!(marked);

    // unread_only = true should only return alert1.
    let unread = storage.get_user_alerts(&user_id, true).await.expect("should get unread alerts");
    assert_eq!(unread.len(), 1);
    assert_eq!(unread[0].id, alert1.id);
    assert!(!unread[0].is_read);

    // unread_only = false should return both.
    let all = storage.get_user_alerts(&user_id, false).await.expect("should get all alerts");
    assert_eq!(all.len(), 2);

    cleanup_test_data(&pool, &suffix).await;
}

// —— mark_alert_read ——

#[tokio::test]
async fn test_mark_alert_read_already_read() {
    let pool = test_pool().await;
    let storage = MediaQuotaStorage::new(&pool);
    let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
    let user_id = format!("@mq_mar_{suffix}:localhost");

    cleanup_test_data(&pool, &suffix).await;
    ensure_test_user(&pool, &user_id).await;

    let alert =
        storage.create_alert(&user_id, "info", 30, 300_000, 1_000_000, None).await.expect("should create alert");

    // First mark works.
    let first = storage.mark_alert_read(alert.id).await.expect("should succeed");
    assert!(first);

    // Second mark on already-read alert returns false.
    let second = storage.mark_alert_read(alert.id).await.expect("should succeed");
    assert!(!second);

    cleanup_test_data(&pool, &suffix).await;
}

// —— get_usage_stats ——

#[tokio::test]
async fn test_get_usage_stats() {
    let pool = test_pool().await;
    let storage = MediaQuotaStorage::new(&pool);
    let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
    let user_id = format!("@mq_stats_{suffix}:localhost");

    cleanup_test_data(&pool, &suffix).await;
    ensure_test_user(&pool, &user_id).await;
    ensure_server_quota_row(&pool).await;

    storage
        .update_usage(UpdateUsageRequest {
            user_id: user_id.clone(),
            media_id: format!("media_stats_{suffix}"),
            file_size_bytes: 200_000,
            mime_type: Some("image/png".to_string()),
            operation: "upload".to_string(),
        })
        .await
        .expect("should log upload");

    let stats = storage.get_usage_stats(&user_id).await.expect("should get usage stats");

    assert_eq!(stats["current_storage_bytes"], 200_000);
    assert_eq!(stats["current_files_count"], 1);
    // recent_uploads_bytes should be >= 200_000 (the upload we just did).
    assert!(stats["recent_uploads_bytes"].as_i64().unwrap() >= 200_000);

    cleanup_test_data(&pool, &suffix).await;
}

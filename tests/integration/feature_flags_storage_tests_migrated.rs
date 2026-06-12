#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use synapse_rust::storage::feature_flags::{
    CreateFeatureFlagRequest, FeatureFlagFilters, FeatureFlagStorage, FeatureFlagTargetInput, UpdateFeatureFlagRequest,
};
use synapse_services::cache::{CacheConfig, CacheManager};

static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

fn unique_id() -> u64 {
    TEST_COUNTER.fetch_add(1, Ordering::SeqCst)
}

fn create_test_cache() -> Arc<CacheManager> {
    Arc::new(CacheManager::new(&CacheConfig::default()))
}

async fn setup_test_database(pool: &Arc<sqlx::PgPool>) {

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS feature_flags (
            flag_key TEXT PRIMARY KEY,
            target_scope TEXT NOT NULL,
            rollout_percent INTEGER NOT NULL,
            expires_at BIGINT NULL,
            reason TEXT NOT NULL,
            status TEXT NOT NULL,
            created_by TEXT NOT NULL,
            created_ts BIGINT NOT NULL,
            updated_ts BIGINT NOT NULL
        )
        "#,
    )
    .execute(pool.as_ref())
    .await
    .expect("Failed to create feature_flags table");

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS feature_flag_targets (
            id BIGSERIAL PRIMARY KEY,
            flag_key TEXT NOT NULL REFERENCES feature_flags(flag_key) ON DELETE CASCADE,
            subject_type TEXT NOT NULL,
            subject_id TEXT NOT NULL,
            created_ts BIGINT NOT NULL
        )
        "#,
    )
    .execute(pool.as_ref())
    .await
    .expect("Failed to create feature_flag_targets table");
}

fn create_storage(pool: &Arc<sqlx::PgPool>) -> FeatureFlagStorage {
    FeatureFlagStorage::new(pool, create_test_cache())
}

fn make_create_request(flag_key: &str, target_scope: &str) -> CreateFeatureFlagRequest {
    CreateFeatureFlagRequest {
        flag_key: flag_key.to_string(),
        target_scope: target_scope.to_string(),
        rollout_percent: 50,
        expires_at: None,
        reason: "test".to_string(),
        status: Some("draft".to_string()),
        targets: vec![],
    }
}

#[tokio::test]
async fn test_feature_flag_filters_default() {
    let filters = FeatureFlagFilters::default();
    assert!(filters.target_scope.is_none());
    assert!(filters.status.is_none());
    assert_eq!(filters.limit, 0);
    assert!(filters.cursor_updated_ts.is_none());
    assert!(filters.cursor_flag_key.is_none());
}

#[tokio::test]
async fn test_update_feature_flag_request_default() {
    let request = UpdateFeatureFlagRequest::default();
    assert!(request.rollout_percent.is_none());
    assert!(request.expires_at.is_none());
    assert!(request.reason.is_none());
    assert!(request.status.is_none());
    assert!(request.targets.is_none());
}

#[tokio::test]
async fn test_create_feature_flag_request_serialization() {
    let request = CreateFeatureFlagRequest {
        flag_key: "beta.feature".to_string(),
        target_scope: "global".to_string(),
        rollout_percent: 75,
        expires_at: Some(1_700_000_000_000),
        reason: "rollout".to_string(),
        status: Some("active".to_string()),
        targets: vec![FeatureFlagTargetInput {
            subject_type: "user".to_string(),
            subject_id: "@alice:test".to_string(),
        }],
    };

    let json = serde_json::to_string(&request).unwrap();
    let parsed: CreateFeatureFlagRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.flag_key, "beta.feature");
    assert_eq!(parsed.target_scope, "global");
    assert_eq!(parsed.rollout_percent, 75);
    assert_eq!(parsed.expires_at, Some(1_700_000_000_000));
    assert_eq!(parsed.reason, "rollout");
    assert_eq!(parsed.status, Some("active".to_string()));
    assert_eq!(parsed.targets.len(), 1);
    assert_eq!(parsed.targets[0].subject_type, "user");
    assert_eq!(parsed.targets[0].subject_id, "@alice:test");
}

#[tokio::test]
async fn test_create_feature_flag_request_default_targets() {
    let json = r#"{"flag_key":"k","target_scope":"s","rollout_percent":10,"reason":"r"}"#;
    let parsed: CreateFeatureFlagRequest = serde_json::from_str(json).unwrap();
    assert!(parsed.targets.is_empty());
    assert!(parsed.status.is_none());
}

#[tokio::test]
async fn test_feature_flag_target_input_serialization() {
    let input = FeatureFlagTargetInput { subject_type: "group".to_string(), subject_id: "admin".to_string() };
    let json = serde_json::to_string(&input).unwrap();
    let parsed: FeatureFlagTargetInput = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.subject_type, "group");
    assert_eq!(parsed.subject_id, "admin");
}

#[tokio::test]
async fn test_feature_flag_filters_with_all_fields() {
    let filters = FeatureFlagFilters {
        target_scope: Some("tenant".to_string()),
        status: Some("active".to_string()),
        limit: 25,
        cursor_updated_ts: Some(1_700_000_000_000),
        cursor_flag_key: Some("beta.rollout".to_string()),
    };
    assert_eq!(filters.target_scope.as_deref(), Some("tenant"));
    assert_eq!(filters.status.as_deref(), Some("active"));
    assert_eq!(filters.limit, 25);
    assert_eq!(filters.cursor_updated_ts, Some(1_700_000_000_000));
    assert_eq!(filters.cursor_flag_key.as_deref(), Some("beta.rollout"));
}

#[tokio::test]
async fn test_create_flag_basic() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let uid = unique_id();
    let flag_key = format!("test-create-basic-{uid}");
    let created_ts = 1_700_000_000_000_i64;

    let flag = storage
        .create_flag(
            &CreateFeatureFlagRequest {
                flag_key: flag_key.clone(),
                target_scope: "global".to_string(),
                rollout_percent: 50,
                expires_at: None,
                reason: "unit test".to_string(),
                status: Some("draft".to_string()),
                targets: vec![],
            },
            "@admin:test",
            created_ts,
        )
        .await
        .unwrap();

    assert_eq!(flag.flag_key, flag_key);
    assert_eq!(flag.target_scope, "global");
    assert_eq!(flag.rollout_percent, 50);
    assert!(flag.expires_at.is_none());
    assert_eq!(flag.reason, "unit test");
    assert_eq!(flag.status, "draft");
    assert_eq!(flag.created_by, "@admin:test");
    assert_eq!(flag.created_ts, created_ts);
    assert_eq!(flag.updated_ts, created_ts);
    assert!(flag.targets.is_empty());
}

#[tokio::test]
async fn test_create_flag_with_targets() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let uid = unique_id();
    let flag_key = format!("test-create-targets-{uid}");
    let created_ts = 1_700_000_000_000_i64;

    let flag = storage
        .create_flag(
            &CreateFeatureFlagRequest {
                flag_key: flag_key.clone(),
                target_scope: "tenant".to_string(),
                rollout_percent: 100,
                expires_at: Some(1_800_000_000_000),
                reason: "targeted rollout".to_string(),
                status: Some("active".to_string()),
                targets: vec![
                    FeatureFlagTargetInput { subject_type: "user".to_string(), subject_id: "@alice:test".to_string() },
                    FeatureFlagTargetInput { subject_type: "group".to_string(), subject_id: "admins".to_string() },
                ],
            },
            "@creator:test",
            created_ts,
        )
        .await
        .unwrap();

    assert_eq!(flag.targets.len(), 2);
    assert_eq!(flag.targets[0].subject_type, "user");
    assert_eq!(flag.targets[0].subject_id, "@alice:test");
    assert_eq!(flag.targets[1].subject_type, "group");
    assert_eq!(flag.targets[1].subject_id, "admins");
    assert_eq!(flag.expires_at, Some(1_800_000_000_000));
}

#[tokio::test]
async fn test_create_flag_default_status() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let uid = unique_id();
    let flag_key = format!("test-create-default-status-{uid}");
    let created_ts = 1_700_000_000_000_i64;

    let flag = storage
        .create_flag(
            &CreateFeatureFlagRequest {
                flag_key: flag_key.clone(),
                target_scope: "global".to_string(),
                rollout_percent: 10,
                expires_at: None,
                reason: "no status".to_string(),
                status: None,
                targets: vec![],
            },
            "@admin:test",
            created_ts,
        )
        .await
        .unwrap();

    assert_eq!(flag.status, "draft");
}

#[tokio::test]
async fn test_create_flag_duplicate_key_fails() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let uid = unique_id();
    let flag_key = format!("test-dup-key-{uid}");
    let created_ts = 1_700_000_000_000_i64;

    let request = CreateFeatureFlagRequest {
        flag_key: flag_key.clone(),
        target_scope: "global".to_string(),
        rollout_percent: 50,
        expires_at: None,
        reason: "first".to_string(),
        status: Some("draft".to_string()),
        targets: vec![],
    };

    storage.create_flag(&request, "@admin:test", created_ts).await.unwrap();

    let result = storage.create_flag(&request, "@admin:test", created_ts + 1).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_get_flag_found() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let uid = unique_id();
    let flag_key = format!("test-get-found-{uid}");
    let created_ts = 1_700_000_000_000_i64;

    storage
        .create_flag(
            &CreateFeatureFlagRequest {
                flag_key: flag_key.clone(),
                target_scope: "global".to_string(),
                rollout_percent: 30,
                expires_at: None,
                reason: "get test".to_string(),
                status: Some("active".to_string()),
                targets: vec![FeatureFlagTargetInput {
                    subject_type: "user".to_string(),
                    subject_id: "@bob:test".to_string(),
                }],
            },
            "@admin:test",
            created_ts,
        )
        .await
        .unwrap();

    let flag = storage.get_flag(&flag_key).await.unwrap().unwrap();
    assert_eq!(flag.flag_key, flag_key);
    assert_eq!(flag.rollout_percent, 30);
    assert_eq!(flag.status, "active");
    assert_eq!(flag.targets.len(), 1);
    assert_eq!(flag.targets[0].subject_id, "@bob:test");
}

#[tokio::test]
async fn test_get_flag_not_found() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);

    let result = storage.get_flag("nonexistent.key").await.unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn test_get_flag_cache_hit() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let cache = create_test_cache();
    let storage = FeatureFlagStorage::new(&pool, cache.clone());
    let uid = unique_id();
    let flag_key = format!("test-get-cache-{uid}");
    let created_ts = 1_700_000_000_000_i64;

    storage.create_flag(&make_create_request(&flag_key, "global"), "@admin:test", created_ts).await.unwrap();

    let flag1 = storage.get_flag(&flag_key).await.unwrap().unwrap();
    let flag2 = storage.get_flag(&flag_key).await.unwrap().unwrap();
    assert_eq!(flag1.flag_key, flag2.flag_key);
    assert_eq!(flag1.rollout_percent, flag2.rollout_percent);
}

#[tokio::test]
async fn test_update_flag_status() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let uid = unique_id();
    let flag_key = format!("test-update-status-{uid}");
    let created_ts = 1_700_000_000_000_i64;

    storage
        .create_flag(
            &CreateFeatureFlagRequest {
                flag_key: flag_key.clone(),
                target_scope: "global".to_string(),
                rollout_percent: 50,
                expires_at: None,
                reason: "update test".to_string(),
                status: Some("draft".to_string()),
                targets: vec![],
            },
            "@admin:test",
            created_ts,
        )
        .await
        .unwrap();

    let updated = storage
        .update_flag(
            &flag_key,
            &UpdateFeatureFlagRequest { status: Some("active".to_string()), ..Default::default() },
            created_ts + 100,
        )
        .await
        .unwrap()
        .unwrap();

    assert_eq!(updated.status, "active");
    assert_eq!(updated.updated_ts, created_ts + 100);
    assert_eq!(updated.rollout_percent, 50);
}

#[tokio::test]
async fn test_update_flag_rollout_percent() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let uid = unique_id();
    let flag_key = format!("test-update-rollout-{uid}");
    let created_ts = 1_700_000_000_000_i64;

    storage.create_flag(&make_create_request(&flag_key, "global"), "@admin:test", created_ts).await.unwrap();

    let updated = storage
        .update_flag(
            &flag_key,
            &UpdateFeatureFlagRequest { rollout_percent: Some(100), ..Default::default() },
            created_ts + 200,
        )
        .await
        .unwrap()
        .unwrap();

    assert_eq!(updated.rollout_percent, 100);
    assert_eq!(updated.updated_ts, created_ts + 200);
}

#[tokio::test]
async fn test_update_flag_expires_at() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let uid = unique_id();
    let flag_key = format!("test-update-expires-{uid}");
    let created_ts = 1_700_000_000_000_i64;

    storage
        .create_flag(
            &CreateFeatureFlagRequest {
                flag_key: flag_key.clone(),
                target_scope: "global".to_string(),
                rollout_percent: 50,
                expires_at: None,
                reason: "test".to_string(),
                status: Some("draft".to_string()),
                targets: vec![],
            },
            "@admin:test",
            created_ts,
        )
        .await
        .unwrap();

    let updated = storage
        .update_flag(
            &flag_key,
            &UpdateFeatureFlagRequest { expires_at: Some(1_800_000_000_000), ..Default::default() },
            created_ts + 300,
        )
        .await
        .unwrap()
        .unwrap();

    assert_eq!(updated.expires_at, Some(1_800_000_000_000));
}

#[tokio::test]
async fn test_update_flag_reason() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let uid = unique_id();
    let flag_key = format!("test-update-reason-{uid}");
    let created_ts = 1_700_000_000_000_i64;

    storage.create_flag(&make_create_request(&flag_key, "global"), "@admin:test", created_ts).await.unwrap();

    let updated = storage
        .update_flag(
            &flag_key,
            &UpdateFeatureFlagRequest { reason: Some("updated reason".to_string()), ..Default::default() },
            created_ts + 400,
        )
        .await
        .unwrap()
        .unwrap();

    assert_eq!(updated.reason, "updated reason");
}

#[tokio::test]
async fn test_update_flag_replace_targets() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let uid = unique_id();
    let flag_key = format!("test-update-targets-{uid}");
    let created_ts = 1_700_000_000_000_i64;

    storage
        .create_flag(
            &CreateFeatureFlagRequest {
                flag_key: flag_key.clone(),
                target_scope: "global".to_string(),
                rollout_percent: 50,
                expires_at: None,
                reason: "test".to_string(),
                status: Some("draft".to_string()),
                targets: vec![FeatureFlagTargetInput {
                    subject_type: "user".to_string(),
                    subject_id: "@old:test".to_string(),
                }],
            },
            "@admin:test",
            created_ts,
        )
        .await
        .unwrap();

    let updated = storage
        .update_flag(
            &flag_key,
            &UpdateFeatureFlagRequest {
                targets: Some(vec![
                    FeatureFlagTargetInput { subject_type: "user".to_string(), subject_id: "@new1:test".to_string() },
                    FeatureFlagTargetInput { subject_type: "group".to_string(), subject_id: "beta".to_string() },
                ]),
                ..Default::default()
            },
            created_ts + 500,
        )
        .await
        .unwrap()
        .unwrap();

    assert_eq!(updated.targets.len(), 2);
    let subject_ids: Vec<&str> = updated.targets.iter().map(|t| t.subject_id.as_str()).collect();
    assert!(subject_ids.contains(&"@new1:test"));
    assert!(subject_ids.contains(&"beta"));
    assert!(!subject_ids.contains(&"@old:test"));
}

#[tokio::test]
async fn test_update_flag_nonexistent_returns_none() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);

    let result = storage
        .update_flag(
            "nonexistent.key",
            &UpdateFeatureFlagRequest { status: Some("active".to_string()), ..Default::default() },
            1_700_000_000_000,
        )
        .await
        .unwrap();

    assert!(result.is_none());
}

#[tokio::test]
async fn test_update_flag_no_targets_preserves_existing() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let uid = unique_id();
    let flag_key = format!("test-update-no-targets-{uid}");
    let created_ts = 1_700_000_000_000_i64;

    storage
        .create_flag(
            &CreateFeatureFlagRequest {
                flag_key: flag_key.clone(),
                target_scope: "global".to_string(),
                rollout_percent: 50,
                expires_at: None,
                reason: "test".to_string(),
                status: Some("draft".to_string()),
                targets: vec![FeatureFlagTargetInput {
                    subject_type: "user".to_string(),
                    subject_id: "@preserve:test".to_string(),
                }],
            },
            "@admin:test",
            created_ts,
        )
        .await
        .unwrap();

    let updated = storage
        .update_flag(
            &flag_key,
            &UpdateFeatureFlagRequest { rollout_percent: Some(75), ..Default::default() },
            created_ts + 100,
        )
        .await
        .unwrap()
        .unwrap();

    assert_eq!(updated.rollout_percent, 75);
    assert_eq!(updated.targets.len(), 1);
    assert_eq!(updated.targets[0].subject_id, "@preserve:test");
}

#[tokio::test]
async fn test_list_flags_empty() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);

    let filters = FeatureFlagFilters {
        target_scope: Some(format!("empty-scope-{}", unique_id())),
        status: None,
        limit: 10,
        cursor_updated_ts: None,
        cursor_flag_key: None,
    };

    let (flags, total) = storage.list_flags(&filters).await.unwrap();
    assert!(flags.is_empty());
    assert_eq!(total, 0);
}

#[tokio::test]
async fn test_list_flags_with_target_scope_filter() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let uid = unique_id();
    let scope = format!("scope-filter-{uid}");
    let created_ts = 1_700_000_000_000_i64;

    storage
        .create_flag(
            &CreateFeatureFlagRequest {
                flag_key: format!("ff-scope-a-{uid}"),
                target_scope: scope.clone(),
                rollout_percent: 10,
                expires_at: None,
                reason: "test".to_string(),
                status: Some("draft".to_string()),
                targets: vec![],
            },
            "@admin:test",
            created_ts,
        )
        .await
        .unwrap();

    storage
        .create_flag(
            &CreateFeatureFlagRequest {
                flag_key: format!("ff-scope-b-{uid}"),
                target_scope: scope.clone(),
                rollout_percent: 20,
                expires_at: None,
                reason: "test".to_string(),
                status: Some("active".to_string()),
                targets: vec![],
            },
            "@admin:test",
            created_ts + 1,
        )
        .await
        .unwrap();

    storage
        .create_flag(
            &CreateFeatureFlagRequest {
                flag_key: format!("ff-other-{uid}"),
                target_scope: "other-scope".to_string(),
                rollout_percent: 30,
                expires_at: None,
                reason: "test".to_string(),
                status: Some("draft".to_string()),
                targets: vec![],
            },
            "@admin:test",
            created_ts + 2,
        )
        .await
        .unwrap();

    let filters = FeatureFlagFilters {
        target_scope: Some(scope.clone()),
        status: None,
        limit: 10,
        cursor_updated_ts: None,
        cursor_flag_key: None,
    };

    let (flags, total) = storage.list_flags(&filters).await.unwrap();
    assert_eq!(total, 2);
    assert_eq!(flags.len(), 2);
    assert!(flags.iter().all(|f| f.target_scope == scope));
}

#[tokio::test]
async fn test_list_flags_with_status_filter() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let uid = unique_id();
    let created_ts = 1_700_000_000_000_i64;

    storage
        .create_flag(
            &CreateFeatureFlagRequest {
                flag_key: format!("ff-active-{uid}"),
                target_scope: format!("status-scope-{uid}"),
                rollout_percent: 10,
                expires_at: None,
                reason: "test".to_string(),
                status: Some("active".to_string()),
                targets: vec![],
            },
            "@admin:test",
            created_ts,
        )
        .await
        .unwrap();

    storage
        .create_flag(
            &CreateFeatureFlagRequest {
                flag_key: format!("ff-draft-{uid}"),
                target_scope: format!("status-scope-{uid}"),
                rollout_percent: 20,
                expires_at: None,
                reason: "test".to_string(),
                status: Some("draft".to_string()),
                targets: vec![],
            },
            "@admin:test",
            created_ts + 1,
        )
        .await
        .unwrap();

    let filters = FeatureFlagFilters {
        target_scope: Some(format!("status-scope-{uid}")),
        status: Some("active".to_string()),
        limit: 10,
        cursor_updated_ts: None,
        cursor_flag_key: None,
    };

    let (flags, total) = storage.list_flags(&filters).await.unwrap();
    assert_eq!(total, 1);
    assert_eq!(flags.len(), 1);
    assert_eq!(flags[0].status, "active");
}

#[tokio::test]
async fn test_list_flags_with_limit() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let uid = unique_id();
    let scope = format!("limit-scope-{uid}");
    let created_ts = 1_700_000_000_000_i64;

    for i in 0..5 {
        storage
            .create_flag(
                &CreateFeatureFlagRequest {
                    flag_key: format!("ff-limit-{uid}-{i}"),
                    target_scope: scope.clone(),
                    rollout_percent: i * 10,
                    expires_at: None,
                    reason: "test".to_string(),
                    status: Some("draft".to_string()),
                    targets: vec![],
                },
                "@admin:test",
                created_ts + i as i64,
            )
            .await
            .unwrap();
    }

    let filters = FeatureFlagFilters {
        target_scope: Some(scope.clone()),
        status: None,
        limit: 3,
        cursor_updated_ts: None,
        cursor_flag_key: None,
    };

    let (flags, total) = storage.list_flags(&filters).await.unwrap();
    assert_eq!(total, 5);
    assert_eq!(flags.len(), 3);
}

#[tokio::test]
async fn test_list_flags_with_cursor_pagination() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let uid = unique_id();
    let scope = format!("cursor-scope-{uid}");
    let created_ts = 1_700_000_000_000_i64;

    for i in 0..4 {
        storage
            .create_flag(
                &CreateFeatureFlagRequest {
                    flag_key: format!("ff-cursor-{uid}-{i}"),
                    target_scope: scope.clone(),
                    rollout_percent: i * 10,
                    expires_at: None,
                    reason: "test".to_string(),
                    status: Some("draft".to_string()),
                    targets: vec![],
                },
                "@admin:test",
                created_ts + i as i64,
            )
            .await
            .unwrap();
    }

    let first_page = FeatureFlagFilters {
        target_scope: Some(scope.clone()),
        status: None,
        limit: 2,
        cursor_updated_ts: None,
        cursor_flag_key: None,
    };

    let (flags_page1, total) = storage.list_flags(&first_page).await.unwrap();
    assert_eq!(total, 4);
    assert_eq!(flags_page1.len(), 2);

    let last_flag = flags_page1.last().unwrap();
    let second_page = FeatureFlagFilters {
        target_scope: Some(scope.clone()),
        status: None,
        limit: 2,
        cursor_updated_ts: Some(last_flag.updated_ts),
        cursor_flag_key: Some(last_flag.flag_key.clone()),
    };

    let (flags_page2, total2) = storage.list_flags(&second_page).await.unwrap();
    assert_eq!(total2, 4);
    assert_eq!(flags_page2.len(), 2);

    let page1_keys: Vec<&str> = flags_page1.iter().map(|f| f.flag_key.as_str()).collect();
    let page2_keys: Vec<&str> = flags_page2.iter().map(|f| f.flag_key.as_str()).collect();
    for key in &page1_keys {
        assert!(!page2_keys.contains(key));
    }
}

#[tokio::test]
async fn test_list_flags_includes_targets() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let uid = unique_id();
    let flag_key = format!("ff-list-targets-{uid}");
    let scope = format!("list-targets-scope-{uid}");
    let created_ts = 1_700_000_000_000_i64;

    storage
        .create_flag(
            &CreateFeatureFlagRequest {
                flag_key: flag_key.clone(),
                target_scope: scope.clone(),
                rollout_percent: 50,
                expires_at: None,
                reason: "test".to_string(),
                status: Some("draft".to_string()),
                targets: vec![
                    FeatureFlagTargetInput {
                        subject_type: "user".to_string(),
                        subject_id: "@target1:test".to_string(),
                    },
                    FeatureFlagTargetInput { subject_type: "group".to_string(), subject_id: "group1".to_string() },
                ],
            },
            "@admin:test",
            created_ts,
        )
        .await
        .unwrap();

    let filters = FeatureFlagFilters {
        target_scope: Some(scope.clone()),
        status: None,
        limit: 10,
        cursor_updated_ts: None,
        cursor_flag_key: None,
    };

    let (flags, _) = storage.list_flags(&filters).await.unwrap();
    assert_eq!(flags.len(), 1);
    assert_eq!(flags[0].targets.len(), 2);
}

#[tokio::test]
async fn test_list_flags_cache_returns_same_result() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let cache = create_test_cache();
    let storage = FeatureFlagStorage::new(&pool, cache.clone());
    let uid = unique_id();
    let scope = format!("cache-scope-{uid}");
    let created_ts = 1_700_000_000_000_i64;

    storage
        .create_flag(
            &CreateFeatureFlagRequest {
                flag_key: format!("ff-cache-{uid}"),
                target_scope: scope.clone(),
                rollout_percent: 50,
                expires_at: None,
                reason: "test".to_string(),
                status: Some("draft".to_string()),
                targets: vec![],
            },
            "@admin:test",
            created_ts,
        )
        .await
        .unwrap();

    let filters = FeatureFlagFilters {
        target_scope: Some(scope.clone()),
        status: None,
        limit: 10,
        cursor_updated_ts: None,
        cursor_flag_key: None,
    };

    let (flags1, total1) = storage.list_flags(&filters).await.unwrap();
    let (flags2, total2) = storage.list_flags(&filters).await.unwrap();
    assert_eq!(total1, total2);
    assert_eq!(flags1.len(), flags2.len());
}

#[tokio::test]
async fn test_create_flag_invalidates_list_cache() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let cache = create_test_cache();
    let storage = FeatureFlagStorage::new(&pool, cache.clone());
    let uid = unique_id();
    let scope = format!("create-invalidate-{uid}");
    let created_ts = 1_700_000_000_000_i64;

    let filters = FeatureFlagFilters {
        target_scope: Some(scope.clone()),
        status: None,
        limit: 10,
        cursor_updated_ts: None,
        cursor_flag_key: None,
    };

    let (flags_before, total_before) = storage.list_flags(&filters).await.unwrap();
    assert_eq!(total_before, 0);
    assert!(flags_before.is_empty());

    storage
        .create_flag(
            &CreateFeatureFlagRequest {
                flag_key: format!("ff-invalidate-{uid}"),
                target_scope: scope.clone(),
                rollout_percent: 50,
                expires_at: None,
                reason: "test".to_string(),
                status: Some("draft".to_string()),
                targets: vec![],
            },
            "@admin:test",
            created_ts,
        )
        .await
        .unwrap();

    let (flags_after, total_after) = storage.list_flags(&filters).await.unwrap();
    assert_eq!(total_after, 1);
    assert_eq!(flags_after.len(), 1);
}

#[tokio::test]
async fn test_update_flag_invalidates_list_cache() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let cache = create_test_cache();
    let storage = FeatureFlagStorage::new(&pool, cache.clone());
    let uid = unique_id();
    let flag_key = format!("ff-upd-invalidate-{uid}");
    let scope = format!("upd-invalidate-scope-{uid}");
    let created_ts = 1_700_000_000_000_i64;

    storage
        .create_flag(
            &CreateFeatureFlagRequest {
                flag_key: flag_key.clone(),
                target_scope: scope.clone(),
                rollout_percent: 50,
                expires_at: None,
                reason: "test".to_string(),
                status: Some("draft".to_string()),
                targets: vec![],
            },
            "@admin:test",
            created_ts,
        )
        .await
        .unwrap();

    let filters = FeatureFlagFilters {
        target_scope: Some(scope.clone()),
        status: Some("draft".to_string()),
        limit: 10,
        cursor_updated_ts: None,
        cursor_flag_key: None,
    };

    let (_flags_before, total_before) = storage.list_flags(&filters).await.unwrap();
    assert_eq!(total_before, 1);

    storage
        .update_flag(
            &flag_key,
            &UpdateFeatureFlagRequest { status: Some("active".to_string()), ..Default::default() },
            created_ts + 100,
        )
        .await
        .unwrap()
        .unwrap();

    let (flags_after, total_after) = storage.list_flags(&filters).await.unwrap();
    assert_eq!(total_after, 0);
    assert!(flags_after.is_empty());
}

#[tokio::test]
async fn test_update_flag_invalidates_flag_cache() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let cache = create_test_cache();
    let storage = FeatureFlagStorage::new(&pool, cache.clone());
    let uid = unique_id();
    let flag_key = format!("ff-cache-invalidate-{uid}");
    let created_ts = 1_700_000_000_000_i64;

    storage
        .create_flag(
            &CreateFeatureFlagRequest {
                flag_key: flag_key.clone(),
                target_scope: "global".to_string(),
                rollout_percent: 50,
                expires_at: None,
                reason: "test".to_string(),
                status: Some("draft".to_string()),
                targets: vec![],
            },
            "@admin:test",
            created_ts,
        )
        .await
        .unwrap();

    let flag_before = storage.get_flag(&flag_key).await.unwrap().unwrap();
    assert_eq!(flag_before.rollout_percent, 50);

    storage
        .update_flag(
            &flag_key,
            &UpdateFeatureFlagRequest { rollout_percent: Some(99), ..Default::default() },
            created_ts + 100,
        )
        .await
        .unwrap()
        .unwrap();

    let flag_after = storage.get_flag(&flag_key).await.unwrap().unwrap();
    assert_eq!(flag_after.rollout_percent, 99);
}

#[tokio::test]
async fn test_update_flag_multiple_fields() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let uid = unique_id();
    let flag_key = format!("ff-multi-update-{uid}");
    let created_ts = 1_700_000_000_000_i64;

    storage
        .create_flag(
            &CreateFeatureFlagRequest {
                flag_key: flag_key.clone(),
                target_scope: "global".to_string(),
                rollout_percent: 10,
                expires_at: None,
                reason: "initial".to_string(),
                status: Some("draft".to_string()),
                targets: vec![],
            },
            "@admin:test",
            created_ts,
        )
        .await
        .unwrap();

    let updated = storage
        .update_flag(
            &flag_key,
            &UpdateFeatureFlagRequest {
                rollout_percent: Some(80),
                reason: Some("full rollout".to_string()),
                status: Some("active".to_string()),
                expires_at: Some(1_900_000_000_000),
                ..Default::default()
            },
            created_ts + 500,
        )
        .await
        .unwrap()
        .unwrap();

    assert_eq!(updated.rollout_percent, 80);
    assert_eq!(updated.reason, "full rollout");
    assert_eq!(updated.status, "active");
    assert_eq!(updated.expires_at, Some(1_900_000_000_000));
    assert_eq!(updated.updated_ts, created_ts + 500);
    assert_eq!(updated.created_ts, created_ts);
}

#[tokio::test]
async fn test_update_flag_empty_targets_clears_all() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let uid = unique_id();
    let flag_key = format!("ff-clear-targets-{uid}");
    let created_ts = 1_700_000_000_000_i64;

    storage
        .create_flag(
            &CreateFeatureFlagRequest {
                flag_key: flag_key.clone(),
                target_scope: "global".to_string(),
                rollout_percent: 50,
                expires_at: None,
                reason: "test".to_string(),
                status: Some("draft".to_string()),
                targets: vec![
                    FeatureFlagTargetInput { subject_type: "user".to_string(), subject_id: "@a:test".to_string() },
                    FeatureFlagTargetInput { subject_type: "user".to_string(), subject_id: "@b:test".to_string() },
                ],
            },
            "@admin:test",
            created_ts,
        )
        .await
        .unwrap();

    let updated = storage
        .update_flag(
            &flag_key,
            &UpdateFeatureFlagRequest { targets: Some(vec![]), ..Default::default() },
            created_ts + 100,
        )
        .await
        .unwrap()
        .unwrap();

    assert!(updated.targets.is_empty());
}

#[tokio::test]
async fn test_list_flags_order_by_updated_ts_desc() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let uid = unique_id();
    let scope = format!("order-scope-{uid}");
    let created_ts = 1_700_000_000_000_i64;

    for i in 0..3 {
        storage
            .create_flag(
                &CreateFeatureFlagRequest {
                    flag_key: format!("ff-order-{uid}-{i}"),
                    target_scope: scope.clone(),
                    rollout_percent: i * 10,
                    expires_at: None,
                    reason: "test".to_string(),
                    status: Some("draft".to_string()),
                    targets: vec![],
                },
                "@admin:test",
                created_ts + i as i64 * 1000,
            )
            .await
            .unwrap();
    }

    let filters = FeatureFlagFilters {
        target_scope: Some(scope.clone()),
        status: None,
        limit: 10,
        cursor_updated_ts: None,
        cursor_flag_key: None,
    };

    let (flags, _) = storage.list_flags(&filters).await.unwrap();
    assert_eq!(flags.len(), 3);
    for window in flags.windows(2) {
        assert!(window[0].updated_ts >= window[1].updated_ts);
    }
}

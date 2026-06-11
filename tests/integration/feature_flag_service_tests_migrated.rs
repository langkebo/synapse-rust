#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use synapse_rust::cache::{CacheConfig, CacheManager};
use synapse_rust::services::admin_audit_service::AdminAuditService;
use synapse_rust::services::feature_flag_service::FeatureFlagService;
use synapse_rust::storage::audit::AuditEventStorage;
use synapse_rust::storage::feature_flags::{
    CreateFeatureFlagRequest, FeatureFlagFilters, FeatureFlagStorage, FeatureFlagTargetInput, UpdateFeatureFlagRequest,
};

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

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS audit_events (
            event_id TEXT PRIMARY KEY,
            actor_id TEXT NOT NULL,
            action TEXT NOT NULL,
            resource_type TEXT NOT NULL,
            resource_id TEXT NOT NULL,
            result TEXT NOT NULL,
            request_id TEXT NOT NULL,
            details JSONB NOT NULL DEFAULT '{}',
            created_ts BIGINT NOT NULL
        )
        "#,
    )
    .execute(pool.as_ref())
    .await
    .expect("Failed to create audit_events table");

}

fn create_service(pool: &Arc<sqlx::PgPool>) -> FeatureFlagService {
    let cache = create_test_cache();
    let storage = Arc::new(FeatureFlagStorage::new(pool, cache));
    let audit_storage = Arc::new(AuditEventStorage::new(pool));
    let audit_service = Arc::new(AdminAuditService::new(audit_storage));
    FeatureFlagService::new(storage, audit_service)
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
async fn test_create_flag_empty_flag_key() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = create_service(&pool);
    let request = CreateFeatureFlagRequest {
        flag_key: "".to_string(),
        target_scope: "global".to_string(),
        rollout_percent: 50,
        expires_at: None,
        reason: "test".to_string(),
        status: Some("draft".to_string()),
        targets: vec![],
    };
    let result = service.create_flag("@admin:test", "req-1", request).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.is_bad_request() && err.message().contains("flag_key is required"));
}

#[tokio::test]
async fn test_create_flag_whitespace_flag_key() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = create_service(&pool);
    let request = CreateFeatureFlagRequest {
        flag_key: "   ".to_string(),
        target_scope: "global".to_string(),
        rollout_percent: 50,
        expires_at: None,
        reason: "test".to_string(),
        status: Some("draft".to_string()),
        targets: vec![],
    };
    let result = service.create_flag("@admin:test", "req-1", request).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_create_flag_uppercase_flag_key() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = create_service(&pool);
    let request = CreateFeatureFlagRequest {
        flag_key: "MyFlag".to_string(),
        target_scope: "global".to_string(),
        rollout_percent: 50,
        expires_at: None,
        reason: "test".to_string(),
        status: Some("draft".to_string()),
        targets: vec![],
    };
    let result = service.create_flag("@admin:test", "req-1", request).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.is_bad_request() && err.message().contains("flag_key must use lowercase"));
}

#[tokio::test]
async fn test_create_flag_special_chars_flag_key() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = create_service(&pool);
    let request = CreateFeatureFlagRequest {
        flag_key: "flag@name!".to_string(),
        target_scope: "global".to_string(),
        rollout_percent: 50,
        expires_at: None,
        reason: "test".to_string(),
        status: Some("draft".to_string()),
        targets: vec![],
    };
    let result = service.create_flag("@admin:test", "req-1", request).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_create_flag_valid_flag_key_with_dot_underscore_hyphen() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = create_service(&pool);
    let uid = unique_id();
    let flag_key = format!("beta.feature_flag-test-{uid}");
    let request = CreateFeatureFlagRequest {
        flag_key: flag_key.clone(),
        target_scope: "global".to_string(),
        rollout_percent: 50,
        expires_at: None,
        reason: "test".to_string(),
        status: Some("draft".to_string()),
        targets: vec![],
    };
    let result = service.create_flag("@admin:test", "req-1", request).await;
    assert!(result.is_ok());
    let flag = result.unwrap();
    assert_eq!(flag.flag_key, flag_key);
}

#[tokio::test]
async fn test_create_flag_invalid_target_scope() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = create_service(&pool);
    let uid = unique_id();
    let request = CreateFeatureFlagRequest {
        flag_key: format!("test-scope-{uid}"),
        target_scope: "invalid_scope".to_string(),
        rollout_percent: 50,
        expires_at: None,
        reason: "test".to_string(),
        status: Some("draft".to_string()),
        targets: vec![],
    };
    let result = service.create_flag("@admin:test", "req-1", request).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.is_bad_request() && err.message().contains("target_scope must be one of"));
}

#[tokio::test]
async fn test_create_flag_valid_target_scopes() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = create_service(&pool);
    let uid = unique_id();
    for (i, scope) in ["global", "tenant", "room", "user"].iter().enumerate() {
        let request = CreateFeatureFlagRequest {
            flag_key: format!("test-scope-{uid}-{i}"),
            target_scope: scope.to_string(),
            rollout_percent: 50,
            expires_at: None,
            reason: "test".to_string(),
            status: Some("draft".to_string()),
            targets: vec![],
        };
        let result = service.create_flag("@admin:test", "req-1", request).await;
        assert!(result.is_ok(), "scope '{}' should be valid", scope);
    }
}

#[tokio::test]
async fn test_create_flag_rollout_percent_negative() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = create_service(&pool);
    let uid = unique_id();
    let request = CreateFeatureFlagRequest {
        flag_key: format!("test-rollout-neg-{uid}"),
        target_scope: "global".to_string(),
        rollout_percent: -1,
        expires_at: None,
        reason: "test".to_string(),
        status: Some("draft".to_string()),
        targets: vec![],
    };
    let result = service.create_flag("@admin:test", "req-1", request).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.is_bad_request() && err.message().contains("rollout_percent must be between 0 and 100"));
}

#[tokio::test]
async fn test_create_flag_valid_rollout_percent() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = create_service(&pool);
    let uid = unique_id();
    let request = CreateFeatureFlagRequest {
        flag_key: format!("test-rollout-101-{uid}"),
        target_scope: "global".to_string(),
        rollout_percent: 101,
        expires_at: None,
        reason: "test".to_string(),
        status: Some("draft".to_string()),
        targets: vec![],
    };
    let result = service.create_flag("@admin:test", "req-1", request).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_create_flag_rollout_percent_boundary_0() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = create_service(&pool);
    let uid = unique_id();
    let flag_key = format!("test-rollout-0-{uid}");
    let request = CreateFeatureFlagRequest {
        flag_key: flag_key.clone(),
        target_scope: "global".to_string(),
        rollout_percent: 0,
        expires_at: None,
        reason: "test".to_string(),
        status: Some("draft".to_string()),
        targets: vec![],
    };
    let result = service.create_flag("@admin:test", "req-1", request).await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap().rollout_percent, 0);
}

#[tokio::test]
async fn test_create_flag_rollout_percent_boundary_100() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = create_service(&pool);
    let uid = unique_id();
    let flag_key = format!("test-rollout-100-{uid}");
    let request = CreateFeatureFlagRequest {
        flag_key: flag_key.clone(),
        target_scope: "global".to_string(),
        rollout_percent: 100,
        expires_at: None,
        reason: "test".to_string(),
        status: Some("draft".to_string()),
        targets: vec![],
    };
    let result = service.create_flag("@admin:test", "req-1", request).await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap().rollout_percent, 100);
}

#[tokio::test]
async fn test_create_flag_invalid_status() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = create_service(&pool);
    let uid = unique_id();
    let request = CreateFeatureFlagRequest {
        flag_key: format!("test-status-invalid-{uid}"),
        target_scope: "global".to_string(),
        rollout_percent: 50,
        expires_at: None,
        reason: "test".to_string(),
        status: Some("invalid_status".to_string()),
        targets: vec![],
    };
    let result = service.create_flag("@admin:test", "req-1", request).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.is_bad_request() && err.message().contains("unsupported feature flag status"));
}

#[tokio::test]
async fn test_create_flag_valid_statuses() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = create_service(&pool);
    let uid = unique_id();
    let valid_statuses = [
        "draft",
        "scheduled",
        "active",
        "ramping",
        "fully_enabled",
        "rolled_back",
        "removed",
        "expired_pending_removal",
    ];
    for (i, status) in valid_statuses.iter().enumerate() {
        let request = CreateFeatureFlagRequest {
            flag_key: format!("test-status-{uid}-{i}"),
            target_scope: "global".to_string(),
            rollout_percent: 50,
            expires_at: None,
            reason: "test".to_string(),
            status: Some(status.to_string()),
            targets: vec![],
        };
        let result = service.create_flag("@admin:test", "req-1", request).await;
        assert!(result.is_ok(), "status '{}' should be valid", status);
    }
}

#[tokio::test]
async fn test_create_flag_empty_reason() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = create_service(&pool);
    let uid = unique_id();
    let request = CreateFeatureFlagRequest {
        flag_key: format!("test-reason-empty-{uid}"),
        target_scope: "global".to_string(),
        rollout_percent: 50,
        expires_at: None,
        reason: "".to_string(),
        status: Some("draft".to_string()),
        targets: vec![],
    };
    let result = service.create_flag("@admin:test", "req-1", request).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.is_bad_request() && err.message().contains("reason is required"));
}

#[tokio::test]
async fn test_create_flag_whitespace_reason() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = create_service(&pool);
    let uid = unique_id();
    let request = CreateFeatureFlagRequest {
        flag_key: format!("test-reason-ws-{uid}"),
        target_scope: "global".to_string(),
        rollout_percent: 50,
        expires_at: None,
        reason: "   ".to_string(),
        status: Some("draft".to_string()),
        targets: vec![],
    };
    let result = service.create_flag("@admin:test", "req-1", request).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_create_flag_invalid_target_subject_type() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = create_service(&pool);
    let uid = unique_id();
    let request = CreateFeatureFlagRequest {
        flag_key: format!("test-target-type-{uid}"),
        target_scope: "user".to_string(),
        rollout_percent: 50,
        expires_at: None,
        reason: "test".to_string(),
        status: Some("draft".to_string()),
        targets: vec![FeatureFlagTargetInput {
            subject_type: "invalid_type".to_string(),
            subject_id: "@user:test".to_string(),
        }],
    };
    let result = service.create_flag("@admin:test", "req-1", request).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.is_bad_request() && err.message().contains("subject_type must be one of"));
}

#[tokio::test]
async fn test_create_flag_empty_target_subject_id() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = create_service(&pool);
    let uid = unique_id();
    let request = CreateFeatureFlagRequest {
        flag_key: format!("test-target-id-{uid}"),
        target_scope: "user".to_string(),
        rollout_percent: 50,
        expires_at: None,
        reason: "test".to_string(),
        status: Some("draft".to_string()),
        targets: vec![FeatureFlagTargetInput { subject_type: "user".to_string(), subject_id: "".to_string() }],
    };
    let result = service.create_flag("@admin:test", "req-1", request).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.is_bad_request() && err.message().contains("subject_id is required"));
}

#[tokio::test]
async fn test_create_flag_duplicate_targets() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = create_service(&pool);
    let uid = unique_id();
    let request = CreateFeatureFlagRequest {
        flag_key: format!("test-dup-targets-{uid}"),
        target_scope: "user".to_string(),
        rollout_percent: 50,
        expires_at: None,
        reason: "test".to_string(),
        status: Some("draft".to_string()),
        targets: vec![
            FeatureFlagTargetInput { subject_type: "user".to_string(), subject_id: "@alice:test".to_string() },
            FeatureFlagTargetInput { subject_type: "user".to_string(), subject_id: "@alice:test".to_string() },
        ],
    };
    let result = service.create_flag("@admin:test", "req-1", request).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.is_bad_request() && err.message().contains("duplicated feature flag target"));
}

#[tokio::test]
async fn test_create_flag_expired_expiration() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = create_service(&pool);
    let uid = unique_id();
    let past_ts = chrono::Utc::now().timestamp_millis() - 10000;
    let request = CreateFeatureFlagRequest {
        flag_key: format!("test-expired-{uid}"),
        target_scope: "global".to_string(),
        rollout_percent: 50,
        expires_at: Some(past_ts),
        reason: "test".to_string(),
        status: Some("draft".to_string()),
        targets: vec![],
    };
    let result = service.create_flag("@admin:test", "req-1", request).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.is_bad_request() && err.message().contains("expires_at must be greater than current timestamp")
    );
}

#[tokio::test]
async fn test_create_flag_future_expiration_ok() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = create_service(&pool);
    let uid = unique_id();
    let future_ts = chrono::Utc::now().timestamp_millis() + 3600000;
    let flag_key = format!("test-future-exp-{uid}");
    let request = CreateFeatureFlagRequest {
        flag_key: flag_key.clone(),
        target_scope: "global".to_string(),
        rollout_percent: 50,
        expires_at: Some(future_ts),
        reason: "test".to_string(),
        status: Some("draft".to_string()),
        targets: vec![],
    };
    let result = service.create_flag("@admin:test", "req-1", request).await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap().expires_at, Some(future_ts));
}

#[tokio::test]
async fn test_create_flag_none_expiration_ok() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = create_service(&pool);
    let uid = unique_id();
    let flag_key = format!("test-no-exp-{uid}");
    let request = CreateFeatureFlagRequest {
        flag_key: flag_key.clone(),
        target_scope: "global".to_string(),
        rollout_percent: 50,
        expires_at: None,
        reason: "test".to_string(),
        status: Some("draft".to_string()),
        targets: vec![],
    };
    let result = service.create_flag("@admin:test", "req-1", request).await;
    assert!(result.is_ok());
    assert!(result.unwrap().expires_at.is_none());
}

#[tokio::test]
async fn test_create_flag_none_status_defaults_to_draft() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = create_service(&pool);
    let uid = unique_id();
    let flag_key = format!("test-no-status-{uid}");
    let request = CreateFeatureFlagRequest {
        flag_key: flag_key.clone(),
        target_scope: "global".to_string(),
        rollout_percent: 50,
        expires_at: None,
        reason: "test".to_string(),
        status: None,
        targets: vec![],
    };
    let result = service.create_flag("@admin:test", "req-1", request).await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap().status, "draft");
}

#[tokio::test]
async fn test_create_flag_with_targets_success() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = create_service(&pool);
    let uid = unique_id();
    let flag_key = format!("test-with-targets-{uid}");
    let request = CreateFeatureFlagRequest {
        flag_key: flag_key.clone(),
        target_scope: "user".to_string(),
        rollout_percent: 75,
        expires_at: None,
        reason: "targeted rollout".to_string(),
        status: Some("active".to_string()),
        targets: vec![
            FeatureFlagTargetInput { subject_type: "user".to_string(), subject_id: "@alice:test".to_string() },
            FeatureFlagTargetInput { subject_type: "room".to_string(), subject_id: "!room1:test".to_string() },
            FeatureFlagTargetInput { subject_type: "tenant".to_string(), subject_id: "tenant-1".to_string() },
        ],
    };
    let result = service.create_flag("@admin:test", "req-1", request).await;
    assert!(result.is_ok());
    let flag = result.unwrap();
    assert_eq!(flag.targets.len(), 3);
}

#[tokio::test]
async fn test_create_flag_duplicate_key_returns_conflict() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = create_service(&pool);
    let uid = unique_id();
    let flag_key = format!("test-dup-key-{uid}");
    let request = make_create_request(&flag_key, "global");
    service.create_flag("@admin:test", "req-1", request.clone()).await.unwrap();
    let result = service.create_flag("@admin:test", "req-2", request).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.is_conflict() && err.message().contains("feature flag already exists"));
}

#[tokio::test]
async fn test_get_flag_empty_key() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = create_service(&pool);
    let result = service.get_flag("").await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.is_bad_request() && err.message().contains("flag_key is required"));
}

#[tokio::test]
async fn test_get_flag_not_found() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = create_service(&pool);
    let result = service.get_flag("nonexistent.flag").await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.is_not_found() && err.message().contains("feature flag not found"));
}

#[tokio::test]
async fn test_get_flag_success() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = create_service(&pool);
    let uid = unique_id();
    let flag_key = format!("test-get-ok-{uid}");
    let request = CreateFeatureFlagRequest {
        flag_key: flag_key.clone(),
        target_scope: "global".to_string(),
        rollout_percent: 30,
        expires_at: None,
        reason: "get test".to_string(),
        status: Some("active".to_string()),
        targets: vec![FeatureFlagTargetInput { subject_type: "user".to_string(), subject_id: "@bob:test".to_string() }],
    };
    service.create_flag("@admin:test", "req-1", request).await.unwrap();
    let flag = service.get_flag(&flag_key).await.unwrap();
    assert_eq!(flag.flag_key, flag_key);
    assert_eq!(flag.rollout_percent, 30);
    assert_eq!(flag.status, "active");
    assert_eq!(flag.targets.len(), 1);
}

#[tokio::test]
async fn test_update_flag_empty_key() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = create_service(&pool);
    let request = UpdateFeatureFlagRequest { status: Some("active".to_string()), ..Default::default() };
    let result = service.update_flag("@admin:test", "req-1", "", request).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.is_bad_request() && err.message().contains("flag_key is required"));
}

#[tokio::test]
async fn test_update_flag_invalid_key_chars() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = create_service(&pool);
    let request = UpdateFeatureFlagRequest { status: Some("active".to_string()), ..Default::default() };
    let result = service.update_flag("@admin:test", "req-1", "INVALID KEY!", request).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_update_flag_invalid_status() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = create_service(&pool);
    let uid = unique_id();
    let flag_key = format!("test-upd-status-{uid}");
    service.create_flag("@admin:test", "req-1", make_create_request(&flag_key, "global")).await.unwrap();
    let request = UpdateFeatureFlagRequest { status: Some("bad_status".to_string()), ..Default::default() };
    let result = service.update_flag("@admin:test", "req-2", &flag_key, request).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.is_bad_request() && err.message().contains("unsupported feature flag status"));
}

#[tokio::test]
async fn test_update_flag_rollout_out_of_range() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = create_service(&pool);
    let uid = unique_id();
    let flag_key = format!("test-upd-rollout-{uid}");
    service.create_flag("@admin:test", "req-1", make_create_request(&flag_key, "global")).await.unwrap();
    let request = UpdateFeatureFlagRequest { rollout_percent: Some(200), ..Default::default() };
    let result = service.update_flag("@admin:test", "req-2", &flag_key, request).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_update_flag_empty_reason() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = create_service(&pool);
    let uid = unique_id();
    let flag_key = format!("test-upd-reason-{uid}");
    service.create_flag("@admin:test", "req-1", make_create_request(&flag_key, "global")).await.unwrap();
    let request = UpdateFeatureFlagRequest { reason: Some("   ".to_string()), ..Default::default() };
    let result = service.update_flag("@admin:test", "req-2", &flag_key, request).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_update_flag_invalid_targets() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = create_service(&pool);
    let uid = unique_id();
    let flag_key = format!("test-upd-targets-{uid}");
    service.create_flag("@admin:test", "req-1", make_create_request(&flag_key, "global")).await.unwrap();
    let request = UpdateFeatureFlagRequest {
        targets: Some(vec![FeatureFlagTargetInput {
            subject_type: "bad_type".to_string(),
            subject_id: "@user:test".to_string(),
        }]),
        ..Default::default()
    };
    let result = service.update_flag("@admin:test", "req-2", &flag_key, request).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_update_flag_expired_expiration() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = create_service(&pool);
    let uid = unique_id();
    let flag_key = format!("test-upd-exp-{uid}");
    service.create_flag("@admin:test", "req-1", make_create_request(&flag_key, "global")).await.unwrap();
    let past_ts = chrono::Utc::now().timestamp_millis() - 10000;
    let request = UpdateFeatureFlagRequest { expires_at: Some(past_ts), ..Default::default() };
    let result = service.update_flag("@admin:test", "req-2", &flag_key, request).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_update_flag_not_found() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = create_service(&pool);
    let request = UpdateFeatureFlagRequest { status: Some("active".to_string()), ..Default::default() };
    let result = service.update_flag("@admin:test", "req-1", "nonexistent.key", request).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.is_not_found() && err.message().contains("feature flag not found"));
}

#[tokio::test]
async fn test_update_flag_success() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = create_service(&pool);
    let uid = unique_id();
    let flag_key = format!("test-upd-ok-{uid}");
    service.create_flag("@admin:test", "req-1", make_create_request(&flag_key, "global")).await.unwrap();
    let request = UpdateFeatureFlagRequest {
        status: Some("active".to_string()),
        rollout_percent: Some(80),
        ..Default::default()
    };
    let updated = service.update_flag("@admin:test", "req-2", &flag_key, request).await.unwrap();
    assert_eq!(updated.flag_key, flag_key);
    assert_eq!(updated.status, "active");
    assert_eq!(updated.rollout_percent, 80);
}

#[tokio::test]
async fn test_update_flag_replace_targets() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = create_service(&pool);
    let uid = unique_id();
    let flag_key = format!("test-upd-replace-{uid}");
    let create_req = CreateFeatureFlagRequest {
        flag_key: flag_key.clone(),
        target_scope: "user".to_string(),
        rollout_percent: 50,
        expires_at: None,
        reason: "test".to_string(),
        status: Some("draft".to_string()),
        targets: vec![FeatureFlagTargetInput { subject_type: "user".to_string(), subject_id: "@old:test".to_string() }],
    };
    service.create_flag("@admin:test", "req-1", create_req).await.unwrap();
    let request = UpdateFeatureFlagRequest {
        targets: Some(vec![
            FeatureFlagTargetInput { subject_type: "user".to_string(), subject_id: "@new1:test".to_string() },
            FeatureFlagTargetInput { subject_type: "room".to_string(), subject_id: "!room1:test".to_string() },
        ]),
        ..Default::default()
    };
    let updated = service.update_flag("@admin:test", "req-2", &flag_key, request).await.unwrap();
    assert_eq!(updated.targets.len(), 2);
    let subject_ids: Vec<&str> = updated.targets.iter().map(|t| t.subject_id.as_str()).collect();
    assert!(subject_ids.contains(&"@new1:test"));
    assert!(subject_ids.contains(&"!room1:test"));
    assert!(!subject_ids.contains(&"@old:test"));
}

#[tokio::test]
async fn test_update_flag_multiple_fields() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = create_service(&pool);
    let uid = unique_id();
    let flag_key = format!("test-upd-multi-{uid}");
    let future_ts = chrono::Utc::now().timestamp_millis() + 7200000;
    service.create_flag("@admin:test", "req-1", make_create_request(&flag_key, "global")).await.unwrap();
    let request = UpdateFeatureFlagRequest {
        rollout_percent: Some(100),
        reason: Some("full rollout".to_string()),
        status: Some("fully_enabled".to_string()),
        expires_at: Some(future_ts),
        ..Default::default()
    };
    let updated = service.update_flag("@admin:test", "req-2", &flag_key, request).await.unwrap();
    assert_eq!(updated.rollout_percent, 100);
    assert_eq!(updated.reason, "full rollout");
    assert_eq!(updated.status, "fully_enabled");
    assert_eq!(updated.expires_at, Some(future_ts));
}

#[tokio::test]
async fn test_list_flags_invalid_scope_filter() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = create_service(&pool);
    let filters = FeatureFlagFilters {
        target_scope: Some("invalid_scope".to_string()),
        status: None,
        limit: 10,
        cursor_updated_ts: None,
        cursor_flag_key: None,
    };
    let result = service.list_flags(filters).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.is_bad_request() && err.message().contains("target_scope must be one of"));
}

#[tokio::test]
async fn test_list_flags_invalid_status_filter() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = create_service(&pool);
    let filters = FeatureFlagFilters {
        target_scope: None,
        status: Some("bad_status".to_string()),
        limit: 10,
        cursor_updated_ts: None,
        cursor_flag_key: None,
    };
    let result = service.list_flags(filters).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.is_bad_request() && err.message().contains("unsupported feature flag status"));
}

#[tokio::test]
async fn test_list_flags_no_filters() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = create_service(&pool);
    let uid = unique_id();
    let flag_key = format!("test-list-nofilter-{uid}");
    service.create_flag("@admin:test", "req-1", make_create_request(&flag_key, "global")).await.unwrap();
    let filters = FeatureFlagFilters {
        target_scope: None,
        status: None,
        limit: 10,
        cursor_updated_ts: None,
        cursor_flag_key: None,
    };
    let (flags, total) = service.list_flags(filters).await.unwrap();
    assert!(total >= 1);
    assert!(!flags.is_empty());
}

#[tokio::test]
async fn test_list_flags_with_scope_filter() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = create_service(&pool);
    let uid = unique_id();
    let scope = "room".to_string();
    let flag_key = format!("test-list-scope-{uid}");
    let request = CreateFeatureFlagRequest {
        flag_key: flag_key.clone(),
        target_scope: scope.clone(),
        rollout_percent: 50,
        expires_at: None,
        reason: "test".to_string(),
        status: Some("draft".to_string()),
        targets: vec![],
    };
    service.create_flag("@admin:test", "req-1", request).await.unwrap();
    let filters = FeatureFlagFilters {
        target_scope: Some(scope.clone()),
        status: None,
        limit: 10,
        cursor_updated_ts: None,
        cursor_flag_key: None,
    };
    let (flags, total) = service.list_flags(filters).await.unwrap();
    assert_eq!(total, 1);
    assert_eq!(flags.len(), 1);
    assert_eq!(flags[0].target_scope, scope);
}

#[tokio::test]
async fn test_list_flags_with_status_filter() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = create_service(&pool);
    let uid = unique_id();
    let scope = "room".to_string();
    let flag_key_a = format!("test-list-stata-{uid}");
    let flag_key_b = format!("test-list-statb-{uid}");
    let req_a = CreateFeatureFlagRequest {
        flag_key: flag_key_a,
        target_scope: scope.clone(),
        rollout_percent: 50,
        expires_at: None,
        reason: "test".to_string(),
        status: Some("active".to_string()),
        targets: vec![],
    };
    let req_b = CreateFeatureFlagRequest {
        flag_key: flag_key_b,
        target_scope: scope.clone(),
        rollout_percent: 50,
        expires_at: None,
        reason: "test".to_string(),
        status: Some("draft".to_string()),
        targets: vec![],
    };
    service.create_flag("@admin:test", "req-1", req_a).await.unwrap();
    service.create_flag("@admin:test", "req-2", req_b).await.unwrap();
    let filters = FeatureFlagFilters {
        target_scope: Some(scope),
        status: Some("active".to_string()),
        limit: 10,
        cursor_updated_ts: None,
        cursor_flag_key: None,
    };
    let (flags, total) = service.list_flags(filters).await.unwrap();
    assert_eq!(total, 1);
    assert_eq!(flags.len(), 1);
    assert_eq!(flags[0].status, "active");
}

#[tokio::test]
async fn test_create_flag_audit_event_created() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = create_service(&pool);
    let uid = unique_id();
    let flag_key = format!("test-audit-create-{uid}");
    let request = CreateFeatureFlagRequest {
        flag_key: flag_key.clone(),
        target_scope: "global".to_string(),
        rollout_percent: 50,
        expires_at: None,
        reason: "audit test".to_string(),
        status: Some("draft".to_string()),
        targets: vec![],
    };
    service.create_flag("@admin:test", "req-audit-1", request).await.unwrap();

    let audit_storage = AuditEventStorage::new(&pool);
    let filters = synapse_rust::storage::audit::AuditEventFilters {
        actor_id: Some("@admin:test".to_string()),
        action: Some("admin.feature_flag.create".to_string()),
        resource_type: Some("feature_flag".to_string()),
        resource_id: Some(flag_key.clone()),
        result: Some("success".to_string()),
        limit: 10,
        from: None,
    };
    let (events, total, _) = audit_storage.list_events(&filters).await.unwrap();
    assert_eq!(total, 1);
    assert_eq!(events[0].action, "admin.feature_flag.create");
    assert_eq!(events[0].resource_id, flag_key);
}

#[tokio::test]
async fn test_update_flag_audit_event_created() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = create_service(&pool);
    let uid = unique_id();
    let flag_key = format!("test-audit-update-{uid}");
    service.create_flag("@admin:test", "req-audit-create", make_create_request(&flag_key, "global")).await.unwrap();
    let request = UpdateFeatureFlagRequest { status: Some("active".to_string()), ..Default::default() };
    service.update_flag("@admin:test", "req-audit-update", &flag_key, request).await.unwrap();

    let audit_storage = AuditEventStorage::new(&pool);
    let filters = synapse_rust::storage::audit::AuditEventFilters {
        actor_id: Some("@admin:test".to_string()),
        action: Some("admin.feature_flag.update".to_string()),
        resource_type: Some("feature_flag".to_string()),
        resource_id: Some(flag_key.clone()),
        result: Some("success".to_string()),
        limit: 10,
        from: None,
    };
    let (events, total, _) = audit_storage.list_events(&filters).await.unwrap();
    assert_eq!(total, 1);
    assert_eq!(events[0].action, "admin.feature_flag.update");
}

#[tokio::test]
async fn test_create_flag_valid_target_subject_types() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = create_service(&pool);
    let uid = unique_id();
    for (i, subject_type) in ["tenant", "room", "user"].iter().enumerate() {
        let flag_key = format!("test-subj-type-{uid}-{i}");
        let request = CreateFeatureFlagRequest {
            flag_key: flag_key.clone(),
            target_scope: "user".to_string(),
            rollout_percent: 50,
            expires_at: None,
            reason: "test".to_string(),
            status: Some("draft".to_string()),
            targets: vec![FeatureFlagTargetInput {
                subject_type: subject_type.to_string(),
                subject_id: format!("id-{i}"),
            }],
        };
        let result = service.create_flag("@admin:test", "req-1", request).await;
        assert!(result.is_ok(), "subject_type '{}' should be valid", subject_type);
    }
}

#[tokio::test]
async fn test_update_flag_duplicate_targets_rejected() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = create_service(&pool);
    let uid = unique_id();
    let flag_key = format!("test-upd-dup-targets-{uid}");
    service.create_flag("@admin:test", "req-1", make_create_request(&flag_key, "global")).await.unwrap();
    let request = UpdateFeatureFlagRequest {
        targets: Some(vec![
            FeatureFlagTargetInput { subject_type: "user".to_string(), subject_id: "@dup:test".to_string() },
            FeatureFlagTargetInput { subject_type: "user".to_string(), subject_id: "@dup:test".to_string() },
        ]),
        ..Default::default()
    };
    let result = service.update_flag("@admin:test", "req-2", &flag_key, request).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_update_flag_empty_target_subject_id_rejected() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = create_service(&pool);
    let uid = unique_id();
    let flag_key = format!("test-upd-empty-sid-{uid}");
    service.create_flag("@admin:test", "req-1", make_create_request(&flag_key, "global")).await.unwrap();
    let request = UpdateFeatureFlagRequest {
        targets: Some(vec![FeatureFlagTargetInput { subject_type: "user".to_string(), subject_id: "  ".to_string() }]),
        ..Default::default()
    };
    let result = service.update_flag("@admin:test", "req-2", &flag_key, request).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_update_flag_preserves_unupdated_fields() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = create_service(&pool);
    let uid = unique_id();
    let flag_key = format!("test-upd-preserve-{uid}");
    let create_req = CreateFeatureFlagRequest {
        flag_key: flag_key.clone(),
        target_scope: "global".to_string(),
        rollout_percent: 25,
        expires_at: None,
        reason: "original reason".to_string(),
        status: Some("draft".to_string()),
        targets: vec![FeatureFlagTargetInput {
            subject_type: "user".to_string(),
            subject_id: "@keep:test".to_string(),
        }],
    };
    service.create_flag("@admin:test", "req-1", create_req).await.unwrap();
    let request = UpdateFeatureFlagRequest { rollout_percent: Some(75), ..Default::default() };
    let updated = service.update_flag("@admin:test", "req-2", &flag_key, request).await.unwrap();
    assert_eq!(updated.rollout_percent, 75);
    assert_eq!(updated.reason, "original reason");
    assert_eq!(updated.status, "draft");
    assert_eq!(updated.targets.len(), 1);
    assert_eq!(updated.targets[0].subject_id, "@keep:test");
}

#[tokio::test]
async fn test_list_flags_empty_result() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let service = create_service(&pool);
    let filters = FeatureFlagFilters {
        target_scope: Some("tenant".to_string()),
        status: None,
        limit: 10,
        cursor_updated_ts: None,
        cursor_flag_key: None,
    };
    let (flags, total) = service.list_flags(filters).await.unwrap();
    assert!(flags.is_empty());
    assert_eq!(total, 0);
}

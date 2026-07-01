//! Integration tests for `synapse-services/src/retention_service.rs`.
//!
//! Covers all 25 public methods of `RetentionService` against the shared
//! integration Postgres pool, following the warm_up_pool + Mutex guard +
//! unique_id pattern. Only compilation is verified in CI without a live
//! database; the tests themselves are not run here.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![allow(clippy::await_holding_lock)]

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use synapse_common::config::RetentionConfig;
use synapse_common::metrics::MetricsCollector;
use synapse_services::retention_service::{DataLifecycleCleanupSummary, RetentionService, RetentionStatusSummary};
use synapse_storage::audit::AuditEventStorage;
use synapse_storage::media::ChunkedUploadStorage;
use synapse_storage::retention::{
    CreateRoomRetentionPolicyRequest, EffectiveRetentionPolicy, RoomRetentionPolicy, RetentionCleanupLog,
    RetentionStorage, RetentionStats, ServerRetentionPolicy, UpdateRoomRetentionPolicyRequest,
    UpdateServerRetentionPolicyRequest,
};

static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

fn unique_id() -> u64 {
    TEST_COUNTER.fetch_add(1, Ordering::SeqCst)
}

fn retention_test_guard() -> &'static Mutex<()> {
    static GUARD: OnceLock<Mutex<()>> = OnceLock::new();
    GUARD.get_or_init(|| Mutex::new(()))
}

/// Warm up the shared pool on the current tokio runtime.
/// SELECT 1 with 8 retries and 400ms backoff fixes cross-runtime sqlx pool isolation.
async fn warm_up_pool(pool: &Arc<sqlx::PgPool>) {
    for _ in 0..8 {
        match tokio::time::timeout(
            std::time::Duration::from_secs(5),
            sqlx::query("SELECT 1").execute(pool.as_ref()),
        )
        .await
        {
            Ok(Ok(_)) => return,
            Ok(Err(_)) | Err(_) => {
                tokio::time::sleep(std::time::Duration::from_millis(400)).await;
            }
        }
    }
    let _ = sqlx::query("SELECT 1").execute(pool.as_ref()).await;
}

/// Create all tables used by retention_service.rs (idempotent against the real
/// migrated schema — `CREATE TABLE IF NOT EXISTS` is a no-op there).
async fn setup_test_database(pool: &Arc<sqlx::PgPool>) {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS room_retention_policies (
            id BIGSERIAL PRIMARY KEY,
            room_id TEXT NOT NULL UNIQUE,
            max_lifetime BIGINT,
            min_lifetime BIGINT NOT NULL DEFAULT 0,
            is_expire_on_clients BOOLEAN NOT NULL DEFAULT FALSE,
            is_server_default BOOLEAN NOT NULL DEFAULT FALSE,
            created_ts BIGINT NOT NULL,
            updated_ts BIGINT
        )
        "#,
    )
    .execute(pool.as_ref())
    .await
    .expect("Failed to create room_retention_policies table");

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS server_retention_policy (
            id BIGSERIAL PRIMARY KEY,
            max_lifetime BIGINT,
            min_lifetime BIGINT NOT NULL DEFAULT 0,
            is_expire_on_clients BOOLEAN NOT NULL DEFAULT FALSE,
            created_ts BIGINT NOT NULL,
            updated_ts BIGINT
        )
        "#,
    )
    .execute(pool.as_ref())
    .await
    .expect("Failed to create server_retention_policy table");

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS events (
            event_id TEXT NOT NULL PRIMARY KEY,
            room_id TEXT NOT NULL,
            sender TEXT NOT NULL,
            event_type TEXT NOT NULL,
            content JSONB NOT NULL DEFAULT '{}',
            origin_server_ts BIGINT NOT NULL,
            state_key TEXT,
            is_redacted BOOLEAN DEFAULT FALSE,
            depth BIGINT
        )
        "#,
    )
    .execute(pool.as_ref())
    .await
    .expect("Failed to create events table");

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

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS upload_progress (
            upload_id TEXT NOT NULL PRIMARY KEY,
            user_id TEXT NOT NULL,
            filename TEXT,
            content_type TEXT,
            total_size BIGINT,
            uploaded_size BIGINT NOT NULL DEFAULT 0,
            total_chunks INTEGER NOT NULL DEFAULT 0,
            uploaded_chunks INTEGER NOT NULL DEFAULT 0,
            status TEXT NOT NULL DEFAULT 'pending',
            created_ts BIGINT NOT NULL,
            updated_ts BIGINT,
            expires_at BIGINT
        )
        "#,
    )
    .execute(pool.as_ref())
    .await
    .expect("Failed to create upload_progress table");

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS upload_chunks (
            id BIGSERIAL PRIMARY KEY,
            upload_id TEXT NOT NULL,
            chunk_index INTEGER NOT NULL,
            chunk_data BYTEA NOT NULL,
            chunk_size BIGINT NOT NULL,
            created_ts BIGINT NOT NULL,
            UNIQUE (upload_id, chunk_index)
        )
        "#,
    )
    .execute(pool.as_ref())
    .await
    .expect("Failed to create upload_chunks table");

    // Seed the default server policy row (id=1) so fetch_one-based getters work.
    sqlx::query(
        "INSERT INTO server_retention_policy (id, min_lifetime, is_expire_on_clients, created_ts) \
         VALUES (1, 0, false, 0) ON CONFLICT (id) DO NOTHING",
    )
    .execute(pool.as_ref())
    .await
    .expect("Failed to seed default server retention policy");
}

/// Clean all retention-owned rows and reset server policy to a known default.
async fn setup(pool: &Arc<sqlx::PgPool>) {
    warm_up_pool(pool).await;
    sqlx::query("DELETE FROM upload_chunks").execute(pool.as_ref()).await.ok();
    sqlx::query("DELETE FROM upload_progress").execute(pool.as_ref()).await.ok();
    sqlx::query("DELETE FROM audit_events").execute(pool.as_ref()).await.ok();
    sqlx::query("DELETE FROM events").execute(pool.as_ref()).await.ok();
    sqlx::query("DELETE FROM room_retention_policies").execute(pool.as_ref()).await.ok();
    // Reset the single server policy row to a predictable default state.
    sqlx::query(
        "UPDATE server_retention_policy \
         SET max_lifetime = NULL, min_lifetime = 0, is_expire_on_clients = false, updated_ts = created_ts \
         WHERE id = 1",
    )
    .execute(pool.as_ref())
    .await
    .ok();
}

/// Build a `RetentionService` wired to the shared pool.
fn make_service(pool: &Arc<sqlx::PgPool>) -> RetentionService {
    let storage = Arc::new(RetentionStorage::new(pool));
    let chunked_upload_storage = Arc::new(ChunkedUploadStorage::new(pool));
    let audit_storage = Arc::new(AuditEventStorage::new(pool));
    let metrics = Arc::new(MetricsCollector::new());
    RetentionService::new(storage, chunked_upload_storage, &metrics, audit_storage)
}

fn make_room_id() -> String {
    format!("!rt_room_{}:localhost", unique_id())
}

/// Insert a non-state message event with the given origin_server_ts.
async fn insert_event(pool: &Arc<sqlx::PgPool>, room_id: &str, event_id: &str, ts: i64, event_type: &str) {
    sqlx::query(
        r"INSERT INTO events (event_id, room_id, sender, event_type, content, origin_server_ts, state_key)
           VALUES ($1, $2, '@rt:localhost', $3, '{}'::jsonb, $4, NULL)
           ON CONFLICT (event_id) DO NOTHING",
    )
    .bind(event_id)
    .bind(room_id)
    .bind(event_type)
    .bind(ts)
    .execute(pool.as_ref())
    .await
    .expect("Failed to insert test event");
}

// =============================================================================
// new
// =============================================================================

#[tokio::test]
async fn test_new_constructs_service() {
    let _guard = retention_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    setup(&pool).await;
    let service = make_service(&pool);
    // A trivial no-op call proves the service was constructed with usable storages.
    assert_eq!(service.process_pending_cleanups(10).await.unwrap(), 0);
}

// =============================================================================
// set_room_policy
// =============================================================================

#[tokio::test]
async fn test_set_room_policy_creates_and_returns() {
    let _guard = retention_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    setup(&pool).await;
    let service = make_service(&pool);

    let room_id = make_room_id();
    let policy = service
        .set_room_policy(CreateRoomRetentionPolicyRequest {
            room_id: room_id.clone(),
            max_lifetime: Some(86_400_000),
            min_lifetime: Some(3_600_000),
            is_expire_on_clients: Some(true),
        })
        .await
        .unwrap();

    assert_eq!(policy.room_id, room_id);
    assert_eq!(policy.max_lifetime, Some(86_400_000));
    assert_eq!(policy.min_lifetime, 3_600_000);
    assert!(policy.is_expire_on_clients);
    assert!(!policy.is_server_default);
    assert!(policy.id > 0);
}

#[tokio::test]
async fn test_set_room_policy_rejects_negative_max_lifetime() {
    let _guard = retention_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    setup(&pool).await;
    let service = make_service(&pool);

    let room_id = make_room_id();
    let result = service
        .set_room_policy(CreateRoomRetentionPolicyRequest {
            room_id: room_id.clone(),
            max_lifetime: Some(-1),
            min_lifetime: None,
            is_expire_on_clients: None,
        })
        .await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.is_bad_request(), "negative max_lifetime should be bad_request");
    assert!(err.message().contains("max_lifetime cannot be negative"));

    // No row should have been created.
    assert!(service.get_room_policy(&room_id).await.unwrap().is_none());
}

#[tokio::test]
async fn test_set_room_policy_upserts_on_conflict() {
    let _guard = retention_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    setup(&pool).await;
    let service = make_service(&pool);

    let room_id = make_room_id();
    let first = service
        .set_room_policy(CreateRoomRetentionPolicyRequest {
            room_id: room_id.clone(),
            max_lifetime: Some(86_400_000),
            min_lifetime: Some(0),
            is_expire_on_clients: Some(true),
        })
        .await
        .unwrap();

    // Second call with same room_id upserts.
    let second = service
        .set_room_policy(CreateRoomRetentionPolicyRequest {
            room_id: room_id.clone(),
            max_lifetime: Some(172_800_000),
            min_lifetime: Some(1_000),
            is_expire_on_clients: Some(false),
        })
        .await
        .unwrap();

    assert_eq!(second.id, first.id, "upsert should keep the same row id");
    assert_eq!(second.max_lifetime, Some(172_800_000));
    assert_eq!(second.min_lifetime, 1_000);
    assert!(!second.is_expire_on_clients);
}

#[tokio::test]
async fn test_set_room_policy_applies_defaults_for_none_fields() {
    let _guard = retention_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    setup(&pool).await;
    let service = make_service(&pool);

    let room_id = make_room_id();
    let policy = service
        .set_room_policy(CreateRoomRetentionPolicyRequest {
            room_id: room_id.clone(),
            max_lifetime: Some(1000),
            min_lifetime: None,
            is_expire_on_clients: None,
        })
        .await
        .unwrap();

    // Storage substitutes defaults: min_lifetime=0, is_expire_on_clients=false.
    assert_eq!(policy.min_lifetime, 0);
    assert!(!policy.is_expire_on_clients);
}

// =============================================================================
// get_room_policy
// =============================================================================

#[tokio::test]
async fn test_get_room_policy_existing() {
    let _guard = retention_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    setup(&pool).await;
    let service = make_service(&pool);

    let room_id = make_room_id();
    service
        .set_room_policy(CreateRoomRetentionPolicyRequest {
            room_id: room_id.clone(),
            max_lifetime: Some(86_400_000),
            min_lifetime: Some(0),
            is_expire_on_clients: Some(true),
        })
        .await
        .unwrap();

    let fetched = service.get_room_policy(&room_id).await.unwrap().expect("policy should exist");
    assert_eq!(fetched.room_id, room_id);
    assert_eq!(fetched.max_lifetime, Some(86_400_000));
}

#[tokio::test]
async fn test_get_room_policy_nonexistent_returns_none() {
    let _guard = retention_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    setup(&pool).await;
    let service = make_service(&pool);

    let result = service.get_room_policy(&make_room_id()).await.unwrap();
    assert!(result.is_none());
}

// =============================================================================
// update_room_policy
// =============================================================================

#[tokio::test]
async fn test_update_room_policy_partial_update() {
    let _guard = retention_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    setup(&pool).await;
    let service = make_service(&pool);

    let room_id = make_room_id();
    service
        .set_room_policy(CreateRoomRetentionPolicyRequest {
            room_id: room_id.clone(),
            max_lifetime: Some(86_400_000),
            min_lifetime: Some(0),
            is_expire_on_clients: Some(true),
        })
        .await
        .unwrap();

    // Partial update: only max_lifetime provided; others use COALESCE -> keep existing.
    let updated = service
        .update_room_policy(
            &room_id,
            UpdateRoomRetentionPolicyRequest {
                max_lifetime: Some(172_800_000),
                min_lifetime: None,
                is_expire_on_clients: None,
            },
        )
        .await
        .unwrap();

    assert_eq!(updated.max_lifetime, Some(172_800_000));
    assert_eq!(updated.min_lifetime, 0, "min_lifetime should be unchanged");
    assert!(updated.is_expire_on_clients, "is_expire_on_clients should be unchanged");
}

#[tokio::test]
async fn test_update_room_policy_nonexistent_returns_error() {
    let _guard = retention_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    setup(&pool).await;
    let service = make_service(&pool);

    // fetch_one on missing row -> sqlx::Error::RowNotFound -> ApiError::internal.
    let result = service
        .update_room_policy(
            &make_room_id(),
            UpdateRoomRetentionPolicyRequest { max_lifetime: Some(1), min_lifetime: None, is_expire_on_clients: None },
        )
        .await;
    assert!(result.is_err());
}

// =============================================================================
// delete_room_policy
// =============================================================================

#[tokio::test]
async fn test_delete_room_policy_existing() {
    let _guard = retention_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    setup(&pool).await;
    let service = make_service(&pool);

    let room_id = make_room_id();
    service
        .set_room_policy(CreateRoomRetentionPolicyRequest {
            room_id: room_id.clone(),
            max_lifetime: Some(1000),
            min_lifetime: Some(0),
            is_expire_on_clients: Some(false),
        })
        .await
        .unwrap();
    assert!(service.get_room_policy(&room_id).await.unwrap().is_some());

    service.delete_room_policy(&room_id).await.unwrap();
    assert!(service.get_room_policy(&room_id).await.unwrap().is_none());
}

#[tokio::test]
async fn test_delete_room_policy_nonexistent_no_error() {
    let _guard = retention_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    setup(&pool).await;
    let service = make_service(&pool);

    // DELETE on a missing room_id affects zero rows but is not an error.
    let result = service.delete_room_policy(&make_room_id()).await;
    assert!(result.is_ok());
}

// =============================================================================
// get_rooms_with_policies
// =============================================================================

#[tokio::test]
async fn test_get_rooms_with_policies_returns_all_ordered() {
    let _guard = retention_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    setup(&pool).await;
    let service = make_service(&pool);

    let room_a = format!("!rt_a_{}:localhost", unique_id());
    let room_b = format!("!rt_b_{}:localhost", unique_id());
    service
        .set_room_policy(CreateRoomRetentionPolicyRequest {
            room_id: room_b.clone(),
            max_lifetime: Some(1000),
            min_lifetime: Some(0),
            is_expire_on_clients: Some(false),
        })
        .await
        .unwrap();
    service
        .set_room_policy(CreateRoomRetentionPolicyRequest {
            room_id: room_a.clone(),
            max_lifetime: Some(2000),
            min_lifetime: Some(0),
            is_expire_on_clients: Some(false),
        })
        .await
        .unwrap();

    let policies = service.get_rooms_with_policies().await.unwrap();
    assert_eq!(policies.len(), 2);
    // Storage orders by room_id ASC.
    assert_eq!(policies[0].room_id, room_a);
    assert_eq!(policies[1].room_id, room_b);
}

#[tokio::test]
async fn test_get_rooms_with_policies_empty() {
    let _guard = retention_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    setup(&pool).await;
    let service = make_service(&pool);

    let policies = service.get_rooms_with_policies().await.unwrap();
    assert!(policies.is_empty());
}

// =============================================================================
// get_effective_policy
// =============================================================================

#[tokio::test]
async fn test_get_effective_policy_prefers_room_policy() {
    let _guard = retention_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    setup(&pool).await;
    let service = make_service(&pool);

    let room_id = make_room_id();
    service
        .set_room_policy(CreateRoomRetentionPolicyRequest {
            room_id: room_id.clone(),
            max_lifetime: Some(86_400_000),
            min_lifetime: Some(3_600_000),
            is_expire_on_clients: Some(true),
        })
        .await
        .unwrap();

    let effective = service.get_effective_policy(&room_id).await.unwrap();
    assert_eq!(effective.max_lifetime, Some(86_400_000));
    assert_eq!(effective.min_lifetime, 3_600_000);
    assert!(effective.is_expire_on_clients);
}

#[tokio::test]
async fn test_get_effective_policy_falls_back_to_server_policy() {
    let _guard = retention_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    setup(&pool).await;
    let service = make_service(&pool);

    // Configure server policy with a max_lifetime.
    service
        .update_server_policy(UpdateServerRetentionPolicyRequest {
            max_lifetime: Some(259_200_000),
            min_lifetime: Some(10_800_000),
            is_expire_on_clients: Some(true),
        })
        .await
        .unwrap();

    // No room policy -> effective policy comes from the server policy.
    let effective = service.get_effective_policy(&make_room_id()).await.unwrap();
    assert_eq!(effective.max_lifetime, Some(259_200_000));
    assert_eq!(effective.min_lifetime, 10_800_000);
    assert!(effective.is_expire_on_clients);
}

#[tokio::test]
async fn test_get_effective_policy_with_no_max_lifetime_anywhere() {
    let _guard = retention_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    setup(&pool).await;
    let service = make_service(&pool);

    // Default server policy has max_lifetime = NULL; no room policy either.
    let effective = service.get_effective_policy(&make_room_id()).await.unwrap();
    assert!(effective.max_lifetime.is_none());
    assert_eq!(effective.min_lifetime, 0);
    assert!(!effective.is_expire_on_clients);
}

// =============================================================================
// resolve_effective_policy
// =============================================================================

#[tokio::test]
async fn test_resolve_effective_policy_with_room_policy() {
    let _guard = retention_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    setup(&pool).await;
    let service = make_service(&pool);

    let room_id = make_room_id();
    service
        .set_room_policy(CreateRoomRetentionPolicyRequest {
            room_id: room_id.clone(),
            max_lifetime: Some(86_400_000),
            min_lifetime: Some(3_600_000),
            is_expire_on_clients: Some(true),
        })
        .await
        .unwrap();

    let resolved = service.resolve_effective_policy(&room_id).await.unwrap();
    assert_eq!(resolved.room_id, room_id);
    assert_eq!(resolved.max_lifetime, Some(86_400_000));
    assert_eq!(resolved.min_lifetime, 3_600_000);
    assert!(resolved.is_expire_on_clients);
    assert!(!resolved.is_server_default, "room policy should not be flagged as server default");
    assert!(resolved.id > 0);
}

#[tokio::test]
async fn test_resolve_effective_policy_falls_back_to_server_policy() {
    let _guard = retention_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    setup(&pool).await;
    let service = make_service(&pool);

    service
        .update_server_policy(UpdateServerRetentionPolicyRequest {
            max_lifetime: Some(259_200_000),
            min_lifetime: Some(10_800_000),
            is_expire_on_clients: Some(true),
        })
        .await
        .unwrap();

    let room_id = make_room_id();
    let resolved = service.resolve_effective_policy(&room_id).await.unwrap();
    assert_eq!(resolved.room_id, room_id);
    assert_eq!(resolved.max_lifetime, Some(259_200_000));
    assert_eq!(resolved.min_lifetime, 10_800_000);
    assert!(resolved.is_expire_on_clients);
    assert!(resolved.is_server_default, "should be flagged as a synthesized server default");
    assert_eq!(resolved.id, 0, "synthesized policy uses placeholder id 0");
}

#[tokio::test]
async fn test_resolve_effective_policy_hardcoded_default_when_no_server_policy_max() {
    let _guard = retention_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    setup(&pool).await;
    let service = make_service(&pool);

    // Server policy exists but with max_lifetime = NULL (default seeded row).
    // resolve_effective_policy still falls into the Some(sp) branch (sp exists),
    // so it copies the server policy fields (max_lifetime=None, min=0, expire=false).
    let room_id = make_room_id();
    let resolved = service.resolve_effective_policy(&room_id).await.unwrap();
    assert_eq!(resolved.room_id, room_id);
    assert!(resolved.max_lifetime.is_none());
    assert_eq!(resolved.min_lifetime, 0);
    assert!(!resolved.is_expire_on_clients);
    assert!(resolved.is_server_default);
}

// =============================================================================
// get_server_policy / get_server_policy_optional
// =============================================================================

#[tokio::test]
async fn test_get_server_policy_returns_default_row() {
    let _guard = retention_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    setup(&pool).await;
    let service = make_service(&pool);

    let policy = service.get_server_policy().await.unwrap();
    assert_eq!(policy.id, 1);
    assert!(policy.max_lifetime.is_none());
    assert_eq!(policy.min_lifetime, 0);
}

#[tokio::test]
async fn test_get_server_policy_optional_existing() {
    let _guard = retention_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    setup(&pool).await;
    let service = make_service(&pool);

    let policy = service.get_server_policy_optional().await.unwrap();
    assert!(policy.is_some());
    assert_eq!(policy.unwrap().id, 1);
}

// =============================================================================
// update_server_policy / upsert_server_policy
// =============================================================================

#[tokio::test]
async fn test_update_server_policy_partial_update() {
    let _guard = retention_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    setup(&pool).await;
    let service = make_service(&pool);

    // First set both fields.
    service
        .update_server_policy(UpdateServerRetentionPolicyRequest {
            max_lifetime: Some(86_400_000),
            min_lifetime: Some(3_600_000),
            is_expire_on_clients: Some(true),
        })
        .await
        .unwrap();

    // Partial update: only max_lifetime — others keep prior values.
    let updated = service
        .update_server_policy(UpdateServerRetentionPolicyRequest {
            max_lifetime: Some(172_800_000),
            min_lifetime: None,
            is_expire_on_clients: None,
        })
        .await
        .unwrap();

    assert_eq!(updated.max_lifetime, Some(172_800_000));
    assert_eq!(updated.min_lifetime, 3_600_000, "min_lifetime should be unchanged");
    assert!(updated.is_expire_on_clients, "is_expire_on_clients should be unchanged");
}

#[tokio::test]
async fn test_upsert_server_policy_inserts_then_updates() {
    let _guard = retention_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    setup(&pool).await;
    let service = make_service(&pool);

    // upsert always targets id=1; the default row already exists, so this updates it.
    let first = service
        .upsert_server_policy(UpdateServerRetentionPolicyRequest {
            max_lifetime: Some(100_000),
            min_lifetime: Some(1),
            is_expire_on_clients: Some(true),
        })
        .await
        .unwrap();
    assert_eq!(first.id, 1);
    assert_eq!(first.max_lifetime, Some(100_000));

    // Second upsert overwrites.
    let second = service
        .upsert_server_policy(UpdateServerRetentionPolicyRequest {
            max_lifetime: Some(200_000),
            min_lifetime: Some(2),
            is_expire_on_clients: Some(false),
        })
        .await
        .unwrap();
    assert_eq!(second.id, 1);
    assert_eq!(second.max_lifetime, Some(200_000));
    assert_eq!(second.min_lifetime, 2);
    assert!(!second.is_expire_on_clients);

    // Only one row exists.
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM server_retention_policy")
        .fetch_one(pool.as_ref())
        .await
        .unwrap();
    assert_eq!(count, 1);
}

// =============================================================================
// run_cleanup
// =============================================================================

#[tokio::test]
async fn test_run_cleanup_deletes_expired_events() {
    let _guard = retention_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    setup(&pool).await;
    let service = make_service(&pool);

    let room_id = make_room_id();
    // 1 day max lifetime.
    service
        .set_room_policy(CreateRoomRetentionPolicyRequest {
            room_id: room_id.clone(),
            max_lifetime: Some(86_400_000),
            min_lifetime: Some(0),
            is_expire_on_clients: Some(false),
        })
        .await
        .unwrap();

    let now = chrono::Utc::now().timestamp_millis();
    // Expired (2 days old) + recent (1 second ago).
    insert_event(&pool, &room_id, "$rt_old:localhost", now - 2 * 86_400_000, "m.room.message").await;
    insert_event(&pool, &room_id, "$rt_new:localhost", now - 1_000, "m.room.message").await;

    let log = service.run_cleanup(&room_id).await.unwrap();
    assert_eq!(log.room_id, room_id);
    assert_eq!(log.events_deleted, 1, "only the expired event should be deleted");
    assert_eq!(log.status, "completed");
    assert!(log.completed_ts.is_some());

    // The recent event survives.
    let remaining: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE room_id = $1").bind(&room_id).fetch_one(pool.as_ref()).await.unwrap();
    assert_eq!(remaining, 1);
}

#[tokio::test]
async fn test_run_cleanup_no_max_lifetime_returns_bad_request() {
    let _guard = retention_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    setup(&pool).await;
    let service = make_service(&pool);

    let room_id = make_room_id();
    // Room policy with max_lifetime = None, and server policy max_lifetime = NULL too.
    service
        .set_room_policy(CreateRoomRetentionPolicyRequest {
            room_id: room_id.clone(),
            max_lifetime: None,
            min_lifetime: Some(0),
            is_expire_on_clients: Some(false),
        })
        .await
        .unwrap();

    let result = service.run_cleanup(&room_id).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.is_bad_request());
    assert!(err.message().contains("No retention policy configured"));
}

#[tokio::test]
async fn test_run_cleanup_protects_state_events() {
    let _guard = retention_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    setup(&pool).await;
    let service = make_service(&pool);

    let room_id = make_room_id();
    service
        .set_room_policy(CreateRoomRetentionPolicyRequest {
            room_id: room_id.clone(),
            max_lifetime: Some(1), // 1ms — anything older than 1ms ago is "expired"
            min_lifetime: Some(0),
            is_expire_on_clients: Some(false),
        })
        .await
        .unwrap();

    let now = chrono::Utc::now().timestamp_millis();
    // A state event (state_key IS NOT NULL) — should NOT be deleted even when old.
    sqlx::query(
        r"INSERT INTO events (event_id, room_id, sender, event_type, content, origin_server_ts, state_key)
           VALUES ($1, $2, '@rt:localhost', 'm.room.member', '{}'::jsonb, $3, '')",
    )
    .bind("$rt_state:localhost")
    .bind(&room_id)
    .bind(now - 86_400_000)
    .execute(pool.as_ref())
    .await
    .unwrap();
    // A protected event_type message with state_key NULL — protected by event_type filter.
    insert_event(&pool, &room_id, "$rt_create:localhost", now - 86_400_000, "m.room.create").await;

    let log = service.run_cleanup(&room_id).await.unwrap();
    assert_eq!(log.events_deleted, 0, "state events and protected types must not be deleted");

    let remaining: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE room_id = $1").bind(&room_id).fetch_one(pool.as_ref()).await.unwrap();
    assert_eq!(remaining, 2);
}

#[tokio::test]
async fn test_run_cleanup_for_nonexistent_room_deletes_nothing() {
    let _guard = retention_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    setup(&pool).await;
    let service = make_service(&pool);

    // No room policy + default server policy has max_lifetime = NULL -> bad_request.
    let result = service.run_cleanup(&make_room_id()).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().is_bad_request());
}

// =============================================================================
// is_event_expired
// =============================================================================

#[tokio::test]
async fn test_is_event_expired_true_when_old() {
    let _guard = retention_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    setup(&pool).await;
    let service = make_service(&pool);

    let room_id = make_room_id();
    // 1 day max lifetime.
    service
        .set_room_policy(CreateRoomRetentionPolicyRequest {
            room_id: room_id.clone(),
            max_lifetime: Some(86_400_000),
            min_lifetime: Some(0),
            is_expire_on_clients: Some(false),
        })
        .await
        .unwrap();

    let now = chrono::Utc::now().timestamp_millis();
    // Event older than the cutoff.
    assert!(service.is_event_expired(&room_id, now - 2 * 86_400_000).await.unwrap());
}

#[tokio::test]
async fn test_is_event_expired_false_when_recent() {
    let _guard = retention_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    setup(&pool).await;
    let service = make_service(&pool);

    let room_id = make_room_id();
    service
        .set_room_policy(CreateRoomRetentionPolicyRequest {
            room_id: room_id.clone(),
            max_lifetime: Some(86_400_000),
            min_lifetime: Some(0),
            is_expire_on_clients: Some(false),
        })
        .await
        .unwrap();

    let now = chrono::Utc::now().timestamp_millis();
    // Event 1 second ago — within the retention window.
    assert!(!service.is_event_expired(&room_id, now - 1_000).await.unwrap());
}

#[tokio::test]
async fn test_is_event_expired_false_when_no_max_lifetime() {
    let _guard = retention_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    setup(&pool).await;
    let service = make_service(&pool);

    let room_id = make_room_id();
    // Room policy with max_lifetime = None, server policy max_lifetime = NULL.
    service
        .set_room_policy(CreateRoomRetentionPolicyRequest {
            room_id: room_id.clone(),
            max_lifetime: None,
            min_lifetime: Some(0),
            is_expire_on_clients: Some(false),
        })
        .await
        .unwrap();

    // No max_lifetime -> nothing is ever expired.
    assert!(!service.is_event_expired(&room_id, 0).await.unwrap());
}

// =============================================================================
// run_scheduled_cleanups
// =============================================================================

#[tokio::test]
async fn test_run_scheduled_cleanups_processes_all_rooms_with_max_lifetime() {
    let _guard = retention_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    setup(&pool).await;
    let service = make_service(&pool);

    let room_a = format!("!rt_sch_a_{}:localhost", unique_id());
    let room_b = format!("!rt_sch_b_{}:localhost", unique_id());
    // room_a has a max_lifetime and an expired event -> cleaned.
    service
        .set_room_policy(CreateRoomRetentionPolicyRequest {
            room_id: room_a.clone(),
            max_lifetime: Some(1),
            min_lifetime: Some(0),
            is_expire_on_clients: Some(false),
        })
        .await
        .unwrap();
    let now = chrono::Utc::now().timestamp_millis();
    insert_event(&pool, &room_a, "$rt_sch_a_old:localhost", now - 86_400_000, "m.room.message").await;

    // room_b has a policy but max_lifetime = None -> skipped (no cleanup).
    service
        .set_room_policy(CreateRoomRetentionPolicyRequest {
            room_id: room_b.clone(),
            max_lifetime: None,
            min_lifetime: Some(0),
            is_expire_on_clients: Some(false),
        })
        .await
        .unwrap();

    let cleaned = service.run_scheduled_cleanups().await.unwrap();
    assert_eq!(cleaned, 1, "only room_a's expired event should be cleaned");

    // room_b still has no events (none inserted); room_a's expired event is gone.
    let remaining_a: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE room_id = $1").bind(&room_a).fetch_one(pool.as_ref()).await.unwrap();
    assert_eq!(remaining_a, 0);
}

#[tokio::test]
async fn test_run_scheduled_cleanups_empty_policies() {
    let _guard = retention_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    setup(&pool).await;
    let service = make_service(&pool);

    let cleaned = service.run_scheduled_cleanups().await.unwrap();
    assert_eq!(cleaned, 0);
}

// =============================================================================
// No-op methods (cleanup queue table removed)
// =============================================================================

#[tokio::test]
async fn test_process_pending_cleanups_is_noop() {
    let _guard = retention_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    setup(&pool).await;
    let service = make_service(&pool);
    assert_eq!(service.process_pending_cleanups(100).await.unwrap(), 0);
}

#[tokio::test]
async fn test_schedule_room_cleanup_is_noop() {
    let _guard = retention_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    setup(&pool).await;
    let service = make_service(&pool);
    assert_eq!(service.schedule_room_cleanup(&make_room_id()).await.unwrap(), 0);
}

#[tokio::test]
async fn test_get_stats_is_noop() {
    let _guard = retention_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    setup(&pool).await;
    let service = make_service(&pool);
    assert!(service.get_stats(&make_room_id()).await.unwrap().is_none());
}

#[tokio::test]
async fn test_get_cleanup_logs_is_noop() {
    let _guard = retention_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    setup(&pool).await;
    let service = make_service(&pool);
    let logs = service.get_cleanup_logs(&make_room_id(), 10).await.unwrap();
    assert!(logs.is_empty());
}

#[tokio::test]
async fn test_get_deleted_events_is_noop() {
    let _guard = retention_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    setup(&pool).await;
    let service = make_service(&pool);
    let events = service.get_deleted_events(&make_room_id(), 0).await.unwrap();
    assert!(events.is_empty());
}

#[tokio::test]
async fn test_get_pending_cleanup_count_is_noop() {
    let _guard = retention_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    setup(&pool).await;
    let service = make_service(&pool);
    assert_eq!(service.get_pending_cleanup_count(&make_room_id()).await.unwrap(), 0);
}

// =============================================================================
// get_status_summary
// =============================================================================

#[tokio::test]
async fn test_get_status_summary_initial() {
    let _guard = retention_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    setup(&pool).await;
    let service = make_service(&pool);

    let summary = service.get_status_summary().await.unwrap();
    assert_eq!(summary.rooms_with_custom_policy, 0);
    assert!(summary.server_policy_enabled, "default server policy row exists");
    assert!(summary.last_run.is_none(), "no lifecycle cycle has run yet");
}

#[tokio::test]
async fn test_get_status_summary_counts_room_policies() {
    let _guard = retention_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    setup(&pool).await;
    let service = make_service(&pool);

    service
        .set_room_policy(CreateRoomRetentionPolicyRequest {
            room_id: make_room_id(),
            max_lifetime: Some(1000),
            min_lifetime: Some(0),
            is_expire_on_clients: Some(false),
        })
        .await
        .unwrap();
    service
        .set_room_policy(CreateRoomRetentionPolicyRequest {
            room_id: make_room_id(),
            max_lifetime: Some(2000),
            min_lifetime: Some(0),
            is_expire_on_clients: Some(false),
        })
        .await
        .unwrap();

    let summary = service.get_status_summary().await.unwrap();
    assert_eq!(summary.rooms_with_custom_policy, 2);
    assert!(summary.server_policy_enabled);
}

// =============================================================================
// get_last_lifecycle_summary
// =============================================================================

#[tokio::test]
async fn test_get_last_lifecycle_summary_initially_none() {
    let _guard = retention_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    setup(&pool).await;
    let service = make_service(&pool);
    assert!(service.get_last_lifecycle_summary().await.is_none());
}

#[tokio::test]
async fn test_get_last_lifecycle_summary_populated_after_cycle() {
    let _guard = retention_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    setup(&pool).await;
    let service = make_service(&pool);

    // Insert an expired upload so cleanup_expired_uploads does real work.
    let now = chrono::Utc::now().timestamp_millis();
    sqlx::query(
        r"INSERT INTO upload_progress (upload_id, user_id, total_chunks, status, created_ts, expires_at)
           VALUES ($1, $2, 1, 'pending', $3, $4)",
    )
    .bind(format!("rt_up_{}", unique_id()))
    .bind("@rt_user:localhost")
    .bind(now - 200_000)
    .bind(now - 100_000) // expired
    .execute(pool.as_ref())
    .await
    .unwrap();

    let config = RetentionConfig::default();
    let summary = service.run_data_lifecycle_cycle_no_beacons(&config).await;

    assert!(summary.completed_ts > 0);
    assert!(summary.duration_ms >= 0);
    assert_eq!(summary.expired_uploads_deleted, 1);
    // No policies with max_lifetime -> run_scheduled_cleanups cleans 0 events.
    assert_eq!(summary.expired_events_deleted, 0);
    // process_pending_cleanups and prune_finished_cleanup_queue are no-ops.
    assert_eq!(summary.cleanup_queue_items_processed, 0);
    assert_eq!(summary.cleanup_queue_rows_pruned, 0);
    // audit_retention_days defaults to 90, so audit cleanup runs but finds nothing.
    assert_eq!(summary.expired_audit_events_deleted, 0);
    assert_eq!(summary.failed_tasks, 0);

    // Summary is now retrievable via get_last_lifecycle_summary.
    let cached = service.get_last_lifecycle_summary().await.expect("summary should be cached");
    assert_eq!(cached.completed_ts, summary.completed_ts);
    assert_eq!(cached.expired_uploads_deleted, 1);

    // And surfaced through get_status_summary.
    let status = service.get_status_summary().await.unwrap();
    assert!(status.last_run.is_some());
    assert_eq!(status.last_run.unwrap().expired_uploads_deleted, 1);
}

// =============================================================================
// run_data_lifecycle_cycle_no_beacons (full lifecycle)
// =============================================================================

#[tokio::test]
async fn test_run_data_lifecycle_cycle_cleans_expired_audit_events() {
    let _guard = retention_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    setup(&pool).await;
    let service = make_service(&pool);

    let now = chrono::Utc::now().timestamp_millis();
    // Insert two audit events: one ancient (older than 1 day), one recent.
    for i in 0..2 {
        sqlx::query(
            r"INSERT INTO audit_events (event_id, actor_id, action, resource_type, resource_id, result, request_id, details, created_ts)
               VALUES ($1, $2, $3, $4, $5, $6, $7, '{}'::jsonb, $8)
               ON CONFLICT (event_id) DO NOTHING",
        )
        .bind(format!("$rt_audit_{i}:localhost"))
        .bind("@rt_admin:localhost")
        .bind("delete")
        .bind("event")
        .bind(format!("$evt_{i}"))
        .bind("success")
        .bind("req-1")
        .bind(if i == 0 { now - 10 * 86_400_000 } else { now - 1_000 })
        .execute(pool.as_ref())
        .await
        .unwrap();
    }

    // 1 day audit retention.
    let mut config = RetentionConfig::default();
    config.audit_retention_days = 1;
    let summary = service.run_data_lifecycle_cycle_no_beacons(&config).await;

    assert_eq!(summary.expired_audit_events_deleted, 1, "only the ancient audit event is deleted");
    assert_eq!(summary.failed_tasks, 0);

    let remaining: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM audit_events")
        .fetch_one(pool.as_ref())
        .await
        .unwrap();
    assert_eq!(remaining, 1);
}

#[tokio::test]
async fn test_run_data_lifecycle_cycle_zero_audit_retention_skips_audit_cleanup() {
    let _guard = retention_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    setup(&pool).await;
    let service = make_service(&pool);

    let now = chrono::Utc::now().timestamp_millis();
    // Even an ancient audit event should survive when retention_days = 0.
    sqlx::query(
        r"INSERT INTO audit_events (event_id, actor_id, action, resource_type, resource_id, result, request_id, details, created_ts)
           VALUES ($1, $2, $3, $4, $5, $6, $7, '{}'::jsonb, $8)
           ON CONFLICT (event_id) DO NOTHING",
    )
    .bind("$rt_audit_keep:localhost")
    .bind("@rt_admin:localhost")
    .bind("delete")
    .bind("event")
    .bind("$evt_keep")
    .bind("success")
    .bind("req-2")
    .bind(now - 365 * 86_400_000)
    .execute(pool.as_ref())
    .await
    .unwrap();

    let mut config = RetentionConfig::default();
    config.audit_retention_days = 0; // disables cleanup
    let summary = service.run_data_lifecycle_cycle_no_beacons(&config).await;

    assert_eq!(summary.expired_audit_events_deleted, 0, "retention_days=0 must skip audit cleanup");
    assert_eq!(summary.failed_tasks, 0);

    let remaining: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM audit_events")
        .fetch_one(pool.as_ref())
        .await
        .unwrap();
    assert_eq!(remaining, 1);
}

#[tokio::test]
async fn test_run_data_lifecycle_cycle_runs_scheduled_cleanups() {
    let _guard = retention_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    setup(&pool).await;
    let service = make_service(&pool);

    let room_id = make_room_id();
    service
        .set_room_policy(CreateRoomRetentionPolicyRequest {
            room_id: room_id.clone(),
            max_lifetime: Some(1),
            min_lifetime: Some(0),
            is_expire_on_clients: Some(false),
        })
        .await
        .unwrap();
    let now = chrono::Utc::now().timestamp_millis();
    insert_event(&pool, &room_id, "$rt_cycle_evt:localhost", now - 86_400_000, "m.room.message").await;

    let config = RetentionConfig::default();
    let summary = service.run_data_lifecycle_cycle_no_beacons(&config).await;

    assert_eq!(summary.expired_events_deleted, 1, "scheduled cleanup should delete the expired event");
    assert_eq!(summary.failed_tasks, 0);
}

// =============================================================================
// Return-type / structure sanity tests (no DB round-trip needed for shape)
// =============================================================================

#[tokio::test]
async fn test_effective_policy_structure_round_trip() {
    let _guard = retention_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    setup(&pool).await;
    let service = make_service(&pool);

    let effective = service.get_effective_policy(&make_room_id()).await.unwrap();
    // Sanity: the returned type matches the documented EffectiveRetentionPolicy shape.
    let _: EffectiveRetentionPolicy = effective;
}

#[tokio::test]
async fn test_run_cleanup_log_shape() {
    let _guard = retention_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    setup(&pool).await;
    let service = make_service(&pool);

    let room_id = make_room_id();
    service
        .set_room_policy(CreateRoomRetentionPolicyRequest {
            room_id: room_id.clone(),
            max_lifetime: Some(86_400_000),
            min_lifetime: Some(0),
            is_expire_on_clients: Some(false),
        })
        .await
        .unwrap();
    insert_event(&pool, &room_id, "$rt_shape:localhost", 0, "m.room.message").await;

    let log: RetentionCleanupLog = service.run_cleanup(&room_id).await.unwrap();
    assert_eq!(log.room_id, room_id);
    assert!(log.events_deleted >= 1);
    assert_eq!(log.state_events_deleted, 0);
    assert_eq!(log.media_deleted, 0);
    assert_eq!(log.bytes_freed, 0);
    assert!(log.started_ts > 0);
}

#[tokio::test]
async fn test_retention_stats_type_is_none_noop() {
    let _guard = retention_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    setup(&pool).await;
    let service = make_service(&pool);

    // get_stats always returns None (queue table removed); confirm the Option<RetentionStats> shape.
    let stats: Option<RetentionStats> = service.get_stats(&make_room_id()).await.unwrap();
    assert!(stats.is_none());
}

#[tokio::test]
async fn test_data_lifecycle_summary_defaults_are_zero() {
    // Confirm the Default impl of DataLifecycleCleanupSummary zeroes every counter.
    let summary = DataLifecycleCleanupSummary::default();
    assert_eq!(summary.started_ts, 0);
    assert_eq!(summary.completed_ts, 0);
    assert_eq!(summary.duration_ms, 0);
    assert_eq!(summary.expired_events_deleted, 0);
    assert_eq!(summary.expired_beacons_deleted, 0);
    assert_eq!(summary.expired_uploads_deleted, 0);
    assert_eq!(summary.expired_audit_events_deleted, 0);
    assert_eq!(summary.cleanup_queue_items_processed, 0);
    assert_eq!(summary.cleanup_queue_rows_pruned, 0);
    assert_eq!(summary.failed_tasks, 0);
}

#[tokio::test]
async fn test_status_summary_shape() {
    let _guard = retention_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    setup(&pool).await;
    let service = make_service(&pool);

    let summary: RetentionStatusSummary = service.get_status_summary().await.unwrap();
    assert!(summary.server_policy_enabled);
    assert_eq!(summary.rooms_with_custom_policy, 0);
    assert!(summary.last_run.is_none());
}

// =============================================================================
// RoomRetentionPolicy / ServerRetentionPolicy clone+serialization smoke tests
// =============================================================================

#[tokio::test]
async fn test_room_policy_is_clone_and_serializable() {
    let _guard = retention_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    setup(&pool).await;
    let service = make_service(&pool);

    let room_id = make_room_id();
    let policy = service
        .set_room_policy(CreateRoomRetentionPolicyRequest {
            room_id: room_id.clone(),
            max_lifetime: Some(86_400_000),
            min_lifetime: Some(0),
            is_expire_on_clients: Some(true),
        })
        .await
        .unwrap();

    let cloned: RoomRetentionPolicy = policy.clone();
    // RoomRetentionPolicy does not derive PartialEq, so compare fields.
    assert_eq!(cloned.id, policy.id);
    assert_eq!(cloned.room_id, policy.room_id);
    assert_eq!(cloned.max_lifetime, policy.max_lifetime);
    assert_eq!(cloned.min_lifetime, policy.min_lifetime);
    let json = serde_json::to_string(&policy).unwrap();
    assert!(json.contains(&room_id));
    let parsed: RoomRetentionPolicy = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.id, policy.id);
    assert_eq!(parsed.room_id, policy.room_id);
    assert_eq!(parsed.max_lifetime, policy.max_lifetime);
}

#[tokio::test]
async fn test_server_policy_is_clone_and_serializable() {
    let _guard = retention_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    setup(&pool).await;
    let service = make_service(&pool);

    let policy: ServerRetentionPolicy = service.get_server_policy().await.unwrap();
    let cloned = policy.clone();
    // ServerRetentionPolicy does not derive PartialEq, so compare fields.
    assert_eq!(cloned.id, policy.id);
    assert_eq!(cloned.max_lifetime, policy.max_lifetime);
    assert_eq!(cloned.min_lifetime, policy.min_lifetime);
    let json = serde_json::to_string(&policy).unwrap();
    assert!(json.contains("\"id\":1"));
    let parsed: ServerRetentionPolicy = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.id, policy.id);
    assert_eq!(parsed.max_lifetime, policy.max_lifetime);
}

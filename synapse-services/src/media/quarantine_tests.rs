//! T06: Media Isolation Security Tests — quarantine enforcement.
//!
//! 2026-10-10 从 `media/mod.rs` 的测试模块按 T06 分节拆分而来（原测试模块 546 行，
//! 整体外置会超过 500 行阈值）。两个 helper 由 `tests` 以 `pub(super)` 提供。

use super::*;
// 共享 fixture：定义在同级的 `tests.rs` 中（`pub(super)` 暴露给父模块 `media`）。
use super::tests::{prepare_media_test_pool, setup_test_media_domain};
use crate::media_quota_service::MediaQuotaService;
use std::sync::Arc;
use synapse_storage::media_quota::MediaQuotaStorage;
use synapse_storage::user::UserStorage;

// =========================================================================
// T06: Media Isolation Security Tests — quarantine enforcement
// =========================================================================

/// Test that ensure_media_not_quarantined rejects non-admin access to quarantined media.
/// This verifies the fix for T06 security vulnerability where download routes
/// bypassed quarantine checks.
async fn setup_test_media_domain_with_quarantine(
    username: &str,
) -> (MediaDomainService, synapse_storage::user::User, tempfile::TempDir) {
    let pool = prepare_media_test_pool().await.expect("failed to prepare media test pool");
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let media_path = temp_dir.path().to_str().expect("temp dir path should be valid utf-8");

    let user_storage =
        UserStorage::new(&pool, Arc::new(synapse_cache::CacheManager::new(&synapse_cache::CacheConfig::default())));
    let user = user_storage
        .create_user(&format!("@{username}:test.server"), username, Some("password"), false)
        .await
        .expect("Failed to create test user");

    let media_service = MediaService::with_pool(media_path, None, "test.server", Some(pool.clone())).await;
    let media_quota_storage = Arc::new(MediaQuotaStorage::new(&pool));
    let media_quota_service = Arc::new(MediaQuotaService::new(media_quota_storage));
    let chunked_upload_service = Arc::new(chunked_upload::ChunkedUploadService::new(pool.clone(), 100 * 1024 * 1024));

    // Create quarantine storage for testing
    let quarantine_storage = Arc::new(synapse_storage::media::QuarantinedMediaChangeStorage::new(&pool));

    let media_domain_service =
        MediaDomainService::new(media_service.clone(), media_quota_service, chunked_upload_service)
            .with_quarantine_stream(quarantine_storage, None /* cache_invalidation */);

    (media_domain_service, user, temp_dir)
}

#[tokio::test]
async fn test_ensure_media_not_quarantined_rejects_non_admin() {
    let (media_domain_service, user, _temp_dir) = setup_test_media_domain_with_quarantine("quarantine_tester").await;

    // Upload some media
    let response = media_domain_service
        .upload_media(&user.user_id, b"quarantined content", "text/plain", Some("secret.txt"))
        .await
        .expect("failed to upload media");

    let media_id = response
        .get("content_uri")
        .and_then(|v| v.as_str())
        .and_then(|content_uri| content_uri.rsplit('/').next())
        .expect("upload response should contain media_id")
        .to_string();

    // Initially, media should not be quarantined
    let result = media_domain_service.ensure_media_not_quarantined(false, "test.server", &media_id).await;
    assert!(result.is_ok(), "non-admin should access non-quarantined media");

    // Quarantine the media
    media_domain_service
        .quarantine_media("test.server", &media_id, &user.user_id)
        .await
        .expect("failed to quarantine media");

    // Non-admin should be rejected
    let error = media_domain_service
        .ensure_media_not_quarantined(false, "test.server", &media_id)
        .await
        .expect_err("non-admin should be blocked from quarantined media");

    assert_eq!(error.http_status(), axum::http::StatusCode::FORBIDDEN);
    assert!(
        error.message().contains("quarantined") || error.message().contains("forbidden"),
        "error message should mention quarantine status: {}",
        error.message()
    );

    // Admin should still be able to access quarantined media
    let admin_result = media_domain_service.ensure_media_not_quarantined(true, "test.server", &media_id).await;
    assert!(admin_result.is_ok(), "admin should be able to access quarantined media for forensic purposes");

    // Remote media should bypass quarantine check
    let remote_result = media_domain_service.ensure_media_not_quarantined(false, "remote.server", "some-id").await;
    assert!(remote_result.is_ok(), "remote media should skip quarantine check");
}

/// The media fixture must keep its isolated schema alive for the whole test.
///
/// `setup_test_media_domain*` builds its services from
/// `prepare_isolated_test_pool` and returns without holding an
/// `Arc<PgPool>` of its own. The shared janitor frees a per-test schema as
/// soon as the **last** `Arc<PgPool>` is released, so if the returned
/// services only kept a downgraded inner `PgPool` clone, the schema was
/// dropped mid-test and every unqualified write silently resolved through
/// `search_path` into the shared `public` schema (root cause of the
/// chunked-download `left: []` and quota `fk_user_media_quota_user` flakes,
/// 2026-09-19). Measured with the downgraded storages: the janitor dropped
/// the schema within one 50ms poll and this probe found the leaked row.
#[tokio::test]
async fn media_fixture_keeps_its_isolated_schema_for_the_whole_test() {
    let (media_domain_service, _media_service, user, _temp_dir) = setup_test_media_domain("isolation_guard").await;

    // The fixture's own `Arc<PgPool>` is gone by now; give the janitor
    // several poll intervals (50ms each) to do its worst before the next
    // write. With the schema still leased by the returned services, no
    // janitor pass can touch it.
    // sleep-reason: 等 janitor 扫描窗口（隔离任务周期）
    tokio::time::sleep(std::time::Duration::from_millis(400)).await;

    let upload_id = media_domain_service
        .start_chunked_upload(&user.user_id, Some("isolation-guard.txt"), Some("text/plain"), Some(4), 1)
        .await
        .expect("failed to start chunked upload");

    let url = crate::test_utils::resolve_test_database_url().await.expect("test database url");
    let public_pool = std::sync::Arc::new(sqlx::PgPool::connect(&url).await.expect("public-schema pool"));
    // The shared `public` schema no longer holds the baseline tables: building
    // the template schema rebuilds `public` from scratch (`DROP SCHEMA public
    // CASCADE`), so it ends up with zero tables. A leak into `public` is then
    // structurally impossible — the probe would report "relation does not
    // exist" instead of `None`. Only run the leak probe when the shared schema
    // still has the table this fixture writes to; the write above already
    // fails loudly if the fixture's own schema was dropped.
    let public_table: Option<String> = sqlx::query_scalar("SELECT to_regclass('public.chunked_upload_progress')::text")
        .fetch_one(&*public_pool)
        .await
        .expect("inspecting the shared public schema must not fail");
    if public_table.is_none() {
        return;
    }
    // Read through the production storage on a pool whose `search_path` is
    // the default (`"$user", public`). If the fixture's schema had been
    // dropped, the write above landed in the shared `public` schema and this
    // finds it there; the two paths must not disagree.
    let public_storage = synapse_storage::media::chunked_upload::ChunkedUploadStorage::new(&public_pool);
    match public_storage.get_progress(&upload_id).await {
        Ok(None) => {}
        Ok(Some(leaked)) => panic!(
            "the media fixture's isolated schema was dropped mid-test: upload_progress row for \
             {upload_id} leaked into the shared public schema ({leaked:?})"
        ),
        Err(error) => panic!(
            "this guard needs the shared public schema to be migrated (scripts/ci/prepare_test_db.sh); \
             reading the public schema failed: {error}"
        ),
    }
}

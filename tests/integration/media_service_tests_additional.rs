//! Integration tests for the media service layer.
//!
//! Covers `synapse-services/src/media_service.rs` (`MediaService`, 18 public
//! methods/types) and `synapse-services/src/media/mod.rs` (`MediaDomainService`,
//! 21 public methods/types). Tests exercise the service layer directly against
//! the shared integration Postgres pool plus a per-test tempdir filesystem,
//! following the warm_up_pool + Mutex guard + unique_id pattern.
//!
//! Only compilation is verified in CI without a live database; the tests
//! themselves are not run here.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![allow(clippy::await_holding_lock)]

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use synapse_cache::{CacheConfig, CacheManager};
use synapse_common::media_link_signer::MediaLinkSigner;
use synapse_services::media::chunked_upload::{ChunkUploadRequest, UploadProgress};
use synapse_services::media::ChunkedUploadService;
use synapse_services::media::{MediaDomainService, MediaResponsePayload};
use synapse_services::media_quota_service::MediaQuotaService;
use synapse_services::{MediaService, ThumbnailMethod, ThumbnailSettings};
use synapse_storage::admin_media::AdminMediaStorage;
use synapse_storage::media::QuarantinedMediaChangeStorage;
use synapse_storage::media_quota::{MediaQuotaStorage, SetUserQuotaRequest};
use synapse_storage::user::UserStorage;

static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

fn unique_id() -> u64 {
    TEST_COUNTER.fetch_add(1, Ordering::SeqCst)
}

fn media_test_guard() -> &'static Mutex<()> {
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

/// Clean up only the rows owned by this test file (prefix `mi` / `mi_`).
/// Child tables first to respect FK constraints.
async fn setup(pool: &Arc<sqlx::PgPool>) {
    warm_up_pool(pool).await;
    sqlx::query("DELETE FROM upload_chunks WHERE upload_id IN (SELECT upload_id FROM upload_progress WHERE user_id LIKE '%mi_%')")
        .execute(pool.as_ref())
        .await
        .ok();
    sqlx::query("DELETE FROM upload_progress WHERE user_id LIKE '%mi_%'").execute(pool.as_ref()).await.ok();
    sqlx::query("DELETE FROM quarantined_media_changes WHERE media_id LIKE 'mi%' OR changed_by LIKE '%mi_%'")
        .execute(pool.as_ref())
        .await
        .ok();
    sqlx::query("DELETE FROM media_usage_log WHERE media_id LIKE 'mi%' OR user_id LIKE '%mi_%'")
        .execute(pool.as_ref())
        .await
        .ok();
    sqlx::query("DELETE FROM media_quota_alerts WHERE user_id LIKE '%mi_%'").execute(pool.as_ref()).await.ok();
    sqlx::query("DELETE FROM media_metadata WHERE media_id LIKE 'mi%'").execute(pool.as_ref()).await.ok();
    sqlx::query("DELETE FROM user_media_quota WHERE user_id LIKE '%mi_%'").execute(pool.as_ref()).await.ok();
    sqlx::query("DELETE FROM users WHERE user_id LIKE '%mi_%'").execute(pool.as_ref()).await.ok();
}

/// Generate a small in-memory PNG to exercise thumbnail generation.
fn tiny_png() -> Vec<u8> {
    use image::{ImageBuffer, ImageFormat, Rgb};
    let image = ImageBuffer::from_pixel(8, 8, Rgb([0_u8, 128_u8, 255_u8]));
    let dynamic = image::DynamicImage::ImageRgb8(image);
    let mut output = Vec::new();
    dynamic
        .write_to(&mut std::io::Cursor::new(&mut output), ImageFormat::Png)
        .expect("encode test png");
    output
}

/// Parse a `signature=...&expires=...` query string into its components.
fn parse_signed_query(query: &str) -> (String, u64) {
    let mut signature = String::new();
    let mut expires: u64 = 0;
    for part in query.split('&') {
        let mut kv = part.splitn(2, '=');
        match kv.next() {
            Some("signature") => signature = kv.next().unwrap_or("").to_string(),
            Some("expires") => expires = kv.next().unwrap_or("0").parse().unwrap_or(0),
            _ => {}
        }
    }
    (signature, expires)
}

/// Build a `MediaService` backed by the shared pool inside a fresh tempdir.
fn make_media_service(temp_dir: &tempfile::TempDir, pool: &Arc<sqlx::PgPool>) -> MediaService {
    let media_path = temp_dir.path().to_str().expect("temp dir is valid utf-8");
    MediaService::with_pool(media_path, None, "test.server", Some(pool.clone()))
}

/// Build a `MediaService` without a pool (filesystem-only) inside a fresh tempdir.
fn make_fs_media_service(temp_dir: &tempfile::TempDir) -> MediaService {
    let media_path = temp_dir.path().to_str().expect("temp dir is valid utf-8");
    MediaService::new(media_path, None, "test.server")
}

/// Create a real user in the `users` table with a `mi_`-prefixed id.
async fn create_test_user(pool: &Arc<sqlx::PgPool>, suffix: &str) -> String {
    let user_id = format!("@mi_{suffix}:test.server");
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let storage = UserStorage::new(pool, cache);
    storage
        .create_user(&user_id, &format!("mi_{suffix}"), Some("hash123"), false)
        .await
        .expect("create test user");
    user_id
}

/// Build a `MediaDomainService` with quota configured for the given user.
async fn make_media_domain(
    pool: &Arc<sqlx::PgPool>,
    temp_dir: &tempfile::TempDir,
    user_id: &str,
    max_storage_bytes: i64,
    max_file_size_bytes: i64,
) -> MediaDomainService {
    let media_service = make_media_service(temp_dir, pool);
    let media_quota_storage = Arc::new(MediaQuotaStorage::new(pool));
    let media_quota_service = Arc::new(MediaQuotaService::new(media_quota_storage));
    media_quota_service
        .set_user_quota(SetUserQuotaRequest {
            user_id: user_id.to_string(),
            quota_config_id: None,
            custom_max_storage_bytes: Some(max_storage_bytes),
            custom_max_file_size_bytes: Some(max_file_size_bytes),
            custom_max_files_count: Some(10),
        })
        .await
        .expect("set user quota");
    let chunked_upload_service = Arc::new(ChunkedUploadService::new(pool.clone()));
    MediaDomainService::new(media_service, media_quota_service, chunked_upload_service)
}

// =============================================================================
// MediaService::new / with_pool / construction
// =============================================================================

#[tokio::test]
async fn test_media_service_new_creates_directories() {
    let _guard = media_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let temp_dir = tempfile::tempdir().expect("temp dir");
    let service = make_fs_media_service(&temp_dir);
    assert!(temp_dir.path().exists());
    assert!(service.get_thumbnail_configurations().len() == 5);
}

#[tokio::test]
async fn test_media_service_with_pool_attaches_admin_storage() {
    let _guard = media_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let temp_dir = tempfile::tempdir().expect("temp dir");
    let service = make_media_service(&temp_dir, &pool);

    // Upload should persist a media_metadata row via AdminMediaStorage.
    let media_id = format!("mi_withpool_{}", unique_id());
    let response = service
        .upload_media_with_id("@mi_user:test.server", &media_id, b"hello", "text/plain", Some("f.txt"))
        .await
        .expect("upload");
    assert!(response["content_uri"].as_str().unwrap().contains(&media_id));

    // The metadata is now retrievable from the DB-backed path.
    let metadata = service.get_media_metadata("test.server", &media_id).await;
    assert!(metadata.is_some(), "DB-backed metadata should be present");
    assert_eq!(metadata.unwrap()["content_type"].as_str().unwrap(), "text/plain");
}

#[tokio::test]
async fn test_media_service_without_pool_falls_back_to_filesystem_metadata() {
    let _guard = media_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let temp_dir = tempfile::tempdir().expect("temp dir");
    let service = make_fs_media_service(&temp_dir);

    let media_id = format!("mi_fs_{}", unique_id());
    service
        .upload_media_with_id("@mi_user:test.server", &media_id, b"data", "text/plain", None)
        .await
        .unwrap();

    let metadata = service.get_media_metadata("test.server", &media_id).await.expect("fs metadata");
    assert_eq!(metadata["media_id"].as_str().unwrap(), media_id);
    assert!(metadata["filename"].as_str().unwrap().contains(&media_id));
}

// =============================================================================
// Link signing
// =============================================================================

#[tokio::test]
async fn test_sign_media_download_url_without_signer_returns_none() {
    let _guard = media_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let temp_dir = tempfile::tempdir().expect("temp dir");
    let service = make_fs_media_service(&temp_dir);
    assert!(service.sign_media_download_url("test.server", "abc").is_none());
}

#[tokio::test]
async fn test_verify_media_download_url_without_signer_returns_false() {
    let _guard = media_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let temp_dir = tempfile::tempdir().expect("temp dir");
    let service = make_fs_media_service(&temp_dir);
    assert!(!service.verify_media_download_url("test.server", "abc", "sig", 0));
}

#[tokio::test]
async fn test_sign_and_verify_media_download_url_round_trip() {
    let _guard = media_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let temp_dir = tempfile::tempdir().expect("temp dir");
    let mut service = make_fs_media_service(&temp_dir);
    let signer = Arc::new(MediaLinkSigner::new(b"test-secret-key-32-bytes-xxxxx", 3600));
    service.set_link_signer(signer);

    let query = service.sign_media_download_url("test.server", "media123").expect("signed");
    let (signature, expires) = parse_signed_query(&query);
    assert!(!signature.is_empty());
    assert!(expires > 0);
    assert!(service.verify_media_download_url("test.server", "media123", &signature, expires));
}

#[tokio::test]
async fn test_verify_media_download_url_rejects_wrong_path() {
    let _guard = media_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let temp_dir = tempfile::tempdir().expect("temp dir");
    let mut service = make_fs_media_service(&temp_dir);
    let signer = Arc::new(MediaLinkSigner::new(b"test-secret-key-32-bytes-xxxxx", 3600));
    service.set_link_signer(signer);

    let query = service.sign_media_download_url("test.server", "real_id").expect("signed");
    let (signature, expires) = parse_signed_query(&query);
    assert!(!service.verify_media_download_url("test.server", "wrong_id", &signature, expires));
}

#[tokio::test]
async fn test_verify_media_download_url_rejects_expired() {
    let _guard = media_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let temp_dir = tempfile::tempdir().expect("temp dir");
    let mut service = make_fs_media_service(&temp_dir);
    let signer = Arc::new(MediaLinkSigner::new(b"test-secret-key-32-bytes-xxxxx", 3600));
    service.set_link_signer(signer);

    let past = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs().saturating_sub(3600);
    assert!(!service.verify_media_download_url("test.server", "id", "deadbeef", past));
}

// =============================================================================
// upload_media / upload_media_with_id
// =============================================================================

#[tokio::test]
async fn test_upload_media_writes_file_and_returns_mxc_uri() {
    let _guard = media_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let temp_dir = tempfile::tempdir().expect("temp dir");
    let service = make_media_service(&temp_dir, &pool);
    let content = b"upload-content";

    let response = service.upload_media("@mi_user:test.server", content, "image/png", Some("pic.png")).await.unwrap();
    let content_uri = response["content_uri"].as_str().unwrap();
    assert!(content_uri.starts_with("mxc://test.server/"));

    // File written to the media directory (excluding the thumbnails subdir).
    let entries = std::fs::read_dir(temp_dir.path())
        .unwrap()
        .filter_map(Result::ok)
        .filter(|e| e.file_name() != "thumbnails")
        .count();
    assert!(entries >= 1);
}

#[tokio::test]
async fn test_upload_media_with_id_round_trip_and_db_metadata() {
    let _guard = media_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let temp_dir = tempfile::tempdir().expect("temp dir");
    let service = make_media_service(&temp_dir, &pool);
    let media_id = format!("mi_rnd_{}", unique_id());

    let response = service
        .upload_media_with_id("@mi_uploader:test.server", &media_id, b"abc", "image/jpeg", Some("a.jpg"))
        .await
        .unwrap();
    assert!(response["content_uri"].as_str().unwrap().ends_with(&media_id));

    let meta = service.get_media_metadata("test.server", &media_id).await.expect("metadata");
    assert_eq!(meta["content_type"].as_str().unwrap(), "image/jpeg");
    assert_eq!(meta["size"].as_i64().unwrap(), 3);
    assert_eq!(meta["uploader_user_id"].as_str().unwrap(), "@mi_uploader:test.server");
}

#[tokio::test]
async fn test_upload_media_with_id_rejects_empty_id() {
    let _guard = media_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let temp_dir = tempfile::tempdir().expect("temp dir");
    let service = make_media_service(&temp_dir, &pool);
    let err = service
        .upload_media_with_id("@mi_user:test.server", "", b"x", "text/plain", None)
        .await
        .expect_err("empty media_id rejected");
    assert!(err.message().contains("media_id must be 1..=255 chars"));
}

#[tokio::test]
async fn test_upload_media_with_id_rejects_oversized_id() {
    let _guard = media_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let temp_dir = tempfile::tempdir().expect("temp dir");
    let service = make_media_service(&temp_dir, &pool);
    let long_id = "a".repeat(256);
    let err = service
        .upload_media_with_id("@mi_user:test.server", &long_id, b"x", "text/plain", None)
        .await
        .expect_err("oversized media_id rejected");
    assert!(err.message().contains("media_id must be 1..=255 chars"));
}

#[tokio::test]
async fn test_upload_media_with_id_rejects_path_traversal() {
    let _guard = media_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let temp_dir = tempfile::tempdir().expect("temp dir");
    let service = make_media_service(&temp_dir, &pool);
    let err = service
        .upload_media_with_id("@mi_user:test.server", "../etc/passwd", b"x", "text/plain", None)
        .await
        .expect_err("path traversal rejected");
    assert!(err.message().contains("illegal characters"));
}

#[tokio::test]
async fn test_upload_media_with_id_rejects_slash_in_id() {
    let _guard = media_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let temp_dir = tempfile::tempdir().expect("temp dir");
    let service = make_media_service(&temp_dir, &pool);
    let err = service
        .upload_media_with_id("@mi_user:test.server", "mi/sub", b"x", "text/plain", None)
        .await
        .expect_err("slash rejected");
    assert!(err.message().contains("illegal characters"));
}

#[tokio::test]
async fn test_upload_media_with_id_rejects_duplicate() {
    let _guard = media_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let temp_dir = tempfile::tempdir().expect("temp dir");
    let service = make_media_service(&temp_dir, &pool);
    let media_id = format!("mi_dup_{}", unique_id());

    service
        .upload_media_with_id("@mi_user:test.server", &media_id, b"first", "text/plain", None)
        .await
        .unwrap();
    let err = service
        .upload_media_with_id("@mi_user:test.server", &media_id, b"second", "text/plain", None)
        .await
        .expect_err("duplicate rejected");
    assert!(err.message().contains("already exists"));
}

#[tokio::test]
async fn test_upload_media_with_id_accepts_base64_chars() {
    let _guard = media_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let temp_dir = tempfile::tempdir().expect("temp dir");
    let service = make_media_service(&temp_dir, &pool);
    let media_id = format!("mi_b64_{}+=_", unique_id());
    service
        .upload_media_with_id("@mi_user:test.server", &media_id, b"x", "application/pdf", None)
        .await
        .expect("base64 chars are allowed");
}

#[tokio::test]
async fn test_upload_media_sanitizes_filename_with_control_chars() {
    let _guard = media_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let temp_dir = tempfile::tempdir().expect("temp dir");
    let service = make_media_service(&temp_dir, &pool);
    let media_id = format!("mi_fn_{}", unique_id());

    // Filename with control chars / path separators is sanitized, not rejected.
    service
        .upload_media_with_id(
            "@mi_user:test.server",
            &media_id,
            b"data",
            "text/plain",
            Some("bad\u{0000}/name\\file.txt"),
        )
        .await
        .expect("filename is sanitized");

    // File should exist on disk with a sanitized name.
    let found = service.get_media("test.server", &media_id).await;
    assert_eq!(found.as_deref(), Some(b"data".as_slice()));
}

// =============================================================================
// get_media / download_media
// =============================================================================

#[tokio::test]
async fn test_get_media_returns_uploaded_content() {
    let _guard = media_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let temp_dir = tempfile::tempdir().expect("temp dir");
    let service = make_media_service(&temp_dir, &pool);
    let media_id = format!("mi_get_{}", unique_id());
    let content = b"\x89PNG\r\n\x1a\nfake";
    service
        .upload_media_with_id("@mi_user:test.server", &media_id, content, "image/png", None)
        .await
        .unwrap();

    let fetched = service.get_media("test.server", &media_id).await;
    assert_eq!(fetched.as_deref(), Some(&content[..]));
}

#[tokio::test]
async fn test_get_media_returns_none_for_missing() {
    let _guard = media_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let temp_dir = tempfile::tempdir().expect("temp dir");
    let service = make_fs_media_service(&temp_dir);
    assert!(service.get_media("test.server", &format!("mi_missing_{}", unique_id())).await.is_none());
}

#[tokio::test]
async fn test_download_media_returns_content() {
    let _guard = media_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let temp_dir = tempfile::tempdir().expect("temp dir");
    let service = make_media_service(&temp_dir, &pool);
    let media_id = format!("mi_dl_{}", unique_id());
    service
        .upload_media_with_id("@mi_user:test.server", &media_id, b"download-me", "text/plain", None)
        .await
        .unwrap();

    let bytes = service.download_media("test.server", &media_id).await.unwrap();
    assert_eq!(bytes, b"download-me");
}

#[tokio::test]
async fn test_download_media_returns_not_found_for_missing() {
    let _guard = media_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let temp_dir = tempfile::tempdir().expect("temp dir");
    let service = make_fs_media_service(&temp_dir);
    let err = service.download_media("test.server", &format!("mi_absent_{}", unique_id())).await.unwrap_err();
    assert_eq!(err.http_status(), axum::http::StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_download_media_rejects_invalid_id() {
    let _guard = media_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let temp_dir = tempfile::tempdir().expect("temp dir");
    let service = make_fs_media_service(&temp_dir);
    let err = service.download_media("test.server", "../bad").await.unwrap_err();
    assert_eq!(err.http_status(), axum::http::StatusCode::BAD_REQUEST);
}

// =============================================================================
// get_media_metadata / get_media_info
// =============================================================================

#[tokio::test]
async fn test_get_media_metadata_returns_none_for_missing() {
    let _guard = media_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let temp_dir = tempfile::tempdir().expect("temp dir");
    let service = make_media_service(&temp_dir, &pool);
    assert!(service.get_media_metadata("test.server", &format!("mi_none_{}", unique_id())).await.is_none());
}

#[tokio::test]
async fn test_get_media_metadata_returns_none_for_invalid_id() {
    let _guard = media_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let temp_dir = tempfile::tempdir().expect("temp dir");
    let service = make_media_service(&temp_dir, &pool);
    // Invalid id short-circuits before hitting the DB.
    assert!(service.get_media_metadata("test.server", "../etc").await.is_none());
}

#[tokio::test]
async fn test_get_media_info_returns_file_metadata() {
    let _guard = media_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let temp_dir = tempfile::tempdir().expect("temp dir");
    let service = make_media_service(&temp_dir, &pool);
    let media_id = format!("mi_info_{}", unique_id());
    service
        .upload_media_with_id("@mi_user:test.server", &media_id, b"info-content", "image/gif", Some("g.gif"))
        .await
        .unwrap();

    let info = service.get_media_info("test.server", &media_id).await.unwrap();
    assert_eq!(info["media_id"].as_str().unwrap(), media_id);
    assert_eq!(info["size"].as_i64().unwrap(), b"info-content".len() as i64);
    assert!(info["content_uri"].as_str().unwrap().starts_with("mxc://"));
}

#[tokio::test]
async fn test_get_media_info_returns_not_found_for_missing() {
    let _guard = media_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let temp_dir = tempfile::tempdir().expect("temp dir");
    let service = make_media_service(&temp_dir, &pool);
    let err = service.get_media_info("test.server", &format!("mi_no_{}", unique_id())).await.unwrap_err();
    assert_eq!(err.http_status(), axum::http::StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_get_media_info_rejects_invalid_id() {
    let _guard = media_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let temp_dir = tempfile::tempdir().expect("temp dir");
    let service = make_fs_media_service(&temp_dir);
    let err = service.get_media_info("test.server", "bad/id").await.unwrap_err();
    assert_eq!(err.http_status(), axum::http::StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_get_media_metadata_persists_uploaded_filename() {
    let _guard = media_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let temp_dir = tempfile::tempdir().expect("temp dir");
    let service = make_media_service(&temp_dir, &pool);
    let media_id = format!("mi_fnmeta_{}", unique_id());
    service
        .upload_media_with_id("@mi_user:test.server", &media_id, b"x", "application/pdf", Some("report.pdf"))
        .await
        .unwrap();
    let meta = service.get_media_metadata("test.server", &media_id).await.unwrap();
    assert_eq!(meta["filename"].as_str().unwrap(), "report.pdf");
}

// =============================================================================
// delete_media
// =============================================================================

#[tokio::test]
async fn test_delete_media_removes_file() {
    let _guard = media_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let temp_dir = tempfile::tempdir().expect("temp dir");
    let service = make_media_service(&temp_dir, &pool);
    let media_id = format!("mi_del_{}", unique_id());
    service
        .upload_media_with_id("@mi_user:test.server", &media_id, b"to-delete", "text/plain", None)
        .await
        .unwrap();
    assert!(service.get_media("test.server", &media_id).await.is_some());

    service.delete_media("test.server", &media_id).await.unwrap();
    assert!(service.get_media("test.server", &media_id).await.is_none());
}

#[tokio::test]
async fn test_delete_media_returns_not_found_for_missing() {
    let _guard = media_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let temp_dir = tempfile::tempdir().expect("temp dir");
    let service = make_fs_media_service(&temp_dir);
    let err = service.delete_media("test.server", &format!("mi_gone_{}", unique_id())).await.unwrap_err();
    assert_eq!(err.http_status(), axum::http::StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_delete_media_rejects_invalid_id() {
    let _guard = media_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let temp_dir = tempfile::tempdir().expect("temp dir");
    let service = make_fs_media_service(&temp_dir);
    let err = service.delete_media("test.server", "..").await.unwrap_err();
    assert_eq!(err.http_status(), axum::http::StatusCode::BAD_REQUEST);
}

// =============================================================================
// Thumbnails (get_thumbnail / generate_all_thumbnails)
// =============================================================================

#[tokio::test]
async fn test_get_thumbnail_generates_and_caches_jpeg() {
    let _guard = media_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let temp_dir = tempfile::tempdir().expect("temp dir");
    let service = make_media_service(&temp_dir, &pool);
    let media_id = format!("mi_thumb_{}", unique_id());
    let png = tiny_png();
    service
        .upload_media_with_id("@mi_user:test.server", &media_id, &png, "image/png", None)
        .await
        .unwrap();

    let thumb = service.get_thumbnail("test.server", &media_id, 64, 48, "scale").await.unwrap();
    assert!(!thumb.is_empty());
    // JPEG magic bytes.
    assert_eq!(&thumb[..2], &[0xFF, 0xD8]);

    // Second call should serve from cache (same content).
    let cached = service.get_thumbnail("test.server", &media_id, 64, 48, "scale").await.unwrap();
    assert_eq!(thumb, cached);
}

#[tokio::test]
async fn test_get_thumbnail_supports_crop_method() {
    let _guard = media_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let temp_dir = tempfile::tempdir().expect("temp dir");
    let service = make_media_service(&temp_dir, &pool);
    let media_id = format!("mi_crop_{}", unique_id());
    service
        .upload_media_with_id("@mi_user:test.server", &media_id, &tiny_png(), "image/png", None)
        .await
        .unwrap();

    let thumb = service.get_thumbnail("test.server", &media_id, 32, 32, "crop").await.unwrap();
    assert_eq!(&thumb[..2], &[0xFF, 0xD8]);
}

#[tokio::test]
async fn test_get_thumbnail_rejects_invalid_method() {
    let _guard = media_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let temp_dir = tempfile::tempdir().expect("temp dir");
    let service = make_media_service(&temp_dir, &pool);
    let media_id = format!("mi_tm_{}", unique_id());
    service
        .upload_media_with_id("@mi_user:test.server", &media_id, &tiny_png(), "image/png", None)
        .await
        .unwrap();

    let err = service.get_thumbnail("test.server", &media_id, 32, 32, "frobnicate").await.unwrap_err();
    assert_eq!(err.http_status(), axum::http::StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_get_thumbnail_rejects_invalid_media_id() {
    let _guard = media_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let temp_dir = tempfile::tempdir().expect("temp dir");
    let service = make_fs_media_service(&temp_dir);
    let err = service.get_thumbnail("test.server", "../x", 32, 32, "scale").await.unwrap_err();
    assert_eq!(err.http_status(), axum::http::StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_get_thumbnail_falls_back_to_original_for_bad_image() {
    let _guard = media_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let temp_dir = tempfile::tempdir().expect("temp dir");
    let service = make_media_service(&temp_dir, &pool);
    let media_id = format!("mi_badimg_{}", unique_id());
    // Non-image content; thumbnail generation fails and the original is returned.
    service
        .upload_media_with_id("@mi_user:test.server", &media_id, b"not-an-image", "text/plain", None)
        .await
        .unwrap();

    let result = service.get_thumbnail("test.server", &media_id, 32, 32, "scale").await.unwrap();
    assert_eq!(result, b"not-an-image");
}

#[tokio::test]
async fn test_generate_all_thumbnails_creates_default_configs() {
    let _guard = media_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let temp_dir = tempfile::tempdir().expect("temp dir");
    let service = make_media_service(&temp_dir, &pool);
    let media_id = format!("mi_all_{}", unique_id());
    service
        .upload_media_with_id("@mi_user:test.server", &media_id, &tiny_png(), "image/png", None)
        .await
        .unwrap();

    let generated = service.generate_all_thumbnails(&media_id).await.unwrap();
    assert_eq!(generated.len(), 5, "all default thumbnail configs should be generated");
    for name in &generated {
        assert!(name.starts_with(&format!("{media_id}_")));
    }
}

#[tokio::test]
async fn test_generate_all_thumbnails_rejects_invalid_id() {
    let _guard = media_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let temp_dir = tempfile::tempdir().expect("temp dir");
    let service = make_fs_media_service(&temp_dir);
    let err = service.generate_all_thumbnails("../bad").await.unwrap_err();
    assert_eq!(err.http_status(), axum::http::StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_generate_all_thumbnails_returns_not_found_for_missing() {
    let _guard = media_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let temp_dir = tempfile::tempdir().expect("temp dir");
    let service = make_fs_media_service(&temp_dir);
    let err = service.generate_all_thumbnails(&format!("mi_missing_{}", unique_id())).await.unwrap_err();
    assert_eq!(err.http_status(), axum::http::StatusCode::NOT_FOUND);
}

// =============================================================================
// cleanup_old_thumbnails / purge_media_cache
// =============================================================================

#[tokio::test]
async fn test_cleanup_old_thumbnails_empty_directory_returns_zero() {
    let _guard = media_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let temp_dir = tempfile::tempdir().expect("temp dir");
    let service = make_fs_media_service(&temp_dir);
    let deleted = service.cleanup_old_thumbnails(30).await.unwrap();
    assert_eq!(deleted, 0);
}

#[tokio::test]
async fn test_cleanup_old_thumbnails_keeps_fresh_files() {
    let _guard = media_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let temp_dir = tempfile::tempdir().expect("temp dir");
    let service = make_fs_media_service(&temp_dir);

    // Place a fresh thumbnail file.
    let thumb_path = temp_dir.path().join("thumbnails").join("mi_fresh_100x100_scale.jpg");
    std::fs::create_dir_all(thumb_path.parent().unwrap()).unwrap();
    std::fs::write(&thumb_path, b"thumb").unwrap();

    // 100-year max age: nothing should be deleted.
    let deleted = service.cleanup_old_thumbnails(36500).await.unwrap();
    assert_eq!(deleted, 0);
    assert!(thumb_path.exists());
}

#[tokio::test]
async fn test_cleanup_old_thumbnails_deletes_aged_files() {
    let _guard = media_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let temp_dir = tempfile::tempdir().expect("temp dir");
    let service = make_fs_media_service(&temp_dir);

    let thumb_dir = temp_dir.path().join("thumbnails");
    std::fs::create_dir_all(&thumb_dir).unwrap();
    let thumb_path = thumb_dir.join("mi_aged_100x100_scale.jpg");
    std::fs::write(&thumb_path, b"thumb").unwrap();

    // Backdate the file mtime by 30 days.
    let old_time = SystemTime::now().checked_sub(Duration::from_secs(30 * 24 * 60 * 60)).unwrap();
    let times = std::fs::FileTimes::new().set_modified(old_time);
    let file = std::fs::File::open(&thumb_path).unwrap();
    file.set_times(times).unwrap();
    drop(file);

    // max_age_days = 1 -> the 30-day-old file qualifies for deletion.
    let deleted = service.cleanup_old_thumbnails(1).await.unwrap();
    assert_eq!(deleted, 1);
    assert!(!thumb_path.exists());
}

#[tokio::test]
async fn test_purge_media_cache_zero_ts_deletes_nothing() {
    let _guard = media_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let temp_dir = tempfile::tempdir().expect("temp dir");
    let service = make_media_service(&temp_dir, &pool);
    let media_id = format!("mi_purge0_{}", unique_id());
    service
        .upload_media_with_id("@mi_user:test.server", &media_id, b"keep", "text/plain", None)
        .await
        .unwrap();

    // before_ts = 0 -> before_time = 1970 -> no current file is older.
    let deleted = service.purge_media_cache(0).await.unwrap();
    assert_eq!(deleted, 0);
    assert!(service.get_media("test.server", &media_id).await.is_some());
}

#[tokio::test]
async fn test_purge_media_cache_future_ts_deletes_all() {
    let _guard = media_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let temp_dir = tempfile::tempdir().expect("temp dir");
    let service = make_media_service(&temp_dir, &pool);
    let media_id = format!("mi_purgeall_{}", unique_id());
    service
        .upload_media_with_id("@mi_user:test.server", &media_id, b"delete", "text/plain", None)
        .await
        .unwrap();
    assert!(service.get_media("test.server", &media_id).await.is_some());

    // Far-future cutoff -> all current files are older and get purged.
    let future_ts = chrono::Utc::now().timestamp_millis() + 3_600_000;
    let deleted = service.purge_media_cache(future_ts).await.unwrap();
    assert!(deleted >= 1, "at least the uploaded file should be purged");
    assert!(service.get_media("test.server", &media_id).await.is_none());
}

// =============================================================================
// preview_url / get_thumbnail_configurations
// =============================================================================

#[tokio::test]
async fn test_preview_url_returns_metadata_fields() {
    let _guard = media_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let temp_dir = tempfile::tempdir().expect("temp dir");
    let service = make_fs_media_service(&temp_dir);
    let result = service.preview_url("https://example.com/page", 1234567).unwrap();
    assert_eq!(result["url"].as_str().unwrap(), "https://example.com/page");
    assert_eq!(result["og:title"].as_str().unwrap(), "URL Preview");
    assert!(result["matrix:image:size"].as_i64().unwrap() > 0);
}

#[tokio::test]
async fn test_get_thumbnail_configurations_returns_five_defaults() {
    let _guard = media_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let temp_dir = tempfile::tempdir().expect("temp dir");
    let service = make_fs_media_service(&temp_dir);
    let configs: Vec<ThumbnailSettings> = service.get_thumbnail_configurations();
    assert_eq!(configs.len(), 5);
    assert_eq!(configs[0].method, ThumbnailMethod::Crop);
    assert_eq!(configs[4].method, ThumbnailMethod::Scale);
}

// =============================================================================
// MediaDomainService::new / with_quarantine_stream / quarantine
// =============================================================================

#[tokio::test]
async fn test_quarantine_without_storage_returns_internal_error() {
    let _guard = media_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let temp_dir = tempfile::tempdir().expect("temp dir");
    let user_id = create_test_user(&pool, &format!("q_{}", unique_id())).await;
    let domain = make_media_domain(&pool, &temp_dir, &user_id, 10 * 1024 * 1024, 10 * 1024 * 1024).await;

    let err = domain.quarantine_media("test.server", "mi_q1", &user_id).await.unwrap_err();
    assert_eq!(err.http_status(), axum::http::StatusCode::INTERNAL_SERVER_ERROR);
    assert!(err.message().contains("Quarantine stream storage not configured"));
}

#[tokio::test]
async fn test_quarantine_and_unquarantine_records_stream_and_updates_status() {
    let _guard = media_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let temp_dir = tempfile::tempdir().expect("temp dir");
    let uid = unique_id();
    let user_id = create_test_user(&pool, &format!("q2_{uid}")).await;

    let media_service = make_media_service(&temp_dir, &pool);
    let media_quota_storage = Arc::new(MediaQuotaStorage::new(&pool));
    let media_quota_service = Arc::new(MediaQuotaService::new(media_quota_storage));
    media_quota_service
        .set_user_quota(SetUserQuotaRequest {
            user_id: user_id.clone(),
            quota_config_id: None,
            custom_max_storage_bytes: Some(10 * 1024 * 1024),
            custom_max_file_size_bytes: Some(10 * 1024 * 1024),
            custom_max_files_count: Some(10),
        })
        .await
        .unwrap();
    let chunked = Arc::new(ChunkedUploadService::new(pool.clone()));
    let quarantine_storage = Arc::new(QuarantinedMediaChangeStorage::new(&pool));
    let domain = MediaDomainService::new(media_service.clone(), media_quota_service, chunked)
        .with_quarantine_stream(quarantine_storage.clone(), None);

    let media_id = format!("mi_q_{uid}");
    domain
        .upload_media_with_id(&user_id, &media_id, b"q-data", "text/plain", Some("q.txt"))
        .await
        .unwrap();

    let stream_id = domain.quarantine_media("test.server", &media_id, &user_id).await.unwrap();
    assert!(stream_id > 0);

    // media_metadata.quarantine_status updated.
    let admin = AdminMediaStorage::new(&pool);
    let info = admin.get_media_info(&media_id).await.unwrap().unwrap();
    assert!(info.quarantined);

    let unquarantine_id = domain.unquarantine_media("test.server", &media_id, &user_id).await.unwrap();
    assert!(unquarantine_id > stream_id, "unquarantine should produce a later stream id");

    let after = admin.get_media_info(&media_id).await.unwrap().unwrap();
    assert!(!after.quarantined);
}

#[tokio::test]
async fn test_get_quarantined_media_changes_streams_in_order() {
    let _guard = media_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let temp_dir = tempfile::tempdir().expect("temp dir");
    let uid = unique_id();
    let user_id = create_test_user(&pool, &format!("q3_{uid}")).await;

    let media_service = make_media_service(&temp_dir, &pool);
    let media_quota_storage = Arc::new(MediaQuotaStorage::new(&pool));
    let media_quota_service = Arc::new(MediaQuotaService::new(media_quota_storage));
    media_quota_service
        .set_user_quota(SetUserQuotaRequest {
            user_id: user_id.clone(),
            quota_config_id: None,
            custom_max_storage_bytes: Some(10 * 1024 * 1024),
            custom_max_file_size_bytes: Some(10 * 1024 * 1024),
            custom_max_files_count: Some(10),
        })
        .await
        .unwrap();
    let chunked = Arc::new(ChunkedUploadService::new(pool.clone()));
    let quarantine_storage = Arc::new(QuarantinedMediaChangeStorage::new(&pool));
    let domain = MediaDomainService::new(media_service, media_quota_service, chunked)
        .with_quarantine_stream(quarantine_storage, None);

    let media_id = format!("mi_qc_{uid}");
    domain
        .upload_media_with_id(&user_id, &media_id, b"data", "text/plain", None)
        .await
        .unwrap();
    let first = domain.quarantine_media("test.server", &media_id, &user_id).await.unwrap();
    let second = domain.unquarantine_media("test.server", &media_id, &user_id).await.unwrap();

    let changes = domain.get_quarantined_media_changes(0, 100).await.unwrap();
    assert!(changes.len() >= 2);
    // Strictly increasing stream ids.
    let ids: Vec<i64> = changes.iter().map(|c| c.stream_id).collect();
    assert!(ids.contains(&first));
    assert!(ids.contains(&second));
    assert!(first < second);

    // Limit respected.
    let limited = domain.get_quarantined_media_changes(first, 100).await.unwrap();
    assert!(limited.iter().all(|c| c.stream_id > first));
}

// =============================================================================
// MediaDomainService upload/download with quota
// =============================================================================

#[tokio::test]
async fn test_domain_upload_records_quota_usage() {
    let _guard = media_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let temp_dir = tempfile::tempdir().expect("temp dir");
    let uid = unique_id();
    let user_id = create_test_user(&pool, &format!("up_{uid}")).await;
    let domain = make_media_domain(&pool, &temp_dir, &user_id, 10 * 1024 * 1024, 10 * 1024 * 1024).await;

    let response = domain
        .upload_media(&user_id, b"quota-content", "text/plain", Some("q.txt"))
        .await
        .unwrap();
    assert!(response["content_uri"].as_str().unwrap().starts_with("mxc://test.server/"));

    let quota = domain.get_user_quota(&user_id).await.unwrap();
    assert_eq!(quota.current_storage_bytes, b"quota-content".len() as i64);
    assert_eq!(quota.current_files_count, 1);
}

#[tokio::test]
async fn test_domain_upload_rejects_oversized_file() {
    let _guard = media_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let temp_dir = tempfile::tempdir().expect("temp dir");
    let uid = unique_id();
    let user_id = create_test_user(&pool, &format!("big_{uid}")).await;
    // Per-file limit of 4 bytes.
    let domain = make_media_domain(&pool, &temp_dir, &user_id, 1024, 4).await;

    let err = domain
        .upload_media(&user_id, b"way-too-big", "text/plain", Some("big.txt"))
        .await
        .unwrap_err();
    assert_eq!(err.http_status(), axum::http::StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_domain_upload_with_id_records_quota_and_metadata() {
    let _guard = media_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let temp_dir = tempfile::tempdir().expect("temp dir");
    let uid = unique_id();
    let user_id = create_test_user(&pool, &format!("uid_{uid}")).await;
    let domain = make_media_domain(&pool, &temp_dir, &user_id, 10 * 1024 * 1024, 10 * 1024 * 1024).await;

    let media_id = format!("mi_domid_{uid}");
    domain
        .upload_media_with_id(&user_id, &media_id, b"hello", "image/png", Some("h.png"))
        .await
        .unwrap();

    let quota = domain.get_user_quota(&user_id).await.unwrap();
    assert_eq!(quota.current_storage_bytes, 5);
    assert_eq!(quota.current_files_count, 1);
}

#[tokio::test]
async fn test_domain_download_returns_payload_with_inline_disposition_for_images() {
    let _guard = media_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let temp_dir = tempfile::tempdir().expect("temp dir");
    let uid = unique_id();
    let user_id = create_test_user(&pool, &format!("dl_{uid}")).await;
    let domain = make_media_domain(&pool, &temp_dir, &user_id, 10 * 1024 * 1024, 10 * 1024 * 1024).await;

    let png = tiny_png();
    let response = domain.upload_media(&user_id, &png, "image/png", Some("pic.png")).await.unwrap();
    let media_id = response["content_uri"].as_str().unwrap().rsplit('/').next().unwrap().to_string();

    let payload: MediaResponsePayload = domain.download_media("test.server", &media_id, None).await.unwrap();
    assert_eq!(payload.content, png);
    assert_eq!(payload.headers.content_type, "image/png");
    assert_eq!(payload.headers.content_length, png.len());
    // Safe image type -> inline disposition with filename.
    assert!(payload.headers.content_disposition.starts_with("inline;"));
    assert!(payload.headers.content_disposition.contains("pic.png"));
    assert_eq!(payload.headers.x_content_type_options, "nosniff");
    assert_eq!(payload.headers.cross_origin_resource_policy, "cross-origin");
}

#[tokio::test]
async fn test_domain_download_uses_attachment_for_unsafe_types() {
    let _guard = media_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let temp_dir = tempfile::tempdir().expect("temp dir");
    let uid = unique_id();
    let user_id = create_test_user(&pool, &format!("att_{uid}")).await;
    let domain = make_media_domain(&pool, &temp_dir, &user_id, 10 * 1024 * 1024, 10 * 1024 * 1024).await;

    let response = domain
        .upload_media(&user_id, b"<html>unsafe</html>", "text/html", Some("page.html"))
        .await
        .unwrap();
    let media_id = response["content_uri"].as_str().unwrap().rsplit('/').next().unwrap().to_string();

    let payload = domain.download_media("test.server", &media_id, None).await.unwrap();
    assert_eq!(payload.headers.content_type, "text/html");
    assert!(payload.headers.content_disposition.starts_with("attachment;"));
}

#[tokio::test]
async fn test_domain_download_overrides_filename_with_response_param() {
    let _guard = media_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let temp_dir = tempfile::tempdir().expect("temp dir");
    let uid = unique_id();
    let user_id = create_test_user(&pool, &format!("ovr_{uid}")).await;
    let domain = make_media_domain(&pool, &temp_dir, &user_id, 10 * 1024 * 1024, 10 * 1024 * 1024).await;

    let response = domain.upload_media(&user_id, b"x", "image/png", Some("stored.png")).await.unwrap();
    let media_id = response["content_uri"].as_str().unwrap().rsplit('/').next().unwrap().to_string();

    let payload = domain.download_media("test.server", &media_id, Some("override.png")).await.unwrap();
    assert!(payload.headers.content_disposition.contains("override.png"));
    assert!(!payload.headers.content_disposition.contains("stored.png"));
}

#[tokio::test]
async fn test_domain_download_guesses_content_type_from_bytes_when_missing() {
    let _guard = media_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let temp_dir = tempfile::tempdir().expect("temp dir");
    let uid = unique_id();
    let user_id = create_test_user(&pool, &format!("guess_{uid}")).await;
    let domain = make_media_domain(&pool, &temp_dir, &user_id, 10 * 1024 * 1024, 10 * 1024 * 1024).await;

    // Upload with a non-image content type but PNG magic bytes.
    let png = tiny_png();
    let response = domain.upload_media(&user_id, &png, "application/octet-stream", Some("file.bin")).await.unwrap();
    let media_id = response["content_uri"].as_str().unwrap().rsplit('/').next().unwrap().to_string();

    let payload = domain.download_media("test.server", &media_id, None).await.unwrap();
    // guess_content_type detects PNG from magic bytes and overrides the stored type.
    assert_eq!(payload.headers.content_type, "image/png");
}

#[tokio::test]
async fn test_domain_get_thumbnail_returns_jpeg_payload() {
    let _guard = media_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let temp_dir = tempfile::tempdir().expect("temp dir");
    let uid = unique_id();
    let user_id = create_test_user(&pool, &format!("th_{uid}")).await;
    let domain = make_media_domain(&pool, &temp_dir, &user_id, 10 * 1024 * 1024, 10 * 1024 * 1024).await;

    let png = tiny_png();
    let response = domain.upload_media(&user_id, &png, "image/png", Some("pic.png")).await.unwrap();
    let media_id = response["content_uri"].as_str().unwrap().rsplit('/').next().unwrap().to_string();

    let payload = domain.get_thumbnail("test.server", &media_id, 96, 96, "crop").await.unwrap();
    assert_eq!(payload.headers.content_type, "image/jpeg");
    assert_eq!(&payload.content[..2], &[0xFF, 0xD8]);
}

#[tokio::test]
async fn test_domain_preview_url_delegates_to_media_service() {
    let _guard = media_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let temp_dir = tempfile::tempdir().expect("temp dir");
    let uid = unique_id();
    let user_id = create_test_user(&pool, &format!("pv_{uid}")).await;
    let domain = make_media_domain(&pool, &temp_dir, &user_id, 10 * 1024 * 1024, 10 * 1024 * 1024).await;

    let result = domain.preview_url("https://example.org/x", 1).unwrap();
    assert_eq!(result["url"].as_str().unwrap(), "https://example.org/x");
}

#[tokio::test]
async fn test_domain_sign_and_verify_delegates_to_media_service() {
    let _guard = media_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let temp_dir = tempfile::tempdir().expect("temp dir");
    let uid = unique_id();
    let user_id = create_test_user(&pool, &format!("sg_{uid}")).await;
    // Build the domain then attach a signer via the underlying media_service.
    let media_service = make_media_service(&temp_dir, &pool);
    let media_quota_storage = Arc::new(MediaQuotaStorage::new(&pool));
    let media_quota_service = Arc::new(MediaQuotaService::new(media_quota_storage));
    media_quota_service
        .set_user_quota(SetUserQuotaRequest {
            user_id: user_id.clone(),
            quota_config_id: None,
            custom_max_storage_bytes: Some(10 * 1024 * 1024),
            custom_max_file_size_bytes: Some(10 * 1024 * 1024),
            custom_max_files_count: Some(10),
        })
        .await
        .unwrap();
    let chunked = Arc::new(ChunkedUploadService::new(pool.clone()));

    // Build a media_service with a signer attached, then construct the domain.
    let mut signed_media = media_service;
    signed_media.set_link_signer(Arc::new(MediaLinkSigner::new(b"test-secret-key-32-bytes-xxxxx", 3600)));
    let domain = MediaDomainService::new(signed_media, media_quota_service, chunked);

    let query = domain.sign_media_download_url("test.server", "mid").expect("signed");
    let (sig, exp) = parse_signed_query(&query);
    assert!(domain.verify_media_download_url("test.server", "mid", &sig, exp));
}

// =============================================================================
// delete_media_for_user (quota rollback + ownership)
// =============================================================================

#[tokio::test]
async fn test_delete_media_for_user_rolls_back_quota() {
    let _guard = media_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let temp_dir = tempfile::tempdir().expect("temp dir");
    let uid = unique_id();
    let user_id = create_test_user(&pool, &format!("del_{uid}")).await;
    let domain = make_media_domain(&pool, &temp_dir, &user_id, 10 * 1024 * 1024, 10 * 1024 * 1024).await;

    let response = domain.upload_media(&user_id, b"delete-me", "text/plain", Some("d.txt")).await.unwrap();
    let media_id = response["content_uri"].as_str().unwrap().rsplit('/').next().unwrap().to_string();

    let before = domain.get_user_quota(&user_id).await.unwrap();
    assert_eq!(before.current_storage_bytes, b"delete-me".len() as i64);
    assert_eq!(before.current_files_count, 1);

    domain.delete_media_for_user("test.server", &media_id, &user_id).await.unwrap();

    let after = domain.get_user_quota(&user_id).await.unwrap();
    assert_eq!(after.current_storage_bytes, 0);
    assert_eq!(after.current_files_count, 0);
}

#[tokio::test]
async fn test_delete_media_for_user_forbids_other_users() {
    let _guard = media_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let temp_dir = tempfile::tempdir().expect("temp dir");
    let uid = unique_id();
    let owner = create_test_user(&pool, &format!("owner_{uid}")).await;
    let intruder = create_test_user(&pool, &format!("intruder_{uid}")).await;

    let media_service = make_media_service(&temp_dir, &pool);
    let media_quota_storage = Arc::new(MediaQuotaStorage::new(&pool));
    let media_quota_service = Arc::new(MediaQuotaService::new(media_quota_storage));
    for u in [&owner, &intruder] {
        media_quota_service
            .set_user_quota(SetUserQuotaRequest {
                user_id: u.clone(),
                quota_config_id: None,
                custom_max_storage_bytes: Some(10 * 1024 * 1024),
                custom_max_file_size_bytes: Some(10 * 1024 * 1024),
                custom_max_files_count: Some(10),
            })
            .await
            .unwrap();
    }
    let chunked = Arc::new(ChunkedUploadService::new(pool.clone()));
    let domain = MediaDomainService::new(media_service, media_quota_service, chunked);

    let response = domain.upload_media(&owner, b"private", "text/plain", Some("p.txt")).await.unwrap();
    let media_id = response["content_uri"].as_str().unwrap().rsplit('/').next().unwrap().to_string();

    let err = domain.delete_media_for_user("test.server", &media_id, &intruder).await.unwrap_err();
    assert_eq!(err.http_status(), axum::http::StatusCode::FORBIDDEN);

    // Media is still downloadable after the forbidden attempt.
    let payload = domain.download_media("test.server", &media_id, None).await.unwrap();
    assert_eq!(payload.content, b"private");
}

#[tokio::test]
async fn test_delete_media_for_user_returns_not_found_for_missing() {
    let _guard = media_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let temp_dir = tempfile::tempdir().expect("temp dir");
    let uid = unique_id();
    let user_id = create_test_user(&pool, &format!("nf_{uid}")).await;
    let domain = make_media_domain(&pool, &temp_dir, &user_id, 10 * 1024 * 1024, 10 * 1024 * 1024).await;

    let err = domain
        .delete_media_for_user("test.server", &format!("mi_missing_{uid}"), &user_id)
        .await
        .unwrap_err();
    assert_eq!(err.http_status(), axum::http::StatusCode::NOT_FOUND);
}

// =============================================================================
// Chunked upload flows
// =============================================================================

#[tokio::test]
async fn test_start_chunked_upload_rejects_negative_total_size() {
    let _guard = media_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let temp_dir = tempfile::tempdir().expect("temp dir");
    let uid = unique_id();
    let user_id = create_test_user(&pool, &format!("neg_{uid}")).await;
    let domain = make_media_domain(&pool, &temp_dir, &user_id, 10 * 1024 * 1024, 10 * 1024 * 1024).await;

    let err = domain
        .start_chunked_upload(&user_id, Some("bad.txt"), Some("text/plain"), Some(-1), 1)
        .await
        .unwrap_err();
    assert_eq!(err.http_status(), axum::http::StatusCode::BAD_REQUEST);
    assert!(err.message().contains("total_size must not be negative"));
}

#[tokio::test]
async fn test_start_chunked_upload_rejects_when_quota_exceeded() {
    let _guard = media_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let temp_dir = tempfile::tempdir().expect("temp dir");
    let uid = unique_id();
    let user_id = create_test_user(&pool, &format!("qre_{uid}")).await;
    // 4-byte limit.
    let domain = make_media_domain(&pool, &temp_dir, &user_id, 4, 4).await;

    let err = domain
        .start_chunked_upload(&user_id, Some("big.txt"), Some("text/plain"), Some(5), 1)
        .await
        .unwrap_err();
    assert_eq!(err.http_status(), axum::http::StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_chunked_upload_full_lifecycle_downloads_assembled_content() {
    let _guard = media_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let temp_dir = tempfile::tempdir().expect("temp dir");
    let uid = unique_id();
    let user_id = create_test_user(&pool, &format!("chk_{uid}")).await;
    let domain = make_media_domain(&pool, &temp_dir, &user_id, 10 * 1024 * 1024, 10 * 1024 * 1024).await;

    let upload_id = domain
        .start_chunked_upload(&user_id, Some("greet.txt"), Some("text/plain"), Some(11), 2)
        .await
        .unwrap();

    for (idx, chunk) in [b"hello ".to_vec(), b"world".to_vec()].iter().enumerate() {
        let chunk = chunk.clone();
        domain
            .upload_chunk(
                ChunkUploadRequest {
                    upload_id: Some(upload_id.clone()),
                    chunk_index: idx as i32,
                    total_chunks: 2,
                    chunk_data: chunk,
                    filename: Some("greet.txt".to_string()),
                    content_type: Some("text/plain".to_string()),
                    total_size: Some(11),
                },
                &user_id,
            )
            .await
            .unwrap();
    }

    let progress: UploadProgress = domain.get_chunked_upload_progress(&upload_id).await.unwrap();
    assert_eq!(progress.status, "pending");
    assert_eq!(progress.uploaded_chunks, 2);

    let finalized = domain.complete_chunked_upload(&upload_id, &user_id).await.unwrap();
    assert_eq!(finalized.size, 11);
    assert!(finalized.content_uri.ends_with(&finalized.media_id));

    let payload = domain.download_media("test.server", &finalized.media_id, None).await.unwrap();
    assert_eq!(payload.content, b"hello world");
    assert_eq!(payload.headers.content_type, "text/plain");

    // Progress record remains accessible and marked finalized.
    let after = domain.get_chunked_upload_progress(&upload_id).await.unwrap();
    assert_eq!(after.status, "finalized");

    // Quota recorded for the assembled file.
    let quota = domain.get_user_quota(&user_id).await.unwrap();
    assert_eq!(quota.current_storage_bytes, 11);
    assert_eq!(quota.current_files_count, 1);
}

#[tokio::test]
async fn test_cancel_chunked_upload_marks_cancelled() {
    let _guard = media_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let temp_dir = tempfile::tempdir().expect("temp dir");
    let uid = unique_id();
    let user_id = create_test_user(&pool, &format!("cnl_{uid}")).await;
    let domain = make_media_domain(&pool, &temp_dir, &user_id, 10 * 1024 * 1024, 10 * 1024 * 1024).await;

    let upload_id = domain
        .start_chunked_upload(&user_id, Some("c.txt"), Some("text/plain"), Some(4), 1)
        .await
        .unwrap();

    domain.cancel_chunked_upload(&upload_id, &user_id).await.unwrap();
    let progress = domain.get_chunked_upload_progress(&upload_id).await.unwrap();
    assert_eq!(progress.status, "cancelled");
}

#[tokio::test]
async fn test_upload_chunk_rejects_wrong_user() {
    let _guard = media_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let temp_dir = tempfile::tempdir().expect("temp dir");
    let uid = unique_id();
    let owner = create_test_user(&pool, &format!("co_{uid}")).await;
    let other = create_test_user(&pool, &format!("ot_{uid}")).await;
    let domain = make_media_domain(&pool, &temp_dir, &owner, 10 * 1024 * 1024, 10 * 1024 * 1024).await;

    let upload_id = domain
        .start_chunked_upload(&owner, Some("c.txt"), Some("text/plain"), Some(4), 1)
        .await
        .unwrap();

    let err = domain
        .upload_chunk(
            ChunkUploadRequest {
                upload_id: Some(upload_id),
                chunk_index: 0,
                total_chunks: 1,
                chunk_data: b"abc".to_vec(),
                filename: None,
                content_type: None,
                total_size: None,
            },
            &other,
        )
        .await
        .unwrap_err();
    assert_eq!(err.http_status(), axum::http::StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_complete_chunked_upload_unknown_id_returns_not_found() {
    let _guard = media_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let temp_dir = tempfile::tempdir().expect("temp dir");
    let uid = unique_id();
    let user_id = create_test_user(&pool, &format!("unk_{uid}")).await;
    let domain = make_media_domain(&pool, &temp_dir, &user_id, 10 * 1024 * 1024, 10 * 1024 * 1024).await;

    let err = domain.complete_chunked_upload("nonexistent-upload-id", &user_id).await.unwrap_err();
    assert_eq!(err.http_status(), axum::http::StatusCode::NOT_FOUND);
}

// =============================================================================
// Quota helpers (get_user_quota / get_usage_stats / get_user_alerts)
// =============================================================================

#[tokio::test]
async fn test_get_usage_stats_returns_json_with_usage_fields() {
    let _guard = media_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let temp_dir = tempfile::tempdir().expect("temp dir");
    let uid = unique_id();
    let user_id = create_test_user(&pool, &format!("stat_{uid}")).await;
    let domain = make_media_domain(&pool, &temp_dir, &user_id, 10 * 1024 * 1024, 10 * 1024 * 1024).await;

    domain.upload_media(&user_id, b"stat", "text/plain", Some("s.txt")).await.unwrap();

    let stats = domain.get_usage_stats(&user_id).await.unwrap();
    let obj = stats.as_object().expect("usage stats is an object");
    assert!(obj.contains_key("current_storage_bytes") || obj.contains_key("used_bytes") || obj.contains_key("size"));
}

#[tokio::test]
async fn test_get_user_alerts_returns_empty_for_new_user() {
    let _guard = media_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let temp_dir = tempfile::tempdir().expect("temp dir");
    let uid = unique_id();
    let user_id = create_test_user(&pool, &format!("al_{uid}")).await;
    let domain = make_media_domain(&pool, &temp_dir, &user_id, 10 * 1024 * 1024, 10 * 1024 * 1024).await;

    let alerts = domain.get_user_alerts(&user_id, false).await.unwrap();
    assert!(alerts.is_empty(), "a fresh user should have no quota alerts");
}

#[tokio::test]
async fn test_get_user_quota_reports_configured_limits() {
    let _guard = media_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup(&pool).await;

    let temp_dir = tempfile::tempdir().expect("temp dir");
    let uid = unique_id();
    let user_id = create_test_user(&pool, &format!("lim_{uid}")).await;
    let domain = make_media_domain(&pool, &temp_dir, &user_id, 5_000_000, 500_000).await;

    let quota = domain.get_user_quota(&user_id).await.unwrap();
    assert_eq!(quota.max_storage_bytes, 5_000_000);
    assert_eq!(quota.max_file_size_bytes, 500_000);
    assert_eq!(quota.current_storage_bytes, 0);
    assert_eq!(quota.current_files_count, 0);
}

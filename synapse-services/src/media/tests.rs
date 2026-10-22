//! Media service tests — quota / chunked upload / content-type / header helpers.
//!
//! 2026-10-10 从 `media/mod.rs` 外置（该文件回到体积天花板以内）。
//! T06 隔离/检疫相关用例在同级的 `quarantine_tests.rs`。

use super::*;
use crate::media_quota_service::MediaQuotaService;
use crate::test_utils;
use std::sync::Arc;
use synapse_common::error::MatrixErrorCode;
use synapse_storage::media_quota::MediaQuotaStorage;
use synapse_storage::user::UserStorage;

pub(super) async fn prepare_media_test_pool() -> Result<Arc<sqlx::PgPool>, String> {
    // Use the shared isolated test pool which clones from the v11 baseline.
    // This ensures all required tables (users, upload_progress, media_metadata,
    // quarantined_media_changes, server_media_quota, etc.) are present in the
    // test schema, avoiding FK violations when queries silently fall back to
    // the shared public schema.
    test_utils::prepare_isolated_test_pool().await
}

async fn setup_test_media_domain_users_with_quota(
    usernames: &[&str],
    max_storage_bytes: i64,
    max_file_size_bytes: i64,
) -> (MediaDomainService, MediaService, Vec<synapse_storage::user::User>, tempfile::TempDir) {
    let pool = prepare_media_test_pool().await.expect("failed to prepare media test pool");
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let media_path = temp_dir.path().to_str().expect("temp dir path should be valid utf-8");

    let user_storage =
        UserStorage::new(&pool, Arc::new(synapse_cache::CacheManager::new(&synapse_cache::CacheConfig::default())));
    let mut users = Vec::new();
    for username in usernames {
        let user_id = format!("@{username}:test.server");
        let user = user_storage
            .create_user(&user_id, username, Some("password"), false)
            .await
            .expect("Failed to create test user");
        users.push(user);
    }

    let media_service = MediaService::with_pool(media_path, None, "test.server", Some(pool.clone())).await;
    let media_quota_storage = Arc::new(MediaQuotaStorage::new(&pool));
    let media_quota_service = Arc::new(MediaQuotaService::new(media_quota_storage));
    for user in &users {
        let now = synapse_common::current_timestamp_millis();
        sqlx::query(
            "INSERT INTO user_media_quota (user_id, custom_max_storage_bytes, custom_max_file_size_bytes, custom_max_files_count, created_ts, updated_ts) VALUES ($1, $2, $3, $4, $5, $5) ON CONFLICT (user_id) DO UPDATE SET custom_max_storage_bytes = $2, custom_max_file_size_bytes = $3, custom_max_files_count = $4, updated_ts = $5",
        )
        .bind(&user.user_id)
        .bind(max_storage_bytes)
        .bind(max_file_size_bytes)
        .bind(10_i32)
        .bind(now)
        .execute(pool.as_ref())
        .await
        .expect("failed to set user quota");
    }

    let chunked_upload_service = Arc::new(chunked_upload::ChunkedUploadService::new(pool.clone(), 100 * 1024 * 1024));
    let media_domain_service =
        MediaDomainService::new(media_service.clone(), media_quota_service, chunked_upload_service);

    (media_domain_service, media_service, users, temp_dir)
}

pub(super) async fn setup_test_media_domain(
    username: &str,
) -> (MediaDomainService, MediaService, synapse_storage::user::User, tempfile::TempDir) {
    let (media_domain_service, media_service, mut users, temp_dir) =
        setup_test_media_domain_users_with_quota(&[username], 10 * 1024 * 1024, 10 * 1024 * 1024).await;
    (media_domain_service, media_service, users.remove(0), temp_dir)
}

async fn setup_test_media_domain_with_quota(
    username: &str,
    max_storage_bytes: i64,
    max_file_size_bytes: i64,
) -> (MediaDomainService, MediaService, synapse_storage::user::User, tempfile::TempDir) {
    let (media_domain_service, media_service, mut users, temp_dir) =
        setup_test_media_domain_users_with_quota(&[username], max_storage_bytes, max_file_size_bytes).await;
    (media_domain_service, media_service, users.remove(0), temp_dir)
}

#[tokio::test]
async fn test_chunked_complete_can_be_downloaded_via_media_service() {
    let (media_domain_service, media_service, user, _temp_dir) = setup_test_media_domain("chunk_tester").await;

    let user_id = &user.user_id;
    let first_chunk = b"hello ".to_vec();
    let second_chunk = b"world".to_vec();

    let upload_id = media_domain_service
        .start_chunked_upload(
            user_id,
            Some("greeting.txt"),
            Some("text/plain"),
            Some((first_chunk.len() + second_chunk.len()) as i64),
            2,
        )
        .await
        .expect("failed to start chunked upload");

    media_domain_service
        .upload_chunk(
            chunked_upload::ChunkUploadRequest {
                upload_id: Some(upload_id.clone()),
                chunk_index: 0,
                total_chunks: 2,
                chunk_data: first_chunk,
                filename: Some("greeting.txt".to_string()),
                content_type: Some("text/plain".to_string()),
                total_size: Some(11),
            },
            user_id,
        )
        .await
        .expect("failed to upload first chunk");

    media_domain_service
        .upload_chunk(
            chunked_upload::ChunkUploadRequest {
                upload_id: Some(upload_id.clone()),
                chunk_index: 1,
                total_chunks: 2,
                chunk_data: second_chunk,
                filename: Some("greeting.txt".to_string()),
                content_type: Some("text/plain".to_string()),
                total_size: Some(11),
            },
            user_id,
        )
        .await
        .expect("failed to upload second chunk");

    let response = media_domain_service
        .complete_chunked_upload(&upload_id, user_id)
        .await
        .expect("failed to finalize chunked upload");

    let downloaded = media_domain_service
        .download_media("test.server", &response.media_id, None)
        .await
        .expect("failed to download finalized media");

    assert_eq!(downloaded.content, b"hello world");
    assert_eq!(downloaded.headers.content_type, "text/plain");
    assert_eq!(
        downloaded.headers.content_disposition,
        "attachment; filename=\"greeting.txt\"; filename*=UTF-8''greeting.txt"
    );

    let raw_download = media_service
        .download_media("test.server", &response.media_id)
        .await
        .expect("failed to download finalized media directly");
    assert_eq!(raw_download, b"hello world");

    let progress = media_domain_service
        .get_chunked_upload_progress(&upload_id)
        .await
        .expect("finalized progress record should remain accessible");
    assert_eq!(progress.status, "finalized");
}

#[tokio::test]
async fn test_delete_media_rolls_back_quota_usage() {
    let (media_domain_service, _media_service, user, _temp_dir) = setup_test_media_domain("quota_delete_tester").await;

    let upload = media_domain_service
        .upload_media(&user.user_id, b"delete me", "text/plain", Some("delete-me.txt"))
        .await
        .expect("failed to upload media");

    let media_id = upload
        .get("content_uri")
        .and_then(|v| v.as_str())
        .and_then(|content_uri| content_uri.rsplit('/').next())
        .expect("upload response should contain media_id")
        .to_string();

    let quota_after_upload =
        media_domain_service.get_user_quota(&user.user_id).await.expect("failed to load quota after upload");
    assert_eq!(quota_after_upload.current_storage_bytes, 9);
    assert_eq!(quota_after_upload.current_files_count, 1);

    media_domain_service
        .delete_media_for_user("test.server", &media_id, &user.user_id)
        .await
        .expect("failed to delete uploaded media");

    let quota_after_delete =
        media_domain_service.get_user_quota(&user.user_id).await.expect("failed to load quota after delete");
    assert_eq!(quota_after_delete.current_storage_bytes, 0);
    assert_eq!(quota_after_delete.current_files_count, 0);
}

#[tokio::test]
async fn test_start_chunked_upload_rejects_when_quota_would_be_exceeded() {
    let (media_domain_service, _media_service, user, _temp_dir) =
        setup_test_media_domain_with_quota("quota_reject_tester", 4, 4).await;

    let error = media_domain_service
        .start_chunked_upload(&user.user_id, Some("too-large.txt"), Some("text/plain"), Some(5), 1)
        .await
        .expect_err("chunked upload start should fail when quota is exceeded");

    // The configured limit is the **user's aggregate** allowance (see
    // `setup_test_media_domain_users_with_quota`), so this is a resource
    // limit → 403, not 413 (the per-file cap is the server quota's job).
    assert_eq!(
        error.http_status(),
        axum::http::StatusCode::FORBIDDEN,
        "an aggregate quota refusal is 403 M_RESOURCE_LIMIT_EXCEEDED, not 400 M_BAD_JSON: {error:?}"
    );
    assert!(
        error.code_is(MatrixErrorCode::ResourceLimitExceeded),
        "expected M_RESOURCE_LIMIT_EXCEEDED, got {}",
        error.code_str()
    );
    assert!(error.message().contains("Quota exceeded"), "unexpected error message: {}", error.message());
}

/// The server-level **per-file** cap is the one refusal that maps to 413:
/// the request body itself is too large (upstream Synapse's `max_upload_size`).
#[tokio::test]
async fn test_server_per_file_cap_yields_413_m_too_large() {
    let pool = prepare_media_test_pool().await.expect("pool");
    sqlx::query(
        "UPDATE server_media_quota SET max_storage_bytes = $1, max_file_size_bytes = $2, max_files_count = $3, alert_threshold_percent = $4 WHERE id = 1",
    )
    .bind(1_000_000_i64)
    .bind(4_i64)
    .bind(100_i32)
    .bind(95_i32)
    .execute(pool.as_ref())
    .await
    .expect("set server quota");
    let quota_storage = Arc::new(MediaQuotaStorage::new(&pool));
    let quota_service = MediaQuotaService::new(quota_storage);

    let check = quota_service.check_upload_quota("@cap:test.server", 5).await.expect("quota check");
    assert!(!check.is_allowed);
    assert_eq!(
        check.rejection,
        Some(QuotaRejection::FileTooLarge),
        "the per-file cap must be reported as FileTooLarge"
    );

    let error = quota_error(&check);
    assert_eq!(error.http_status(), axum::http::StatusCode::PAYLOAD_TOO_LARGE);
    assert!(error.code_is(MatrixErrorCode::TooLarge), "expected M_TOO_LARGE, got {}", error.code_str());
    assert!(error.message().contains("File size 5 exceeds maximum allowed size 4"));
}

/// The mapping is driven by the **typed** rejection, not by the message.
#[test]
fn test_quota_error_mapping_is_typed() {
    let base = QuotaCheckResult {
        is_allowed: false,
        reason: Some("whatever the message says".to_string()),
        current_usage: 10,
        quota_limit: 10,
        usage_percent: 100.0,
        rejection: Some(QuotaRejection::FileTooLarge),
    };
    assert_eq!(quota_error(&base).http_status(), axum::http::StatusCode::PAYLOAD_TOO_LARGE);

    let aggregate = QuotaCheckResult { rejection: Some(QuotaRejection::StorageExceeded), ..base };
    assert_eq!(quota_error(&aggregate).http_status(), axum::http::StatusCode::FORBIDDEN);

    // A refusal that forgot to name its reason degrades to the resource
    // limit, never to 400 (which would blame the client's JSON).
    let unnamed = QuotaCheckResult { rejection: None, ..aggregate };
    assert_eq!(quota_error(&unnamed).http_status(), axum::http::StatusCode::FORBIDDEN);
}

/// The aggregate storage quota is a **resource** limit, not a malformed
/// request: 403 `M_RESOURCE_LIMIT_EXCEEDED` (upstream Synapse has no
/// per-user total, so the code follows the spec's errcode semantics).
#[tokio::test]
async fn test_upload_rejects_with_resource_limit_when_storage_quota_exceeded() {
    // helper order is (max_storage_bytes, max_file_size_bytes): the file
    // fits, the aggregate does not.
    let (media_domain_service, _media_service, user, _temp_dir) =
        setup_test_media_domain_with_quota("storage_quota_tester", 4, 1000).await;

    let error = media_domain_service
        .start_chunked_upload(&user.user_id, Some("fits.txt"), Some("text/plain"), Some(5), 1)
        .await
        .expect_err("chunked upload start should fail when the storage quota is exceeded");

    assert_eq!(
        error.http_status(),
        axum::http::StatusCode::FORBIDDEN,
        "an aggregate-quota refusal is 403 M_RESOURCE_LIMIT_EXCEEDED: {error:?}"
    );
    assert!(
        error.code_is(MatrixErrorCode::ResourceLimitExceeded),
        "expected M_RESOURCE_LIMIT_EXCEEDED, got {}",
        error.code_str()
    );
}

#[tokio::test]
async fn test_delete_media_for_other_user_returns_forbidden() {
    let (media_domain_service, _media_service, users, _temp_dir) = setup_test_media_domain_users_with_quota(
        &["media_owner_tester", "media_intruder_tester"],
        10 * 1024 * 1024,
        10 * 1024 * 1024,
    )
    .await;
    let owner = &users[0];
    let intruder = &users[1];

    let upload = media_domain_service
        .upload_media(&owner.user_id, b"private media", "text/plain", Some("private.txt"))
        .await
        .expect("failed to upload owner media");

    let media_id = upload
        .get("content_uri")
        .and_then(|v| v.as_str())
        .and_then(|content_uri| content_uri.rsplit('/').next())
        .expect("upload response should contain media_id");

    let error = media_domain_service
        .delete_media_for_user("test.server", media_id, &intruder.user_id)
        .await
        .expect_err("deleting another user's media should be forbidden");

    assert_eq!(error.http_status(), axum::http::StatusCode::FORBIDDEN);
    assert!(error.message().contains("only delete your own media"));

    let downloaded = media_domain_service
        .download_media("test.server", media_id, None)
        .await
        .expect("media should remain downloadable after forbidden delete");
    assert_eq!(downloaded.content, b"private media");
}

#[tokio::test]
async fn test_start_chunked_upload_rejects_negative_total_size() {
    let (media_domain_service, _media_service, user, _temp_dir) = setup_test_media_domain("negative_size_tester").await;

    let error = media_domain_service
        .start_chunked_upload(&user.user_id, Some("bad.txt"), Some("text/plain"), Some(-1), 1)
        .await
        .expect_err("negative total_size should be rejected");

    assert_eq!(error.http_status(), axum::http::StatusCode::BAD_REQUEST);
    assert!(error.message().contains("total_size must not be negative"));
}

/// PERF-07 gate: `total_chunks` sizes the reassembly read, so an absurd value is
/// refused instead of being stored and later trusted.
#[tokio::test]
async fn chunked_upload_rejects_a_chunk_count_above_the_ceiling() {
    let (media_domain_service, _media_service, user, _temp_dir) = setup_test_media_domain("chunk_count_ceiling").await;

    let error = media_domain_service
        .start_chunked_upload(
            &user.user_id,
            Some("many.bin"),
            Some("application/octet-stream"),
            Some(1),
            chunked_upload::MAX_CHUNKS_PER_UPLOAD + 1,
        )
        .await
        .expect_err("a chunk count above the ceiling must be rejected");

    assert_eq!(error.http_status(), axum::http::StatusCode::PAYLOAD_TOO_LARGE);
    assert!(error.code_is(MatrixErrorCode::TooLarge), "expected M_TOO_LARGE, got {}", error.code_str());
}

/// PERF-07 gate: a zero/negative chunk count can never complete, so it is
/// refused at the single write entry point rather than only at the HTTP layer.
#[tokio::test]
async fn chunked_upload_rejects_a_chunk_count_below_one() {
    let (media_domain_service, _media_service, user, _temp_dir) = setup_test_media_domain("chunk_count_zero").await;

    let error = media_domain_service
        .start_chunked_upload(&user.user_id, Some("zero.bin"), Some("application/octet-stream"), Some(1), 0)
        .await
        .expect_err("a chunk count below one must be rejected");

    assert_eq!(error.http_status(), axum::http::StatusCode::BAD_REQUEST);
    assert!(error.message().contains("total_chunks"));
}

/// PERF-07 gate: a completed upload yields exactly the chunks it declared. A
/// progress row whose chunk rows outnumber `total_chunks` can only come from an
/// out-of-band write, so the read refuses it instead of silently dropping rows.
#[tokio::test]
async fn test_completed_upload_refuses_more_chunks_than_declared() {
    let pool = prepare_media_test_pool().await.expect("failed to prepare media test pool");
    let user_storage =
        UserStorage::new(&pool, Arc::new(synapse_cache::CacheManager::new(&synapse_cache::CacheConfig::default())));
    let user = user_storage
        .create_user("@chunk_overflow:test.server", "chunk_overflow", Some("password"), false)
        .await
        .expect("failed to create test user");

    let service = chunked_upload::ChunkedUploadService::new(pool.clone(), 1024 * 1024);

    let upload_id = service
        .start_upload(&user.user_id, Some("overflow.bin"), Some("application/octet-stream"), Some(2), 1)
        .await
        .expect("start should succeed");

    service
        .upload_chunk(
            chunked_upload::ChunkUploadRequest {
                upload_id: Some(upload_id.clone()),
                chunk_index: 0,
                total_chunks: 1,
                chunk_data: b"a".to_vec(),
                filename: None,
                content_type: None,
                total_size: Some(2),
            },
            &user.user_id,
        )
        .await
        .expect("the single declared chunk should upload");

    // The upload is already `complete`, so no service path can add this row.
    sqlx::query(
        "INSERT INTO upload_chunks (upload_id, chunk_index, chunk_data, chunk_size, created_ts) VALUES ($1, 1, $2, 1, $3)",
    )
    .bind(&upload_id)
    .bind(b"b".to_vec())
    .bind(synapse_common::current_timestamp_millis())
    .execute(pool.as_ref())
    .await
    .expect("out-of-band chunk insert should succeed");

    let error = service
        .load_completed_upload(&upload_id, &user.user_id)
        .await
        .expect_err("an upload with more chunks than declared must be refused");

    assert_eq!(error.http_status(), axum::http::StatusCode::INTERNAL_SERVER_ERROR);
}

/// FUNC-06 gate: a chunked upload must declare its total size, otherwise the
/// quota pre-check has nothing to reserve against. Red while `None` is accepted.
#[tokio::test]
async fn chunked_upload_requires_total_size() {
    let (media_domain_service, _media_service, user, _temp_dir) = setup_test_media_domain("chunk_missing_total").await;

    let error = media_domain_service
        .start_chunked_upload(&user.user_id, Some("unknown.bin"), Some("application/octet-stream"), None, 1)
        .await
        .expect_err("start without total_size must be rejected");

    assert_eq!(error.http_status(), axum::http::StatusCode::BAD_REQUEST);
    assert!(error.code_is(MatrixErrorCode::MissingParam), "expected M_MISSING_PARAM, got {}", error.code_str());
}

/// FUNC-06 gate: a client may not declare 1 KiB and then stream 2 KiB. The
/// chunk that crosses the declared total is refused and the session is closed,
/// instead of holding the excess on disk until completion.
#[tokio::test]
async fn chunked_upload_rejects_lying_total_size() {
    let (media_domain_service, _media_service, user, _temp_dir) = setup_test_media_domain("chunk_lying_total").await;

    let upload_id = media_domain_service
        .start_chunked_upload(&user.user_id, Some("lie.bin"), Some("application/octet-stream"), Some(1024), 2)
        .await
        .expect("start should succeed");

    media_domain_service
        .upload_chunk(
            chunked_upload::ChunkUploadRequest {
                upload_id: Some(upload_id.clone()),
                chunk_index: 0,
                total_chunks: 2,
                chunk_data: vec![b'a'; 1024],
                filename: None,
                content_type: None,
                total_size: Some(1024),
            },
            &user.user_id,
        )
        .await
        .expect("first chunk fits the declared size");

    let error = media_domain_service
        .upload_chunk(
            chunked_upload::ChunkUploadRequest {
                upload_id: Some(upload_id.clone()),
                chunk_index: 1,
                total_chunks: 2,
                chunk_data: vec![b'b'; 1024],
                filename: None,
                content_type: None,
                total_size: Some(1024),
            },
            &user.user_id,
        )
        .await
        .expect_err("second chunk exceeds the declared total_size");

    assert_eq!(error.http_status(), axum::http::StatusCode::PAYLOAD_TOO_LARGE);
    assert!(error.code_is(MatrixErrorCode::TooLarge), "expected M_TOO_LARGE, got {}", error.code_str());

    let progress_error = media_domain_service
        .get_chunked_upload_progress(&upload_id)
        .await
        .expect_err("the lying upload must be terminated");
    assert_eq!(
        progress_error.http_status(),
        axum::http::StatusCode::NOT_FOUND,
        "terminated upload should be gone, got {progress_error:?}"
    );
}

/// FUNC-03 gate: completing a chunked upload is still an upload, so it has to be
/// counted against the user's quota exactly like a single-shot upload.
#[tokio::test]
async fn chunked_upload_counts_toward_quota() {
    let (media_domain_service, _media_service, user, _temp_dir) = setup_test_media_domain("chunk_quota").await;

    let upload_id = media_domain_service
        .start_chunked_upload(&user.user_id, Some("counted.txt"), Some("text/plain"), Some(11), 2)
        .await
        .expect("start should succeed");

    for (chunk_index, chunk_data) in [b"hello ".to_vec(), b"world".to_vec()].into_iter().enumerate() {
        media_domain_service
            .upload_chunk(
                chunked_upload::ChunkUploadRequest {
                    upload_id: Some(upload_id.clone()),
                    chunk_index: chunk_index as i32,
                    total_chunks: 2,
                    chunk_data,
                    filename: None,
                    content_type: None,
                    total_size: Some(11),
                },
                &user.user_id,
            )
            .await
            .expect("chunk should upload");
    }

    let quota_before = media_domain_service.get_user_quota(&user.user_id).await.expect("failed to load quota");
    assert_eq!(quota_before.current_storage_bytes, 0, "uncompleted upload must not be billed yet");

    media_domain_service
        .complete_chunked_upload(&upload_id, &user.user_id)
        .await
        .expect("failed to finalize chunked upload");

    let quota_after = media_domain_service.get_user_quota(&user.user_id).await.expect("failed to load quota");
    assert_eq!(quota_after.current_storage_bytes, 11, "completed chunked upload must be billed");
    assert_eq!(quota_after.current_files_count, 1);
}

/// FUNC-06 gate: an in-flight upload already holds its declared size, so a second
/// upload cannot over-commit the same quota. Cancelling releases the hold.
#[tokio::test]
async fn chunked_upload_reserves_pending_quota() {
    let (media_domain_service, _media_service, user, _temp_dir) =
        setup_test_media_domain_with_quota("chunk_pending", 10, 10).await;

    let first_upload = media_domain_service
        .start_chunked_upload(&user.user_id, Some("first.bin"), Some("application/octet-stream"), Some(6), 1)
        .await
        .expect("first start should fit the quota");

    let error = media_domain_service
        .start_chunked_upload(&user.user_id, Some("second.bin"), Some("application/octet-stream"), Some(6), 1)
        .await
        .expect_err("second start must not over-commit the quota already held by the first upload");
    assert_eq!(error.http_status(), axum::http::StatusCode::FORBIDDEN);

    media_domain_service
        .cancel_chunked_upload(&first_upload, &user.user_id)
        .await
        .expect("cancel should release the hold");

    media_domain_service
        .start_chunked_upload(&user.user_id, Some("second.bin"), Some("application/octet-stream"), Some(6), 1)
        .await
        .expect("after the cancel the quota is free again");
}

#[test]
fn test_guess_content_type_prefers_detected_bytes_over_filename_extension() {
    let png_bytes = b"\x89PNG\r\n\x1a\nrest";

    assert_eq!(guess_content_type("document.txt", png_bytes), "image/png");
}

#[test]
fn test_guess_content_type_falls_back_to_filename_extension() {
    assert_eq!(guess_content_type("notes.json", b"plain text"), "application/json");
    assert_eq!(guess_content_type("archive.unknown", b"plain text"), "application/octet-stream");
}

#[test]
fn test_sanitize_attachment_filename_removes_control_and_path_characters() {
    let sanitized = sanitize_attachment_filename("..\u{0000}/bad\\name\".txt");

    assert_eq!(sanitized, "..badname.txt");
}

#[test]
fn test_build_media_response_headers_uses_inline_for_safe_media_types() {
    let headers = build_media_response_headers("image/png".to_string(), 42, Some("photo.png"));

    assert_eq!(headers.content_type, "image/png");
    assert_eq!(headers.content_length, 42);
    assert_eq!(headers.content_disposition, "inline; filename=\"photo.png\"; filename*=UTF-8''photo.png");
    assert_eq!(headers.x_content_type_options, "nosniff");
}

#[test]
fn test_build_media_response_headers_uses_attachment_for_unsafe_types_and_encodes_filename() {
    let headers = build_media_response_headers("text/html".to_string(), 99, Some("report 2026.html"));

    assert_eq!(
        headers.content_disposition,
        "attachment; filename=\"report 2026.html\"; filename*=UTF-8''report%202026.html"
    );
    assert_eq!(headers.cross_origin_resource_policy, "cross-origin");
    assert_eq!(headers.referrer_policy, "no-referrer");
}

#[test]
fn test_build_media_response_headers_falls_back_to_kind_when_filename_sanitizes_empty() {
    let headers = build_media_response_headers("text/html".to_string(), 7, Some("/\\\"\u{0000}"));

    assert_eq!(headers.content_disposition, "attachment");
}

#[test]
fn test_build_media_response_headers_treats_safe_media_type_case_insensitively() {
    let headers = build_media_response_headers("IMAGE/PNG; charset=binary".to_string(), 5, Some("photo.png"));

    assert_eq!(headers.content_disposition, "inline; filename=\"photo.png\"; filename*=UTF-8''photo.png");
}

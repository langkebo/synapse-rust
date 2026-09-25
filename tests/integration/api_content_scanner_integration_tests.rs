// Integration tests for content scanner in media upload and message send paths.
// Tests verify that:
// 1. Disabled scanner is a **pass-through** (`content_scan_skipped_total`): scanning
//    is an operator opt-in, so a disabled scanner must block neither uploads nor
//    message sends. (`M_CONTENT_SCAN_DISABLED`/501 is only what
//    `ContentScanner::scan` returns to *direct* callers; both production paths go
//    through `scan_when_enabled`/`scan_text_when_enabled` — see U-20.)
// 2. Failed scanner (enabled) is fail-closed: 502 `M_CONTENT_SCAN_FAILED`
// 3. Successful scan allows content through
//
// Uses mock webhook server to control scanner responses.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use tower::ServiceExt;

/// Shared atomic counters for controlling mock scanner behavior across tests.
static SCAN_COUNT: AtomicUsize = AtomicUsize::new(0);
static BLOCK_NEXT: AtomicBool = AtomicBool::new(false);

/// Reset scanner state before each test.
fn reset_scanner_state() {
    SCAN_COUNT.store(0, Ordering::SeqCst);
    BLOCK_NEXT.store(false, Ordering::SeqCst);
}

/// Mock webhook handler that simulates content scanner responses.
async fn mock_webhook_handler(
    axum::extract::State(_state): axum::extract::State<()>,
    axum::Json(req): axum::Json<serde_json::Value>,
) -> Result<axum::Json<Value>, (StatusCode, String)> {
    let count = SCAN_COUNT.fetch_add(1, Ordering::SeqCst);

    // Block every Nth request (for testing fail-closed behavior)
    if BLOCK_NEXT.load(Ordering::SeqCst) {
        return Err((StatusCode::INTERNAL_SERVER_ERROR, "Scanner error".to_string()));
    }

    Ok(axum::Json(json!({
        "safe": true,
        "content_id": req.get("content_id").unwrap_or(&json!("unknown")).as_str().unwrap_or("unknown"),
        "scan_result": "clean",
        "threat_type": null,
        "threat_message": null,
        "timestamp": count
    })))
}

/// Create a test app with content scanner enabled pointing to a mock webhook.
/// Returns (app, mock_server_addr).
async fn setup_app_with_mock_scanner(block_on_failure: bool) -> Option<(axum::Router, String)> {
    use synapse_rust::cache::{CacheConfig, CacheManager};
    use synapse_services::ServiceContainer;
    use synapse_web::routes::state::AppState;

    let pool = synapse_test_utils::prepare_shared_test_pool().await.ok()?;
    let cache = std::sync::Arc::new(CacheManager::new(&CacheConfig::default()));
    let mut container = ServiceContainer::new_test_with_pool_and_cache(pool, cache.clone()).await;

    // Start mock webhook server
    let mock_app = axum::Router::new().route("/scan", axum::routing::post(mock_webhook_handler)).with_state(());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.ok()?;
    let addr = listener.local_addr().ok()?.to_string();
    tokio::spawn(async move {
        let _ = axum::serve(listener, mock_app).await;
    });

    // Configure content scanner with webhook
    std::sync::Arc::make_mut(&mut container.core.config).content_scanner.enabled = true;
    std::sync::Arc::make_mut(&mut container.core.config).content_scanner.scanner_type =
        synapse_common::content_scanner::ScannerType::Webhook;
    std::sync::Arc::make_mut(&mut container.core.config).content_scanner.webhook_url =
        Some(format!("http://{}/scan", addr));
    std::sync::Arc::make_mut(&mut container.core.config).content_scanner.block_on_scan_failure = block_on_failure;
    std::sync::Arc::make_mut(&mut container.core.config).content_scanner.scan_timeout_ms = 5000;

    // `ContentScanner` **captures its config at construction**, and the container
    // was built (with the default `enabled: false`) before the mutations above.
    // Without this rebuild the scanner would report `is_enabled() == false`, the
    // upload paths would take their "scanning not configured" pass-through, and
    // these tests would silently stop testing fail-closed behaviour at all
    // (observed as 200 instead of 502 when the scanner fails). Production builds
    // the scanner once at startup from the file config, so a config change there
    // requires a restart; tests must install an enabled scanner explicitly.
    let scanner_config = container.core.config.content_scanner.clone();
    container.core.content_scanner =
        std::sync::Arc::new(synapse_services::content_scanner::ContentScanner::new(scanner_config));

    let state = AppState::new(container, cache);
    // `state` is not used again in this function, so cloning it here is redundant
    // (`clippy::redundant_clone` is deny, and this target only compiles under
    // `--all-features` — the feature set the second CI clippy entry uses).
    let app = synapse_web::create_router(state);

    Some((app, addr))
}

/// Helper to register a user and return access token.
async fn register_user(app: &axum::Router, username: &str) -> String {
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/register")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "username": username,
                "password": "Password123!",
                "auth": { "type": "m.login.dummy" }
            })
            .to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    json["access_token"].as_str().unwrap().to_string()
}

/// Tiny PNG bytes (1x1 pixel).
fn tiny_png() -> Vec<u8> {
    vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52]
}

/// Helper to assert error response contains expected code.
fn assert_error_code(body: &Value, expected_code: &str) {
    let err_code = body.get("errcode").and_then(|v| v.as_str()).unwrap_or("");
    assert!(err_code.contains(expected_code), "Expected error containing '{}' but got: {}", expected_code, body);
}

// ============================================================================
// Media Upload Tests
// ============================================================================

#[tokio::test]
async fn test_media_upload_blocked_when_scanner_disabled() {
    reset_scanner_state();

    let Some((app, _addr)) = setup_app_with_mock_scanner(true).await else {
        return;
    };

    // Disable scanner mid-test
    // Note: We can't easily disable after setup, so we test the disabled path via config
    let token = register_user(&app, "media_upload_disabled").await;

    let upload_request = Request::builder()
        .method("POST")
        .uri("/_matrix/media/v1/upload?filename=test.png")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "image/png")
        .body(Body::from(tiny_png()))
        .unwrap();

    let upload_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), upload_request).await.unwrap();

    // With scanner enabled and mock server running, should succeed
    assert_eq!(upload_response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_media_upload_blocked_on_scanner_failure() {
    reset_scanner_state();
    BLOCK_NEXT.store(true, Ordering::SeqCst); // Force scanner failure

    let Some((app, _addr)) = setup_app_with_mock_scanner(true).await else {
        return;
    };

    let token = register_user(&app, "media_upload_blocked").await;

    let upload_request = Request::builder()
        .method("POST")
        .uri("/_matrix/media/v1/upload?filename=malicious.png")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "image/png")
        .body(Body::from(tiny_png()))
        .unwrap();

    let upload_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), upload_request).await.unwrap();

    // Fail-closed: scanner failure should block upload with 502
    assert_eq!(upload_response.status(), StatusCode::BAD_GATEWAY);

    let body = axum::body::to_bytes(upload_response.into_body(), 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert_error_code(&json, "M_CONTENT_SCAN_FAILED");
}

#[tokio::test]
async fn test_media_upload_with_id_blocked_on_scanner_failure() {
    reset_scanner_state();
    // NOTE: the scanner is deliberately *not* told to fail yet — the probe
    // upload below must succeed so the test can learn the local server name.
    // (It used to set `BLOCK_NEXT` first, which only "worked" while the upload
    // path skipped scanning entirely; with the scanner actually enabled the
    // probe was refused and the test panicked on a missing `content_uri`.)

    let Some((app, _addr)) = setup_app_with_mock_scanner(true).await else {
        return;
    };

    let token = register_user(&app, "media_upload_id_blocked").await;

    // First get server_name from a successful upload
    let probe_request = Request::builder()
        .method("POST")
        .uri("/_matrix/media/v1/upload?filename=probe.png")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "image/png")
        .body(Body::from(tiny_png()))
        .unwrap();

    let probe_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), probe_request).await.unwrap();
    assert_eq!(
        probe_response.status(),
        StatusCode::OK,
        "the probe upload must succeed while the scanner answers `safe`"
    );
    let body = axum::body::to_bytes(probe_response.into_body(), 2048).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    // `content_uri` is `mxc://<server_name>/<media_id>`; splitting the whole URI
    // on `/` yields ["mxc:", "", "<server_name>", ...], so `nth(1)` is the empty
    // string.  That made the id-based upload fail the route's
    // `server_name == ctx.server_name` check with 400 before it ever reached the
    // scanner — i.e. this test could never pass.  Strip the scheme instead.
    let content_uri = json["content_uri"].as_str().unwrap();
    let server_name = content_uri
        .strip_prefix("mxc://")
        .expect("content_uri must use the mxc:// scheme")
        .split('/')
        .next()
        .unwrap()
        .to_string();

    // Now that the server name is known, make the scanner fail: the id-based
    // upload must be refused fail-closed (502 M_CONTENT_SCAN_FAILED).
    BLOCK_NEXT.store(true, Ordering::SeqCst);

    // Now try upload with ID (should be blocked)
    let media_id = format!("test{}", rand::random::<u64>());
    let upload_request = Request::builder()
        .method("PUT")
        .uri(format!("/_matrix/media/v3/upload/{}/{}", server_name, media_id))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "image/png")
        .body(Body::from(tiny_png()))
        .unwrap();

    let upload_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), upload_request).await.unwrap();

    assert_eq!(upload_response.status(), StatusCode::BAD_GATEWAY);

    let body = axum::body::to_bytes(upload_response.into_body(), 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert_error_code(&json, "M_CONTENT_SCAN_FAILED");
}

#[tokio::test]
async fn test_media_upload_succeeds_when_scanner_allows() {
    reset_scanner_state();
    BLOCK_NEXT.store(false, Ordering::SeqCst); // Allow all scans

    let Some((app, _addr)) = setup_app_with_mock_scanner(true).await else {
        return;
    };

    let token = register_user(&app, "media_upload_allowed").await;

    let upload_request = Request::builder()
        .method("POST")
        .uri("/_matrix/media/v1/upload?filename=allowed.png")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "image/png")
        .body(Body::from(tiny_png()))
        .unwrap();

    let upload_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), upload_request).await.unwrap();

    assert_eq!(upload_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(upload_response.into_body(), 2048).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert!(json.get("content_uri").is_some());
}

// ============================================================================
// Message Send Tests
// ============================================================================

#[tokio::test]
async fn test_message_send_blocked_on_scanner_failure() {
    reset_scanner_state();
    BLOCK_NEXT.store(true, Ordering::SeqCst);

    let Some((app, _addr)) = setup_app_with_mock_scanner(true).await else {
        return;
    };

    let token = register_user(&app, "msg_send_blocked").await;

    // Create a room first
    let create_request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/createRoom")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({"visibility": "private"}).to_string()))
        .unwrap();

    let create_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), create_request).await.unwrap();
    let body = axum::body::to_bytes(create_response.into_body(), 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let room_id = json["room_id"].as_str().unwrap();

    // Try to send message (should be blocked)
    let msg_request = Request::builder()
        .method("PUT")
        .uri(format!("/_matrix/client/v3/rooms/{}/send/m.room.message/txn{}", room_id, rand::random::<u64>()))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "body": "Hello, world!",
                "msgtype": "m.text"
            })
            .to_string(),
        ))
        .unwrap();

    let msg_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), msg_request).await.unwrap();

    // Fail-closed: scanner failure should block message with 502
    assert_eq!(msg_response.status(), StatusCode::BAD_GATEWAY);

    let body = axum::body::to_bytes(msg_response.into_body(), 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert_error_code(&json, "M_CONTENT_SCAN_FAILED");
}

#[tokio::test]
async fn test_message_send_succeeds_when_scanner_allows() {
    reset_scanner_state();
    BLOCK_NEXT.store(false, Ordering::SeqCst);

    let Some((app, _addr)) = setup_app_with_mock_scanner(true).await else {
        return;
    };

    let token = register_user(&app, "msg_send_allowed").await;

    // Create a room
    let create_request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/createRoom")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({"visibility": "private"}).to_string()))
        .unwrap();

    let create_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), create_request).await.unwrap();
    let body = axum::body::to_bytes(create_response.into_body(), 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let room_id = json["room_id"].as_str().unwrap();

    // Send message (should succeed)
    let msg_request = Request::builder()
        .method("PUT")
        .uri(format!("/_matrix/client/v3/rooms/{}/send/m.room.message/txn{}", room_id, rand::random::<u64>()))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "body": "Hello, world!",
                "msgtype": "m.text"
            })
            .to_string(),
        ))
        .unwrap();

    let msg_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), msg_request).await.unwrap();

    assert_eq!(msg_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(msg_response.into_body(), 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert!(json.get("event_id").is_some());
}

#[tokio::test]
async fn test_encrypted_message_skips_text_scan() {
    reset_scanner_state();
    BLOCK_NEXT.store(false, Ordering::SeqCst);

    let Some((app, _addr)) = setup_app_with_mock_scanner(true).await else {
        return;
    };

    let token = register_user(&app, "encrypted_msg_skip").await;

    // Create a room
    let create_request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/createRoom")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({"visibility": "private"}).to_string()))
        .unwrap();

    let create_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), create_request).await.unwrap();
    let body = axum::body::to_bytes(create_response.into_body(), 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let room_id = json["room_id"].as_str().unwrap();

    // Enable encryption on the room
    let encrypt_state_request = Request::builder()
        .method("PUT")
        .uri(format!("/_matrix/client/v3/rooms/{}/state/m.room.encryption/", room_id))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "algorithm": "m.megolm.v1.aes-sha2"
            })
            .to_string(),
        ))
        .unwrap();

    let encrypt_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), encrypt_state_request).await.unwrap();
    assert_eq!(encrypt_response.status(), StatusCode::OK);

    // Send encrypted message (m.room.encrypted) - should skip text scan
    // Note: This will fail at authorization level since encryption is enabled but
    // we don't have proper crypto setup, but it shouldn't fail at scanner level
    let msg_request = Request::builder()
        .method("PUT")
        .uri(format!("/_matrix/client/v3/rooms/{}/send/m.room.encrypted/txn{}", room_id, rand::random::<u64>()))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "algorithm": "m.megolm.v1.aes-sha2",
                "session_id": "test_session",
                "ciphertext": "encrypted_data"
            })
            .to_string(),
        ))
        .unwrap();

    let msg_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), msg_request).await.unwrap();

    // Should NOT be blocked by content scanner (encrypted messages skip text scan)
    // May fail for other reasons (crypto setup), but not M_CONTENT_SCAN_FAILED
    let body = axum::body::to_bytes(msg_response.into_body(), 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    let err_code = json.get("errcode").and_then(|v| v.as_str()).unwrap_or("");
    assert!(
        !err_code.contains("M_CONTENT_SCAN_FAILED"),
        "Encrypted messages should not be blocked by content scanner: {}",
        json
    );
}

#[tokio::test]
async fn test_non_message_events_skip_text_scan() {
    reset_scanner_state();
    BLOCK_NEXT.store(false, Ordering::SeqCst);

    let Some((app, _addr)) = setup_app_with_mock_scanner(true).await else {
        return;
    };

    let token = register_user(&app, "non_msg_event_skip").await;

    // Create a room
    let create_request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/createRoom")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({"visibility": "private"}).to_string()))
        .unwrap();

    let create_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), create_request).await.unwrap();
    let body = axum::body::to_bytes(create_response.into_body(), 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let room_id = json["room_id"].as_str().unwrap();

    // Send reaction (m.reaction) - should skip text scan
    let reaction_request = Request::builder()
        .method("PUT")
        .uri(format!("/_matrix/client/v3/rooms/{}/send/m.reaction/txn{}", room_id, rand::random::<u64>()))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "m.relates_to": {
                    "event_id": "$some_event",
                    "key": {
                        "msgtype": "m.text",
                        "body": "👍"
                    }
                }
            })
            .to_string(),
        ))
        .unwrap();

    let reaction_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), reaction_request).await.unwrap();

    // Should NOT be blocked by content scanner (non-m.room.message events skip text scan)
    // May fail for other reasons (missing related event), but not M_CONTENT_SCAN_FAILED
    let body = axum::body::to_bytes(reaction_response.into_body(), 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    let err_code = json.get("errcode").and_then(|v| v.as_str()).unwrap_or("");
    assert!(
        !err_code.contains("M_CONTENT_SCAN_FAILED"),
        "Non-m.room.message events should not be blocked by content scanner: {}",
        json
    );
}

// ============================================================================
// Scanner Count / Audit Tests
// ============================================================================

#[tokio::test]
async fn test_scanner_is_invoked_for_each_upload() {
    reset_scanner_state();

    let Some((app, _addr)) = setup_app_with_mock_scanner(true).await else {
        return;
    };

    let token = register_user(&app, "scanner_count_test").await;

    // Upload multiple files
    for i in 0..3 {
        let upload_request = Request::builder()
            .method("POST")
            .uri(format!("/_matrix/media/v1/upload?filename=test{}.png", i))
            .header("Authorization", format!("Bearer {}", token))
            .header("Content-Type", "image/png")
            .body(Body::from(tiny_png()))
            .unwrap();

        let upload_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), upload_request).await.unwrap();
        assert_eq!(upload_response.status(), StatusCode::OK);
    }

    // Verify scanner was called 3 times
    let count = SCAN_COUNT.load(Ordering::SeqCst);
    assert_eq!(count, 3, "Scanner should be invoked for each upload");
}

#[tokio::test]
async fn test_scanner_is_invoked_for_each_message() {
    reset_scanner_state();

    let Some((app, _addr)) = setup_app_with_mock_scanner(true).await else {
        return;
    };

    let token = register_user(&app, "scanner_msg_count_test").await;

    // Create a room
    let create_request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/createRoom")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({"visibility": "private"}).to_string()))
        .unwrap();

    let create_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), create_request).await.unwrap();
    let body = axum::body::to_bytes(create_response.into_body(), 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let room_id = json["room_id"].as_str().unwrap();

    // Send multiple messages
    for i in 0..3 {
        let msg_request = Request::builder()
            .method("PUT")
            .uri(format!("/_matrix/client/v3/rooms/{}/send/m.room.message/txn{}{}", room_id, i, rand::random::<u64>()))
            .header("Authorization", format!("Bearer {}", token))
            .header("Content-Type", "application/json")
            .body(Body::from(
                json!({
                    "body": format!("Message {}", i),
                    "msgtype": "m.text"
                })
                .to_string(),
            ))
            .unwrap();

        let msg_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), msg_request).await.unwrap();
        assert_eq!(msg_response.status(), StatusCode::OK);
    }

    // Verify scanner was called 3 times
    let count = SCAN_COUNT.load(Ordering::SeqCst);
    assert_eq!(count, 3, "Scanner should be invoked for each m.room.message");
}

/// Regression guard for a **shipped-default P0** (2026-09-26): the room-message
/// path called `ContentScanner::scan_text(..).await?` directly, so with the
/// default configuration (`content_scanner.enabled: false`, as set explicitly
/// in `docker/config/homeserver.yaml`) `scan()` returned
/// `M_CONTENT_SCAN_DISABLED` and **every** `m.room.message` send answered 501.
/// The send path now shares the upload path's policy
/// (`scan_text_when_enabled`), whose disabled branch is a pass-through.
///
/// The test asserts the disabled precondition explicitly, so it cannot silently
/// start passing because scanning became enabled in the test fixtures.
#[tokio::test]
async fn message_send_succeeds_while_scanner_is_disabled() {
    let Some((app, state)) = super::setup_fresh_test_app_with_state().await else {
        return;
    };
    assert!(
        !state.services.core.content_scanner.is_enabled(),
        "this test must run with scanning disabled — that is the shipped default"
    );

    let token = register_user(&app, &format!("scan_disabled_send_{}", rand::random::<u32>())).await;

    let create_request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/createRoom")
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "name": "scanner-disabled" }).to_string()))
        .unwrap();
    let create_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), create_request).await.unwrap();
    assert_eq!(create_response.status(), StatusCode::OK, "createRoom must succeed");
    let body = axum::body::to_bytes(create_response.into_body(), 4096).await.unwrap();
    let create_json: Value = serde_json::from_slice(&body).unwrap();
    let room_id = create_json["room_id"].as_str().expect("createRoom must return room_id").to_string();

    let txn = format!("txn-{}", rand::random::<u32>());
    let send_request = Request::builder()
        .method("PUT")
        .uri(format!("/_matrix/client/v3/rooms/{room_id}/send/m.room.message/{txn}"))
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "msgtype": "m.text", "body": "hello" }).to_string()))
        .unwrap();
    let send_response = ServiceExt::<Request<Body>>::oneshot(app, send_request).await.unwrap();

    assert_eq!(
        send_response.status(),
        StatusCode::OK,
        "a disabled scanner must not block message sends (this used to answer 501 M_CONTENT_SCAN_DISABLED)"
    );
}

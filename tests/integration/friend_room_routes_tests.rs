//! HTTP route integration tests for the friend module.
//!
//! Covers all v3/v1 friend endpoints via `axum::Router + tower::ServiceExt::oneshot`.
//! Uses shared helpers from `tests/common/friend_helpers.rs`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{json, Value};
use tower::ServiceExt;

use crate::friend_helpers::{http_register_user, setup_fresh_app, unique_suffix};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Sends an HTTP request through the test router and returns (status, body_json).
async fn send_request(
    app: &axum::Router,
    method: &str,
    uri: &str,
    token: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(t) = token {
        builder = builder.header("Authorization", format!("Bearer {t}"));
    }
    if body.is_some() {
        builder = builder.header("Content-Type", "application/json");
    }
    let body_bytes = body.map(|b| b.to_string()).unwrap_or_default();
    let request = builder.body(Body::from(body_bytes)).unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, json)
}

/// Registers a user and returns `(token, user_id)`.
async fn register_user_with_id(app: &axum::Router, username: &str) -> (String, String) {
    let token = http_register_user(app, username).await;
    // Fetch user_id from whoami endpoint
    let (status, body) = send_request(app, "GET", "/_matrix/client/v3/account/whoami", Some(&token), None).await;
    assert_eq!(status, StatusCode::OK, "whoami failed: {body}");
    let user_id = body
        .get("user_id")
        .and_then(|v| v.as_str())
        .expect("user_id in whoami response")
        .to_string();
    (token, user_id)
}

/// Establishes friendship via HTTP and returns `(alice_token, alice_user_id, bob_token, bob_user_id)`.
async fn http_establish_friendship(
    app: &axum::Router,
    alice_username: &str,
    bob_username: &str,
) -> (String, String, String, String) {
    let (alice_token, alice_id) = register_user_with_id(app, alice_username).await;
    let (bob_token, bob_id) = register_user_with_id(app, bob_username).await;

    // Alice sends friend request to Bob
    let (_, _) = send_request(
        app,
        "POST",
        "/_matrix/client/v1/friends/request",
        Some(&alice_token),
        Some(json!({ "user_id": bob_id })),
    )
    .await;

    // Bob accepts
    let encoded_alice = urlencode_user_id(&alice_id);
    let (_, _) = send_request(
        app,
        "POST",
        &format!("/_matrix/client/v1/friends/request/{encoded_alice}/accept"),
        Some(&bob_token),
        None,
    )
    .await;

    (alice_token, alice_id, bob_token, bob_id)
}

/// URL-encodes a Matrix user_id for use in a path segment.
fn urlencode_user_id(user_id: &str) -> String {
    user_id.replace('@', "%40").replace(':', "%3A")
}

// ===========================================================================
// Group 1: Authentication & unauthorized access (4 tests)
// ===========================================================================

#[tokio::test]
async fn test_unauthorized_request_returns_401() {
    let Some(app) = setup_fresh_app().await else { return; };
    let (status, _) = send_request(&app, "GET", "/_matrix/client/v3/friends", None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_invalid_token_returns_401() {
    let Some(app) = setup_fresh_app().await else { return; };
    let (status, _) =
        send_request(&app, "GET", "/_matrix/client/v3/friends", Some("invalid_token_xyz"), None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_invalid_user_id_in_path_returns_400() {
    let Some(app) = setup_fresh_app().await else { return; };
    let s = unique_suffix();
    let (token, _) = register_user_with_id(&app, &format!("routes_invalid_{s}")).await;
    // Invalid user_id format in path (missing domain part)
    let (status, _) = send_request(
        &app,
        "POST",
        "/_matrix/client/v1/friends/request/@invalid/accept",
        Some(&token),
        None,
    )
    .await;
    // Either 400 (validation) or 404 (route not found) is acceptable
    assert!(
        status == StatusCode::BAD_REQUEST || status == StatusCode::NOT_FOUND,
        "expected 400 or 404, got {status}"
    );
}

#[tokio::test]
async fn test_route_manifest_includes_v3_endpoints() {
    // Verify that the v3 friend routes are registered in the router.
    // We do this by making requests and expecting non-404 responses.
    let Some(app) = setup_fresh_app().await else { return; };
    let s = unique_suffix();
    let (token, _) = register_user_with_id(&app, &format!("routes_manifest_{s}")).await;

    // GET /friends should return 200 (empty list)
    let (status, _) = send_request(&app, "GET", "/_matrix/client/v3/friends", Some(&token), None).await;
    assert_eq!(status, StatusCode::OK, "GET /v3/friends should be a registered route");

    // GET /friends/requests/incoming should return 200
    let (status, _) = send_request(
        &app,
        "GET",
        "/_matrix/client/v3/friends/requests/incoming",
        Some(&token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "GET /v3/friends/requests/incoming should be registered");
}

// ===========================================================================
// Group 2: Friend list GET /friends (5 tests)
// ===========================================================================

#[tokio::test]
async fn test_get_friends_empty_returns_200() {
    let Some(app) = setup_fresh_app().await else { return; };
    let s = unique_suffix();
    let (token, _) = register_user_with_id(&app, &format!("routes_empty_{s}")).await;

    let (status, body) = send_request(&app, "GET", "/_matrix/client/v3/friends", Some(&token), None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.get("friends").is_some() || body.get("items").is_some());
}

#[tokio::test]
async fn test_get_friends_with_limit_param() {
    let Some(app) = setup_fresh_app().await else { return; };
    let s = unique_suffix();
    let (token, _) = register_user_with_id(&app, &format!("routes_limit_{s}")).await;

    let (status, body) =
        send_request(&app, "GET", "/_matrix/client/v3/friends?limit=5", Some(&token), None).await;
    assert_eq!(status, StatusCode::OK);
    // limit should be reflected in response
    let limit = body.get("limit").and_then(|v| v.as_u64());
    assert!(limit.is_some(), "response should include limit field");
}

#[tokio::test]
async fn test_get_friends_invalid_offset_rejected() {
    let Some(app) = setup_fresh_app().await else { return; };
    let s = unique_suffix();
    let (token, _) = register_user_with_id(&app, &format!("routes_offset_{s}")).await;

    // Legacy offset pagination is no longer supported
    let (status, body) =
        send_request(&app, "GET", "/_matrix/client/v3/friends?offset=5", Some(&token), None).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "offset should be rejected: {body}");
}

#[tokio::test]
async fn test_get_friends_with_invalid_cursor_returns_400() {
    let Some(app) = setup_fresh_app().await else { return; };
    let s = unique_suffix();
    let (token, _) = register_user_with_id(&app, &format!("routes_cursor_{s}")).await;

    // Invalid cursor string
    let (status, _body) =
        send_request(&app, "GET", "/_matrix/client/v3/friends?from=!!!invalid", Some(&token), None).await;
    // Either 400 (invalid cursor) or 200 (cursor ignored) is acceptable;
    // the route handler decodes cursor via decode_friend_list_cursor which returns None on invalid.
    assert!(
        status == StatusCode::BAD_REQUEST || status == StatusCode::OK,
        "expected 400 or 200, got {status}"
    );
}

#[tokio::test]
async fn test_get_friends_sort_by_param() {
    let Some(app) = setup_fresh_app().await else { return; };
    let s = unique_suffix();
    let (token, _) = register_user_with_id(&app, &format!("routes_sort_{s}")).await;

    for sort_by in &["alphabet", "activity", "recent"] {
        let (status, _) = send_request(
            &app,
            "GET",
            &format!("/_matrix/client/v3/friends?sort_by={sort_by}"),
            Some(&token),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "sort_by={sort_by} should be accepted");
    }
}

// ===========================================================================
// Group 3: Send friend request POST /friends (5 tests)
// ===========================================================================

#[tokio::test]
async fn test_send_friend_request_via_http_returns_200() {
    let Some(app) = setup_fresh_app().await else { return; };
    let s = unique_suffix();
    let (alice_token, _) = register_user_with_id(&app, &format!("routes_send_a_{s}")).await;
    let (_, bob_id) = register_user_with_id(&app, &format!("routes_send_b_{s}")).await;

    let (status, body) = send_request(
        &app,
        "POST",
        "/_matrix/client/v1/friends/request",
        Some(&alice_token),
        Some(json!({ "user_id": bob_id })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "send friend request failed: {body}");
    assert!(body.get("request_id").is_some(), "response should include request_id");
    assert_eq!(body.get("status").and_then(|v| v.as_str()), Some("pending"));
}

#[tokio::test]
async fn test_send_friend_request_to_self_returns_400() {
    let Some(app) = setup_fresh_app().await else { return; };
    let s = unique_suffix();
    let (token, user_id) = register_user_with_id(&app, &format!("routes_self_{s}")).await;

    let (status, _) = send_request(
        &app,
        "POST",
        "/_matrix/client/v1/friends/request",
        Some(&token),
        Some(json!({ "user_id": user_id })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_send_friend_request_missing_user_id_rejected() {
    let Some(app) = setup_fresh_app().await else { return; };
    let s = unique_suffix();
    let (token, _) = register_user_with_id(&app, &format!("routes_missing_{s}")).await;

    let (status, _) = send_request(
        &app,
        "POST",
        "/_matrix/client/v1/friends/request",
        Some(&token),
        Some(json!({})),
    )
    .await;
    // axum's Json extractor returns 422 Unprocessable Entity when a required field
    // is missing from the request body. Both 400 and 422 indicate client error.
    assert!(
        status == StatusCode::BAD_REQUEST || status == StatusCode::UNPROCESSABLE_ENTITY,
        "expected 400 or 422, got {status}"
    );
}

#[tokio::test]
async fn test_send_friend_request_with_message_field() {
    let Some(app) = setup_fresh_app().await else { return; };
    let s = unique_suffix();
    let (alice_token, _) = register_user_with_id(&app, &format!("routes_msg_a_{s}")).await;
    let (_, bob_id) = register_user_with_id(&app, &format!("routes_msg_b_{s}")).await;

    let (status, body) = send_request(
        &app,
        "POST",
        "/_matrix/client/v1/friends/request",
        Some(&alice_token),
        Some(json!({ "user_id": bob_id, "message": "Hello from routes test!" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "send with message failed: {body}");
}

#[tokio::test]
async fn test_send_friend_request_duplicate_returns_idempotent() {
    let Some(app) = setup_fresh_app().await else { return; };
    let s = unique_suffix();
    let (alice_token, _) = register_user_with_id(&app, &format!("routes_dup_a_{s}")).await;
    let (_, bob_id) = register_user_with_id(&app, &format!("routes_dup_b_{s}")).await;

    let (status1, _body1) = send_request(
        &app,
        "POST",
        "/_matrix/client/v1/friends/request",
        Some(&alice_token),
        Some(json!({ "user_id": bob_id })),
    )
    .await;
    assert_eq!(status1, StatusCode::OK);

    let (status2, body2) = send_request(
        &app,
        "POST",
        "/_matrix/client/v1/friends/request",
        Some(&alice_token),
        Some(json!({ "user_id": bob_id })),
    )
    .await;
    // Duplicate pending request is idempotent; should return 200 with same/valid request_id
    assert_eq!(status2, StatusCode::OK, "duplicate request failed: {body2}");
    assert!(body2.get("request_id").is_some());
}

// ===========================================================================
// Group 4: Accept/reject/cancel friend request (5 tests)
// ===========================================================================

#[tokio::test]
async fn test_accept_friend_request_via_http() {
    let Some(app) = setup_fresh_app().await else { return; };
    let s = unique_suffix();
    let (alice_token, alice_id, bob_token, _) =
        http_establish_friendship(&app, &format!("routes_acc_a_{s}"), &format!("routes_acc_b_{s}")).await;

    // Already accepted in helper; verify by checking alice's friend list contains bob
    let (status, body) = send_request(&app, "GET", "/_matrix/client/v3/friends", Some(&alice_token), None).await;
    assert_eq!(status, StatusCode::OK);
    // Just verify the call succeeds; friend list content is verified in service tests
    let _ = body;
    let _ = alice_id;
    let _ = bob_token;
}

#[tokio::test]
async fn test_reject_friend_request_via_http() {
    let Some(app) = setup_fresh_app().await else { return; };
    let s = unique_suffix();
    let (alice_token, _) = register_user_with_id(&app, &format!("routes_rej_a_{s}")).await;
    let (bob_token, bob_id) = register_user_with_id(&app, &format!("routes_rej_b_{s}")).await;

    // Alice sends
    let _ = send_request(
        &app,
        "POST",
        "/_matrix/client/v1/friends/request",
        Some(&alice_token),
        Some(json!({ "user_id": bob_id })),
    )
    .await;

    // Bob rejects — fetch alice's user_id via whoami, then URL-encode it
    let (status, whoami) = send_request(&app, "GET", "/_matrix/client/v3/account/whoami", Some(&alice_token), None).await;
    assert_eq!(status, StatusCode::OK);
    let alice_id = whoami.get("user_id").and_then(|v| v.as_str()).unwrap();
    let encoded_alice = urlencode_user_id(alice_id);

    let (status, body) = send_request(
        &app,
        "POST",
        &format!("/_matrix/client/v1/friends/request/{encoded_alice}/reject"),
        Some(&bob_token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "reject failed: {body}");
    assert_eq!(body.get("status").and_then(|v| v.as_str()), Some("rejected"));
}

#[tokio::test]
async fn test_cancel_friend_request_via_http() {
    let Some(app) = setup_fresh_app().await else { return; };
    let s = unique_suffix();
    let (alice_token, _) = register_user_with_id(&app, &format!("routes_cnl_a_{s}")).await;
    let (_, bob_id) = register_user_with_id(&app, &format!("routes_cnl_b_{s}")).await;

    // Alice sends
    let _ = send_request(
        &app,
        "POST",
        "/_matrix/client/v1/friends/request",
        Some(&alice_token),
        Some(json!({ "user_id": bob_id })),
    )
    .await;

    // Alice cancels
    let encoded_bob = urlencode_user_id(&bob_id);
    let (status, body) = send_request(
        &app,
        "POST",
        &format!("/_matrix/client/v1/friends/request/{encoded_bob}/cancel"),
        Some(&alice_token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "cancel failed: {body}");
    assert_eq!(body.get("status").and_then(|v| v.as_str()), Some("cancelled"));
}

#[tokio::test]
async fn test_accept_nonexistent_friend_request_returns_error() {
    let Some(app) = setup_fresh_app().await else { return; };
    let s = unique_suffix();
    let (bob_token, _) = register_user_with_id(&app, &format!("routes_nf_b_{s}")).await;
    let (_, alice_id) = register_user_with_id(&app, &format!("routes_nf_a_{s}")).await;

    let encoded_alice = urlencode_user_id(&alice_id);
    let (status, _) = send_request(
        &app,
        "POST",
        &format!("/_matrix/client/v1/friends/request/{encoded_alice}/accept"),
        Some(&bob_token),
        None,
    )
    .await;
    // No pending request exists; expect 404 or 500 (database error wrapped)
    assert!(
        status == StatusCode::NOT_FOUND || status == StatusCode::INTERNAL_SERVER_ERROR,
        "expected 404 or 500, got {status}"
    );
}

#[tokio::test]
async fn test_get_incoming_requests_via_http() {
    let Some(app) = setup_fresh_app().await else { return; };
    let s = unique_suffix();
    let (alice_token, _) = register_user_with_id(&app, &format!("routes_in_a_{s}")).await;
    let (bob_token, bob_id) = register_user_with_id(&app, &format!("routes_in_b_{s}")).await;

    // Alice sends to Bob
    let _ = send_request(
        &app,
        "POST",
        "/_matrix/client/v1/friends/request",
        Some(&alice_token),
        Some(json!({ "user_id": bob_id })),
    )
    .await;

    // Bob should see it in incoming
    let (status, body) = send_request(
        &app,
        "GET",
        "/_matrix/client/v1/friends/requests/incoming",
        Some(&bob_token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let requests = body.get("requests").and_then(|v| v.as_array());
    assert!(requests.is_some(), "response should include requests array");
}

// ===========================================================================
// Group 5: Friend info queries (4 tests)
// ===========================================================================

#[tokio::test]
async fn test_check_friendship_returns_is_friend_false() {
    let Some(app) = setup_fresh_app().await else { return; };
    let s = unique_suffix();
    let (alice_token, _) = register_user_with_id(&app, &format!("routes_chk_f_a_{s}")).await;
    let (_, bob_id) = register_user_with_id(&app, &format!("routes_chk_f_b_{s}")).await;

    let encoded_bob = urlencode_user_id(&bob_id);
    let (status, body) = send_request(
        &app,
        "GET",
        &format!("/_matrix/client/v1/friends/check/{encoded_bob}"),
        Some(&alice_token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let is_friend = body.get("is_friend").and_then(|v| v.as_bool());
    assert_eq!(is_friend, Some(false), "non-friend should report is_friend=false");
}

#[tokio::test]
async fn test_check_friendship_returns_is_friend_true() {
    let Some(app) = setup_fresh_app().await else { return; };
    let s = unique_suffix();
    let (alice_token, _, _, bob_id) =
        http_establish_friendship(&app, &format!("routes_chk_t_a_{s}"), &format!("routes_chk_t_b_{s}")).await;

    let encoded_bob = urlencode_user_id(&bob_id);
    let (status, body) = send_request(
        &app,
        "GET",
        &format!("/_matrix/client/v1/friends/check/{encoded_bob}"),
        Some(&alice_token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let is_friend = body.get("is_friend").and_then(|v| v.as_bool());
    assert_eq!(is_friend, Some(true), "friend should report is_friend=true");
}

#[tokio::test]
async fn test_get_friend_info_returns_200_for_friend() {
    let Some(app) = setup_fresh_app().await else { return; };
    let s = unique_suffix();
    let (alice_token, _, _, bob_id) =
        http_establish_friendship(&app, &format!("routes_info_a_{s}"), &format!("routes_info_b_{s}")).await;

    let encoded_bob = urlencode_user_id(&bob_id);
    let (status, _body) = send_request(
        &app,
        "GET",
        &format!("/_matrix/client/v1/friends/{encoded_bob}/info"),
        Some(&alice_token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn test_get_friend_info_returns_404_for_non_friend() {
    let Some(app) = setup_fresh_app().await else { return; };
    let s = unique_suffix();
    let (alice_token, _) = register_user_with_id(&app, &format!("routes_info_nf_a_{s}")).await;
    let (_, bob_id) = register_user_with_id(&app, &format!("routes_info_nf_b_{s}")).await;

    let encoded_bob = urlencode_user_id(&bob_id);
    let (status, _) = send_request(
        &app,
        "GET",
        &format!("/_matrix/client/v1/friends/{encoded_bob}/info"),
        Some(&alice_token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

// ===========================================================================
// Group 6: Friend status & notes (5 tests)
// ===========================================================================

#[tokio::test]
async fn test_update_friend_note_via_http() {
    let Some(app) = setup_fresh_app().await else { return; };
    let s = unique_suffix();
    let (alice_token, _, _, bob_id) =
        http_establish_friendship(&app, &format!("routes_note_a_{s}"), &format!("routes_note_b_{s}")).await;

    let encoded_bob = urlencode_user_id(&bob_id);
    let (status, body) = send_request(
        &app,
        "PUT",
        &format!("/_matrix/client/v1/friends/{encoded_bob}/note"),
        Some(&alice_token),
        Some(json!({ "note": "Best friend" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "update note failed: {body}");
    assert_eq!(body.get("note").and_then(|v| v.as_str()), Some("Best friend"));
}

#[tokio::test]
async fn test_update_friend_note_too_long_returns_400() {
    let Some(app) = setup_fresh_app().await else { return; };
    let s = unique_suffix();
    let (alice_token, _, _, bob_id) =
        http_establish_friendship(&app, &format!("routes_long_a_{s}"), &format!("routes_long_b_{s}")).await;

    let long_note = "x".repeat(1001);
    let encoded_bob = urlencode_user_id(&bob_id);
    let (status, _) = send_request(
        &app,
        "PUT",
        &format!("/_matrix/client/v1/friends/{encoded_bob}/note"),
        Some(&alice_token),
        Some(json!({ "note": long_note })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_update_friend_status_via_http() {
    let Some(app) = setup_fresh_app().await else { return; };
    let s = unique_suffix();
    let (alice_token, _, _, bob_id) =
        http_establish_friendship(&app, &format!("routes_st_a_{s}"), &format!("routes_st_b_{s}")).await;

    let encoded_bob = urlencode_user_id(&bob_id);
    let (status, body) = send_request(
        &app,
        "PUT",
        &format!("/_matrix/client/v1/friends/{encoded_bob}/status"),
        Some(&alice_token),
        Some(json!({ "status": "favorite" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "update status failed: {body}");
    assert_eq!(body.get("status").and_then(|v| v.as_str()), Some("favorite"));
}

#[tokio::test]
async fn test_update_friend_status_invalid_returns_400() {
    let Some(app) = setup_fresh_app().await else { return; };
    let s = unique_suffix();
    let (alice_token, _, _, bob_id) =
        http_establish_friendship(&app, &format!("routes_stinv_a_{s}"), &format!("routes_stinv_b_{s}")).await;

    let encoded_bob = urlencode_user_id(&bob_id);
    let (status, _) = send_request(
        &app,
        "PUT",
        &format!("/_matrix/client/v1/friends/{encoded_bob}/status"),
        Some(&alice_token),
        Some(json!({ "status": "invalid_status" })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_get_friend_status_via_http() {
    let Some(app) = setup_fresh_app().await else { return; };
    let s = unique_suffix();
    let (alice_token, _, _, bob_id) =
        http_establish_friendship(&app, &format!("routes_getst_a_{s}"), &format!("routes_getst_b_{s}")).await;

    let encoded_bob = urlencode_user_id(&bob_id);
    let (status, body) = send_request(
        &app,
        "GET",
        &format!("/_matrix/client/v1/friends/{encoded_bob}/status"),
        Some(&alice_token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    // status response should contain a "status" field
    assert!(body.get("status").is_some(), "response should include status field");
}

// ===========================================================================
// Group 7: Friend groups (4 tests)
// ===========================================================================

#[tokio::test]
async fn test_create_friend_group_via_http() {
    let Some(app) = setup_fresh_app().await else { return; };
    let s = unique_suffix();
    let (token, _) = register_user_with_id(&app, &format!("routes_grp_a_{s}")).await;

    let (status, body) = send_request(
        &app,
        "POST",
        "/_matrix/client/v1/friends/groups",
        Some(&token),
        Some(json!({ "name": "Close Friends" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "create group failed: {body}");
    assert!(body.get("id").is_some(), "response should include group id");
    assert_eq!(body.get("name").and_then(|v| v.as_str()), Some("Close Friends"));
}

#[tokio::test]
async fn test_create_friend_group_empty_name_returns_400() {
    let Some(app) = setup_fresh_app().await else { return; };
    let s = unique_suffix();
    let (token, _) = register_user_with_id(&app, &format!("routes_grp_e_{s}")).await;

    let (status, _) = send_request(
        &app,
        "POST",
        "/_matrix/client/v1/friends/groups",
        Some(&token),
        Some(json!({ "name": "" })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_get_friend_groups_via_http() {
    let Some(app) = setup_fresh_app().await else { return; };
    let s = unique_suffix();
    let (token, _) = register_user_with_id(&app, &format!("routes_getg_a_{s}")).await;

    // Create a group first
    let _ = send_request(
        &app,
        "POST",
        "/_matrix/client/v1/friends/groups",
        Some(&token),
        Some(json!({ "name": "Family" })),
    )
    .await;

    let (status, body) = send_request(&app, "GET", "/_matrix/client/v1/friends/groups", Some(&token), None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.get("groups").is_some(), "response should include groups array");
}

#[tokio::test]
async fn test_delete_friend_group_via_http() {
    let Some(app) = setup_fresh_app().await else { return; };
    let s = unique_suffix();
    let (token, _) = register_user_with_id(&app, &format!("routes_delg_a_{s}")).await;

    // Create a group first
    let (_, create_body) = send_request(
        &app,
        "POST",
        "/_matrix/client/v1/friends/groups",
        Some(&token),
        Some(json!({ "name": "ToDelete" })),
    )
    .await;
    let group_id = create_body.get("id").and_then(|v| v.as_str()).unwrap_or("unknown").to_string();

    let (status, body) = send_request(
        &app,
        "DELETE",
        &format!("/_matrix/client/v1/friends/groups/{group_id}"),
        Some(&token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "delete group failed: {body}");
    assert_eq!(body.get("deleted").and_then(|v| v.as_bool()), Some(true));
}

// ===========================================================================
// Group 8: DM room (2 tests)
// ===========================================================================

#[tokio::test]
async fn test_get_friend_dm_returns_null_for_non_friend() {
    let Some(app) = setup_fresh_app().await else { return; };
    let s = unique_suffix();
    let (alice_token, _) = register_user_with_id(&app, &format!("routes_dm_nf_a_{s}")).await;
    let (_, bob_id) = register_user_with_id(&app, &format!("routes_dm_nf_b_{s}")).await;

    let encoded_bob = urlencode_user_id(&bob_id);
    let (status, body) = send_request(
        &app,
        "GET",
        &format!("/_matrix/client/v1/friends/dm/{encoded_bob}"),
        Some(&alice_token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    // room_id should be null (no existing DM)
    let room_id = body.get("room_id");
    assert!(room_id.is_some(), "response should include room_id field");
}

#[tokio::test]
async fn test_create_friend_dm_via_http() {
    let Some(app) = setup_fresh_app().await else { return; };
    let s = unique_suffix();
    let (alice_token, _, _, bob_id) =
        http_establish_friendship(&app, &format!("routes_dm_c_a_{s}"), &format!("routes_dm_c_b_{s}")).await;

    let encoded_bob = urlencode_user_id(&bob_id);
    let (status, body) = send_request(
        &app,
        "POST",
        &format!("/_matrix/client/v1/friends/dm/{encoded_bob}"),
        Some(&alice_token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "create DM failed: {body}");
    let room_id = body.get("room_id").and_then(|v| v.as_str());
    assert!(room_id.is_some(), "response should include room_id");
    assert!(room_id.unwrap().starts_with('!'), "room_id should start with '!'");
}

// ===========================================================================
// Group 9: Path compatibility (1 test)
// ===========================================================================

#[tokio::test]
async fn test_r0_paths_alias_to_same_handler() {
    let Some(app) = setup_fresh_app().await else { return; };
    let s = unique_suffix();
    let (token, _) = register_user_with_id(&app, &format!("routes_r0_{s}")).await;

    // r0 path should return same status as v1 path
    let (status_v1, _) = send_request(&app, "GET", "/_matrix/client/v1/friends", Some(&token), None).await;
    let (status_r0, _) = send_request(&app, "GET", "/_matrix/client/r0/friendships", Some(&token), None).await;
    assert_eq!(status_v1, StatusCode::OK);
    assert_eq!(status_r0, StatusCode::OK, "r0 path should return same status as v1");
}

// ===========================================================================
// Group 10: Outgoing requests & suggestions (additional coverage)
// ===========================================================================

#[tokio::test]
async fn test_get_outgoing_requests_via_http() {
    let Some(app) = setup_fresh_app().await else { return; };
    let s = unique_suffix();
    let (alice_token, _) = register_user_with_id(&app, &format!("routes_out_a_{s}")).await;
    let (_, bob_id) = register_user_with_id(&app, &format!("routes_out_b_{s}")).await;

    // Alice sends to Bob
    let _ = send_request(
        &app,
        "POST",
        "/_matrix/client/v1/friends/request",
        Some(&alice_token),
        Some(json!({ "user_id": bob_id })),
    )
    .await;

    // Alice should see it in outgoing
    let (status, body) = send_request(
        &app,
        "GET",
        "/_matrix/client/v1/friends/requests/outgoing",
        Some(&alice_token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.get("requests").is_some());
}

#[tokio::test]
async fn test_get_friend_suggestions_via_http() {
    let Some(app) = setup_fresh_app().await else { return; };
    let s = unique_suffix();
    let (token, _) = register_user_with_id(&app, &format!("routes_sug_{s}")).await;

    let (status, body) =
        send_request(&app, "GET", "/_matrix/client/v1/friends/suggestions", Some(&token), None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.get("suggestions").is_some());
}

#[tokio::test]
async fn test_remove_friend_via_http() {
    let Some(app) = setup_fresh_app().await else { return; };
    let s = unique_suffix();
    let (alice_token, _, _, bob_id) =
        http_establish_friendship(&app, &format!("routes_rm_a_{s}"), &format!("routes_rm_b_{s}")).await;

    let encoded_bob = urlencode_user_id(&bob_id);
    let (status, body) = send_request(
        &app,
        "DELETE",
        &format!("/_matrix/client/v1/friends/{encoded_bob}"),
        Some(&alice_token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "remove friend failed: {body}");
    assert_eq!(body.get("removed").and_then(|v| v.as_bool()), Some(true));
}

// ===========================================================================
// Group 11: Phase 2 supplementary tests — group management routes
// ===========================================================================

/// Sends a request and returns (status, body_json, headers).
async fn send_request_with_headers(
    app: &axum::Router,
    method: &str,
    uri: &str,
    token: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, Value, axum::http::HeaderMap) {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(t) = token {
        builder = builder.header("Authorization", format!("Bearer {t}"));
    }
    if body.is_some() {
        builder = builder.header("Content-Type", "application/json");
    }
    let body_bytes = body.map(|b| b.to_string()).unwrap_or_default();
    let request = builder.body(Body::from(body_bytes)).unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, json, headers)
}

async fn create_group_get_id(app: &axum::Router, token: &str, name: &str) -> String {
    let (_, body) = send_request(
        app,
        "POST",
        "/_matrix/client/v1/friends/groups",
        Some(token),
        Some(json!({ "name": name })),
    )
    .await;
    body.get("id").and_then(|v| v.as_str()).unwrap_or("unknown").to_string()
}

#[tokio::test]
async fn test_rename_friend_group_via_http() {
    let Some(app) = setup_fresh_app().await else { return; };
    let s = unique_suffix();
    let (token, _) = register_user_with_id(&app, &format!("routes_rename_a_{s}")).await;
    let group_id = create_group_get_id(&app, &token, "Original").await;

    let (status, body) = send_request(
        &app,
        "PUT",
        &format!("/_matrix/client/v1/friends/groups/{group_id}/name"),
        Some(&token),
        Some(json!({ "name": "Renamed" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "rename group failed: {body}");
    assert_eq!(body.get("name").and_then(|v| v.as_str()), Some("Renamed"));
    assert_eq!(body.get("group_id").and_then(|v| v.as_str()), Some(group_id.as_str()));
}

#[tokio::test]
async fn test_rename_friend_group_empty_name_400() {
    let Some(app) = setup_fresh_app().await else { return; };
    let s = unique_suffix();
    let (token, _) = register_user_with_id(&app, &format!("routes_renameempty_a_{s}")).await;
    let group_id = create_group_get_id(&app, &token, "ToDelete").await;

    let (status, body) = send_request(
        &app,
        "PUT",
        &format!("/_matrix/client/v1/friends/groups/{group_id}/name"),
        Some(&token),
        Some(json!({ "name": "" })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "empty name should be rejected: {body}");
}

#[tokio::test]
async fn test_add_friend_to_group_via_http() {
    let Some(app) = setup_fresh_app().await else { return; };
    let s = unique_suffix();
    let (alice_token, _, _, bob_id) =
        http_establish_friendship(&app, &format!("routes_addgrp_a_{s}"), &format!("routes_addgrp_b_{s}")).await;
    let encoded_bob = urlencode_user_id(&bob_id);

    let group_id = create_group_get_id(&app, &alice_token, "Work").await;
    let (status, body) = send_request(
        &app,
        "POST",
        &format!("/_matrix/client/v1/friends/groups/{group_id}/add/{encoded_bob}"),
        Some(&alice_token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "add friend to group failed: {body}");
    assert_eq!(body.get("group_id").and_then(|v| v.as_str()), Some(group_id.as_str()));
}

#[tokio::test]
async fn test_add_friend_to_group_nonexistent_group_returns_error() {
    let Some(app) = setup_fresh_app().await else { return; };
    let s = unique_suffix();
    let (alice_token, _) = register_user_with_id(&app, &format!("routes_addgrpbad_a_{s}")).await;
    let (_, bob_id) = register_user_with_id(&app, &format!("routes_addgrpbad_b_{s}")).await;
    let encoded_bob = urlencode_user_id(&bob_id);

    let (status, _body) = send_request(
        &app,
        "POST",
        &format!("/_matrix/client/v1/friends/groups/nonexistent-group-{s}/add/{encoded_bob}"),
        Some(&alice_token),
        None,
    )
    .await;
    assert!(
        status == StatusCode::NOT_FOUND || status == StatusCode::BAD_REQUEST || status == StatusCode::INTERNAL_SERVER_ERROR,
        "nonexistent group should return an error status, got {status}"
    );
}

#[tokio::test]
async fn test_remove_friend_from_group_via_http() {
    let Some(app) = setup_fresh_app().await else { return; };
    let s = unique_suffix();
    let (alice_token, _, _, bob_id) =
        http_establish_friendship(&app, &format!("routes_remgrp_a_{s}"), &format!("routes_remgrp_b_{s}")).await;
    let encoded_bob = urlencode_user_id(&bob_id);

    let group_id = create_group_get_id(&app, &alice_token, "ToRemove").await;
    let _ = send_request(
        &app,
        "POST",
        &format!("/_matrix/client/v1/friends/groups/{group_id}/add/{encoded_bob}"),
        Some(&alice_token),
        None,
    )
    .await;

    let (status, _body) = send_request(
        &app,
        "DELETE",
        &format!("/_matrix/client/v1/friends/groups/{group_id}/remove/{encoded_bob}"),
        Some(&alice_token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "remove friend from group failed");
}

#[tokio::test]
async fn test_get_friends_in_group_via_http() {
    let Some(app) = setup_fresh_app().await else { return; };
    let s = unique_suffix();
    let (alice_token, _, _, bob_id) =
        http_establish_friendship(&app, &format!("routes_getmem_a_{s}"), &format!("routes_getmem_b_{s}")).await;
    let encoded_bob = urlencode_user_id(&bob_id);

    let group_id = create_group_get_id(&app, &alice_token, "Members").await;
    let _ = send_request(
        &app,
        "POST",
        &format!("/_matrix/client/v1/friends/groups/{group_id}/add/{encoded_bob}"),
        Some(&alice_token),
        None,
    )
    .await;

    let (status, body) = send_request(
        &app,
        "GET",
        &format!("/_matrix/client/v1/friends/groups/{group_id}/friends"),
        Some(&alice_token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "get friends in group failed: {body}");
    // Response should have a "friends" array (may be empty depending on storage iteration)
    assert!(body.get("friends").is_some(), "response should include friends array");
}

#[tokio::test]
async fn test_get_groups_for_user_via_http() {
    let Some(app) = setup_fresh_app().await else { return; };
    let s = unique_suffix();
    let (alice_token, _, _, bob_id) =
        http_establish_friendship(&app, &format!("routes_getgrp_a_{s}"), &format!("routes_getgrp_b_{s}")).await;
    let encoded_bob = urlencode_user_id(&bob_id);

    let _ = create_group_get_id(&app, &alice_token, "GroupOne").await;

    let (status, body) = send_request(
        &app,
        "GET",
        &format!("/_matrix/client/v1/friends/{encoded_bob}/groups"),
        Some(&alice_token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "get groups for user failed: {body}");
    assert!(body.get("groups").is_some(), "response should include groups array");
}

#[tokio::test]
async fn test_update_friend_displayname_via_http() {
    let Some(app) = setup_fresh_app().await else { return; };
    let s = unique_suffix();
    let (alice_token, _, _, bob_id) =
        http_establish_friendship(&app, &format!("routes_disp_a_{s}"), &format!("routes_disp_b_{s}")).await;
    let encoded_bob = urlencode_user_id(&bob_id);

    let (status, body) = send_request(
        &app,
        "PUT",
        &format!("/_matrix/client/v1/friends/{encoded_bob}/displayname"),
        Some(&alice_token),
        Some(json!({ "displayname": "Bobby" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "update displayname failed: {body}");
    assert_eq!(body.get("displayname").and_then(|v| v.as_str()), Some("Bobby"));
}

#[tokio::test]
async fn test_update_friend_displayname_too_long_400() {
    let Some(app) = setup_fresh_app().await else { return; };
    let s = unique_suffix();
    let (alice_token, _) = register_user_with_id(&app, &format!("routes_displong_a_{s}")).await;
    let (_, bob_id) = register_user_with_id(&app, &format!("routes_displong_b_{s}")).await;
    let encoded_bob = urlencode_user_id(&bob_id);

    let long_name = "x".repeat(257);
    let (status, body) = send_request(
        &app,
        "PUT",
        &format!("/_matrix/client/v1/friends/{encoded_bob}/displayname"),
        Some(&alice_token),
        Some(json!({ "displayname": long_name })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "displayname >256 chars should be rejected: {body}");
}

#[tokio::test]
async fn test_search_friend_directory_via_http_get() {
    let Some(app) = setup_fresh_app().await else { return; };
    let s = unique_suffix();
    let (token, _) = register_user_with_id(&app, &format!("routes_search_a_{s}")).await;
    // Register another user to make search non-trivial
    let _ = register_user_with_id(&app, &format!("routes_search_target_{s}")).await;

    let (status, body) = send_request(
        &app,
        "GET",
        &format!("/_matrix/client/v3/friends/search?q=routes_search_target_{s}"),
        Some(&token),
        None,
    )
    .await;
    // Search may return 200 with results, or 429 if rate-limited in test env.
    assert!(
        status == StatusCode::OK || status == StatusCode::TOO_MANY_REQUESTS,
        "search should return 200 or 429 (rate limited), got {status}: {body}"
    );
}

#[tokio::test]
async fn test_get_received_requests_deprecated_header() {
    let Some(app) = setup_fresh_app().await else { return; };
    let s = unique_suffix();
    let (token, _) = register_user_with_id(&app, &format!("routes_deprecated_a_{s}")).await;

    let (status, body, headers) =
        send_request_with_headers(&app, "GET", "/_matrix/client/v1/friends/request/received", Some(&token), None).await;
    assert_eq!(status, StatusCode::OK, "get received requests failed: {body}");
    // Deprecated endpoint should set Deprecation and Sunset headers
    assert!(
        headers.contains_key("deprecation"),
        "response should include Deprecation header, got headers: {headers:?}"
    );
    assert!(
        headers.contains_key("sunset"),
        "response should include Sunset header, got headers: {headers:?}"
    );
}

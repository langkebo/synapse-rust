//! MSC4140 — delayed **state** event scheduling round trips.
//!
//! The message (`/send`) delay path was the only one wired to `state_key: None`;
//! these tests pin down the state write paths (`PUT /state/...` variants) so a
//! delayed state event carries its real `state_key` into `delayed_events` and the
//! management endpoint reports it back verbatim.

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use std::sync::Arc;
use tower::ServiceExt;

async fn setup_test_app_with_pool() -> Option<(axum::Router, Arc<sqlx::PgPool>)> {
    let (app, pool, _) = super::setup_fresh_test_app_with_pool().await?;
    Some((app, pool))
}

async fn whoami(app: &axum::Router, token: &str) -> String {
    let request = Request::builder()
        .method("GET")
        .uri("/_matrix/client/v3/account/whoami")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(super::with_local_connect_info(request)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    json["user_id"].as_str().unwrap().to_string()
}

async fn create_room(app: &axum::Router, token: &str) -> String {
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/createRoom")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "name": "Delayed State Room" }).to_string()))
        .unwrap();

    let response = app.clone().oneshot(super::with_local_connect_info(request)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    json["room_id"].as_str().unwrap().to_string()
}

async fn put_state(app: &axum::Router, token: &str, uri: String, body: Value) -> (StatusCode, Value) {
    let request = Request::builder()
        .method("PUT")
        .uri(uri)
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();

    let response = app.clone().oneshot(super::with_local_connect_info(request)).await.unwrap();
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 4096).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap_or_else(|_| json!({}));
    (status, json)
}

async fn get_delayed_event(app: &axum::Router, token: &str, delay_id: i64) -> (StatusCode, Value) {
    let request = Request::builder()
        .method("GET")
        .uri(format!("/_matrix/client/unstable/org.matrix.msc4140/delayed_events/{}", delay_id))
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(super::with_local_connect_info(request)).await.unwrap();
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 4096).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap_or_else(|_| json!({}));
    (status, json)
}

/// `PUT /state/{type}/` (empty state_key route) schedules a delayed event whose
/// `state_key` is the empty string and whose content has the delay hint stripped.
#[tokio::test]
async fn test_delayed_state_event_empty_key_round_trips() {
    let Some((app, pool)) = setup_test_app_with_pool().await else {
        eprintln!("Skipping test: database not available");
        return;
    };

    let token = super::create_test_user(&app).await;
    let room_id = create_room(&app, &token).await;

    let (status, body) = put_state(
        &app,
        &token,
        format!("/_matrix/client/v3/rooms/{}/state/m.room.topic/", room_id),
        json!({ "topic": "Delayed Topic", "org.matrix.msc4140.delay": 60_000 }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {body}");
    let delay_id = body["delay_id"].as_i64().expect("delay_id must be present");

    // The management endpoint reflects the real state_key and the stripped content.
    let (get_status, event) = get_delayed_event(&app, &token, delay_id).await;
    assert_eq!(get_status, StatusCode::OK, "body: {event}");
    assert_eq!(event["type"], "m.room.topic");
    assert_eq!(event["state_key"], "");
    assert_eq!(event["content"]["topic"], "Delayed Topic");
    assert!(event["content"].get("org.matrix.msc4140.delay").is_none(), "delay hint must be stripped");

    // The persisted row carries the empty state_key (not NULL).
    let state_key: Option<String> = sqlx::query_scalar("SELECT state_key FROM delayed_events WHERE id = $1")
        .bind(delay_id)
        .fetch_one(&*pool)
        .await
        .unwrap();
    assert_eq!(state_key.as_deref(), Some(""));
}

/// `PUT /state/{type}/{state_key}` (explicit key route) preserves the caller's
/// explicit state_key for a sender-owned state event.
#[tokio::test]
async fn test_delayed_state_event_explicit_state_key_round_trips() {
    let Some((app, _pool)) = setup_test_app_with_pool().await else {
        eprintln!("Skipping test: database not available");
        return;
    };

    let token = super::create_test_user(&app).await;
    let user_id = whoami(&app, &token).await;
    let room_id = create_room(&app, &token).await;

    let (status, body) = put_state(
        &app,
        &token,
        format!("/_matrix/client/v3/rooms/{}/state/m.custom.delayed/{}", room_id, user_id),
        json!({ "value": 42, "org.matrix.msc4140.delay": 60_000 }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {body}");
    let delay_id = body["delay_id"].as_i64().expect("delay_id must be present");

    let (get_status, event) = get_delayed_event(&app, &token, delay_id).await;
    assert_eq!(get_status, StatusCode::OK, "body: {event}");
    assert_eq!(event["type"], "m.custom.delayed");
    assert_eq!(event["state_key"], user_id);
    assert_eq!(event["content"]["value"], 42);
    assert!(event["content"].get("org.matrix.msc4140.delay").is_none(), "delay hint must be stripped");
}

/// `POST /state/{type}` (send_state_event) computes the state_key per event type;
/// a global state event must be scheduled with the empty state_key.
#[tokio::test]
async fn test_delayed_state_event_send_route_computes_empty_state_key() {
    let Some((app, _pool)) = setup_test_app_with_pool().await else {
        eprintln!("Skipping test: database not available");
        return;
    };

    let token = super::create_test_user(&app).await;
    let room_id = create_room(&app, &token).await;

    let request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/v3/rooms/{}/state/m.room.topic", room_id))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "topic": "Posted Topic", "org.matrix.msc4140.delay": 60_000 }).to_string()))
        .unwrap();

    let response = app.clone().oneshot(super::with_local_connect_info(request)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 4096).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let delay_id = json["delay_id"].as_i64().expect("delay_id must be present");

    let (get_status, event) = get_delayed_event(&app, &token, delay_id).await;
    assert_eq!(get_status, StatusCode::OK, "body: {event}");
    assert_eq!(event["type"], "m.room.topic");
    assert_eq!(event["state_key"], "");
}

/// A non-positive or over-24h delay on the state path is rejected (400) rather
/// than silently scheduled.
#[tokio::test]
async fn test_delayed_state_event_rejects_invalid_delay() {
    let Some((app, _pool)) = setup_test_app_with_pool().await else {
        eprintln!("Skipping test: database not available");
        return;
    };

    let token = super::create_test_user(&app).await;
    let room_id = create_room(&app, &token).await;
    let uri = format!("/_matrix/client/v3/rooms/{}/state/m.room.topic/", room_id);

    let (zero_status, _) =
        put_state(&app, &token, uri.clone(), json!({ "topic": "T", "org.matrix.msc4140.delay": 0 })).await;
    assert_eq!(zero_status, StatusCode::BAD_REQUEST);

    let (too_long_status, _) =
        put_state(&app, &token, uri, json!({ "topic": "T", "org.matrix.msc4140.delay": 86_400_001 })).await;
    assert_eq!(too_long_status, StatusCode::BAD_REQUEST);
}

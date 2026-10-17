//! MSC4354 — the sticky-duration hint on the **`/state`** write surface.
//!
//! Upstream reads `org.matrix.msc4354.sticky_duration_ms` from the query string of
//! both `/send` and `/state`, and records the resolved TTL on the event's
//! `unsigned` projection (`msc4354_sticky_duration_ttl_ms`) instead of dropping it.
//!
//! `/send` has had three tests since 2026-10-16 (see
//! `api_delayed_event_schedule_tests.rs`). The `/state` side was wired later — four
//! write handlers in `state.rs`, all sharing the same parser — and shipped without
//! any. This file is that guard.
//!
//! It deliberately exercises **all three** state write shapes rather than one:
//! `PUT /state/{type}`, `PUT /state/{type}/` (empty key) and `PUT
//! /state/{type}/{state_key}` are separate handlers, and a hint honoured on one
//! door but dropped on another is exactly the half-wired shape that a
//! single-path test would pass straight through. The negative controls matter as
//! much: a TTL that appears *without* the hint (or after a rejected hint) would
//! be the same bug wearing the opposite mask.

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;

/// The namespaced spelling upstream reads; the short alias is the fallback the
/// server's own pin routes have always accepted.
const STICKY_FIELD: &str = "org.matrix.msc4354.sticky_duration_ms";
const STICKY_SHORT_FIELD: &str = "sticky_duration_ms";
/// The `unsigned` key the resolved TTL is written to (outside the signed bytes,
/// so writing it after persistence cannot invalidate the event's signature).
const TTL_FIELD: &str = "msc4354_sticky_duration_ttl_ms";

async fn create_public_room(app: &axum::Router, token: &str) -> String {
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/createRoom")
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({ "name": "Sticky State Room", "preset": "public_chat", "visibility": "public" }).to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK, "createRoom must succeed");
    let body = axum::body::to_bytes(response.into_body(), 8192).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    json["room_id"].as_str().expect("createRoom must return a room_id").to_string()
}

/// `PUT /rooms/{roomId}/state/{state_path}{query}` — `state_path` deliberately
/// includes the shape (`m.room.topic`, `m.room.topic/`, `com.example.pin/pin-1`)
/// because the three shapes route to different handlers.
async fn put_state(
    app: &axum::Router,
    token: &str,
    room_id: &str,
    state_path: &str,
    query: &str,
    content: Value,
) -> (StatusCode, Value) {
    let request = Request::builder()
        .method("PUT")
        .uri(format!("/_matrix/client/v3/rooms/{room_id}/state/{state_path}{query}"))
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(content.to_string()))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or_else(|_| json!({})))
}

/// `GET /rooms/{roomId}/state/{type}` — 404 when that state was never written,
/// which is how these tests prove a rejected write did not persist.
async fn get_state(app: &axum::Router, token: &str, room_id: &str, event_type: &str) -> (StatusCode, Value) {
    let request = Request::builder()
        .method("GET")
        .uri(format!("/_matrix/client/v3/rooms/{room_id}/state/{event_type}"))
        .header("Authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or_else(|_| json!({})))
}

/// `GET /rooms/{roomId}/event/{eventId}` — the read path a client uses to see the
/// `unsigned` block of what it just wrote.
async fn get_room_event(app: &axum::Router, token: &str, room_id: &str, event_id: &str) -> (StatusCode, Value) {
    let request = Request::builder()
        .method("GET")
        .uri(format!("/_matrix/client/v3/rooms/{room_id}/event/{event_id}"))
        .header("Authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or_else(|_| json!({})))
}

/// The hint on `PUT /state/{type}` (no state key) must land on `unsigned`.
///
/// Control: the *same* route without the hint must not grow a TTL — so the
/// assertion is about the hint, not about state events carrying one by accident.
#[tokio::test]
async fn state_sticky_duration_hint_is_recorded_on_unsigned() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let alice = super::create_test_user(&app).await;
    let room_id = create_public_room(&app, &alice).await;

    let (status, body) = put_state(
        &app,
        &alice,
        &room_id,
        "m.room.topic",
        &format!("?{STICKY_FIELD}=60000"),
        json!({ "topic": "sticky topic" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "a valid sticky hint must not fail the state write: {body}");
    let event_id = body["event_id"].as_str().expect("a state write must return an event_id").to_string();

    let (status, event) = get_room_event(&app, &alice, &room_id, &event_id).await;
    assert_eq!(status, StatusCode::OK, "the written state event must be readable: {event}");
    assert_eq!(
        event["unsigned"][TTL_FIELD].as_i64(),
        Some(60_000),
        "the requested sticky TTL must round-trip through `unsigned`, got {event}"
    );

    let (status, body) = put_state(&app, &alice, &room_id, "m.room.name", "", json!({ "name": "plain" })).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let plain_id = body["event_id"].as_str().expect("a state write must return an event_id").to_string();
    let (status, plain) = get_room_event(&app, &alice, &room_id, &plain_id).await;
    assert_eq!(status, StatusCode::OK, "{plain}");
    assert!(
        plain["unsigned"].get(TTL_FIELD).is_none(),
        "state written without the hint must not carry a sticky TTL: {plain}"
    );
}

/// The other two state write shapes — the empty-key path (`/state/{type}/`) and
/// the keyed path (`/state/{type}/{state_key}`) — are separate handlers; both
/// must honour the hint, and the short alias must be accepted by them.
#[tokio::test]
async fn state_sticky_duration_is_recorded_on_every_write_shape() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let alice = super::create_test_user(&app).await;
    let room_id = create_public_room(&app, &alice).await;

    // Shape 2: empty state key, short alias.
    let (status, body) = put_state(
        &app,
        &alice,
        &room_id,
        "m.room.topic/",
        &format!("?{STICKY_SHORT_FIELD}=120000"),
        json!({ "topic": "sticky topic (empty key)" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "the empty-key path must accept the hint: {body}");
    let empty_key_id = body["event_id"].as_str().expect("a state write must return an event_id").to_string();
    let (status, event) = get_room_event(&app, &alice, &room_id, &empty_key_id).await;
    assert_eq!(status, StatusCode::OK, "{event}");
    assert_eq!(
        event["unsigned"][TTL_FIELD].as_i64(),
        Some(120_000),
        "the empty-key path must record the TTL, got {event}"
    );

    // Shape 3: explicit state key, namespaced spelling.
    let (status, body) = put_state(
        &app,
        &alice,
        &room_id,
        "com.example.pin/pin-1",
        &format!("?{STICKY_FIELD}=30000"),
        json!({ "pinned": true }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "the keyed path must accept the hint: {body}");
    let keyed_id = body["event_id"].as_str().expect("a state write must return an event_id").to_string();
    let (status, event) = get_room_event(&app, &alice, &room_id, &keyed_id).await;
    assert_eq!(status, StatusCode::OK, "{event}");
    assert_eq!(event["unsigned"][TTL_FIELD].as_i64(), Some(30_000), "the keyed path must record the TTL, got {event}");
    assert_eq!(event["state_key"].as_str(), Some("pin-1"), "the keyed path must keep its state key: {event}");
}

/// An out-of-range hint is a **client error before persistence**, not a silent
/// clamp and not a persisted event with an impossible TTL.
///
/// The upper bound is upstream's `StickyEvent.MAX_DURATION` (1 hour); the 404 on
/// the follow-up read is the proof that nothing was written.
#[tokio::test]
async fn state_sticky_duration_hint_out_of_range_is_rejected() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let alice = super::create_test_user(&app).await;
    let room_id = create_public_room(&app, &alice).await;

    let (status, body) = put_state(
        &app,
        &alice,
        &room_id,
        "m.room.topic",
        &format!("?{STICKY_FIELD}=3600001"),
        json!({ "topic": "too long" }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "an over-long duration must be refused: {body}");
    assert_eq!(body["errcode"].as_str(), Some("M_INVALID_PARAM"), "the refusal must name the parameter, got {body}");

    let (status, state) = get_state(&app, &alice, &room_id, "m.room.topic").await;
    assert_eq!(status, StatusCode::NOT_FOUND, "a refused write must not persist the state event, got {state}");
}

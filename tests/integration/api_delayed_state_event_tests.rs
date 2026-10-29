//! MSC4140 — delayed event scheduling round trips.
//!
//! Two things are pinned down here:
//!
//! 1. **The state write paths** (`/state/...` variants). The message (`/send`)
//!    delay path was the only one wired to `state_key: None`, so a delayed state
//!    event would have lost its `state_key`; these tests assert it reaches
//!    `delayed_events` and comes back off the management endpoint verbatim.
//! 2. **Where the delay hint is read from.** The specification — and upstream
//!    Synapse — put `org.matrix.msc4140.delay` in the **query string**; this
//!    server only ever read a body field of the same name. A spec-conformant
//!    client's `?org.matrix.msc4140.delay=30000` was therefore ignored and the
//!    event went out immediately, while the client still believed it could
//!    cancel it. The `query_*` tests below drive that form end to end; the older
//!    tests in this file still use the body field and are the regression guard
//!    for the fallback.

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

/// The `delay_ms` actually stored for a scheduled event.
///
/// This is the only observable that distinguishes *which* hint was applied, so
/// it is what makes "the query string wins over the body field" a real assertion
/// rather than a re-reading of the request back to itself.
async fn persisted_delay_ms(pool: &sqlx::PgPool, delay_id: i64) -> i64 {
    sqlx::query_scalar("SELECT delay_ms FROM delayed_events WHERE id = $1")
        .bind(delay_id)
        .fetch_one(pool)
        .await
        .expect("a scheduled delayed event must have a row")
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

// ============================================================================
// The delay hint's *source*: query string (spec) first, body field (legacy)
// ============================================================================

/// The hint arrives as a **query parameter** — the spec location — with no body
/// field at all ⇒ the event must be scheduled rather than sent.
#[tokio::test]
async fn test_query_delay_schedules_a_state_event() {
    let Some((app, pool)) = setup_test_app_with_pool().await else {
        eprintln!("Skipping test: database not available");
        return;
    };

    let token = super::create_test_user(&app).await;
    let room_id = create_room(&app, &token).await;

    let (status, body) = put_state(
        &app,
        &token,
        format!("/_matrix/client/v3/rooms/{room_id}/state/m.room.topic/?org.matrix.msc4140.delay=60000"),
        json!({ "topic": "Query Topic" }),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "a query-string delay must be accepted (body: {body})");
    let delay_id =
        body["delay_id"].as_i64().expect("a query-string delay hint must schedule the event instead of sending it");
    assert_eq!(persisted_delay_ms(&pool, delay_id).await, 60_000);

    // It really is pending, and the hint did not leak into the event content.
    let (get_status, event) = get_delayed_event(&app, &token, delay_id).await;
    assert_eq!(get_status, StatusCode::OK, "body: {event}");
    assert_eq!(event["type"], "m.room.topic");
    assert_eq!(event["state_key"], "");
    assert_eq!(event["content"]["topic"], "Query Topic");
    assert!(event["content"].get("org.matrix.msc4140.delay").is_none(), "the hint is transport-level only");
}

/// The `/send` path reads the same parameter. Before this change it had no
/// `Query` extractor at all, so the hint was invisible there in principle.
#[tokio::test]
async fn test_query_delay_schedules_a_message_event() {
    let Some((app, pool)) = setup_test_app_with_pool().await else {
        eprintln!("Skipping test: database not available");
        return;
    };

    let token = super::create_test_user(&app).await;
    let room_id = create_room(&app, &token).await;

    let request = Request::builder()
        .method("PUT")
        .uri(format!(
            "/_matrix/client/v3/rooms/{room_id}/send/m.room.message/query-delay-txn?org.matrix.msc4140.delay=30000"
        ))
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "msgtype": "m.text", "body": "delayed" }).to_string()))
        .unwrap();

    let response = app.clone().oneshot(super::with_local_connect_info(request)).await.unwrap();
    let status = response.status();
    let raw = axum::body::to_bytes(response.into_body(), 4096).await.unwrap();
    let body: Value = serde_json::from_slice(&raw).unwrap_or_else(|_| json!({}));

    assert_eq!(status, StatusCode::OK, "body: {body}");
    let delay_id = body["delay_id"].as_i64().expect("the query-string hint must delay the send");
    assert_eq!(persisted_delay_ms(&pool, delay_id).await, 30_000);
}

/// Both spellings present ⇒ the query string wins. The stored `delay_ms` is what
/// proves which hint was applied; asserting on the response alone would only
/// re-read the request back to itself.
#[tokio::test]
async fn test_query_delay_wins_over_the_legacy_body_field() {
    let Some((app, pool)) = setup_test_app_with_pool().await else {
        eprintln!("Skipping test: database not available");
        return;
    };

    let token = super::create_test_user(&app).await;
    let room_id = create_room(&app, &token).await;

    let (status, body) = put_state(
        &app,
        &token,
        format!("/_matrix/client/v3/rooms/{room_id}/state/m.room.topic/?org.matrix.msc4140.delay=60000"),
        json!({ "topic": "Both", "org.matrix.msc4140.delay": 1_000 }),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "body: {body}");
    let delay_id = body["delay_id"].as_i64().expect("delay_id must be present");
    assert_eq!(
        persisted_delay_ms(&pool, delay_id).await,
        60_000,
        "the query-string delay must win over the body field"
    );
}

/// The `> 0` and `≤ 24h` bounds still apply when the value comes from the query
/// string, and a non-integer value is a 400 `M_INVALID_PARAM` rather than a
/// silent immediate send.
#[tokio::test]
async fn test_query_delay_is_validated() {
    let Some((app, _pool)) = setup_test_app_with_pool().await else {
        eprintln!("Skipping test: database not available");
        return;
    };

    let token = super::create_test_user(&app).await;
    let room_id = create_room(&app, &token).await;
    let base = format!("/_matrix/client/v3/rooms/{room_id}/state/m.room.topic/");

    for (raw, why) in [
        ("abc", "non-integer"),
        ("", "empty value"),
        ("0", "zero"),
        ("-1", "negative"),
        ("86400001", "over the 24h bound"),
    ] {
        let (status, body) =
            put_state(&app, &token, format!("{base}?org.matrix.msc4140.delay={raw}"), json!({ "topic": "T" })).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "delay={raw:?} ({why}) must be rejected — body: {body}");
        if raw == "abc" {
            assert_eq!(body["errcode"], "M_INVALID_PARAM", "a malformed query value is M_INVALID_PARAM upstream too");
        }
    }

    // Control: a valid value on the same route still schedules. Without it the
    // loop above would pass just as well against an endpoint that rejects
    // everything.
    let (ok_status, ok_body) =
        put_state(&app, &token, format!("{base}?org.matrix.msc4140.delay=1000"), json!({ "topic": "T" })).await;
    assert_eq!(ok_status, StatusCode::OK, "body: {ok_body}");
    assert!(ok_body["delay_id"].is_i64(), "a valid query delay must still schedule: {ok_body}");
}

/// No hint anywhere ⇒ immediate send. This guards the *opposite* direction from
/// every test above: if anything ever started scheduling unconditionally, those
/// tests would still be green and only this one would notice.
#[tokio::test]
async fn test_absent_delay_hint_still_sends_immediately() {
    let Some((app, _pool)) = setup_test_app_with_pool().await else {
        eprintln!("Skipping test: database not available");
        return;
    };

    let token = super::create_test_user(&app).await;
    let room_id = create_room(&app, &token).await;

    let (status, body) = put_state(
        &app,
        &token,
        format!("/_matrix/client/v3/rooms/{room_id}/state/m.room.topic/"),
        json!({ "topic": "Immediate" }),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert!(body.get("delay_id").is_none(), "nothing may be scheduled without a hint: {body}");
    assert!(body["event_id"].is_string(), "the event must be sent now: {body}");
}

// ============================================================================
// The delayed surface is a *second door* to the same write — and it is the
// sharpest one, because the background dispatcher replays the event later with
// no further authorization. The `m.room.member` target check therefore has to
// run at *scheduling* time, not only on the immediate `/state` write.
// ============================================================================

/// A PL-50 moderator schedules a delayed `ban` of the room creator. The
/// scheduling request itself must be refused: if it were accepted, the ban
/// would land minutes later, from a task nobody is watching, and the target
/// check on the immediate path would never see it.
#[tokio::test]
async fn test_delayed_membership_write_is_authorized_before_scheduling() {
    let Some((app, _pool)) = setup_test_app_with_pool().await else {
        eprintln!("Skipping test: database not available");
        return;
    };

    let owner_token = super::create_test_user(&app).await;
    let owner_id = whoami(&app, &owner_token).await;
    let mod_token = super::create_test_user(&app).await;
    let mod_id = whoami(&app, &mod_token).await;
    let room_id = create_room(&app, &owner_token).await;

    let invite = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/v3/rooms/{room_id}/invite"))
        .header("Authorization", format!("Bearer {owner_token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "user_id": mod_id }).to_string()))
        .unwrap();
    let response = app.clone().oneshot(super::with_local_connect_info(invite)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let join = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/v3/rooms/{room_id}/join"))
        .header("Authorization", format!("Bearer {mod_token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({}).to_string()))
        .unwrap();
    let response = app.clone().oneshot(super::with_local_connect_info(join)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK, "the invited moderator must be able to join");

    // Give the moderator exactly `state_default` (50): the plain power-level gate
    // lets them write member state, so only the target check can stop them.
    let mut users = serde_json::Map::new();
    users.insert(mod_id.clone(), json!(50));
    let (status, body) = put_state(
        &app,
        &owner_token,
        format!("/_matrix/client/v3/rooms/{room_id}/state/m.room.power_levels/"),
        json!({
            "users": users,
            "state_default": 50,
            "ban": 50,
            "kick": 50,
            "invite": 0
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "granting the moderator 50 must succeed: {body}");

    let (status, body) = put_state(
        &app,
        &mod_token,
        format!("/_matrix/client/v3/rooms/{room_id}/state/m.room.member/{owner_id}?org.matrix.msc4140.delay=60000"),
        json!({ "membership": "ban" }),
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN, "a delayed ban of the creator must be refused up front: {body}");
    assert_eq!(body["errcode"], json!("M_FORBIDDEN"), "error code: {body}");
    assert!(body.get("delay_id").is_none(), "nothing may be queued for the dispatcher: {body}");
}

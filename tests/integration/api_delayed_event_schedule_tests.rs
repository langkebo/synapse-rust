//! MSC4140 — the **dedicated** scheduling endpoint.
//!
//! `PUT /_matrix/client/unstable/org.matrix.msc4140/rooms/{roomId}/delayed_event/{eventType}/{txnId}`
//!
//! Upstream added this endpoint (deprecating, without removing, the `delay`
//! query parameter on `/send` and `/state`) so a client can schedule without
//! overloading an ordinary send. It is the same feature through a second door,
//! which is exactly why it needs its own guards: a door that is *nearly* the
//! same is where authorization quietly goes missing.
//!
//! The point these tests defend: **scheduling is the only authorization a
//! delayed event ever gets.** The dispatcher (`src/server/mod.rs`) replays a due
//! row through `room_service` without re-checking anything, so whatever is not
//! refused here is written later, by a background task, with no caller to blame.

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;

const MSG_DELAY_MS: i64 = 60_000;

async fn create_public_room(app: &axum::Router, token: &str) -> String {
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/createRoom")
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({ "name": "Delayed Schedule Room", "preset": "public_chat", "visibility": "public" }).to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK, "createRoom must succeed");
    let body = axum::body::to_bytes(response.into_body(), 8192).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    json["room_id"].as_str().expect("createRoom must return a room_id").to_string()
}

/// The caller's `user_id`, so a test can count that user's scheduled rows.
async fn whoami_user_id(app: &axum::Router, token: &str) -> String {
    let request = Request::builder()
        .method("GET")
        .uri("/_matrix/client/v3/account/whoami")
        .header("Authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK, "whoami must succeed");
    let bytes = axum::body::to_bytes(response.into_body(), 4096).await.unwrap();
    let json: Value = serde_json::from_slice(&bytes).unwrap();
    json["user_id"].as_str().expect("whoami must return a user_id").to_string()
}

async fn join_room(app: &axum::Router, token: &str, room_id: &str) {
    let request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/v3/rooms/{room_id}/join"))
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({}).to_string()))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK, "join must succeed");
}

/// The dedicated endpoint, verbatim. Returns the status and the JSON body.
async fn schedule(
    app: &axum::Router,
    token: &str,
    room_id: &str,
    event_type: &str,
    txn_id: &str,
    body: Value,
) -> (StatusCode, Value) {
    let request = Request::builder()
        .method("PUT")
        .uri(format!("/_matrix/client/unstable/org.matrix.msc4140/rooms/{room_id}/delayed_event/{event_type}/{txn_id}"))
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or_else(|_| json!({})))
}

/// `PUT /send/...` without any delay hint, returning the raw body so a test can
/// assert what is *absent* as well as what is present.
async fn send_immediately(
    app: &axum::Router,
    token: &str,
    room_id: &str,
    txn_id: &str,
    text: &str,
) -> (StatusCode, Value) {
    let request = Request::builder()
        .method("PUT")
        .uri(format!("/_matrix/client/v3/rooms/{room_id}/send/m.room.message/{txn_id}"))
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "msgtype": "m.text", "body": text }).to_string()))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or_else(|_| json!({})))
}

async fn get_delayed(app: &axum::Router, token: &str, delay_id: i64) -> (StatusCode, Value) {
    let request = Request::builder()
        .method("GET")
        .uri(format!("/_matrix/client/unstable/org.matrix.msc4140/delayed_events/{delay_id}"))
        .header("Authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or_else(|_| json!({})))
}

fn delay_id_of(body: &Value) -> i64 {
    body.get("delay_id")
        .and_then(Value::as_i64)
        .unwrap_or_else(|| panic!("response must carry a numeric delay_id, got {body}"))
}

async fn stored_row_count(pool: &sqlx::PgPool, room_id: &str, user_id: &str, event_type: &str) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM delayed_events WHERE room_id = $1 AND user_id = $2 AND event_type = $3")
        .bind(room_id)
        .bind(user_id)
        .bind(event_type)
        .fetch_one(pool)
        .await
        .expect("delayed_events must be queryable")
}

/// The scheduled row must be readable back verbatim through the management
/// endpoint, and the id handed out must be the one that endpoint accepts — a
/// string id would make this endpoint's own answer unusable.
#[tokio::test]
async fn dedicated_endpoint_schedules_a_message_event() {
    let Some((app, pool, _)) = super::setup_fresh_test_app_with_pool().await else {
        return;
    };
    let alice = super::create_test_user(&app).await;
    let alice_id = whoami_user_id(&app, &alice).await;
    let room_id = create_public_room(&app, &alice).await;

    let (status, body) = schedule(
        &app,
        &alice,
        &room_id,
        "m.room.message",
        "dedicated-1",
        json!({ "delay_ms": MSG_DELAY_MS, "content": { "msgtype": "m.text", "body": "later" } }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "scheduling must succeed: {body}");
    let delay_id = delay_id_of(&body);

    let (status, stored) = get_delayed(&app, &alice, delay_id).await;
    assert_eq!(status, StatusCode::OK, "the management endpoint must accept the id we just returned");
    assert_eq!(stored["delay_id"].as_i64(), Some(delay_id));
    assert_eq!(stored["room_id"].as_str(), Some(room_id.as_str()));
    assert_eq!(stored["type"].as_str(), Some("m.room.message"));
    assert_eq!(stored["delay_ms"].as_i64(), Some(MSG_DELAY_MS));
    assert_eq!(stored["content"]["body"].as_str(), Some("later"), "the content must survive intact");
    assert_eq!(
        stored_row_count(&pool, &room_id, &alice_id, "m.room.message").await,
        1,
        "exactly one row must be scheduled"
    );
}

/// A `state_key` selects the state write path — and the event must carry the key
/// into storage, because that is what makes the dispatcher replay it as a state
/// event rather than as a message.
#[tokio::test]
async fn dedicated_endpoint_carries_the_state_key_into_the_stored_row() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let alice = super::create_test_user(&app).await;
    let room_id = create_public_room(&app, &alice).await;

    let (status, body) = schedule(
        &app,
        &alice,
        &room_id,
        "m.room.topic",
        "dedicated-state-1",
        json!({ "delay_ms": MSG_DELAY_MS, "content": { "topic": "later" }, "state_key": "" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "the creator may schedule a state event: {body}");

    let (status, stored) = get_delayed(&app, &alice, delay_id_of(&body)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(stored["type"].as_str(), Some("m.room.topic"));
    assert_eq!(
        stored["state_key"].as_str(),
        Some(""),
        "the state key must reach delayed_events, otherwise the dispatcher replays it as a message"
    );

    // Control: without `state_key` the same call is a message event, and the
    // management response must NOT carry a `state_key` field at all.
    let (status, body) = schedule(
        &app,
        &alice,
        &room_id,
        "m.room.topic",
        "dedicated-state-2",
        json!({ "delay_ms": MSG_DELAY_MS, "content": { "topic": "later" } }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (_, stored) = get_delayed(&app, &alice, delay_id_of(&body)).await;
    assert!(
        stored.get("state_key").is_none(),
        "an absent state_key must stay absent, not become an empty one: {stored}"
    );
}

/// A retry of the same `(room, txn_id)` must return the *same* `delay_id` and
/// must not schedule a second event — the client's retry is not a second intent.
#[tokio::test]
async fn dedicated_endpoint_replays_the_same_delay_id_for_the_same_txn_id() {
    let Some((app, pool, _)) = super::setup_fresh_test_app_with_pool().await else {
        return;
    };
    let alice = super::create_test_user(&app).await;
    let alice_id = whoami_user_id(&app, &alice).await;
    let room_id = create_public_room(&app, &alice).await;
    let request_body = json!({ "delay_ms": MSG_DELAY_MS, "content": { "msgtype": "m.text", "body": "later" } });

    let (status, first) = schedule(&app, &alice, &room_id, "m.room.message", "retry-txn", request_body.clone()).await;
    assert_eq!(status, StatusCode::OK, "{first}");

    let (status, second) = schedule(&app, &alice, &room_id, "m.room.message", "retry-txn", request_body).await;
    assert_eq!(status, StatusCode::OK, "{second}");
    assert_eq!(
        delay_id_of(&second),
        delay_id_of(&first),
        "a replay of the same txn_id must be answered with the first delay_id"
    );
    assert_eq!(
        stored_row_count(&pool, &room_id, &alice_id, "m.room.message").await,
        1,
        "a replay must not schedule a second event"
    );
}

/// The dedicated endpoint and `/send` are different operations on the same
/// `(room, txn_id)` coordinate, so their idempotency caches must not meet: a
/// client retrying one must never be handed the other's answer.
#[tokio::test]
async fn dedicated_endpoint_and_send_keep_separate_idempotency_namespaces() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let alice = super::create_test_user(&app).await;
    let room_id = create_public_room(&app, &alice).await;

    // Direction 1: schedule first, then a plain `/send` reusing the txn_id. The
    // send must go out (`event_id`), not be answered with the delay_id.
    let (status, scheduled) = schedule(
        &app,
        &alice,
        &room_id,
        "m.room.message",
        "shared-txn",
        json!({ "delay_ms": MSG_DELAY_MS, "content": { "msgtype": "m.text", "body": "later" } }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{scheduled}");
    assert!(delay_id_of(&scheduled) > 0);

    let (status, sent) = send_immediately(&app, &alice, &room_id, "shared-txn", "immediate").await;
    assert_eq!(status, StatusCode::OK, "{sent}");
    assert!(sent.get("event_id").is_some(), "the plain send must produce an event_id, got {sent}");
    assert!(
        sent.get("delay_id").is_none(),
        "the scheduling call's delay_id must not be replayed to a plain send: {sent}"
    );

    // Direction 2: send first, then schedule reusing the txn_id. The scheduling
    // call must schedule, not echo the earlier event_id.
    let (status, sent) = send_immediately(&app, &alice, &room_id, "shared-txn-2", "immediate").await;
    assert_eq!(status, StatusCode::OK, "{sent}");
    let (status, scheduled) = schedule(
        &app,
        &alice,
        &room_id,
        "m.room.message",
        "shared-txn-2",
        json!({ "delay_ms": MSG_DELAY_MS, "content": { "msgtype": "m.text", "body": "later" } }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{scheduled}");
    assert!(
        scheduled.get("delay_id").is_some(),
        "the scheduling call must not be answered with the earlier send's event_id: {scheduled}"
    );
}

/// The gate that matters: a member who may post messages but may not write this
/// state event must not be able to *schedule* it either.
///
/// The dispatcher re-checks nothing when the row comes due, so a version of this
/// endpoint that only asked "are you a member?" would let an ordinary member
/// queue a `m.room.power_levels` write and have a background task perform it.
#[tokio::test]
async fn dedicated_endpoint_refuses_a_state_event_the_caller_may_not_write() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let alice = super::create_test_user(&app).await;
    let room_id = create_public_room(&app, &alice).await;

    let bob = super::create_test_user(&app).await;
    join_room(&app, &bob, &room_id).await;

    let escalation = json!({
        "delay_ms": MSG_DELAY_MS,
        "content": { "users": { "": 100 } },
        "state_key": ""
    });

    let (status, body) = schedule(&app, &bob, &room_id, "m.room.power_levels", "bob-pl", escalation.clone()).await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "an ordinary member must not be able to schedule a power-levels write: {body}"
    );

    // Control 1: the same member *can* schedule a message — so the refusal above
    // is about the state gate, not about Bob being blocked from the room.
    let (status, body) = schedule(
        &app,
        &bob,
        &room_id,
        "m.room.message",
        "bob-msg",
        json!({ "delay_ms": MSG_DELAY_MS, "content": { "msgtype": "m.text", "body": "hi" } }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "a member may schedule a message: {body}");

    // Control 2: the room creator may schedule the very same state event.
    let (status, body) = schedule(&app, &alice, &room_id, "m.room.power_levels", "alice-pl", escalation).await;
    assert_eq!(status, StatusCode::OK, "the creator must be able to schedule it: {body}");
}

/// Non-members and unknown rooms are refused before anything is stored.
#[tokio::test]
async fn dedicated_endpoint_refuses_non_members_and_unknown_rooms() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let alice = super::create_test_user(&app).await;
    let room_id = create_public_room(&app, &alice).await;
    let stranger = super::create_test_user(&app).await;

    let body = json!({ "delay_ms": MSG_DELAY_MS, "content": { "msgtype": "m.text", "body": "hi" } });

    let (status, response) = schedule(&app, &stranger, &room_id, "m.room.message", "stranger-1", body.clone()).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "a non-member must be refused: {response}");

    let (status, response) =
        schedule(&app, &alice, "!doesnotexist:localhost", "m.room.message", "missing-room", body.clone()).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "an unknown room must be 404: {response}");

    // Control: the same body from a member of the real room succeeds, so both
    // refusals above are about who/where and not about the body.
    let (status, response) = schedule(&app, &alice, &room_id, "m.room.message", "alice-ok", body).await;
    assert_eq!(status, StatusCode::OK, "{response}");
}

/// MSC4354's sticky duration is refused with 501 rather than silently dropped:
/// this server pins sticky events through a separate per-user endpoint, so the
/// field cannot be honoured, and a client that asked for a pinned event must not
/// be handed an unpinned one that looks like success.
#[tokio::test]
async fn dedicated_endpoint_refuses_the_sticky_duration_hint() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let alice = super::create_test_user(&app).await;
    let room_id = create_public_room(&app, &alice).await;

    let (status, body) = schedule(
        &app,
        &alice,
        &room_id,
        "m.room.message",
        "sticky-hint",
        json!({
            "delay_ms": MSG_DELAY_MS,
            "content": { "msgtype": "m.text", "body": "later" },
            "org.matrix.msc4354.sticky_duration_ms": 60_000
        }),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_IMPLEMENTED, "the sticky hint must be refused loudly: {body}");
    assert_eq!(body["errcode"].as_str(), Some("M_UNKNOWN"), "501 has no dedicated Matrix errcode: {body}");

    // Control: the same request without the hint succeeds. This is what makes the
    // assertion above about the field rather than about the request being broken.
    let (status, body) = schedule(
        &app,
        &alice,
        &room_id,
        "m.room.message",
        "sticky-hint-control",
        json!({ "delay_ms": MSG_DELAY_MS, "content": { "msgtype": "m.text", "body": "later" } }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
}

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

/// `PUT /send/...` carrying MSC4354's sticky-duration hint in the query string,
/// exactly as a spec-conformant client sends it. Returns the status and body so
/// the caller can read the `event_id` back out.
async fn send_with_sticky_hint(
    app: &axum::Router,
    token: &str,
    room_id: &str,
    txn_id: &str,
    hint: &str,
    duration_ms: i64,
) -> (StatusCode, Value) {
    let request = Request::builder()
        .method("PUT")
        .uri(format!("/_matrix/client/v3/rooms/{room_id}/send/m.room.message/{txn_id}?{hint}={duration_ms}"))
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "msgtype": "m.text", "body": "sticky" }).to_string()))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or_else(|_| json!({})))
}

/// `GET /rooms/{roomId}/event/{eventId}` — the read path a client uses to see the
/// `unsigned` block of an event it just sent.
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

/// MSC4354 (V-19-9): the `/send` sticky-duration hint must be *recorded*, not
/// dropped.
///
/// Upstream reads `org.matrix.msc4354.sticky_duration_ms` from `/send`'s query
/// string and writes the resolved TTL into the event's `unsigned` as
/// `msc4354_sticky_duration_ttl_ms`, so a client can read back how long its
/// message stays pinned. This server historically routed sticky through a
/// separate per-user endpoint; the two models coexist, and this test pins down
/// the upstream one: after the send, the number a conformant client reads must
/// be the number it asked for.
#[tokio::test]
async fn send_sticky_duration_hint_is_recorded_on_unsigned() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let alice = super::create_test_user(&app).await;
    let room_id = create_public_room(&app, &alice).await;

    let (status, body) =
        send_with_sticky_hint(&app, &alice, &room_id, "sticky-send-1", "org.matrix.msc4354.sticky_duration_ms", 60_000)
            .await;
    assert_eq!(status, StatusCode::OK, "a valid sticky hint must not fail the send: {body}");
    let event_id = body["event_id"].as_str().expect("a send must return an event_id").to_string();

    let (status, event) = get_room_event(&app, &alice, &room_id, &event_id).await;
    assert_eq!(status, StatusCode::OK, "the sent event must be readable: {event}");
    assert_eq!(
        event["unsigned"]["msc4354_sticky_duration_ttl_ms"].as_i64(),
        Some(60_000),
        "the requested sticky TTL must round-trip through `unsigned`, got {event}"
    );

    // Control: the same send *without* the hint must not grow a sticky TTL — so
    // the assertion above is about the hint, not about every event carrying one.
    let (status, body) = send_immediately(&app, &alice, &room_id, "sticky-send-control", "plain").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let plain_id = body["event_id"].as_str().expect("a send must return an event_id").to_string();
    let (status, plain) = get_room_event(&app, &alice, &room_id, &plain_id).await;
    assert_eq!(status, StatusCode::OK, "{plain}");
    assert!(
        plain["unsigned"].get("msc4354_sticky_duration_ttl_ms").is_none(),
        "an event sent without the hint must not carry a sticky TTL: {plain}"
    );
}

/// The short (non-namespaced) spelling is accepted as an alias, and the
/// namespaced spelling wins when both are present — the same precedence the
/// server's own sticky routes have always used.
#[tokio::test]
async fn send_sticky_duration_short_alias_is_accepted() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let alice = super::create_test_user(&app).await;
    let room_id = create_public_room(&app, &alice).await;

    let (status, body) =
        send_with_sticky_hint(&app, &alice, &room_id, "sticky-alias", "sticky_duration_ms", 30_000).await;
    assert_eq!(status, StatusCode::OK, "the short alias must be honoured: {body}");
    let event_id = body["event_id"].as_str().expect("a send must return an event_id").to_string();

    let (status, event) = get_room_event(&app, &alice, &room_id, &event_id).await;
    assert_eq!(status, StatusCode::OK, "{event}");
    assert_eq!(
        event["unsigned"]["msc4354_sticky_duration_ttl_ms"].as_i64(),
        Some(30_000),
        "the short alias must resolve to the same `unsigned` field: {event}"
    );
}

/// The bound is enforced *before* anything is written: a duration over one hour
/// (or non-positive, or not a number) is refused with 400 `M_INVALID_PARAM`,
/// never silently clamped or dropped.
#[tokio::test]
async fn send_sticky_duration_hint_out_of_range_is_rejected() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let alice = super::create_test_user(&app).await;
    let room_id = create_public_room(&app, &alice).await;

    // 1 ms over the one-hour cap.
    let (status, body) = send_with_sticky_hint(
        &app,
        &alice,
        &room_id,
        "sticky-too-long",
        "org.matrix.msc4354.sticky_duration_ms",
        3_600_001,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "an over-long duration must be refused: {body}");
    assert_eq!(body["errcode"].as_str(), Some("M_INVALID_PARAM"), "{body}");

    // Zero is not a duration.
    let (status, body) =
        send_with_sticky_hint(&app, &alice, &room_id, "sticky-zero", "org.matrix.msc4354.sticky_duration_ms", 0).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "a non-positive duration must be refused: {body}");
    assert_eq!(body["errcode"].as_str(), Some("M_INVALID_PARAM"), "{body}");

    // Control: the cap itself is allowed — the refusal above is about being
    // strictly over the limit, not about the limit being off by one.
    let (status, body) = send_with_sticky_hint(
        &app,
        &alice,
        &room_id,
        "sticky-at-cap",
        "org.matrix.msc4354.sticky_duration_ms",
        3_600_000,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "the cap itself must be accepted: {body}");
}

// ─────────────────────────────────────────────────────────────────────────────
// §11.7.1: the three convenience sub-paths + the list endpoint
//
// Upstream serves these from their own servlets (`CancelDelayedEventServlet`,
// `RestartDelayedEventServlet`, `SendDelayedEventServlet`, `DelayedEventsServlet`
// in `synapse/rest/client/delayed_events.py`, release-v1.161). They are the same
// operations as `POST …/{delay_id}` with an `action` body, so the thing worth
// pinning is that the extra doors **do not skip the ownership check** and that
// the list cannot show anyone else's rows.
// ─────────────────────────────────────────────────────────────────────────────

/// `POST …/delayed_events/{delay_id}/{action}` — no body, matching upstream.
async fn manage_via_subpath(app: &axum::Router, token: &str, delay_id: i64, action: &str) -> (StatusCode, Value) {
    let request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/unstable/org.matrix.msc4140/delayed_events/{delay_id}/{action}"))
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(Body::from("{}"))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or_else(|_| json!({})))
}

/// `GET …/delayed_events`.
async fn list_delayed(app: &axum::Router, token: &str) -> (StatusCode, Value) {
    let request = Request::builder()
        .method("GET")
        .uri("/_matrix/client/unstable/org.matrix.msc4140/delayed_events")
        .header("Authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or_else(|_| json!({})))
}

/// The `delay_id`s in a list response.
fn listed_delay_ids(body: &Value) -> Vec<i64> {
    body.get("delayed_events")
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(|item| item.get("delay_id").and_then(Value::as_i64)).collect())
        .unwrap_or_default()
}

#[tokio::test]
async fn convenience_subpaths_act_on_the_same_row_as_the_action_body() {
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
        "subpath-restart",
        json!({ "delay_ms": 60_000, "content": { "body": "later" } }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "scheduling must succeed first: {body}");
    let restart_id = delay_id_of(&body);

    // `restart` (heartbeat) keeps the row pending and answers MSC4140's `{}`.
    let (status, body) = manage_via_subpath(&app, &alice, restart_id, "restart").await;
    assert_eq!(status, StatusCode::OK, "restart sub-path must succeed: {body}");
    assert_eq!(body, json!({}), "MSC4140 success body is an empty object");
    let (status, body) = get_delayed(&app, &alice, restart_id).await;
    assert_eq!(status, StatusCode::OK, "the row must still be readable after restart: {body}");

    // `cancel` settles it...
    let (status, body) = manage_via_subpath(&app, &alice, restart_id, "cancel").await;
    assert_eq!(status, StatusCode::OK, "cancel sub-path must succeed: {body}");
    // ...so a second cancel is the "no longer pending" error, not a second 200.
    let (status, body) = manage_via_subpath(&app, &alice, restart_id, "cancel").await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "a settled row must be refused by the sub-path too (same service transition): {body}"
    );

    // `send` (send now) on a fresh row.
    let (status, body) = schedule(
        &app,
        &alice,
        &room_id,
        "m.room.message",
        "subpath-send",
        json!({ "delay_ms": 60_000, "content": { "body": "now" } }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let send_id = delay_id_of(&body);
    let (status, body) = manage_via_subpath(&app, &alice, send_id, "send").await;
    assert_eq!(status, StatusCode::OK, "send sub-path must succeed: {body}");
    assert_eq!(body, json!({}));
}

#[tokio::test]
async fn subpaths_do_not_bypass_ownership() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let alice = super::create_test_user(&app).await;
    let bob = super::create_test_user(&app).await;
    let room_id = create_public_room(&app, &alice).await;
    join_room(&app, &bob, &room_id).await;

    let (status, body) = schedule(
        &app,
        &alice,
        &room_id,
        "m.room.message",
        "owned-by-alice",
        json!({ "delay_ms": 60_000, "content": { "body": "mine" } }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let delay_id = delay_id_of(&body);

    // Every sub-path must answer a foreign caller exactly like the body form: 404,
    // so the endpoint never confirms that someone else's delay_id exists.
    for action in ["cancel", "restart", "send"] {
        let (status, _) = manage_via_subpath(&app, &bob, delay_id, action).await;
        assert_eq!(
            status,
            StatusCode::NOT_FOUND,
            "'{action}' must not let a non-owner act on the row (and must not leak its existence)"
        );
    }

    // Still pending for its owner — the three attempts above changed nothing.
    let (status, _) = get_delayed(&app, &alice, delay_id).await;
    assert_eq!(status, StatusCode::OK, "the owner's row must be untouched");
}

#[tokio::test]
async fn the_list_endpoint_returns_only_the_callers_pending_events() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let alice = super::create_test_user(&app).await;
    let bob = super::create_test_user(&app).await;
    let room_id = create_public_room(&app, &alice).await;
    join_room(&app, &bob, &room_id).await;

    let mut alice_ids = Vec::new();
    for index in 0..2 {
        let (status, body) = schedule(
            &app,
            &alice,
            &room_id,
            "m.room.message",
            &format!("list-alice-{index}"),
            json!({ "delay_ms": 60_000, "content": { "body": "a" } }),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        alice_ids.push(delay_id_of(&body));
    }
    let (status, body) = schedule(
        &app,
        &bob,
        &room_id,
        "m.room.message",
        "list-bob-0",
        json!({ "delay_ms": 60_000, "content": { "body": "b" } }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let bob_id = delay_id_of(&body);

    let (status, body) = list_delayed(&app, &alice).await;
    assert_eq!(status, StatusCode::OK, "list must succeed: {body}");
    let mut listed = listed_delay_ids(&body);
    listed.sort_unstable();
    let mut expected = alice_ids.clone();
    expected.sort_unstable();
    assert_eq!(listed, expected, "only the caller's own pending events may be listed");

    // Settled rows drop out of the list (upstream reads the pending view only),
    // and the other user's row was never in it.
    let (status, _) = manage_via_subpath(&app, &alice, alice_ids[0], "cancel").await;
    assert_eq!(status, StatusCode::OK);
    let (_, body) = list_delayed(&app, &alice).await;
    let listed = listed_delay_ids(&body);
    assert_eq!(listed.len(), 1, "a cancelled event must no longer be listed: {body}");
    assert!(!listed.contains(&bob_id), "another user's event must never appear: {body}");

    // A user with nothing pending gets an empty array, not an error.
    let (status, body) = list_delayed(&app, &bob).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(listed_delay_ids(&body), vec![bob_id], "bob sees exactly his own: {body}");
}

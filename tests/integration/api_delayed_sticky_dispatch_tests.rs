//! MSC4140 × MSC4354 — a delayed event scheduled **sticky** must come out pinned.
//!
//! Both halves already had tests of their own: scheduling (`delay_ms`, the three
//! endpoints) and the sticky TTL on an immediate `/send`/`/state`. The join
//! between them had none, and that join is where the old 501 lived: the TTL has
//! to survive the round trip *storage → dispatcher → event|unsigned*, and nothing
//! checked that it did.
//!
//! The dispatcher is exercised directly
//! (`synapse_services::room::scheduled_dispatch::dispatch_due_event`) rather than
//! by waiting for the background loop: the loop in `src/server/mod.rs` is not
//! callable from a test, and a `sleep`-based test would be both slow and flaky.

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;

const STICKY_FIELD: &str = "org.matrix.msc4354.sticky_duration_ms";
const TTL_FIELD: &str = "msc4354_sticky_duration_ttl_ms";
const DELAY_MS: i64 = 60_000;

async fn create_public_room(app: &axum::Router, token: &str) -> String {
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/createRoom")
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({ "name": "Scheduled Sticky Room", "preset": "public_chat", "visibility": "public" }).to_string(),
        ))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK, "createRoom must succeed");
    let body = axum::body::to_bytes(response.into_body(), 8192).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    json["room_id"].as_str().expect("createRoom must return a room_id").to_string()
}

async fn whoami(app: &axum::Router, token: &str) -> String {
    let request = Request::builder()
        .method("GET")
        .uri("/_matrix/client/v3/account/whoami")
        .header("Authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK, "whoami must succeed");
    let body = axum::body::to_bytes(response.into_body(), 4096).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    json["user_id"].as_str().expect("whoami must return a user_id").to_string()
}

/// `PUT …/org.matrix.msc4140/rooms/{room}/delayed_event/{type}/{txn}` with the
/// sticky hint either in the query string or in the body.
async fn schedule(
    app: &axum::Router,
    token: &str,
    room_id: &str,
    txn: &str,
    query: &str,
    body: Value,
) -> (StatusCode, Value) {
    let request = Request::builder()
        .method("PUT")
        .uri(format!(
            "/_matrix/client/unstable/org.matrix.msc4140/rooms/{room_id}/delayed_event/m.room.message/{txn}{query}"
        ))
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or_else(|_| json!({})))
}

/// The newest events in the room, oldest-last (`dir=b`).
async fn latest_messages(app: &axum::Router, token: &str, room_id: &str) -> Vec<Value> {
    let request = Request::builder()
        .method("GET")
        .uri(format!("/_matrix/client/v3/rooms/{room_id}/messages?dir=b&limit=10"))
        .header("Authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK, "messages must be readable");
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&bytes).unwrap();
    json["chunk"].as_array().cloned().unwrap_or_default()
}

/// `GET /rooms/{room}/event/{event}` — the only read path that returns the full
/// `unsigned` block (timeline chunks deliberately omit it).
async fn get_event(app: &axum::Router, token: &str, room_id: &str, event_id: &str) -> Value {
    let request = Request::builder()
        .method("GET")
        .uri(format!("/_matrix/client/v3/rooms/{room_id}/event/{event_id}"))
        .header("Authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK, "the dispatched event must be readable");
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

fn body_with(text: &str) -> Value {
    json!({
        "delay_ms": DELAY_MS,
        "content": { "msgtype": "m.text", "body": text },
    })
}

/// Schedule one row through the real endpoint, then dispatch it through the real
/// dispatcher, and read the event back through the real API.
#[tokio::test]
async fn a_scheduled_sticky_message_is_pinned_when_the_dispatcher_sends_it() {
    let Some((app, state)) = super::setup_fresh_test_app_with_state().await else {
        return;
    };
    let token = super::create_test_user(&app).await;
    let user_id = whoami(&app, &token).await;
    let room_id = create_public_room(&app, &token).await;

    // 1. Schedule with the hint in the **body** (the dedicated endpoint's shape).
    let content = body_with("scheduled sticky");
    let mut body = content.clone();
    body[STICKY_FIELD] = json!(DELAY_MS);
    let (status, response) = schedule(&app, &token, &room_id, "sticky-1", "", body).await;
    assert_eq!(status, StatusCode::OK, "a sticky hint must not be refused any more: {response}");

    // 2. The row carries the hint (this is the storage half of the join).
    let storage = state.services.admin.modules.delayed_event_storage.clone();
    let pending = storage.list_pending_delayed_events_for_user(&user_id).await.expect("list delayed events");
    let row = pending.into_iter().find(|row| row.room_id == room_id).expect("the scheduled row must be listed");
    assert_eq!(
        row.sticky_duration_ms,
        Some(DELAY_MS),
        "the sticky duration must be persisted on the row, or the dispatcher cannot apply it"
    );

    // 3. Dispatch it, exactly as the background loop does.
    let room_service = state.services.rooms.room_service.clone();
    let server_name = state.services.core.server_name.clone();
    synapse_services::room::scheduled_dispatch::dispatch_due_event(&room_service, &server_name, &row)
        .await
        .expect("dispatch must succeed");

    // 4. The event the client can read back must carry the TTL.
    let messages = latest_messages(&app, &token, &room_id).await;
    let event_id = messages
        .iter()
        .find(|event| event["content"]["body"] == "scheduled sticky")
        .and_then(|event| event["event_id"].as_str())
        .unwrap_or_else(|| panic!("the dispatched event must be in the timeline: {messages:?}"))
        .to_string();
    let sent = get_event(&app, &token, &room_id, &event_id).await;
    assert_eq!(
        sent["unsigned"][TTL_FIELD].as_i64(),
        Some(DELAY_MS),
        "a scheduled sticky event must come out pinned, got {sent}"
    );
}

/// Control: the same path **without** the hint must not grow a TTL — so the
/// assertion above is about the hint, not about scheduled events carrying one.
#[tokio::test]
async fn a_scheduled_message_without_the_hint_is_not_pinned() {
    let Some((app, state)) = super::setup_fresh_test_app_with_state().await else {
        return;
    };
    let token = super::create_test_user(&app).await;
    let user_id = whoami(&app, &token).await;
    let room_id = create_public_room(&app, &token).await;

    let (status, response) = schedule(&app, &token, &room_id, "plain-1", "", body_with("scheduled plain")).await;
    assert_eq!(status, StatusCode::OK, "{response}");

    let storage = state.services.admin.modules.delayed_event_storage.clone();
    let pending = storage.list_pending_delayed_events_for_user(&user_id).await.expect("list delayed events");
    let row = pending.into_iter().find(|row| row.room_id == room_id).expect("the scheduled row must be listed");
    assert_eq!(row.sticky_duration_ms, None);

    let room_service = state.services.rooms.room_service.clone();
    let server_name = state.services.core.server_name.clone();
    synapse_services::room::scheduled_dispatch::dispatch_due_event(&room_service, &server_name, &row)
        .await
        .expect("dispatch must succeed");

    let messages = latest_messages(&app, &token, &room_id).await;
    let event_id = messages
        .iter()
        .find(|event| event["content"]["body"] == "scheduled plain")
        .and_then(|event| event["event_id"].as_str())
        .unwrap_or_else(|| panic!("the dispatched event must be in the timeline: {messages:?}"))
        .to_string();
    let sent = get_event(&app, &token, &room_id, &event_id).await;
    assert!(
        sent["unsigned"].get(TTL_FIELD).is_none(),
        "an event scheduled without the hint must not carry a sticky TTL: {sent}"
    );
}

/// A sticky **state** event is refused (upstream: "Sticky state events are not
/// allowed") — the dispatcher would have no meaningful place to put the TTL.
#[tokio::test]
async fn a_sticky_state_event_cannot_be_scheduled() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let token = super::create_test_user(&app).await;
    let room_id = create_public_room(&app, &token).await;

    let mut body = json!({ "delay_ms": DELAY_MS, "content": { "topic": "later" }, "state_key": "topic" });
    body[STICKY_FIELD] = json!(DELAY_MS);
    let (status, response) = schedule(&app, &token, &room_id, "sticky-state-1", "", body).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "a sticky state event must be refused: {response}");
    assert_eq!(response["errcode"], "M_INVALID_PARAM", "{response}");
}

/// The **legacy** scheduling location: `?org.matrix.msc4140.delay=` on `/send`.
///
/// It shares `schedule_delayed_event_core` with the dedicated endpoint, but reads
/// its hints from the query string instead of the body — and upstream passes
/// `sticky_duration_ms` through this path too. One test, because "same core" has
/// been wrong before: the query and body forms reach the row through different
/// parsers.
#[tokio::test]
async fn the_send_delay_query_form_also_carries_the_sticky_hint() {
    let Some((app, state)) = super::setup_fresh_test_app_with_state().await else {
        return;
    };
    let token = super::create_test_user(&app).await;
    let user_id = whoami(&app, &token).await;
    let room_id = create_public_room(&app, &token).await;

    let request = Request::builder()
        .method("PUT")
        .uri(format!(
            "/_matrix/client/v3/rooms/{room_id}/send/m.room.message/legacy-1?org.matrix.msc4140.delay={DELAY_MS}&{STICKY_FIELD}={DELAY_MS}"
        ))
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "msgtype": "m.text", "body": "legacy scheduled sticky" }).to_string()))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK, "the delay hint must schedule, not send");

    let storage = state.services.admin.modules.delayed_event_storage.clone();
    let pending = storage.list_pending_delayed_events_for_user(&user_id).await.expect("list delayed events");
    let row = pending.into_iter().find(|row| row.room_id == room_id).expect("the scheduled row must be listed");
    assert_eq!(row.sticky_duration_ms, Some(DELAY_MS), "the query hint must reach the row");

    let room_service = state.services.rooms.room_service.clone();
    let server_name = state.services.core.server_name.clone();
    synapse_services::room::scheduled_dispatch::dispatch_due_event(&room_service, &server_name, &row)
        .await
        .expect("dispatch must succeed");

    let messages = latest_messages(&app, &token, &room_id).await;
    let event_id = messages
        .iter()
        .find(|event| event["content"]["body"] == "legacy scheduled sticky")
        .and_then(|event| event["event_id"].as_str())
        .unwrap_or_else(|| panic!("the dispatched event must be in the timeline: {messages:?}"))
        .to_string();
    let sent = get_event(&app, &token, &room_id, &event_id).await;
    assert_eq!(sent["unsigned"][TTL_FIELD].as_i64(), Some(DELAY_MS), "got {sent}");
}

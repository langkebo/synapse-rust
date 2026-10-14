//! `m.room.history_visibility` enforcement on the client read paths.
//!
//! Before this guard existed the value was stored, echoed in room summaries and
//! protected as an auth event, but **no read path consulted it**: a member could
//! page back through the room's entire history, including everything that
//! happened before they were ever invited or joined.
//!
//! These tests drive the real router, so they fail if the wiring is removed from
//! the handlers as well as if the policy itself is wrong.

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;

async fn create_public_room(app: &axum::Router, token: &str) -> String {
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/createRoom")
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({ "name": "History Visibility Room", "preset": "public_chat", "visibility": "public" }).to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK, "createRoom must succeed");

    let body = axum::body::to_bytes(response.into_body(), 8192).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    json["room_id"].as_str().expect("createRoom must return a room_id").to_string()
}

async fn send_message(app: &axum::Router, token: &str, room_id: &str, txn_id: &str, text: &str) -> String {
    let request = Request::builder()
        .method("PUT")
        .uri(format!("/_matrix/client/v3/rooms/{room_id}/send/m.room.message/{txn_id}"))
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "msgtype": "m.text", "body": text }).to_string()))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK, "send must succeed");

    let body = axum::body::to_bytes(response.into_body(), 4096).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    json["event_id"].as_str().expect("send must return an event_id").to_string()
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

async fn set_history_visibility(app: &axum::Router, token: &str, room_id: &str, value: &str) -> StatusCode {
    let request = Request::builder()
        .method("PUT")
        .uri(format!("/_matrix/client/v3/rooms/{room_id}/state/m.room.history_visibility"))
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "history_visibility": value }).to_string()))
        .unwrap();

    ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap().status()
}

/// The `event_id`s in a `/messages` chunk, in server order.
async fn messages_event_ids(app: &axum::Router, token: &str, room_id: &str) -> Vec<String> {
    let request = Request::builder()
        .method("GET")
        .uri(format!("/_matrix/client/v3/rooms/{room_id}/messages?dir=b&limit=100"))
        .header("Authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK, "/messages must succeed");

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    json["chunk"]
        .as_array()
        .expect("/messages must return a chunk array")
        .iter()
        .filter_map(|event| event["event_id"].as_str().map(ToOwned::to_owned))
        .collect()
}

async fn single_event_status(app: &axum::Router, token: &str, room_id: &str, event_id: &str) -> StatusCode {
    let request = Request::builder()
        .method("GET")
        .uri(format!("/_matrix/client/v3/rooms/{room_id}/event/{event_id}"))
        .header("Authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();

    ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap().status()
}

/// Send an `m.annotation` (reaction) targeting `target`, returning its event id.
async fn react(app: &axum::Router, token: &str, room_id: &str, txn_id: &str, target: &str) -> String {
    let request = Request::builder()
        .method("PUT")
        .uri(format!("/_matrix/client/v3/rooms/{room_id}/send/m.reaction/{txn_id}"))
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({ "m.relates_to": { "rel_type": "m.annotation", "event_id": target, "key": "👍" } }).to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK, "reaction send must succeed");

    let body = axum::body::to_bytes(response.into_body(), 8192).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    json["event_id"].as_str().expect("send must return an event_id").to_string()
}

/// The annotation `event_id`s `/relations/{target}/m.annotation` returns.
async fn annotation_event_ids(app: &axum::Router, token: &str, room_id: &str, target: &str) -> Vec<String> {
    let request = Request::builder()
        .method("GET")
        .uri(format!("/_matrix/client/v3/rooms/{room_id}/relations/{target}/m.annotation"))
        .header("Authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK, "/relations must succeed");

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    json["chunk"]
        .as_array()
        .expect("/relations must return a chunk array")
        .iter()
        .filter_map(|event| event["event_id"].as_str().map(ToOwned::to_owned))
        .collect()
}

/// A relation is an ordinary room event, so an annotation made before the caller
/// was involved must not be served either.
#[tokio::test]
async fn relations_hide_annotations_created_before_the_caller_joined() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let alice = super::create_test_user(&app).await;
    let room_id = create_public_room(&app, &alice).await;

    let target = send_message(&app, &alice, &room_id, "r-root", "root").await;
    let before_bob = react(&app, &alice, &room_id, "r-pre", &target).await;

    let bob = super::create_test_user(&app).await;
    join_room(&app, &bob, &room_id).await;
    let after_bob = react(&app, &alice, &room_id, "r-post", &target).await;

    // Precondition: under the creation default (`shared`) both annotations reach
    // Bob, so the assertion below is about the policy and not about the relation
    // never having been indexed.
    let shared = annotation_event_ids(&app, &bob, &room_id, &target).await;
    assert!(
        shared.contains(&before_bob) && shared.contains(&after_bob),
        "precondition: `shared` must expose both annotations: {shared:?}"
    );

    assert_eq!(set_history_visibility(&app, &alice, &room_id, "joined").await, StatusCode::OK);

    let joined = annotation_event_ids(&app, &bob, &room_id, &target).await;
    assert!(joined.contains(&after_bob), "an annotation made after the join must stay visible: {joined:?}");
    assert!(!joined.contains(&before_bob), "`joined` must hide annotations made before the caller joined: {joined:?}");

    // Control: the creator sees both, so this is a per-caller boundary.
    let alice_ids = annotation_event_ids(&app, &alice, &room_id, &target).await;
    assert!(
        alice_ids.contains(&before_bob) && alice_ids.contains(&after_bob),
        "the creator must keep seeing both annotations: {alice_ids:?}"
    );
}

/// `POST /search` for a `room_events` term, returning the `event_id`s it matched.
async fn search_event_ids(app: &axum::Router, token: &str, term: &str) -> Vec<String> {
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/search")
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "search_categories": { "room_events": { "search_term": term } } }).to_string()))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK, "/search must succeed");

    let body = axum::body::to_bytes(response.into_body(), 2 * 1024 * 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    json["search_categories"]["room_events"]["results"]
        .as_array()
        .expect("/search must return a results array")
        .iter()
        .filter_map(|hit| hit["result"]["event_id"].as_str().map(ToOwned::to_owned))
        .collect()
}

/// A search hit is an ordinary room event, so one from before the caller was
/// involved must not be served either — otherwise `/search` becomes the way to
/// read pre-join history that `/messages` and `/sync` now hide.
#[tokio::test]
async fn search_hides_hits_from_before_the_caller_joined() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let alice = super::create_test_user(&app).await;
    let room_id = create_public_room(&app, &alice).await;

    // A term that appears only in these two messages, so the hit set is exactly
    // what this test put there.
    let term = "zqxjvp";
    let before_bob = send_message(&app, &alice, &room_id, "q1", &format!("{term} before")).await;
    let bob = super::create_test_user(&app).await;
    join_room(&app, &bob, &room_id).await;
    let after_bob = send_message(&app, &alice, &room_id, "q2", &format!("{term} after")).await;

    // Precondition: under the creation default (`shared`) both are searchable —
    // otherwise the assertion below proves nothing about the policy.
    let shared = search_event_ids(&app, &bob, term).await;
    assert!(
        shared.contains(&before_bob) && shared.contains(&after_bob),
        "precondition: `shared` must expose both search hits: {shared:?}"
    );

    assert_eq!(set_history_visibility(&app, &alice, &room_id, "joined").await, StatusCode::OK);

    let joined = search_event_ids(&app, &bob, term).await;
    assert!(joined.contains(&after_bob), "a hit from after the join must stay visible: {joined:?}");
    assert!(!joined.contains(&before_bob), "`joined` must hide search hits from before the caller joined: {joined:?}");

    // Control: the creator was in the room before both messages.
    let alice_ids = search_event_ids(&app, &alice, term).await;
    assert!(
        alice_ids.contains(&before_bob) && alice_ids.contains(&after_bob),
        "the creator must keep seeing both hits: {alice_ids:?}"
    );
}

/// The `event_id`s of one room's timeline in an initial `/sync`.
async fn sync_timeline_event_ids(app: &axum::Router, token: &str, room_id: &str) -> Vec<String> {
    let request = Request::builder()
        .method("GET")
        .uri("/_matrix/client/v3/sync?timeout=0")
        .header("Authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK, "/sync must succeed");

    let body = axum::body::to_bytes(response.into_body(), 4 * 1024 * 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    json["rooms"]["join"][room_id]["timeline"]["events"]
        .as_array()
        .map(|events| events.iter().filter_map(|event| event["event_id"].as_str().map(ToOwned::to_owned)).collect())
        .unwrap_or_default()
}

/// The worst form of the leak: no client action at all — a newly joined member's
/// very first `/sync` used to hand over the room's whole pre-join history.
#[tokio::test]
async fn sync_timeline_hides_history_from_before_the_caller_joined() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let alice = super::create_test_user(&app).await;
    let room_id = create_public_room(&app, &alice).await;

    let before_bob = send_message(&app, &alice, &room_id, "s1", "before-bob").await;
    let bob = super::create_test_user(&app).await;
    join_room(&app, &bob, &room_id).await;
    let after_bob = send_message(&app, &alice, &room_id, "s2", "after-bob").await;

    // Precondition: under the creation default (`shared`) Bob's sync carries the
    // earlier message too — otherwise the assertion below would be vacuous.
    let shared_ids = sync_timeline_event_ids(&app, &bob, &room_id).await;
    assert!(
        shared_ids.contains(&before_bob),
        "precondition: `shared` must expose pre-join history through /sync: {shared_ids:?}"
    );

    assert_eq!(set_history_visibility(&app, &alice, &room_id, "joined").await, StatusCode::OK);

    let joined_ids = sync_timeline_event_ids(&app, &bob, &room_id).await;
    assert!(
        joined_ids.contains(&after_bob),
        "a message sent after the caller joined must stay in the timeline: {joined_ids:?}"
    );
    assert!(
        !joined_ids.contains(&before_bob),
        "`joined` visibility must hide pre-join history from /sync as well: {joined_ids:?}"
    );

    // Control: the creator was in the room before both messages, so this is a
    // per-caller boundary rather than a blanket wipe.
    let alice_ids = sync_timeline_event_ids(&app, &alice, &room_id).await;
    assert!(
        alice_ids.contains(&before_bob) && alice_ids.contains(&after_bob),
        "the creator must keep seeing the full timeline: {alice_ids:?}"
    );

    // Control 2: back to `shared` restores the earlier history, proving the
    // filtering above came from the policy and not from the event disappearing.
    assert_eq!(set_history_visibility(&app, &alice, &room_id, "shared").await, StatusCode::OK);
    let restored = sync_timeline_event_ids(&app, &bob, &room_id).await;
    assert!(restored.contains(&before_bob), "`shared` must restore pre-join history: {restored:?}");
}

/// `unread_notifications` of one room in an initial `/sync`, as
/// `(notification_count, highlight_count)`.
async fn sync_unread_counts(app: &axum::Router, token: &str, room_id: &str) -> (i64, i64) {
    let request = Request::builder()
        .method("GET")
        .uri("/_matrix/client/v3/sync?timeout=0")
        .header("Authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK, "/sync must succeed");

    let body = axum::body::to_bytes(response.into_body(), 4 * 1024 * 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let counts = &json["rooms"]["join"][room_id]["unread_notifications"];
    let read = |key: &str| counts[key].as_i64().unwrap_or_else(|| panic!("`{key}` missing from {counts}"));
    (read("notification_count"), read("highlight_count"))
}

/// The **count** surface must use the same boundary as the list surfaces.
///
/// `/messages`, `/sync`'s timeline, `/relations` and `/search` all hide pre-join
/// history — but a count is not a list, and it was computed from the read marker
/// alone. A member who joined later has **no read marker** in that room (joining
/// does not write one; only a client receipt does), so `last_read_ts` fell back to
/// `0` and every pre-join message was counted as unread. The client then showed
/// "4 unread" over an empty timeline — wrong, and a (count-only) hash of history
/// it is not allowed to read.
#[tokio::test]
async fn unread_counts_ignore_history_from_before_the_caller_joined() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let alice = super::create_test_user(&app).await;
    let room_id = create_public_room(&app, &alice).await;

    for index in 0..3 {
        send_message(&app, &alice, &room_id, &format!("u-pre-{index}"), "pre-join").await;
    }
    assert_eq!(set_history_visibility(&app, &alice, &room_id, "joined").await, StatusCode::OK);

    let bob = super::create_test_user(&app).await;
    join_room(&app, &bob, &room_id).await;

    // The one message Bob is allowed to see, and therefore the only one that may
    // be counted. It doubles as the control that the count is not simply stuck at
    // zero: without it, a `0` assertion would pass against any constant-zero
    // implementation.
    let visible = send_message(&app, &alice, &room_id, "u-post", "after-bob").await;

    // Precondition: Bob's timeline really does contain only the post-join message,
    // so "1 unread" and "1 readable message" are the same statement.
    let timeline = sync_timeline_event_ids(&app, &bob, &room_id).await;
    assert!(timeline.contains(&visible), "the post-join message must be visible: {timeline:?}");
    assert_eq!(timeline.iter().filter(|id| id.as_str() == visible).count(), 1);

    let (notifications, highlights) = sync_unread_counts(&app, &bob, &room_id).await;
    assert_eq!(
        notifications, 1,
        "only the readable (post-join) message may be counted; the 3 pre-join messages are hidden \
         from every list surface, so counting them is a count-only leak of history"
    );
    assert_eq!(highlights, 0, "none of them mentions Bob");
}

/// The `event_id`s in one room's timeline from `GET /rooms/{roomId}/sync`.
///
/// That endpoint lives under `/_matrix/vendor/v1` (this project's own client
/// surface rather than a spec one) — see
/// `synapse-web/src/routes/derived_route_table_always.inc.rs`.
async fn room_sync_event_ids(app: &axum::Router, token: &str, room_id: &str) -> Vec<String> {
    let request = Request::builder()
        .method("GET")
        .uri(format!("/_matrix/vendor/v1/rooms/{room_id}/sync?timeout=0"))
        .header("Authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK, "/rooms/{{roomId}}/sync must succeed");

    let body = axum::body::to_bytes(response.into_body(), 4 * 1024 * 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    json["timeline"]["events"]
        .as_array()
        .map(|events| events.iter().filter_map(|event| event["event_id"].as_str().map(ToOwned::to_owned)).collect())
        .unwrap_or_default()
}

/// The **per-room** sync is a read path like the whole-account one, and it was
/// the one timeline builder that never consulted the policy.
///
/// `GET /rooms/{roomId}/sync` assembles the room's timeline through the same
/// builder as `/sync` but reached it without a cut, so a newly joined member got
/// the room's pre-join history from here after `/sync` itself had been fixed.
#[tokio::test]
async fn room_sync_hides_history_from_before_the_caller_joined() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let alice = super::create_test_user(&app).await;
    let room_id = create_public_room(&app, &alice).await;

    let before_bob = send_message(&app, &alice, &room_id, "rs1", "before-bob").await;
    assert_eq!(set_history_visibility(&app, &alice, &room_id, "joined").await, StatusCode::OK);

    let bob = super::create_test_user(&app).await;
    join_room(&app, &bob, &room_id).await;
    let after_bob = send_message(&app, &alice, &room_id, "rs2", "after-bob").await;

    let bob_ids = room_sync_event_ids(&app, &bob, &room_id).await;
    assert!(bob_ids.contains(&after_bob), "the post-join message must be served: {bob_ids:?}");
    assert!(
        !bob_ids.contains(&before_bob),
        "`joined` must hide the pre-join message on /rooms/{{roomId}}/sync too: {bob_ids:?}"
    );

    // Control: the creator keeps seeing both, so the boundary is per-caller and
    // the assertion above is not just "the timeline is empty".
    let alice_ids = room_sync_event_ids(&app, &alice, &room_id).await;
    assert!(
        alice_ids.contains(&before_bob) && alice_ids.contains(&after_bob),
        "the creator must keep seeing the full timeline: {alice_ids:?}"
    );
}

/// The floor is a **policy** floor, not a membership floor.
///
/// Under `shared` (the room-version default) a member really can read the whole
/// history, so those messages are legitimately unread for someone who has no read
/// marker — flooring at "when you joined" here would have been a different bug
/// (silent undercounting) in place of the one being fixed.
#[tokio::test]
async fn unread_counts_still_count_history_a_shared_room_makes_readable() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let alice = super::create_test_user(&app).await;
    let room_id = create_public_room(&app, &alice).await;

    let mut sent = Vec::new();
    for index in 0..3 {
        sent.push(send_message(&app, &alice, &room_id, &format!("sh-pre-{index}"), "shared history").await);
    }

    let bob = super::create_test_user(&app).await;
    join_room(&app, &bob, &room_id).await;

    // NB: `timeline.events` carries membership and state events too, so this
    // asserts on the messages themselves rather than on the array's length.
    let timeline = sync_timeline_event_ids(&app, &bob, &room_id).await;
    for event_id in &sent {
        assert!(timeline.contains(event_id), "`shared` exposes the pre-join messages: {timeline:?}");
    }

    let (notifications, _) = sync_unread_counts(&app, &bob, &room_id).await;
    assert_eq!(
        notifications, 3,
        "every message Bob can read and has not read must be counted — the floor must not turn into \
         a blanket membership floor"
    );
}

/// The core leak, at the endpoint clients actually page with.
#[tokio::test]
async fn joined_visibility_hides_history_from_before_the_caller_joined() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let alice = super::create_test_user(&app).await;
    let room_id = create_public_room(&app, &alice).await;

    // Sent before Bob is in the room.
    let before_bob = send_message(&app, &alice, &room_id, "before", "before-bob").await;

    let bob = super::create_test_user(&app).await;
    join_room(&app, &bob, &room_id).await;

    // Sent after Bob joined.
    let after_bob = send_message(&app, &alice, &room_id, "after", "after-bob").await;

    // Precondition: under the room's default (`shared`, set at creation) both
    // messages are visible to Bob — otherwise the assertion below is vacuous,
    // because hiding them would prove nothing about the policy.
    let shared_ids = messages_event_ids(&app, &bob, &room_id).await;
    assert!(
        shared_ids.contains(&before_bob),
        "precondition: `shared` must expose pre-join history, chunk={shared_ids:?}"
    );
    assert!(shared_ids.contains(&after_bob), "precondition: the post-join message must be visible");

    // Now make it restrictive.
    assert_eq!(
        set_history_visibility(&app, &alice, &room_id, "joined").await,
        StatusCode::OK,
        "the creator must be able to set m.room.history_visibility"
    );

    let joined_ids = messages_event_ids(&app, &bob, &room_id).await;
    assert!(
        joined_ids.contains(&after_bob),
        "a message sent after the caller joined must stay visible: chunk={joined_ids:?}"
    );
    assert!(
        !joined_ids.contains(&before_bob),
        "`joined` visibility must hide history from before the caller joined: chunk={joined_ids:?}"
    );

    // Control 1: the creator, who was in the room from the start, keeps seeing
    // everything — this must be a *per-caller* boundary, not a blanket wipe.
    let alice_ids = messages_event_ids(&app, &alice, &room_id).await;
    assert!(
        alice_ids.contains(&before_bob) && alice_ids.contains(&after_bob),
        "the creator joined before both messages and must see both: chunk={alice_ids:?}"
    );

    // Control 2: going back to `shared` restores the earlier history, proving the
    // earlier assertion came from the policy and not from the message vanishing.
    assert_eq!(set_history_visibility(&app, &alice, &room_id, "shared").await, StatusCode::OK);
    let restored = messages_event_ids(&app, &bob, &room_id).await;
    assert!(restored.contains(&before_bob), "`shared` must restore pre-join history: chunk={restored:?}");
}

/// `invited` is one step looser than `joined`: being invited is enough.
#[tokio::test]
async fn invited_visibility_exposes_history_from_the_invite() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let alice = super::create_test_user(&app).await;
    let room_id = create_public_room(&app, &alice).await;

    let before_join = send_message(&app, &alice, &room_id, "t1", "before-join").await;
    let bob = super::create_test_user(&app).await;
    join_room(&app, &bob, &room_id).await;
    let after_join = send_message(&app, &alice, &room_id, "t2", "after-join").await;

    assert_eq!(set_history_visibility(&app, &alice, &room_id, "invited").await, StatusCode::OK);

    // Bob was never invited (he joined a public room directly), so his boundary is
    // his *join*: `invited` must behave like `joined` for him.
    let ids = messages_event_ids(&app, &bob, &room_id).await;
    assert!(ids.contains(&after_join), "post-join message must be visible: chunk={ids:?}");
    assert!(
        !ids.contains(&before_join),
        "a caller who was never invited is still bounded by their join: chunk={ids:?}"
    );
}

/// `GET /event/{eventId}` must not become an oracle for hidden events.
#[tokio::test]
async fn single_event_read_hides_a_pre_join_event_before_its_id() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let alice = super::create_test_user(&app).await;
    let room_id = create_public_room(&app, &alice).await;

    let before_bob = send_message(&app, &alice, &room_id, "t1", "before-bob").await;
    let bob = super::create_test_user(&app).await;
    join_room(&app, &bob, &room_id).await;
    let after_bob = send_message(&app, &alice, &room_id, "t2", "after-bob").await;

    assert_eq!(set_history_visibility(&app, &alice, &room_id, "joined").await, StatusCode::OK);

    assert_eq!(
        single_event_status(&app, &bob, &room_id, &before_bob).await,
        StatusCode::NOT_FOUND,
        "a pre-join event must be reported as missing, not served"
    );
    assert_eq!(
        single_event_status(&app, &bob, &room_id, &after_bob).await,
        StatusCode::OK,
        "a post-join event must still be readable"
    );
    // Control: the creator may still fetch the earlier event by id.
    assert_eq!(
        single_event_status(&app, &alice, &room_id, &before_bob).await,
        StatusCode::OK,
        "the creator must keep access to events from before Bob joined"
    );
}

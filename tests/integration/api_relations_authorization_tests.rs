//! Authorization regression tests for the relation/reaction write paths.
//!
//! Covers S5/S6/S7 from `docs/audit/DB_REVIEW_2026-09-17.md` §13.7:
//!
//! * S5 `PUT .../send/m.reaction/...` checked only that the room EXISTED, so any
//!   authenticated user could inject `m.annotation` events into any room.
//! * S6 `PUT .../relations/{event_id}/{rel_type}/{txn_id}` had the same gap while
//!   its read counterpart (`GET .../relations/...`) had always required
//!   membership.
//! * S7 `PUT .../anti_screenshot` writes a room STATE event but only checked
//!   membership, so any joined user with power level 0 could flip a room-wide
//!   privacy setting.
//!
//! Each case asserts BOTH directions: the outsider is rejected and the
//! legitimate member still succeeds (a fix that breaks the happy path is not a
//! fix).

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;

async fn setup_test_app() -> Option<axum::Router> {
    super::setup_fresh_test_app().await
}

async fn register_user_with_id(app: &axum::Router, username: &str) -> (String, String) {
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
    let body = axum::body::to_bytes(response.into_body(), 4096).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    (
        json["access_token"].as_str().expect("access_token").to_string(),
        json["user_id"].as_str().expect("user_id").to_string(),
    )
}

async fn create_room(app: &axum::Router, token: &str, name: &str) -> String {
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/createRoom")
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "name": name }).to_string()))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK, "createRoom must succeed");
    let body = axum::body::to_bytes(response.into_body(), 4096).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    json["room_id"].as_str().expect("room_id").to_string()
}

async fn invite_user(app: &axum::Router, token: &str, room_id: &str, user_id: &str) {
    let request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/v3/rooms/{room_id}/invite"))
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "user_id": user_id }).to_string()))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK, "invite must succeed");
}

async fn join_room(app: &axum::Router, token: &str, room_id: &str) {
    let request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/v3/rooms/{room_id}/join"))
        .header("Authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK, "join must succeed");
}

/// Send a plain message and return its event id (the relation target).
async fn send_message(app: &axum::Router, token: &str, room_id: &str) -> String {
    let txn = format!("txn_{}", rand::random::<u32>());
    let request = Request::builder()
        .method("PUT")
        .uri(format!("/_matrix/client/v3/rooms/{room_id}/send/m.room.message/{txn}"))
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "msgtype": "m.text", "body": "hello" }).to_string()))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK, "send must succeed");
    let body = axum::body::to_bytes(response.into_body(), 4096).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    json["event_id"].as_str().expect("event_id").to_string()
}

fn put_json(uri: String, token: &str, body: Value) -> Request<Body> {
    Request::builder()
        .method("PUT")
        .uri(uri)
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

// ─────────────────────────────── S5: reactions ───────────────────────────────

#[tokio::test]
async fn reaction_is_forbidden_for_non_members() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let suffix = rand::random::<u32>();
    let (owner_token, _) = register_user_with_id(&app, &format!("react_owner_{suffix}")).await;
    let (outsider_token, _) = register_user_with_id(&app, &format!("react_outsider_{suffix}")).await;

    let room_id = create_room(&app, &owner_token, "Reaction authz").await;
    let event_id = send_message(&app, &owner_token, &room_id).await;

    let request = put_json(
        format!("/_matrix/client/v3/rooms/{room_id}/send/m.reaction/r1"),
        &outsider_token,
        json!({
            "m.relates_to": { "rel_type": "m.annotation", "event_id": event_id },
            "body": "👍"
        }),
    );
    let response = ServiceExt::<Request<Body>>::oneshot(app, request).await.unwrap();
    assert_eq!(
        response.status(),
        StatusCode::FORBIDDEN,
        "a non-member must not be able to react into a room they are not in"
    );
}

#[tokio::test]
async fn reaction_is_allowed_for_members() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let suffix = rand::random::<u32>();
    let (owner_token, owner_id) = register_user_with_id(&app, &format!("react_member_{suffix}")).await;
    let (member_token, member_id) = register_user_with_id(&app, &format!("react_member2_{suffix}")).await;

    let room_id = create_room(&app, &owner_token, "Reaction member path").await;
    invite_user(&app, &owner_token, &room_id, &member_id).await;
    join_room(&app, &member_token, &room_id).await;
    let event_id = send_message(&app, &owner_token, &room_id).await;

    // The joined member (power level 0) must still be able to react: reactions are
    // governed by `events["m.reaction"]`/`events_default`, which default to 0.
    let request = put_json(
        format!("/_matrix/client/v3/rooms/{room_id}/send/m.reaction/r2"),
        &member_token,
        json!({
            "m.relates_to": { "rel_type": "m.annotation", "event_id": event_id },
            "body": "👍"
        }),
    );
    let response = ServiceExt::<Request<Body>>::oneshot(app, request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK, "a joined member must be able to react");
    let _ = owner_id;
}

// ─────────────────────────────── S6: relations ───────────────────────────────

#[tokio::test]
async fn relation_is_forbidden_for_non_members() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let suffix = rand::random::<u32>();
    let (owner_token, _) = register_user_with_id(&app, &format!("rel_owner_{suffix}")).await;
    let (outsider_token, _) = register_user_with_id(&app, &format!("rel_outsider_{suffix}")).await;

    let room_id = create_room(&app, &owner_token, "Relation authz").await;
    let event_id = send_message(&app, &owner_token, &room_id).await;

    let request = put_json(
        format!("/_matrix/client/v3/rooms/{room_id}/relations/{event_id}/m.reference/r3"),
        &outsider_token,
        json!({ "content": { "body": "injected" } }),
    );
    let response = ServiceExt::<Request<Body>>::oneshot(app, request).await.unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN, "a non-member must not be able to write relations");
}

#[tokio::test]
async fn relation_is_allowed_for_members() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let suffix = rand::random::<u32>();
    let (owner_token, _) = register_user_with_id(&app, &format!("rel_member_{suffix}")).await;

    let room_id = create_room(&app, &owner_token, "Relation member path").await;
    let event_id = send_message(&app, &owner_token, &room_id).await;

    let request = put_json(
        format!("/_matrix/client/v3/rooms/{room_id}/relations/{event_id}/m.reference/r4"),
        &owner_token,
        json!({ "content": { "body": "legit" } }),
    );
    let response = ServiceExt::<Request<Body>>::oneshot(app, request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK, "a room member must be able to write relations");
}

// ──────────────────────────── S7: room-wide state ────────────────────────────

#[tokio::test]
async fn anti_screenshot_requires_power_level_not_only_membership() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let suffix = rand::random::<u32>();
    let (owner_token, _) = register_user_with_id(&app, &format!("as_owner_{suffix}")).await;
    let (member_token, member_id) = register_user_with_id(&app, &format!("as_member_{suffix}")).await;

    let room_id = create_room(&app, &owner_token, "Anti-screenshot power level").await;
    invite_user(&app, &owner_token, &room_id, &member_id).await;
    join_room(&app, &member_token, &room_id).await;

    // A plain member (power level 0) must NOT be able to flip this room-wide
    // STATE event: `com.hula.privacy` is governed by state_default.
    let request = put_json(
        format!("/_matrix/client/v3/rooms/{room_id}/anti_screenshot"),
        &member_token,
        json!({ "enabled": true }),
    );
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(
        response.status(),
        StatusCode::FORBIDDEN,
        "a power-level-0 member must not be able to set a room state event"
    );

    // The creator (power level 100) still can.
    let request = put_json(
        format!("/_matrix/client/v3/rooms/{room_id}/anti_screenshot"),
        &owner_token,
        json!({ "enabled": true }),
    );
    let response = ServiceExt::<Request<Body>>::oneshot(app, request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK, "the room creator must still be able to set it");
}

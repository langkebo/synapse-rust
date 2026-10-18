//! B-5: room-level retention policy endpoint
//! (`PUT /_matrix/vendor/v1/rooms/{room_id}/retention`).
//!
//! Only room admins (power level 100) may change the policy; everyone else is
//! rejected. The endpoint must also not leak "you are not an admin" for a room
//! that does not exist — that case is a plain 404.

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;

const RETENTION_PATH: &str = "/_matrix/vendor/v1/rooms";

async fn setup_test_app() -> axum::Router {
    super::setup_fresh_test_app().await.expect("test app must build (DB unavailable is a hard failure)")
}

async fn register_user(app: &axum::Router, username: &str) -> (String, String) {
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
    (json["access_token"].as_str().unwrap().to_string(), json["user_id"].as_str().unwrap().to_string())
}

async fn create_room(app: &axum::Router, token: &str, name: &str) -> String {
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/createRoom")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "name": name,
                "preset": "private_chat"
            })
            .to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 2048).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    json["room_id"].as_str().unwrap().to_string()
}

async fn invite_and_join(app: &axum::Router, owner_token: &str, room_id: &str, member_token: &str, member_id: &str) {
    let request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/v3/rooms/{}/invite", room_id))
        .header("Authorization", format!("Bearer {}", owner_token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "user_id": member_id }).to_string()))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/v3/rooms/{}/join", room_id))
        .header("Authorization", format!("Bearer {}", member_token))
        .body(Body::empty())
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

async fn put_retention(app: &axum::Router, token: &str, room_id: &str, body: Value) -> (StatusCode, Value) {
    let request = Request::builder()
        .method("PUT")
        .uri(format!("{}/{}/retention", RETENTION_PATH, room_id))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 4096).await.unwrap();
    let json: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, json)
}

#[tokio::test]
async fn room_admin_can_set_retention_policy() {
    let app = setup_test_app().await;

    let (token, _user_id) = register_user(&app, "retention_admin").await;
    let room_id = create_room(&app, &token, "Retention admin room").await;

    let (status, body) = put_retention(
        &app,
        &token,
        &room_id,
        json!({
            "max_lifetime": 604_800_000_i64,
            "min_lifetime": 0_i64,
            "is_expire_on_clients": true
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "unexpected body: {body}");
    assert_eq!(body["room_id"], json!(room_id));
    assert_eq!(body["max_lifetime"], json!(604_800_000_i64));
    // Response key mirrors the GET on the same path, not the admin request
    // field name (`is_expire_on_clients`).
    assert_eq!(body["expire_on_clients"], json!(true));
}

#[tokio::test]
async fn plain_member_cannot_set_retention_policy() {
    let app = setup_test_app().await;

    let (owner_token, _owner_id) = register_user(&app, "retention_owner").await;
    let (member_token, member_id) = register_user(&app, "retention_member").await;
    let room_id = create_room(&app, &owner_token, "Retention member room").await;
    invite_and_join(&app, &owner_token, &room_id, &member_token, &member_id).await;

    let (status, _body) =
        put_retention(&app, &member_token, &room_id, json!({ "max_lifetime": 604_800_000_i64 })).await;

    assert_eq!(status, StatusCode::FORBIDDEN, "a joined member at PL 0 must not be able to rewrite retention");
}

#[tokio::test]
async fn unknown_room_returns_not_found() {
    let app = setup_test_app().await;

    let (token, _user_id) = register_user(&app, "retention_ghost").await;

    let (status, _body) =
        put_retention(&app, &token, "!nonexistent:example.com", json!({ "max_lifetime": 604_800_000_i64 })).await;

    // 404 (not 403): a missing room must not reveal the caller's power level.
    assert_eq!(status, StatusCode::NOT_FOUND);
}

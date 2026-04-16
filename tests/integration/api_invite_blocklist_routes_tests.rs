use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;

async fn setup_test_app() -> Option<axum::Router> {
    super::setup_test_app().await
}

async fn register_user(app: &axum::Router, username: &str) -> String {
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/r0/register")
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

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    json["access_token"].as_str().unwrap().to_string()
}

async fn create_room(app: &axum::Router, token: &str, name: &str) -> String {
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/r0/createRoom")
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

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 2048)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    json["room_id"].as_str().unwrap().to_string()
}

async fn register_user_with_id(app: &axum::Router, username: &str) -> (String, String) {
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/r0/register")
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

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    (
        json["access_token"].as_str().unwrap().to_string(),
        json["user_id"].as_str().unwrap().to_string(),
    )
}

async fn invite_user(app: &axum::Router, token: &str, room_id: &str, user_id: &str) {
    let request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/r0/rooms/{}/invite", room_id))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "user_id": user_id }).to_string()))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

async fn join_room(app: &axum::Router, token: &str, room_id: &str) {
    let request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/r0/rooms/{}/join", room_id))
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

async fn leave_room(app: &axum::Router, token: &str, room_id: &str) {
    let request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/r0/rooms/{}/leave", room_id))
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

async fn set_invite_list(
    app: &axum::Router,
    token: &str,
    room_id: &str,
    path: &str,
    user_ids: &[&str],
) -> StatusCode {
    let request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/v3/rooms/{}/{}", room_id, path))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "user_ids": user_ids
            })
            .to_string(),
        ))
        .unwrap();

    ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap()
        .status()
}

#[tokio::test]
async fn test_invite_blocklist_read_rejects_non_member() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let owner_token = register_user(&app, "invite_blocklist_owner_guard").await;
    let guest_token = register_user(&app, "invite_blocklist_guest_guard").await;
    let (admin_token, _) = super::get_admin_token(&app).await;
    let room_id = create_room(&app, &owner_token, "Invite Blocklist Guard").await;

    assert_eq!(
        set_invite_list(
            &app,
            &owner_token,
            &room_id,
            "invite_blocklist",
            &["@blocked:example.com"],
        )
        .await,
        StatusCode::OK
    );

    let guest_request = Request::builder()
        .method("GET")
        .uri(format!(
            "/_matrix/client/v3/rooms/{}/invite_blocklist",
            room_id
        ))
        .header("Authorization", format!("Bearer {}", guest_token))
        .body(Body::empty())
        .unwrap();
    let guest_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), guest_request)
        .await
        .unwrap();
    assert_eq!(guest_response.status(), StatusCode::FORBIDDEN);

    let admin_request = Request::builder()
        .method("GET")
        .uri(format!(
            "/_matrix/client/v3/rooms/{}/invite_blocklist",
            room_id
        ))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let admin_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), admin_request)
        .await
        .unwrap();
    assert_eq!(admin_response.status(), StatusCode::FORBIDDEN);

    let owner_request = Request::builder()
        .method("GET")
        .uri(format!(
            "/_matrix/client/v3/rooms/{}/invite_blocklist",
            room_id
        ))
        .header("Authorization", format!("Bearer {}", owner_token))
        .body(Body::empty())
        .unwrap();
    let owner_response = ServiceExt::<Request<Body>>::oneshot(app, owner_request)
        .await
        .unwrap();
    assert_eq!(owner_response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_invite_allowlist_read_rejects_non_member() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let owner_token = register_user(&app, "invite_allowlist_owner_guard").await;
    let guest_token = register_user(&app, "invite_allowlist_guest_guard").await;
    let (admin_token, _) = super::get_admin_token(&app).await;
    let room_id = create_room(&app, &owner_token, "Invite Allowlist Guard").await;

    assert_eq!(
        set_invite_list(
            &app,
            &owner_token,
            &room_id,
            "invite_allowlist",
            &["@allowed:example.com"],
        )
        .await,
        StatusCode::OK
    );

    let guest_request = Request::builder()
        .method("GET")
        .uri(format!(
            "/_matrix/client/v3/rooms/{}/invite_allowlist",
            room_id
        ))
        .header("Authorization", format!("Bearer {}", guest_token))
        .body(Body::empty())
        .unwrap();
    let guest_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), guest_request)
        .await
        .unwrap();
    assert_eq!(guest_response.status(), StatusCode::FORBIDDEN);

    let admin_request = Request::builder()
        .method("GET")
        .uri(format!(
            "/_matrix/client/v3/rooms/{}/invite_allowlist",
            room_id
        ))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let admin_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), admin_request)
        .await
        .unwrap();
    assert_eq!(admin_response.status(), StatusCode::FORBIDDEN);

    let owner_request = Request::builder()
        .method("GET")
        .uri(format!(
            "/_matrix/client/v3/rooms/{}/invite_allowlist",
            room_id
        ))
        .header("Authorization", format!("Bearer {}", owner_token))
        .body(Body::empty())
        .unwrap();
    let owner_response = ServiceExt::<Request<Body>>::oneshot(app, owner_request)
        .await
        .unwrap();
    assert_eq!(owner_response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_invite_lists_reject_joined_non_creator_writes() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let (owner_token, _) =
        register_user_with_id(&app, &format!("invite_owner_{}", rand::random::<u32>())).await;
    let (member_token, member_user_id) =
        register_user_with_id(&app, &format!("invite_member_{}", rand::random::<u32>())).await;
    let room_id = create_room(&app, &owner_token, "Invite List Write Guard").await;

    invite_user(&app, &owner_token, &room_id, &member_user_id).await;
    join_room(&app, &member_token, &room_id).await;

    assert_eq!(
        set_invite_list(
            &app,
            &member_token,
            &room_id,
            "invite_blocklist",
            &["@blocked:example.com"],
        )
        .await,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        set_invite_list(
            &app,
            &member_token,
            &room_id,
            "invite_allowlist",
            &["@allowed:example.com"],
        )
        .await,
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn test_invite_lists_reject_creator_after_leaving_room() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let owner_token = register_user(
        &app,
        &format!("invite_departed_owner_{}", rand::random::<u32>()),
    )
    .await;
    let room_id = create_room(&app, &owner_token, "Invite List Departed Owner Guard").await;

    assert_eq!(
        set_invite_list(
            &app,
            &owner_token,
            &room_id,
            "invite_blocklist",
            &["@blocked:example.com"],
        )
        .await,
        StatusCode::OK
    );

    leave_room(&app, &owner_token, &room_id).await;

    assert_eq!(
        set_invite_list(
            &app,
            &owner_token,
            &room_id,
            "invite_blocklist",
            &["@blocked-again:example.com"],
        )
        .await,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        set_invite_list(
            &app,
            &owner_token,
            &room_id,
            "invite_allowlist",
            &["@allowed:example.com"],
        )
        .await,
        StatusCode::FORBIDDEN
    );
}

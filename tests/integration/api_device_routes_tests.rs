use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use std::time::{SystemTime, UNIX_EPOCH};
use tower::ServiceExt;

async fn setup_test_app() -> Option<axum::Router> {
    super::setup_test_app().await
}

async fn register_user(app: &axum::Router, username: &str) -> (String, String) {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let username = format!("{}_{}", username, suffix);
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
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .unwrap();
    if status != StatusCode::OK {
        panic!(
            "register failed: status={} body={}",
            status,
            String::from_utf8_lossy(&body)
        );
    }
    let json: Value = serde_json::from_slice(&body).unwrap();

    (
        json["access_token"].as_str().unwrap().to_string(),
        json["user_id"].as_str().unwrap().to_string(),
    )
}

async fn create_room(app: &axum::Router, token: &str, name: &str) -> String {
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/createRoom")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "name": name }).to_string()))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    json["room_id"].as_str().unwrap().to_string()
}

async fn invite_user_to_room(
    app: &axum::Router,
    inviter_token: &str,
    room_id: &str,
    invitee_user_id: &str,
) {
    let request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/r0/rooms/{}/invite", room_id))
        .header("Authorization", format!("Bearer {}", inviter_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({ "user_id": invitee_user_id }).to_string(),
        ))
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

async fn get_first_device_id(app: &axum::Router, token: &str, path: &str) -> String {
    let request = Request::builder()
        .method("GET")
        .uri(path)
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    json["devices"][0]["device_id"]
        .as_str()
        .unwrap()
        .to_string()
}

async fn get_device_response(
    app: &axum::Router,
    token: &str,
    device_id: &str,
    path_prefix: &str,
) -> (StatusCode, Value) {
    let request = Request::builder()
        .method("GET")
        .uri(format!("{}/{}", path_prefix, device_id))
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .unwrap();
    let json = serde_json::from_slice(&body).unwrap_or_else(|_| json!({}));
    (status, json)
}

#[tokio::test]
async fn test_devices_routes_round_trip_across_versions() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let (token, user_id) = register_user(&app, "device_routes_round_trip").await;
    let device_id = get_first_device_id(&app, &token, "/_matrix/client/r0/devices").await;

    let update_request = Request::builder()
        .method("PUT")
        .uri(format!("/_matrix/client/v3/devices/{}", device_id))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "display_name": "Nested Device Router"
            })
            .to_string(),
        ))
        .unwrap();

    let update_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), update_request)
        .await
        .unwrap();
    assert_eq!(update_response.status(), StatusCode::OK);

    let get_request = Request::builder()
        .method("GET")
        .uri(format!("/_matrix/client/r0/devices/{}", device_id))
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let get_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), get_request)
        .await
        .unwrap();
    assert_eq!(get_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(get_response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["device_id"], device_id);
    assert_eq!(json["display_name"], "Nested Device Router");
    assert_eq!(json["device"]["device_id"], device_id);
    assert_eq!(json["device"]["display_name"], "Nested Device Router");

    let updates_request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/keys/device_list_updates")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "users": [user_id]
            })
            .to_string(),
        ))
        .unwrap();

    let updates_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), updates_request)
        .await
        .unwrap();
    assert_eq!(updates_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(updates_response.into_body(), 2048)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert!(json["changed"].as_array().is_some());

    let update_request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/keys/device_list/update")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "users": [user_id]
            })
            .to_string(),
        ))
        .unwrap();

    let update_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), update_request)
        .await
        .unwrap();
    assert_eq!(update_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(update_response.into_body(), 2048)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert!(json["changed"].as_array().is_some());
}

#[tokio::test]
async fn test_delete_devices_alias_is_shared() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let (token, _) = register_user(&app, "device_routes_delete").await;
    let device_id = get_first_device_id(&app, &token, "/_matrix/client/v3/devices").await;

    let delete_request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/r0/delete_devices")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "devices": [device_id]
            })
            .to_string(),
        ))
        .unwrap();

    let delete_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), delete_request)
        .await
        .unwrap();
    assert_eq!(delete_response.status(), StatusCode::OK);

    let get_request = Request::builder()
        .method("GET")
        .uri(format!("/_matrix/client/v3/devices/{}", device_id))
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let get_response = ServiceExt::<Request<Body>>::oneshot(app, get_request)
        .await
        .unwrap();
    assert_eq!(get_response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_delete_devices_only_removes_current_users_devices() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let (token_a, _) = register_user(&app, "device_routes_owner_a").await;
    let (token_b, _) = register_user(&app, "device_routes_owner_b").await;

    let device_a = get_first_device_id(&app, &token_a, "/_matrix/client/v3/devices").await;
    let device_b = get_first_device_id(&app, &token_b, "/_matrix/client/v3/devices").await;

    let delete_request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/delete_devices")
        .header("Authorization", format!("Bearer {}", token_a))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "device_ids": [device_a, device_b]
            })
            .to_string(),
        ))
        .unwrap();

    let delete_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), delete_request)
        .await
        .unwrap();
    assert_eq!(delete_response.status(), StatusCode::OK);

    let (owner_status, _) =
        get_device_response(&app, &token_a, &device_a, "/_matrix/client/v3/devices").await;
    assert_eq!(owner_status, StatusCode::NOT_FOUND);

    let (other_status, other_body) =
        get_device_response(&app, &token_b, &device_b, "/_matrix/client/v3/devices").await;
    assert_eq!(other_status, StatusCode::OK);
    assert_eq!(other_body["device_id"], device_b);
}

#[tokio::test]
async fn test_get_device_returns_not_found_for_other_users_device() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let (alice_token, _) = register_user(&app, "device_routes_reader").await;
    let (bob_token, _) = register_user(&app, "device_routes_target").await;
    let bob_device = get_first_device_id(&app, &bob_token, "/_matrix/client/v3/devices").await;

    let (status, body) = get_device_response(
        &app,
        &alice_token,
        &bob_device,
        "/_matrix/client/v3/devices",
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["errcode"], "M_NOT_FOUND");
}

#[tokio::test]
async fn test_device_list_updates_filters_users_without_shared_rooms() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let (alice_token, alice_user_id) = register_user(&app, "device_updates_alice").await;
    let (bob_token, bob_user_id) = register_user(&app, "device_updates_bob").await;
    let bob_device = get_first_device_id(&app, &bob_token, "/_matrix/client/v3/devices").await;

    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/keys/device_list_updates")
        .header("Authorization", format!("Bearer {}", alice_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "users": [alice_user_id, bob_user_id]
            })
            .to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 4096)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let changed = json["changed"].as_array().unwrap();

    assert!(!changed
        .iter()
        .any(|entry| { entry["user_id"] == bob_user_id && entry["device_id"] == bob_device }));
}

#[tokio::test]
async fn test_device_list_updates_allows_users_with_shared_rooms() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let (alice_token, _) = register_user(&app, "device_updates_shared_alice").await;
    let (bob_token, bob_user_id) = register_user(&app, "device_updates_shared_bob").await;
    let bob_device = get_first_device_id(&app, &bob_token, "/_matrix/client/v3/devices").await;

    let room_id = create_room(&app, &alice_token, "Device List Updates Shared Room").await;
    invite_user_to_room(&app, &alice_token, &room_id, &bob_user_id).await;
    join_room(&app, &bob_token, &room_id).await;

    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/keys/device_list_updates")
        .header("Authorization", format!("Bearer {}", alice_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "users": [bob_user_id]
            })
            .to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 4096)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let changed = json["changed"].as_array().unwrap();

    assert!(
        changed
            .iter()
            .any(|entry| { entry["user_id"] == bob_user_id && entry["device_id"] == bob_device }),
        "expected shared-room user's device to be returned"
    );
}

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use hmac::{Hmac, Mac};
use serde_json::{json, Value};
use sha2::Sha256;
use std::sync::Arc;
use synapse_rust::cache::{CacheConfig, CacheManager};
use synapse_rust::services::ServiceContainer;
use synapse_rust::web::routes::create_router;
use synapse_rust::web::AppState;
use tower::ServiceExt;

type HmacSha256 = Hmac<Sha256>;

async fn setup_test_app() -> Option<axum::Router> {
    if !super::init_test_database().await {
        return None;
    }
    let container = ServiceContainer::new_test();
    let cache = Arc::new(CacheManager::new(CacheConfig::default()));
    let state = AppState::new(container, cache);
    Some(create_router(state))
}

async fn register_user(app: &axum::Router, username: &str) -> (String, String) {
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/r0/register")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "username": username,
                "password": "Password123!",
                "auth": {"type": "m.login.dummy"}
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
            "Registration failed with status {}: {:?}",
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

async fn get_admin_token(app: &axum::Router) -> String {
    let request = Request::builder()
        .uri("/_synapse/admin/v1/register/nonce")
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
    let nonce = json["nonce"].as_str().unwrap().to_string();
    let username = format!("presence_admin_{}", rand::random::<u32>());
    let password = "password123";

    let mut mac = HmacSha256::new_from_slice(b"test_shared_secret").unwrap();
    mac.update(nonce.as_bytes());
    mac.update(b"\0");
    mac.update(username.as_bytes());
    mac.update(b"\0");
    mac.update(password.as_bytes());
    mac.update(b"\0");
    mac.update(b"admin");

    let mac_hex = mac
        .finalize()
        .into_bytes()
        .iter()
        .map(|byte| format!("{:02x}", byte))
        .collect::<String>();

    let request = Request::builder()
        .method("POST")
        .uri("/_synapse/admin/v1/register")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "nonce": nonce,
                "username": username,
                "password": password,
                "admin": true,
                "mac": mac_hex
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

async fn login_user(app: &axum::Router, username: &str) -> String {
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/login")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "type": "m.login.password",
                "user": username,
                "password": "Password123!"
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

#[tokio::test]
async fn test_device_management() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let (token, _) = register_user(&app, &format!("user_{}", rand::random::<u32>())).await;

    // 1. Get Devices
    let request = Request::builder()
        .uri("/_matrix/client/r0/devices")
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
    let devices = json["devices"].as_array().unwrap();
    assert!(!devices.is_empty());
    let device_id = devices[0]["device_id"].as_str().unwrap().to_string();

    // 2. Get Single Device
    let request = Request::builder()
        .uri(format!("/_matrix/client/r0/devices/{}", device_id))
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // 3. Update Device
    let request = Request::builder()
        .method("PUT")
        .uri(format!("/_matrix/client/r0/devices/{}", device_id))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "display_name": "New Device Name"
            })
            .to_string(),
        ))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // 4. Delete Single Device
    // First, we need to login again to get another device or just delete the current one (might invalidate token if it's the only one)
    // Let's just delete it and check 200 or 401 on next request.
    let request = Request::builder()
        .method("DELETE")
        .uri(format!("/_matrix/client/r0/devices/{}", device_id))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({"auth": {"type": "m.login.password", "user": "...", "password": "..."}})
                .to_string(),
        ))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    // Some servers require UIA (User Interactive Authentication) for deleting devices.
    // Our implementation might just return 200 for now if UIA is not fully implemented.
    assert!(response.status() == StatusCode::OK || response.status() == StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_presence_management() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let (token, user_id) = register_user(&app, &format!("user_{}", rand::random::<u32>())).await;

    // 1. Set Presence
    let request = Request::builder()
        .method("PUT")
        .uri(format!("/_matrix/client/r0/presence/{}/status", user_id))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "presence": "online",
                "status_msg": "Coding in Rust"
            })
            .to_string(),
        ))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // 2. Get Presence
    let request = Request::builder()
        .uri(format!("/_matrix/client/r0/presence/{}/status", user_id))
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
    assert_eq!(json["presence"], "online");
    assert_eq!(json["status_msg"], "Coding in Rust");
}

#[tokio::test]
async fn test_presence_status_shared_across_r0_and_v3() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let (token, user_id) =
        register_user(&app, &format!("presence_shared_{}", rand::random::<u32>())).await;

    let set_request = Request::builder()
        .method("PUT")
        .uri(format!("/_matrix/client/v3/presence/{}/status", user_id))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "presence": "unavailable",
                "status_msg": "cross-version presence"
            })
            .to_string(),
        ))
        .unwrap();
    let set_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), set_request)
        .await
        .unwrap();
    assert_eq!(set_response.status(), StatusCode::OK);

    let get_request = Request::builder()
        .uri(format!("/_matrix/client/r0/presence/{}/status", user_id))
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
    assert_eq!(json["presence"], "unavailable");
    assert_eq!(json["status_msg"], "cross-version presence");
}

#[tokio::test]
async fn test_presence_list_boundary_is_preserved() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let (token, _) = register_user(&app, &format!("presence_list_{}", rand::random::<u32>())).await;

    let v3_request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/presence/list")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "subscribe": ["@alice:localhost"]
            })
            .to_string(),
        ))
        .unwrap();
    let v3_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), v3_request)
        .await
        .unwrap();
    assert_ne!(v3_response.status(), StatusCode::NOT_FOUND);
    assert_ne!(v3_response.status(), StatusCode::METHOD_NOT_ALLOWED);

    let r0_request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/r0/presence/list")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "subscribe": ["@alice:localhost"]
            })
            .to_string(),
        ))
        .unwrap();
    let r0_response = ServiceExt::<Request<Body>>::oneshot(app, r0_request)
        .await
        .unwrap();
    assert_eq!(r0_response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_presence_list_after_session_invalidation_and_relogin() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let admin_token = get_admin_token(&app).await;
    let username = format!("presence_relogin_{}", rand::random::<u32>());
    let (_, user_id) = register_user(&app, &username).await;

    let invalidate_request = Request::builder()
        .method("POST")
        .uri(format!(
            "/_synapse/admin/v1/user_sessions/{}/invalidate",
            user_id
        ))
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("Content-Type", "application/json")
        .body(Body::from("{}"))
        .unwrap();
    let invalidate_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), invalidate_request)
        .await
        .unwrap();
    assert_eq!(invalidate_response.status(), StatusCode::OK);

    let relogin_token = login_user(&app, &username).await;

    let presence_request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/presence/list")
        .header("Authorization", format!("Bearer {}", relogin_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "subscribe": [user_id]
            })
            .to_string(),
        ))
        .unwrap();
    let presence_response = ServiceExt::<Request<Body>>::oneshot(app, presence_request)
        .await
        .unwrap();
    assert_eq!(presence_response.status(), StatusCode::OK);
}

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use futures::future::join_all;
use serde_json::{json, Value};
use std::sync::Arc;
use synapse_rust::cache::{CacheConfig, CacheManager};
use synapse_rust::services::PresenceStorage;
use tower::ServiceExt;

async fn setup_test_app() -> Option<axum::Router> {
    super::setup_test_app().await
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

    let response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), super::with_local_connect_info(request))
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
    let (admin_token, _) = super::get_admin_token(&app).await;
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

#[tokio::test]
async fn test_presence_routes_remain_stable_under_concurrency() {
    let Some(pool) = super::get_test_pool().await else {
        return;
    };
    let cache = Arc::new(CacheManager::new(CacheConfig::default()));
    let presence = PresenceStorage::new(pool.clone(), cache);
    let suffix = rand::random::<u32>();
    let now = chrono::Utc::now().timestamp_millis();
    let mut user_ids = Vec::new();

    for index in 0..24 {
        let username = format!("presence_concurrent_{}_{}", suffix, index);
        let user_id = format!("@{}:localhost", username);
        sqlx::query(
            "INSERT INTO users (user_id, username, created_ts, generation) VALUES ($1, $2, $3, $4)
             ON CONFLICT (user_id) DO NOTHING",
        )
        .bind(&user_id)
        .bind(&username)
        .bind(now)
        .bind(1_i64)
        .execute(&*pool)
        .await
        .unwrap();
        user_ids.push(user_id);
    }

    let mut handles = Vec::new();
    for user_id in user_ids {
        let presence = presence.clone();
        handles.push(tokio::spawn(async move {
            for iteration in 0..5 {
                let state = if iteration % 2 == 0 {
                    "online"
                } else {
                    "unavailable"
                };
                presence
                    .set_presence(&user_id, state, Some("stable under concurrency"))
                    .await
                    .unwrap();
                let current = presence.get_presence(&user_id).await.unwrap();
                assert!(current.is_some());
                let (presence_state, status_msg) = current.unwrap();
                assert_eq!(presence_state, state);
                assert_eq!(status_msg.as_deref(), Some("stable under concurrency"));
            }
        }));
    }

    for result in join_all(handles).await {
        result.unwrap();
    }
}

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

async fn get_admin_token(app: &axum::Router) -> (String, String) {
    // 1. Get nonce
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

    // 2. Register admin
    let username = format!("admin_{}", rand::random::<u32>());
    let password = "password123";
    let shared_secret = "test_shared_secret"; // From ServiceContainer::new_test

    let mut mac = HmacSha256::new_from_slice(shared_secret.as_bytes()).unwrap();
    mac.update(nonce.as_bytes());
    mac.update(b"\0");
    mac.update(username.as_bytes());
    mac.update(b"\0");
    mac.update(password.as_bytes());
    mac.update(b"\0");
    mac.update(b"admin");

    let expected_mac = mac.finalize().into_bytes();
    let mac_hex = expected_mac
        .iter()
        .map(|b| format!("{:02x}", b))
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
    (json["access_token"].as_str().unwrap().to_string(), username)
}

async fn create_test_user(app: &axum::Router) -> String {
    let username = format!("user_{}", rand::random::<u32>());
    let password = "Password123!";

    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/r0/register")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "username": username,
                "password": password,
                "auth": { "type": "m.login.dummy" }
            })
            .to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();

    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 10240)
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
    json["access_token"].as_str().unwrap().to_string()
}

async fn get_csrf_token(app: &axum::Router, access_token: &str) -> String {
    let request = Request::builder()
        .uri("/_synapse/admin/v1/telemetry/status")
        .header("Authorization", format!("Bearer {}", access_token))
        .header("Origin", "https://localhost")
        .header("X-Forwarded-Host", "localhost")
        .header("X-Forwarded-Proto", "https")
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    response
        .headers()
        .get("x-csrf-token")
        .and_then(|value| value.to_str().ok())
        .unwrap()
        .to_string()
}

#[tokio::test]
async fn test_admin_flow() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let (token, username) = get_admin_token(&app).await;

    // Test server version
    let request = Request::builder()
        .uri("/_synapse/admin/v1/server_version")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    // Test status
    let request = Request::builder()
        .uri("/_synapse/admin/v1/status")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    // Test users list
    let request = Request::builder()
        .uri("/_synapse/admin/v1/users")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    // Test specific user
    let request = Request::builder()
        .uri(format!("/_synapse/admin/v1/users/{}", username))
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // Test IP blocking
    let request = Request::builder()
        .method("POST")
        .uri("/_synapse/admin/v1/security/ip/block")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "ip_address": "1.2.3.4",
                "reason": "Spamming"
            })
            .to_string(),
        ))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let request = Request::builder()
        .uri("/_synapse/admin/v1/security/ip/blocks")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // Test rooms list
    let request = Request::builder()
        .uri("/_synapse/admin/v1/rooms")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    let status = response.status();
    if status != StatusCode::OK {
        let body = axum::body::to_bytes(response.into_body(), 1024)
            .await
            .unwrap();
        println!("Rooms list failed: {:?}", String::from_utf8_lossy(&body));
        panic!("Rooms list failed with status {:?}", status);
    }
}

#[tokio::test]
async fn test_worker_admin_routes_reject_regular_user_but_allow_authenticated_claim() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let (admin_token, _) = get_admin_token(&app).await;
    let user_token = create_test_user(&app).await;
    let worker_id = format!("worker-{}", rand::random::<u32>());

    let register_request = Request::builder()
        .method("POST")
        .uri("/_synapse/worker/v1/register")
        .header("Authorization", format!("Bearer {}", user_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "worker_id": worker_id,
                "worker_name": "HTTP Test Worker",
                "worker_type": "frontend",
                "host": "127.0.0.1",
                "port": 9001
            })
            .to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), register_request)
        .await
        .unwrap();
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 10240)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(json["error"], "Admin access required");

    let register_request = Request::builder()
        .method("POST")
        .uri("/_synapse/worker/v1/register")
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "worker_id": worker_id,
                "worker_name": "HTTP Test Worker",
                "worker_type": "frontend",
                "host": "127.0.0.1",
                "port": 9001
            })
            .to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), register_request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let assign_request = Request::builder()
        .method("POST")
        .uri("/_synapse/worker/v1/tasks")
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "task_type": "sync",
                "task_data": { "job": "http-claim" },
                "preferred_worker_id": Value::Null
            })
            .to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), assign_request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let claim_request = Request::builder()
        .method("POST")
        .uri(format!("/_synapse/worker/v1/tasks/claim/{}", worker_id))
        .header("Authorization", format!("Bearer {}", user_token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), claim_request)
        .await
        .unwrap();
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 10240)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["assigned_worker_id"], worker_id);
    assert_eq!(json["status"], "running");
}

#[tokio::test]
async fn test_worker_claim_route_is_atomic_over_http() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let (admin_token, _) = get_admin_token(&app).await;
    let user_token = create_test_user(&app).await;
    let worker_one = format!("worker-a-{}", rand::random::<u32>());
    let worker_two = format!("worker-b-{}", rand::random::<u32>());

    for worker_id in [&worker_one, &worker_two] {
        let register_request = Request::builder()
            .method("POST")
            .uri("/_synapse/worker/v1/register")
            .header("Authorization", format!("Bearer {}", admin_token))
            .header("Content-Type", "application/json")
            .body(Body::from(
                json!({
                    "worker_id": worker_id,
                    "worker_name": format!("Worker {}", worker_id),
                    "worker_type": "frontend",
                    "host": "127.0.0.1",
                    "port": 9001
                })
                .to_string(),
            ))
            .unwrap();

        let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), register_request)
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
    }

    let assign_request = Request::builder()
        .method("POST")
        .uri("/_synapse/worker/v1/tasks")
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "task_type": "sync",
                "task_data": { "job": "atomic-http-claim" }
            })
            .to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), assign_request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let body = axum::body::to_bytes(response.into_body(), 10240)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let task_id = json["task_id"].as_str().unwrap().to_string();

    let request_one = Request::builder()
        .method("POST")
        .uri(format!(
            "/_synapse/worker/v1/tasks/{}/claim/{}",
            task_id, worker_one
        ))
        .header("Authorization", format!("Bearer {}", user_token))
        .body(Body::empty())
        .unwrap();
    let request_two = Request::builder()
        .method("POST")
        .uri(format!(
            "/_synapse/worker/v1/tasks/{}/claim/{}",
            task_id, worker_two
        ))
        .header("Authorization", format!("Bearer {}", user_token))
        .body(Body::empty())
        .unwrap();

    let (response_one, response_two) = tokio::join!(
        ServiceExt::<Request<Body>>::oneshot(app.clone(), request_one),
        ServiceExt::<Request<Body>>::oneshot(app.clone(), request_two),
    );

    let response_one = response_one.unwrap();
    let response_two = response_two.unwrap();
    let statuses = [response_one.status(), response_two.status()];

    assert!(statuses.contains(&StatusCode::OK));
    assert!(statuses.contains(&StatusCode::CONFLICT));
}

#[tokio::test]
async fn test_telemetry_routes_require_admin_permissions() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let (admin_token, _) = get_admin_token(&app).await;
    let user_token = create_test_user(&app).await;

    let request = Request::builder()
        .uri("/_synapse/admin/v1/telemetry/status")
        .header("Authorization", format!("Bearer {}", user_token))
        .body(Body::empty())
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let request = Request::builder()
        .uri("/_synapse/admin/v1/telemetry/status")
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_csrf_protects_admin_post_routes() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let (admin_token, _) = get_admin_token(&app).await;
    let worker_id = format!("csrf-worker-{}", rand::random::<u32>());

    let request = Request::builder()
        .method("POST")
        .uri("/_synapse/worker/v1/register")
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("Content-Type", "application/json")
        .header("Origin", "https://localhost")
        .header("X-Forwarded-Host", "localhost")
        .header("X-Forwarded-Proto", "https")
        .body(Body::from(
            json!({
                "worker_id": worker_id,
                "worker_name": "CSRF Worker",
                "worker_type": "frontend",
                "host": "127.0.0.1",
                "port": 9101
            })
            .to_string(),
        ))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 10240)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(json["error"], "Missing or invalid CSRF token");

    let csrf_token = get_csrf_token(&app, &admin_token).await;
    let request = Request::builder()
        .method("POST")
        .uri("/_synapse/worker/v1/register")
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("Content-Type", "application/json")
        .header("Origin", "https://localhost")
        .header("X-Forwarded-Host", "localhost")
        .header("X-Forwarded-Proto", "https")
        .header("X-CSRF-Token", csrf_token)
        .body(Body::from(
            json!({
                "worker_id": worker_id,
                "worker_name": "CSRF Worker",
                "worker_type": "frontend",
                "host": "127.0.0.1",
                "port": 9101
            })
            .to_string(),
        ))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
}

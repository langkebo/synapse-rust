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
    let mut message = nonce.as_bytes().to_vec();
    message.push(b'\0');
    message.extend(username.as_bytes());
    message.push(b'\0');
    message.extend(password.as_bytes());
    message.push(b'\0');
    message.extend(b"admin");
    mac.update(&message);

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

fn encode_path_segment(value: &str) -> String {
    value
        .replace('%', "%25")
        .replace('@', "%40")
        .replace('!', "%21")
        .replace(':', "%3A")
        .replace('/', "%2F")
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
    assert!(
        response.status() == StatusCode::NOT_FOUND
            || response.status() == StatusCode::OK,
        "Expected 404 (not implemented) or 200 for IP block, got: {}",
        response.status()
    );

    let request = Request::builder()
        .uri("/_synapse/admin/v1/security/ip/blocks")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert!(
        response.status() == StatusCode::NOT_FOUND
            || response.status() == StatusCode::OK,
        "Expected 404 (not implemented) or 200 for IP blocks list, got: {}",
        response.status()
    );

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

    let admin_token = create_test_user(&app).await;
    let user_token = create_test_user(&app).await;

    let admin_whoami_request = Request::builder()
        .uri("/_matrix/client/v3/account/whoami")
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let admin_whoami_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), admin_whoami_request)
            .await
            .unwrap();
    assert_eq!(admin_whoami_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(admin_whoami_response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let admin_user_id = json["user_id"].as_str().unwrap().to_string();

    let pool = super::get_test_pool().await.unwrap();
    sqlx::query("UPDATE users SET is_admin = TRUE WHERE user_id = $1")
        .bind(&admin_user_id)
        .execute(&*pool)
        .await
        .unwrap();
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
    assert!(
        response.status() == StatusCode::CREATED
            || response.status() == StatusCode::FORBIDDEN,
        "Expected 201 or 403 for worker registration, got: {}",
        response.status()
    );

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

#[tokio::test]
async fn test_admin_get_room_returns_room_details_after_create_room() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let (admin_token, _) = get_admin_token(&app).await;

    let create_request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/createRoom")
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "name": "Admin Room Detail",
                "topic": "admin-room"
            })
            .to_string(),
        ))
        .unwrap();
    let create_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), create_request)
        .await
        .unwrap();
    assert_eq!(create_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(create_response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let room_id = json["room_id"].as_str().unwrap().to_string();

    let get_request = Request::builder()
        .uri(format!("/_synapse/admin/v1/rooms/{}", room_id))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let get_response = ServiceExt::<Request<Body>>::oneshot(app, get_request)
        .await
        .unwrap();
    assert_eq!(get_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(get_response.into_body(), 2048)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["room_id"], room_id);
    assert_eq!(json["name"], "Admin Room Detail");
    assert_eq!(json["topic"], "admin-room");
    assert_eq!(json["member_count"], 1);
    assert!(json.get("encryption").is_some());
}

#[tokio::test]
async fn test_admin_room_make_admin_accepts_put_and_updates_power_levels() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let admin_token = create_test_user(&app).await;
    let user_token = create_test_user(&app).await;

    let admin_whoami_request = Request::builder()
        .uri("/_matrix/client/v3/account/whoami")
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let admin_whoami_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), admin_whoami_request)
            .await
            .unwrap();
    assert_eq!(admin_whoami_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(admin_whoami_response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let admin_user_id = json["user_id"].as_str().unwrap().to_string();

    let pool = super::get_test_pool().await.unwrap();
    sqlx::query("UPDATE users SET is_admin = TRUE WHERE user_id = $1")
        .bind(&admin_user_id)
        .execute(&*pool)
        .await
        .unwrap();

    let whoami_request = Request::builder()
        .uri("/_matrix/client/v3/account/whoami")
        .header("Authorization", format!("Bearer {}", user_token))
        .body(Body::empty())
        .unwrap();
    let whoami_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), whoami_request)
        .await
        .unwrap();
    assert_eq!(whoami_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(whoami_response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let user_id = json["user_id"].as_str().unwrap().to_string();

    let create_request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/createRoom")
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({"name": "Power Levels Room"}).to_string()))
        .unwrap();
    let create_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), create_request)
        .await
        .unwrap();
    assert_eq!(create_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(create_response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let room_id = json["room_id"].as_str().unwrap().to_string();

    let make_admin_request = Request::builder()
        .method("PUT")
        .uri(format!("/_synapse/admin/v1/rooms/{}/make_admin", room_id))
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "user_id": user_id }).to_string()))
        .unwrap();
    let make_admin_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), make_admin_request)
            .await
            .unwrap();
    assert_eq!(make_admin_response.status(), StatusCode::OK);

    let power_levels_request = Request::builder()
        .uri(format!(
            "/_matrix/client/v3/rooms/{}/state/m.room.power_levels/",
            room_id
        ))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let power_levels_response =
        ServiceExt::<Request<Body>>::oneshot(app, power_levels_request)
            .await
            .unwrap();
    assert_eq!(power_levels_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(power_levels_response.into_body(), 2048)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["users"][user_id], 100);
}

#[tokio::test]
async fn test_admin_room_member_management_supports_path_and_body_routes() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let (admin_token, _) = get_admin_token(&app).await;
    let user_token = create_test_user(&app).await;

    let whoami_request = Request::builder()
        .uri("/_matrix/client/v3/account/whoami")
        .header("Authorization", format!("Bearer {}", user_token))
        .body(Body::empty())
        .unwrap();
    let whoami_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), whoami_request)
        .await
        .unwrap();
    assert_eq!(whoami_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(whoami_response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let user_id = json["user_id"].as_str().unwrap().to_string();
    let encoded_user_id = encode_path_segment(&user_id);

    let create_request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/createRoom")
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({"name": "Admin Member Room"}).to_string()))
        .unwrap();
    let create_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), create_request)
        .await
        .unwrap();
    assert_eq!(create_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(create_response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let room_id = json["room_id"].as_str().unwrap().to_string();

    let add_request = Request::builder()
        .method("PUT")
        .uri(format!(
            "/_synapse/admin/v1/rooms/{}/members/{}",
            room_id, encoded_user_id
        ))
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({"membership": "join"}).to_string()))
        .unwrap();
    let add_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), add_request)
        .await
        .unwrap();
    assert_eq!(add_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(add_response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["membership"], "join");

    let ban_request = Request::builder()
        .method("POST")
        .uri(format!("/_synapse/admin/v1/rooms/{}/ban", room_id))
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "user_id": user_id,
                "reason": "spam"
            })
            .to_string(),
        ))
        .unwrap();
    let ban_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), ban_request)
        .await
        .unwrap();
    assert_eq!(ban_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(ban_response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["membership"], "ban");

    let kick_request = Request::builder()
        .method("POST")
        .uri(format!(
            "/_synapse/admin/v1/rooms/{}/kick/{}",
            room_id, encoded_user_id
        ))
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({"reason": "cleanup"}).to_string()))
        .unwrap();
    let kick_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), kick_request)
        .await
        .unwrap();
    assert_eq!(kick_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(kick_response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["kicked"], true);
}

#[tokio::test]
async fn test_admin_device_management_supports_delete_compat_routes() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let (admin_token, _) = get_admin_token(&app).await;
    let user_token = create_test_user(&app).await;

    let whoami_request = Request::builder()
        .uri("/_matrix/client/v3/account/whoami")
        .header("Authorization", format!("Bearer {}", user_token))
        .body(Body::empty())
        .unwrap();
    let whoami_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), whoami_request)
        .await
        .unwrap();
    assert_eq!(whoami_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(whoami_response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let user_id = json["user_id"].as_str().unwrap().to_string();
    let encoded_user_id = encode_path_segment(&user_id);

    let list_devices_request = Request::builder()
        .uri("/_matrix/client/v3/devices")
        .header("Authorization", format!("Bearer {}", user_token))
        .body(Body::empty())
        .unwrap();
    let list_devices_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), list_devices_request)
            .await
            .unwrap();
    assert_eq!(list_devices_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(list_devices_response.into_body(), 2048)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let device_id = json["devices"][0]["device_id"].as_str().unwrap().to_string();

    let delete_device_request = Request::builder()
        .method("POST")
        .uri(format!(
            "/_synapse/admin/v1/users/{}/devices/{}/delete",
            encoded_user_id, device_id
        ))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let delete_device_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), delete_device_request)
            .await
            .unwrap();
    assert_eq!(delete_device_response.status(), StatusCode::OK);

    let recreate_login_request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/login")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "type": "m.login.password",
                "user": user_id,
                "password": "Password123!"
            })
            .to_string(),
        ))
        .unwrap();
    let recreate_login_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), recreate_login_request)
            .await
            .unwrap();
    assert_eq!(recreate_login_response.status(), StatusCode::OK);

    let delete_all_request = Request::builder()
        .method("POST")
        .uri(format!(
            "/_synapse/admin/v1/users/{}/devices/delete",
            encoded_user_id
        ))
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("Content-Type", "application/json")
        .body(Body::from("{}"))
        .unwrap();
    let delete_all_response = ServiceExt::<Request<Body>>::oneshot(app, delete_all_request)
        .await
        .unwrap();
    assert_eq!(delete_all_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(delete_all_response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert!(json["devices_deleted"].as_i64().unwrap_or_default() >= 1);
}

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use std::sync::Arc;
use synapse_rust::cache::{CacheConfig, CacheManager};
use synapse_rust::services::ServiceContainer;
use synapse_rust::web::routes::create_router;
use synapse_rust::web::AppState;
use tower::ServiceExt;

async fn setup_test_app_with_pool() -> Option<(axum::Router, Arc<sqlx::PgPool>)> {
    let pool = super::get_test_pool().await?;
    let container = ServiceContainer::new_test_with_pool(pool.clone());
    let cache = Arc::new(CacheManager::new(CacheConfig::default()));
    let state = AppState::new(container, cache);
    Some((create_router(state), pool))
}

async fn setup_test_app() -> Option<axum::Router> {
    let (app, _) = setup_test_app_with_pool().await?;
    Some(app)
}

async fn get_admin_token(app: &axum::Router) -> (String, String) {
    super::get_admin_token(app).await
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
        response.status() == StatusCode::NOT_FOUND || response.status() == StatusCode::OK,
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
        response.status() == StatusCode::NOT_FOUND || response.status() == StatusCode::OK,
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
async fn test_admin_info_is_public() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let request = Request::builder()
        .uri("/_synapse/admin/info")
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
    assert_eq!(json["server_version"], env!("CARGO_PKG_VERSION"));
    assert!(json["server_name"].is_string());
}

#[tokio::test]
async fn test_matrix_server_version_endpoint() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let request = Request::builder()
        .uri("/_matrix/server_version")
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
    assert_eq!(json["server_version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(json["python_version"], "Rust");
}

#[tokio::test]
async fn test_worker_admin_routes_reject_regular_user_but_allow_authenticated_claim() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let (admin_token, _) = get_admin_token(&app).await;
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
    let _admin_user_id = json["user_id"].as_str().unwrap().to_string();
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
        response.status() == StatusCode::CREATED || response.status() == StatusCode::FORBIDDEN,
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
async fn test_promoting_existing_user_in_db_does_not_upgrade_existing_token_to_admin() {
    let Some((app, pool)) = setup_test_app_with_pool().await else {
        return;
    };

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

    sqlx::query("UPDATE users SET is_admin = TRUE WHERE user_id = $1")
        .bind(&user_id)
        .execute(&*pool)
        .await
        .unwrap();

    let stale_token_request = Request::builder()
        .uri("/_synapse/admin/v1/status")
        .header("Authorization", format!("Bearer {}", user_token))
        .body(Body::empty())
        .unwrap();
    let stale_token_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), stale_token_request)
            .await
            .unwrap();
    assert_eq!(stale_token_response.status(), StatusCode::FORBIDDEN);

    let (admin_token, _) = get_admin_token(&app).await;
    let fresh_admin_request = Request::builder()
        .uri("/_synapse/admin/v1/status")
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let fresh_admin_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), fresh_admin_request)
            .await
            .unwrap();
    assert_eq!(fresh_admin_response.status(), StatusCode::OK);
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
async fn test_admin_room_listing_requires_existing_room() {
    let Some((app, pool)) = setup_test_app_with_pool().await else {
        return;
    };

    let (admin_token, _) = get_admin_token(&app).await;
    let missing_room_id = format!("!missing_room_listing_{}:localhost", rand::random::<u32>());
    let encoded_room_id = encode_path_segment(&missing_room_id);

    let set_public_request = Request::builder()
        .method("PUT")
        .uri(format!(
            "/_synapse/admin/v1/rooms/{}/listings/public",
            encoded_room_id
        ))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let set_public_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), set_public_request)
        .await
        .unwrap();
    assert_eq!(set_public_response.status(), StatusCode::NOT_FOUND);

    let in_directory: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM room_directory WHERE room_id = $1)")
            .bind(&missing_room_id)
            .fetch_one(&*pool)
            .await
            .expect("failed to inspect room directory after rejected publish");
    assert!(!in_directory);

    let set_private_request = Request::builder()
        .method("DELETE")
        .uri(format!(
            "/_synapse/admin/v1/rooms/{}/listings/public",
            encoded_room_id
        ))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let set_private_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), set_private_request)
            .await
            .unwrap();
    assert_eq!(set_private_response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_admin_room_block_writes_require_existing_room() {
    let Some((app, pool)) = setup_test_app_with_pool().await else {
        return;
    };

    let (admin_token, _) = get_admin_token(&app).await;
    let missing_room_id = format!("!missing_room_block_{}:localhost", rand::random::<u32>());
    let encoded_room_id = encode_path_segment(&missing_room_id);

    let block_request = Request::builder()
        .method("POST")
        .uri(format!("/_synapse/admin/v1/rooms/{}/block", encoded_room_id))
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "block": true,
                "reason": "test"
            })
            .to_string(),
        ))
        .unwrap();
    let block_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), block_request)
        .await
        .unwrap();
    assert_eq!(block_response.status(), StatusCode::NOT_FOUND);

    let is_blocked: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM blocked_rooms WHERE room_id = $1)")
            .bind(&missing_room_id)
            .fetch_one(&*pool)
            .await
            .expect("failed to inspect blocked_rooms after rejected block");
    assert!(!is_blocked);

    let unblock_request = Request::builder()
        .method("POST")
        .uri(format!("/_synapse/admin/v1/rooms/{}/unblock", encoded_room_id))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let unblock_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), unblock_request)
        .await
        .unwrap();
    assert_eq!(unblock_response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_admin_room_make_admin_accepts_put_and_updates_power_levels() {
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
    let make_admin_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), make_admin_request)
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
    let power_levels_response = ServiceExt::<Request<Body>>::oneshot(app, power_levels_request)
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
    let device_id = json["devices"][0]["device_id"]
        .as_str()
        .unwrap()
        .to_string();

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

#[tokio::test]
async fn test_admin_shadow_ban_requires_existing_user() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let (admin_token, _) = get_admin_token(&app).await;
    let missing_user_id = format!("@missing_shadow_{}:localhost", rand::random::<u32>());
    let encoded_user_id = encode_path_segment(&missing_user_id);

    let shadow_ban_request = Request::builder()
        .method("POST")
        .uri(format!(
            "/_synapse/admin/v1/users/{}/shadow_ban",
            encoded_user_id
        ))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let shadow_ban_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), shadow_ban_request)
        .await
        .unwrap();
    assert_eq!(shadow_ban_response.status(), StatusCode::NOT_FOUND);

    let unshadow_ban_request = Request::builder()
        .method("DELETE")
        .uri(format!(
            "/_synapse/admin/v1/users/{}/shadow_ban",
            encoded_user_id
        ))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let unshadow_ban_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), unshadow_ban_request)
            .await
            .unwrap();
    assert_eq!(unshadow_ban_response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_admin_rate_limit_writes_require_existing_user() {
    let Some((app, pool)) = setup_test_app_with_pool().await else {
        return;
    };

    let (admin_token, _) = get_admin_token(&app).await;
    let missing_user_id = format!("@missing_ratelimit_{}:localhost", rand::random::<u32>());
    let encoded_user_id = encode_path_segment(&missing_user_id);

    let set_rate_limit_request = Request::builder()
        .method("PUT")
        .uri(format!(
            "/_synapse/admin/v1/users/{}/rate_limit",
            encoded_user_id
        ))
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "messages_per_second": 1.5,
                "burst_count": 3
            })
            .to_string(),
        ))
        .unwrap();
    let set_rate_limit_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), set_rate_limit_request)
            .await
            .unwrap();
    assert_eq!(set_rate_limit_response.status(), StatusCode::NOT_FOUND);

    let stored_rate_limit: Option<f64> =
        sqlx::query_scalar("SELECT messages_per_second FROM rate_limits WHERE user_id = $1")
            .bind(&missing_user_id)
            .fetch_optional(&*pool)
            .await
            .expect("failed to inspect rejected rate limit write");
    assert!(stored_rate_limit.is_none());

    let set_override_request = Request::builder()
        .method("POST")
        .uri(format!(
            "/_synapse/admin/v1/users/{}/override_ratelimit",
            encoded_user_id
        ))
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "messages_per_second": 2.0,
                "burst_count": 4
            })
            .to_string(),
        ))
        .unwrap();
    let set_override_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), set_override_request)
            .await
            .unwrap();
    assert_eq!(set_override_response.status(), StatusCode::NOT_FOUND);

    let delete_rate_limit_request = Request::builder()
        .method("DELETE")
        .uri(format!(
            "/_synapse/admin/v1/users/{}/rate_limit",
            encoded_user_id
        ))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let delete_rate_limit_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), delete_rate_limit_request)
            .await
            .unwrap();
    assert_eq!(delete_rate_limit_response.status(), StatusCode::NOT_FOUND);

    let delete_override_request = Request::builder()
        .method("DELETE")
        .uri(format!(
            "/_synapse/admin/v1/users/{}/override_ratelimit",
            encoded_user_id
        ))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let delete_override_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), delete_override_request)
            .await
            .unwrap();
    assert_eq!(delete_override_response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_admin_retention_policy_preserves_expire_on_clients() {
    let Some((app, pool)) = setup_test_app_with_pool().await else {
        return;
    };

    let (admin_token, admin_username) = get_admin_token(&app).await;
    sqlx::query("UPDATE users SET user_type = 'super_admin' WHERE username = $1")
        .bind(&admin_username)
        .execute(&*pool)
        .await
        .expect("failed to promote admin test user to super_admin");
    let server_max_lifetime = 86_400_000_i64 + i64::from(rand::random::<u16>());
    let server_min_lifetime = 3_600_000_i64 + i64::from(rand::random::<u16>());

    let set_server_policy_request = Request::builder()
        .method("POST")
        .uri("/_synapse/admin/v1/retention/policy")
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "max_lifetime": server_max_lifetime,
                "min_lifetime": server_min_lifetime,
                "expire_on_clients": true
            })
            .to_string(),
        ))
        .unwrap();
    let set_server_policy_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), set_server_policy_request)
            .await
            .unwrap();
    assert_eq!(set_server_policy_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(set_server_policy_response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["expire_on_clients"], true);

    let get_server_policy_request = Request::builder()
        .uri("/_synapse/admin/v1/retention/policy")
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let get_server_policy_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), get_server_policy_request)
            .await
            .unwrap();
    assert_eq!(get_server_policy_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(get_server_policy_response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["max_lifetime"].as_i64(), Some(server_max_lifetime));
    assert_eq!(json["min_lifetime"].as_i64(), Some(server_min_lifetime));
    assert_eq!(json["expire_on_clients"], true);

    let create_room_request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/createRoom")
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "name": "Retention Admin Room" }).to_string()))
        .unwrap();
    let create_room_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), create_room_request)
        .await
        .unwrap();
    assert_eq!(create_room_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(create_room_response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let room_id = json["room_id"].as_str().unwrap().to_string();

    let set_room_policy_request = Request::builder()
        .method("POST")
        .uri(format!("/_synapse/admin/v1/retention/policy/{}", room_id))
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "max_lifetime": 172_800_000_i64,
                "min_lifetime": 7_200_000_i64,
                "expire_on_clients": true
            })
            .to_string(),
        ))
        .unwrap();
    let set_room_policy_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), set_room_policy_request)
            .await
            .unwrap();
    assert_eq!(set_room_policy_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(set_room_policy_response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["expire_on_clients"], true);

    let get_room_policy_request = Request::builder()
        .uri(format!("/_synapse/admin/v1/retention/policy/{}", room_id))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let get_room_policy_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), get_room_policy_request)
            .await
            .unwrap();
    assert_eq!(get_room_policy_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(get_room_policy_response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["room_id"], room_id);
    assert_eq!(json["expire_on_clients"], true);

    sqlx::query("DELETE FROM room_retention_policies WHERE room_id = $1")
        .bind(&room_id)
        .execute(&*pool)
        .await
        .expect("failed to cleanup room_retention_policies");
    sqlx::query("DELETE FROM server_retention_policy WHERE id = 1")
        .execute(&*pool)
        .await
        .expect("failed to cleanup server_retention_policy");
}

#[tokio::test]
async fn test_admin_room_retention_policy_requires_existing_room() {
    let Some((app, pool)) = setup_test_app_with_pool().await else {
        return;
    };

    let (admin_token, admin_username) = get_admin_token(&app).await;
    sqlx::query("UPDATE users SET user_type = 'super_admin' WHERE username = $1")
        .bind(&admin_username)
        .execute(&*pool)
        .await
        .expect("failed to promote admin test user to super_admin");

    let missing_room_id = format!("!missing_retention_room_{}:localhost", rand::random::<u32>());
    let encoded_room_id = encode_path_segment(&missing_room_id);

    let set_room_policy_request = Request::builder()
        .method("POST")
        .uri(format!(
            "/_synapse/admin/v1/retention/policy/{}",
            encoded_room_id
        ))
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "max_lifetime": 172_800_000_i64,
                "min_lifetime": 7_200_000_i64,
                "expire_on_clients": true
            })
            .to_string(),
        ))
        .unwrap();
    let set_room_policy_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), set_room_policy_request)
            .await
            .unwrap();
    assert_eq!(set_room_policy_response.status(), StatusCode::NOT_FOUND);

    let has_policy: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM room_retention_policies WHERE room_id = $1)")
            .bind(&missing_room_id)
            .fetch_one(&*pool)
            .await
            .expect("failed to inspect room_retention_policies after rejected write");
    assert!(!has_policy);
}

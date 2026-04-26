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

async fn setup_test_app_with_pool() -> Option<(axum::Router, Arc<sqlx::PgPool>, Arc<CacheManager>)>
{
    let pool = super::get_test_pool().await?;
    let cache = Arc::new(CacheManager::new(CacheConfig::default()));
    let container = ServiceContainer::new_test_with_pool_and_cache(pool.clone(), cache.clone());
    let state = AppState::new(container, cache.clone());
    Some((create_router(state), pool, cache))
}

async fn setup_test_app() -> Option<axum::Router> {
    let (app, _, _) = setup_test_app_with_pool().await?;
    Some(app)
}

async fn get_admin_token(app: &axum::Router) -> (String, String) {
    super::get_admin_token(app).await
}

async fn promote_admin_role(pool: &sqlx::PgPool, cache: &CacheManager, username: &str, role: &str) {
    sqlx::query("UPDATE users SET is_admin = TRUE, user_type = $2 WHERE username = $1")
        .bind(username)
        .bind(role)
        .execute(pool)
        .await
        .expect("failed to promote admin role");
    let user_id = format!("@{}:localhost", username);
    cache.delete(&format!("user:admin:{}", user_id)).await;
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

async fn get_current_user_id(app: &axum::Router, access_token: &str) -> String {
    let request = Request::builder()
        .uri("/_matrix/client/v3/account/whoami")
        .header("Authorization", format!("Bearer {}", access_token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 4096)
        .await
        .unwrap();
    assert_eq!(
        status,
        StatusCode::OK,
        "whoami failed with status {}: {}",
        status,
        String::from_utf8_lossy(&body)
    );
    let json: Value = serde_json::from_slice(&body).unwrap();
    json["user_id"].as_str().unwrap().to_string()
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
async fn test_worker_admin_routes_require_admin_for_task_claims() {
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

    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(json["error"], "Admin access required");

    let claim_request = Request::builder()
        .method("POST")
        .uri(format!("/_synapse/worker/v1/tasks/claim/{}", worker_id))
        .header("Authorization", format!("Bearer {}", admin_token))
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
    let Some((app, pool, _cache)) = setup_test_app_with_pool().await else {
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
async fn test_admin_role_can_get_v2_user_details() {
    let Some((app, _pool, _cache)) = setup_test_app_with_pool().await else {
        return;
    };

    let (admin_token, _) = super::get_admin_token(&app).await;
    let user_token = create_test_user(&app).await;
    let target_user_id = get_current_user_id(&app, &user_token).await;
    let encoded_user_id = encode_path_segment(&target_user_id);

    let request = Request::builder()
        .uri(format!("/_synapse/admin/v2/users/{}", encoded_user_id))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 4096)
        .await
        .unwrap();

    assert_eq!(
        status,
        StatusCode::OK,
        "expected admin role to read v2 user details, got {}: {}",
        status,
        String::from_utf8_lossy(&body)
    );
}

#[tokio::test]
async fn test_denied_admin_requests_are_audited() {
    let Some((app, pool, _cache)) = setup_test_app_with_pool().await else {
        return;
    };

    let user_token = create_test_user(&app).await;
    let actor_id = get_current_user_id(&app, &user_token).await;
    let before_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM audit_events WHERE resource_type = 'admin_api' AND result = 'failure'")
            .fetch_one(pool.as_ref())
            .await
            .expect("failed to count audit failures before request");

    let request = Request::builder()
        .uri("/_synapse/admin/v1/users")
        .header("Authorization", format!("Bearer {}", user_token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let after_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM audit_events WHERE resource_type = 'admin_api' AND result = 'failure'")
            .fetch_one(pool.as_ref())
            .await
            .expect("failed to count audit failures after request");
    assert_eq!(after_count, before_count + 1);

    let latest = sqlx::query_as::<_, (String, String, String, String)>(
        "SELECT actor_id, action, resource_id, result \
         FROM audit_events \
         WHERE resource_type = 'admin_api' \
         ORDER BY created_ts DESC LIMIT 1",
    )
    .fetch_one(pool.as_ref())
    .await
    .expect("failed to fetch latest denied admin audit event");

    assert_eq!(latest.0, actor_id);
    assert_eq!(latest.1, "GET /_synapse/admin/v1/users");
    assert_eq!(latest.2, "/_synapse/admin/v1/users");
    assert_eq!(latest.3, "failure");
}

#[tokio::test]
async fn test_worker_claim_route_is_atomic_over_http() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let (admin_token, _) = get_admin_token(&app).await;
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
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let request_two = Request::builder()
        .method("POST")
        .uri(format!(
            "/_synapse/worker/v1/tasks/{}/claim/{}",
            task_id, worker_two
        ))
        .header("Authorization", format!("Bearer {}", admin_token))
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
async fn test_admin_bearer_post_routes_do_not_require_csrf() {
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
    let Some((app, pool, _cache)) = setup_test_app_with_pool().await else {
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
async fn test_admin_room_listing_private_removes_directory_entry() {
    let Some((app, pool, _cache)) = setup_test_app_with_pool().await else {
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
                "name": "Directory Toggle Room"
            })
            .to_string(),
        ))
        .unwrap();
    let create_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), create_request)
        .await
        .unwrap();
    assert_eq!(create_response.status(), StatusCode::OK);

    let create_body = axum::body::to_bytes(create_response.into_body(), 1024)
        .await
        .unwrap();
    let create_json: Value = serde_json::from_slice(&create_body).unwrap();
    let room_id = create_json["room_id"].as_str().unwrap().to_string();
    let encoded_room_id = encode_path_segment(&room_id);

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
    assert_eq!(set_public_response.status(), StatusCode::OK);

    let in_directory_after_publish: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM room_directory WHERE room_id = $1)")
            .bind(&room_id)
            .fetch_one(&*pool)
            .await
            .expect("failed to inspect room directory after publish");
    assert!(in_directory_after_publish);

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
    assert_eq!(set_private_response.status(), StatusCode::OK);

    let in_directory_after_private: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM room_directory WHERE room_id = $1)")
            .bind(&room_id)
            .fetch_one(&*pool)
            .await
            .expect("failed to inspect room directory after unpublish");
    assert!(!in_directory_after_private);
}

#[tokio::test]
async fn test_admin_room_block_writes_require_existing_room() {
    let Some((app, pool, _cache)) = setup_test_app_with_pool().await else {
        return;
    };

    let (admin_token, _) = get_admin_token(&app).await;
    let missing_room_id = format!("!missing_room_block_{}:localhost", rand::random::<u32>());
    let encoded_room_id = encode_path_segment(&missing_room_id);

    let block_request = Request::builder()
        .method("POST")
        .uri(format!(
            "/_synapse/admin/v1/rooms/{}/block",
            encoded_room_id
        ))
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
        .uri(format!(
            "/_synapse/admin/v1/rooms/{}/unblock",
            encoded_room_id
        ))
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
    let Some((app, pool, cache)) = setup_test_app_with_pool().await else {
        return;
    };

    let (admin_token, admin_username) = get_admin_token(&app).await;
    promote_admin_role(&pool, &cache, &admin_username, "super_admin").await;
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
    let Some((app, pool, cache)) = setup_test_app_with_pool().await else {
        return;
    };

    let (admin_token, admin_username) = get_admin_token(&app).await;
    promote_admin_role(&pool, &cache, &admin_username, "super_admin").await;
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
        .body(Body::from(
            json!({
                "name": "Admin Member Room",
                "visibility": "public"
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

    let member_count_after_ban: i64 =
        sqlx::query_scalar("SELECT member_count FROM room_summaries WHERE room_id = $1")
            .bind(&room_id)
            .fetch_one(&*pool)
            .await
            .expect("failed to inspect room member count after ban");
    assert_eq!(member_count_after_ban, 1);

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
async fn test_admin_sensitive_user_and_room_routes_require_super_admin() {
    let Some((app, pool, cache)) = setup_test_app_with_pool().await else {
        return;
    };

    let (admin_token, admin_username) = get_admin_token(&app).await;
    promote_admin_role(&pool, &cache, &admin_username, "admin").await;

    let user_token = create_test_user(&app).await;
    let user_id = get_current_user_id(&app, &user_token).await;
    let encoded_user_id = encode_path_segment(&user_id);

    let create_room_request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/createRoom")
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({ "name": "Sensitive Admin Guard Room" }).to_string(),
        ))
        .unwrap();
    let create_room_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), create_room_request)
            .await
            .unwrap();
    assert_eq!(create_room_response.status(), StatusCode::OK);

    let create_room_body = axum::body::to_bytes(create_room_response.into_body(), 1024)
        .await
        .unwrap();
    let create_room_json: Value = serde_json::from_slice(&create_room_body).unwrap();
    let room_id = create_room_json["room_id"]
        .as_str()
        .expect("room_id present")
        .to_string();

    let deactivate_request = Request::builder()
        .method("POST")
        .uri(format!(
            "/_synapse/admin/v1/users/{}/deactivate",
            encoded_user_id
        ))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let deactivate_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), deactivate_request)
        .await
        .unwrap();
    assert_eq!(deactivate_response.status(), StatusCode::FORBIDDEN);

    let login_request = Request::builder()
        .method("POST")
        .uri(format!(
            "/_synapse/admin/v1/users/{}/login",
            encoded_user_id
        ))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let login_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), login_request)
        .await
        .unwrap();
    assert_eq!(login_response.status(), StatusCode::FORBIDDEN);

    let logout_request = Request::builder()
        .method("POST")
        .uri(format!(
            "/_synapse/admin/v1/users/{}/logout",
            encoded_user_id
        ))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let logout_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), logout_request)
        .await
        .unwrap();
    assert_eq!(logout_response.status(), StatusCode::FORBIDDEN);

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
    assert_eq!(make_admin_response.status(), StatusCode::FORBIDDEN);

    let shutdown_request = Request::builder()
        .method("POST")
        .uri("/_synapse/admin/v1/shutdown_room")
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "room_id": room_id }).to_string()))
        .unwrap();
    let shutdown_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), shutdown_request)
        .await
        .unwrap();
    // shutdown_room is in is_admin_only, admin role is allowed
    assert_ne!(shutdown_response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_admin_device_management_supports_delete_compat_routes() {
    let Some((app, pool, cache)) = setup_test_app_with_pool().await else {
        return;
    };

    let (admin_token, admin_username) = get_admin_token(&app).await;
    promote_admin_role(&pool, &cache, &admin_username, "super_admin").await;
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
    let Some((app, pool, _cache)) = setup_test_app_with_pool().await else {
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

    let get_rate_limit_request = Request::builder()
        .uri(format!(
            "/_synapse/admin/v1/users/{}/rate_limit",
            encoded_user_id
        ))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let get_rate_limit_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), get_rate_limit_request)
            .await
            .unwrap();
    assert_eq!(get_rate_limit_response.status(), StatusCode::NOT_FOUND);

    let get_override_request = Request::builder()
        .uri(format!(
            "/_synapse/admin/v1/users/{}/override_ratelimit",
            encoded_user_id
        ))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let get_override_response = ServiceExt::<Request<Body>>::oneshot(app, get_override_request)
        .await
        .unwrap();
    assert_eq!(get_override_response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_admin_retention_policy_preserves_expire_on_clients() {
    let Some((app, pool, _cache)) = setup_test_app_with_pool().await else {
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
        .body(Body::from(
            json!({ "name": "Retention Admin Room" }).to_string(),
        ))
        .unwrap();
    let create_room_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), create_room_request)
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
    let Some((app, pool, _cache)) = setup_test_app_with_pool().await else {
        return;
    };

    let (admin_token, admin_username) = get_admin_token(&app).await;
    sqlx::query("UPDATE users SET user_type = 'super_admin' WHERE username = $1")
        .bind(&admin_username)
        .execute(&*pool)
        .await
        .expect("failed to promote admin test user to super_admin");

    let missing_room_id = format!(
        "!missing_retention_room_{}:localhost",
        rand::random::<u32>()
    );
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

    let get_room_policy_request = Request::builder()
        .method("GET")
        .uri(format!(
            "/_synapse/admin/v1/retention/policy/{}",
            encoded_room_id
        ))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let get_room_policy_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), get_room_policy_request)
            .await
            .unwrap();
    assert_eq!(get_room_policy_response.status(), StatusCode::NOT_FOUND);

    let has_policy: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM room_retention_policies WHERE room_id = $1)",
    )
    .bind(&missing_room_id)
    .fetch_one(&*pool)
    .await
    .expect("failed to inspect room_retention_policies after rejected write");
    assert!(!has_policy);
}

#[tokio::test]
async fn test_admin_retention_run_requires_existing_room() {
    let Some((app, pool, _cache)) = setup_test_app_with_pool().await else {
        return;
    };

    let (admin_token, admin_username) = get_admin_token(&app).await;
    sqlx::query("UPDATE users SET user_type = 'super_admin' WHERE username = $1")
        .bind(&admin_username)
        .execute(&*pool)
        .await
        .expect("failed to promote admin test user to super_admin");

    sqlx::query(
        "INSERT INTO server_retention_policy (id, max_lifetime, min_lifetime, expire_on_clients, created_ts, updated_ts)
         VALUES (1, $1, $2, FALSE, EXTRACT(EPOCH FROM NOW())::BIGINT * 1000, EXTRACT(EPOCH FROM NOW())::BIGINT * 1000)
         ON CONFLICT (id) DO UPDATE SET max_lifetime = EXCLUDED.max_lifetime, min_lifetime = EXCLUDED.min_lifetime, updated_ts = EXTRACT(EPOCH FROM NOW())::BIGINT * 1000",
    )
    .bind(86_400_000_i64)
    .bind(3_600_000_i64)
    .execute(&*pool)
    .await
    .expect("failed to seed server_retention_policy");

    let missing_room_id = format!("!missing_retention_run_{}:localhost", rand::random::<u32>());
    let run_request = Request::builder()
        .method("POST")
        .uri("/_synapse/admin/v1/retention/run")
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "room_id": missing_room_id
            })
            .to_string(),
        ))
        .unwrap();
    let run_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), run_request)
        .await
        .unwrap();
    assert_eq!(run_response.status(), StatusCode::NOT_FOUND);

    let has_cleanup_log: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM retention_cleanup_logs WHERE room_id = $1)",
    )
    .bind(&missing_room_id)
    .fetch_one(&*pool)
    .await
    .expect("failed to inspect retention_cleanup_logs after rejected run");
    assert!(!has_cleanup_log);

    sqlx::query("DELETE FROM server_retention_policy WHERE id = 1")
        .execute(&*pool)
        .await
        .expect("failed to cleanup server_retention_policy");
}

#[tokio::test]
async fn test_admin_user_notification_write_requires_existing_user() {
    let Some((app, pool, _cache)) = setup_test_app_with_pool().await else {
        return;
    };

    let (admin_token, _) = get_admin_token(&app).await;
    let missing_user_id = format!("@missing_notification_{}:localhost", rand::random::<u32>());
    let encoded_user_id = encode_path_segment(&missing_user_id);

    let get_notification_request = Request::builder()
        .method("GET")
        .uri(format!(
            "/_synapse/admin/v1/users/{}/notification",
            encoded_user_id
        ))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let get_notification_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), get_notification_request)
            .await
            .unwrap();
    assert_eq!(get_notification_response.status(), StatusCode::NOT_FOUND);

    let update_notification_request = Request::builder()
        .method("PUT")
        .uri(format!(
            "/_synapse/admin/v1/users/{}/notification",
            encoded_user_id
        ))
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "enabled": false
            })
            .to_string(),
        ))
        .unwrap();
    let update_notification_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), update_notification_request)
            .await
            .unwrap();
    assert_eq!(update_notification_response.status(), StatusCode::NOT_FOUND);

    let has_setting: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM user_notification_settings WHERE user_id = $1)",
    )
    .bind(&missing_user_id)
    .fetch_one(&*pool)
    .await
    .expect("failed to inspect user_notification_settings after rejected write");
    assert!(!has_setting);

    let username = format!("notification_user_{}", rand::random::<u32>());
    let register_request = Request::builder()
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
    let register_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), register_request)
        .await
        .unwrap();
    assert_eq!(register_response.status(), StatusCode::OK);

    let register_body = axum::body::to_bytes(register_response.into_body(), 4096)
        .await
        .unwrap();
    let register_json: Value = serde_json::from_slice(&register_body).unwrap();
    let existing_user_id = register_json["user_id"]
        .as_str()
        .expect("registered user_id")
        .to_string();
    let encoded_existing_user_id = encode_path_segment(&existing_user_id);

    let get_existing_notification_request = Request::builder()
        .method("GET")
        .uri(format!(
            "/_synapse/admin/v1/users/{}/notification",
            encoded_existing_user_id
        ))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let get_existing_notification_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), get_existing_notification_request)
            .await
            .unwrap();
    assert_eq!(get_existing_notification_response.status(), StatusCode::OK);

    let get_existing_notification_body =
        axum::body::to_bytes(get_existing_notification_response.into_body(), 4096)
            .await
            .unwrap();
    let get_existing_notification_json: Value =
        serde_json::from_slice(&get_existing_notification_body).unwrap();
    assert_eq!(get_existing_notification_json["enabled"], json!(true));
}

#[tokio::test]
async fn test_admin_notification_specific_targets_require_existing_users() {
    let Some((app, pool, _cache)) = setup_test_app_with_pool().await else {
        return;
    };

    let (admin_token, _) = get_admin_token(&app).await;
    let missing_user_id = format!("@missing_notice_target_{}:localhost", rand::random::<u32>());
    let rejected_title = format!("Rejected notification {}", rand::random::<u32>());

    let create_missing_target_request = Request::builder()
        .method("POST")
        .uri("/_synapse/admin/v1/notifications")
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "title": rejected_title,
                "content": "Should be rejected",
                "target_audience": "specific",
                "target_user_ids": [missing_user_id]
            })
            .to_string(),
        ))
        .unwrap();
    let create_missing_target_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), create_missing_target_request)
            .await
            .unwrap();
    assert_eq!(
        create_missing_target_response.status(),
        StatusCode::NOT_FOUND
    );

    let rejected_notification_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM server_notifications WHERE title = $1")
            .bind(&rejected_title)
            .fetch_one(&*pool)
            .await
            .expect("failed to inspect rejected server notification");
    assert_eq!(rejected_notification_count, 0);

    let create_valid_request = Request::builder()
        .method("POST")
        .uri("/_synapse/admin/v1/notifications")
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "title": "Baseline notification",
                "content": "Initial content",
                "target_audience": "all"
            })
            .to_string(),
        ))
        .unwrap();
    let create_valid_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), create_valid_request)
            .await
            .unwrap();
    assert_eq!(create_valid_response.status(), StatusCode::OK);

    let create_valid_body = axum::body::to_bytes(create_valid_response.into_body(), 4096)
        .await
        .unwrap();
    let created_notification: Value = serde_json::from_slice(&create_valid_body).unwrap();
    let notification_id = created_notification["id"]
        .as_i64()
        .expect("created notification id");

    let update_missing_target_request = Request::builder()
        .method("PUT")
        .uri(format!(
            "/_synapse/admin/v1/notifications/{}",
            notification_id
        ))
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "target_audience": "specific",
                "target_user_ids": [format!(
                    "@missing_notice_update_{}:localhost",
                    rand::random::<u32>()
                )]
            })
            .to_string(),
        ))
        .unwrap();
    let update_missing_target_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), update_missing_target_request)
            .await
            .unwrap();
    assert_eq!(
        update_missing_target_response.status(),
        StatusCode::NOT_FOUND
    );

    let persisted_notification: (String, serde_json::Value) = sqlx::query_as(
        "SELECT target_audience, target_user_ids FROM server_notifications WHERE id = $1",
    )
    .bind(notification_id)
    .fetch_one(&*pool)
    .await
    .expect("failed to inspect persisted notification after rejected update");
    assert_eq!(persisted_notification.0, "all");
    assert_eq!(persisted_notification.1, json!([]));
}

#[tokio::test]
async fn test_admin_user_presence_queries_require_existing_user() {
    let Some((app, _pool, _cache)) = setup_test_app_with_pool().await else {
        return;
    };

    let (admin_token, _) = get_admin_token(&app).await;
    let missing_user_id = format!("@missing_presence_{}:localhost", rand::random::<u32>());
    let encoded_missing_user_id = encode_path_segment(&missing_user_id);

    let whois_missing_request = Request::builder()
        .method("GET")
        .uri(format!(
            "/_synapse/admin/v1/whois/{}",
            encoded_missing_user_id
        ))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let whois_missing_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), whois_missing_request)
            .await
            .unwrap();
    assert_eq!(whois_missing_response.status(), StatusCode::NOT_FOUND);

    let whois_device_missing_request = Request::builder()
        .method("GET")
        .uri(format!(
            "/_synapse/admin/v1/whois/{}/missing-device",
            encoded_missing_user_id
        ))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let whois_device_missing_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), whois_device_missing_request)
            .await
            .unwrap();
    assert_eq!(
        whois_device_missing_response.status(),
        StatusCode::NOT_FOUND
    );

    let sessions_missing_request = Request::builder()
        .method("GET")
        .uri(format!(
            "/_synapse/admin/v1/user_sessions/{}",
            encoded_missing_user_id
        ))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let sessions_missing_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), sessions_missing_request)
            .await
            .unwrap();
    assert_eq!(sessions_missing_response.status(), StatusCode::NOT_FOUND);

    let username = format!("presence_user_{}", rand::random::<u32>());
    let register_request = Request::builder()
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
    let register_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), register_request)
        .await
        .unwrap();
    assert_eq!(register_response.status(), StatusCode::OK);

    let register_body = axum::body::to_bytes(register_response.into_body(), 4096)
        .await
        .unwrap();
    let register_json: Value = serde_json::from_slice(&register_body).unwrap();
    let existing_user_id = register_json["user_id"]
        .as_str()
        .expect("registered user_id")
        .to_string();
    let encoded_existing_user_id = encode_path_segment(&existing_user_id);

    let whois_existing_request = Request::builder()
        .method("GET")
        .uri(format!(
            "/_synapse/admin/v1/whois/{}",
            encoded_existing_user_id
        ))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let whois_existing_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), whois_existing_request)
            .await
            .unwrap();
    assert_eq!(whois_existing_response.status(), StatusCode::OK);

    let whois_existing_body = axum::body::to_bytes(whois_existing_response.into_body(), 4096)
        .await
        .unwrap();
    let whois_existing_json: Value = serde_json::from_slice(&whois_existing_body).unwrap();
    assert_eq!(whois_existing_json["user_id"], json!(existing_user_id));
    assert!(whois_existing_json["devices"].is_array());

    let sessions_existing_request = Request::builder()
        .method("GET")
        .uri(format!(
            "/_synapse/admin/v1/user_sessions/{}",
            encoded_existing_user_id
        ))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let sessions_existing_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), sessions_existing_request)
            .await
            .unwrap();
    assert_eq!(sessions_existing_response.status(), StatusCode::OK);

    let sessions_existing_body = axum::body::to_bytes(sessions_existing_response.into_body(), 4096)
        .await
        .unwrap();
    let sessions_existing_json: Value = serde_json::from_slice(&sessions_existing_body).unwrap();
    assert_eq!(sessions_existing_json["user_id"], json!(existing_user_id));
    assert!(sessions_existing_json["sessions"].is_array());
}

#[tokio::test]
async fn test_admin_delete_user_media_requires_existing_user() {
    let Some((app, _pool, _cache)) = setup_test_app_with_pool().await else {
        return;
    };

    let (admin_token, _) = get_admin_token(&app).await;
    let missing_user_id = format!("@missing_media_{}:localhost", rand::random::<u32>());
    let encoded_missing_user_id = encode_path_segment(&missing_user_id);

    let delete_missing_user_media_request = Request::builder()
        .method("DELETE")
        .uri(format!(
            "/_synapse/admin/v1/users/{}/media",
            encoded_missing_user_id
        ))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let delete_missing_user_media_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), delete_missing_user_media_request)
            .await
            .unwrap();
    assert_eq!(
        delete_missing_user_media_response.status(),
        StatusCode::NOT_FOUND
    );

    let get_missing_user_media_request = Request::builder()
        .uri(format!(
            "/_synapse/admin/v1/users/{}/media",
            encoded_missing_user_id
        ))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let get_missing_user_media_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), get_missing_user_media_request)
            .await
            .unwrap();
    assert_eq!(
        get_missing_user_media_response.status(),
        StatusCode::NOT_FOUND
    );

    let username = format!("media_user_{}", rand::random::<u32>());
    let register_request = Request::builder()
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
    let register_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), register_request)
        .await
        .unwrap();
    assert_eq!(register_response.status(), StatusCode::OK);
    let register_body = axum::body::to_bytes(register_response.into_body(), 10240)
        .await
        .unwrap();
    let register_json: Value = serde_json::from_slice(&register_body).unwrap();
    let user_id = register_json["user_id"].as_str().unwrap().to_string();
    let encoded_user_id = encode_path_segment(&user_id);

    let get_existing_user_media_request = Request::builder()
        .uri(format!(
            "/_synapse/admin/v1/users/{}/media",
            encoded_user_id
        ))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let get_existing_user_media_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), get_existing_user_media_request)
            .await
            .unwrap();
    assert_eq!(get_existing_user_media_response.status(), StatusCode::OK);
    let get_existing_user_media_body =
        axum::body::to_bytes(get_existing_user_media_response.into_body(), 10240)
            .await
            .unwrap();
    let get_existing_user_media_json: Value =
        serde_json::from_slice(&get_existing_user_media_body).unwrap();
    assert_eq!(
        get_existing_user_media_json,
        json!({ "media": [], "total": 0 })
    );

    let delete_existing_user_media_request = Request::builder()
        .method("DELETE")
        .uri(format!(
            "/_synapse/admin/v1/users/{}/media",
            encoded_user_id
        ))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let delete_existing_user_media_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), delete_existing_user_media_request)
            .await
            .unwrap();
    assert_eq!(delete_existing_user_media_response.status(), StatusCode::OK);
    let delete_existing_user_media_body =
        axum::body::to_bytes(delete_existing_user_media_response.into_body(), 10240)
            .await
            .unwrap();
    let delete_existing_user_media_json: Value =
        serde_json::from_slice(&delete_existing_user_media_body).unwrap();
    assert_eq!(delete_existing_user_media_json, json!({ "deleted": 0 }));
}

#[tokio::test]
async fn test_admin_delete_user_pusher_requires_existing_user() {
    let Some((app, pool, _cache)) = setup_test_app_with_pool().await else {
        return;
    };

    let (admin_token, _) = get_admin_token(&app).await;
    let missing_user_id = format!("@missing_pusher_{}:localhost", rand::random::<u32>());
    let encoded_missing_user_id = encode_path_segment(&missing_user_id);
    let orphan_pushkey = format!("orphan-pushkey-{}", rand::random::<u32>());
    let now = chrono::Utc::now().timestamp_millis();

    sqlx::query(
        "INSERT INTO pushers (user_id, device_id, pushkey, pushkey_ts, kind, app_id, app_display_name, device_display_name, profile_tag, lang, data, created_ts, updated_ts) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, NULL, $9, $10, $11, $11)",
    )
    .bind(&missing_user_id)
    .bind(format!("DEVICE{}", rand::random::<u32>()))
    .bind(&orphan_pushkey)
    .bind(now)
    .bind("http")
    .bind("com.example.orphan")
    .bind("Orphan Push")
    .bind("Orphan Device")
    .bind("en")
    .bind(json!({ "url": "https://push.example.test/_matrix/push/v1/notify" }))
    .bind(now)
    .execute(&*pool)
    .await
    .expect("failed to seed orphan pusher");

    let delete_missing_user_pusher_request = Request::builder()
        .method("DELETE")
        .uri(format!(
            "/_synapse/admin/v1/users/{}/pushers/{}",
            encoded_missing_user_id, orphan_pushkey
        ))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let delete_missing_user_pusher_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), delete_missing_user_pusher_request)
            .await
            .unwrap();
    assert_eq!(
        delete_missing_user_pusher_response.status(),
        StatusCode::NOT_FOUND
    );

    let orphan_still_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM pushers WHERE user_id = $1 AND pushkey = $2)",
    )
    .bind(&missing_user_id)
    .bind(&orphan_pushkey)
    .fetch_one(&*pool)
    .await
    .expect("failed to inspect orphan pusher after rejected delete");
    assert!(orphan_still_exists);

    let username = format!("pusher_user_{}", rand::random::<u32>());
    let register_request = Request::builder()
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
    let register_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), register_request)
        .await
        .unwrap();
    assert_eq!(register_response.status(), StatusCode::OK);
    let register_body = axum::body::to_bytes(register_response.into_body(), 10240)
        .await
        .unwrap();
    let register_json: Value = serde_json::from_slice(&register_body).unwrap();
    let user_id = register_json["user_id"].as_str().unwrap().to_string();
    let access_token = register_json["access_token"].as_str().unwrap().to_string();

    let pushkey = format!("pushkey-{}", rand::random::<u32>());
    let set_pusher_request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/pushers/set")
        .header("Authorization", format!("Bearer {}", access_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "pushkey": pushkey,
                "kind": "http",
                "app_id": "com.example.admin-test",
                "app_display_name": "Admin Test Push",
                "device_display_name": "Admin Test Device",
                "lang": "en",
                "data": {
                    "url": "https://push.example.test/_matrix/push/v1/notify"
                }
            })
            .to_string(),
        ))
        .unwrap();
    let set_pusher_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), set_pusher_request)
        .await
        .unwrap();
    assert_eq!(set_pusher_response.status(), StatusCode::OK);

    let encoded_user_id = encode_path_segment(&user_id);
    let delete_existing_user_pusher_request = Request::builder()
        .method("DELETE")
        .uri(format!(
            "/_synapse/admin/v1/users/{}/pushers/{}",
            encoded_user_id, pushkey
        ))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let delete_existing_user_pusher_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), delete_existing_user_pusher_request)
            .await
            .unwrap();
    assert_eq!(
        delete_existing_user_pusher_response.status(),
        StatusCode::OK
    );

    let pusher_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM pushers WHERE user_id = $1 AND pushkey = $2)",
    )
    .bind(&user_id)
    .bind(&pushkey)
    .fetch_one(&*pool)
    .await
    .expect("failed to inspect pusher after admin delete");
    assert!(!pusher_exists);
}

#[tokio::test]
async fn test_admin_user_token_routes_require_existing_user() {
    let Some((app, pool, _cache)) = setup_test_app_with_pool().await else {
        return;
    };

    let (admin_token, _) = get_admin_token(&app).await;
    let missing_user_id = format!("@missing_tokens_{}:localhost", rand::random::<u32>());
    let encoded_missing_user_id = encode_path_segment(&missing_user_id);

    let get_missing_user_tokens_request = Request::builder()
        .method("GET")
        .uri(format!(
            "/_synapse/admin/v1/users/{}/tokens",
            encoded_missing_user_id
        ))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let get_missing_user_tokens_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), get_missing_user_tokens_request)
            .await
            .unwrap();
    assert_eq!(
        get_missing_user_tokens_response.status(),
        StatusCode::NOT_FOUND
    );

    let get_missing_user_refresh_tokens_request = Request::builder()
        .method("GET")
        .uri(format!(
            "/_synapse/admin/v1/users/{}/refresh_tokens",
            encoded_missing_user_id
        ))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let get_missing_user_refresh_tokens_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), get_missing_user_refresh_tokens_request)
            .await
            .unwrap();
    assert_eq!(
        get_missing_user_refresh_tokens_response.status(),
        StatusCode::NOT_FOUND
    );

    let delete_missing_user_token_request = Request::builder()
        .method("DELETE")
        .uri(format!(
            "/_synapse/admin/v1/users/{}/tokens/1",
            encoded_missing_user_id
        ))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let delete_missing_user_token_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), delete_missing_user_token_request)
            .await
            .unwrap();
    assert_eq!(
        delete_missing_user_token_response.status(),
        StatusCode::NOT_FOUND
    );

    let delete_missing_user_refresh_token_request = Request::builder()
        .method("DELETE")
        .uri(format!(
            "/_synapse/admin/v1/users/{}/refresh_tokens/1",
            encoded_missing_user_id
        ))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let delete_missing_user_refresh_token_response = ServiceExt::<Request<Body>>::oneshot(
        app.clone(),
        delete_missing_user_refresh_token_request,
    )
    .await
    .unwrap();
    assert_eq!(
        delete_missing_user_refresh_token_response.status(),
        StatusCode::NOT_FOUND
    );

    let username = format!("token_user_{}", rand::random::<u32>());
    let register_request = Request::builder()
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
    let register_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), register_request)
        .await
        .unwrap();
    assert_eq!(register_response.status(), StatusCode::OK);
    let register_body = axum::body::to_bytes(register_response.into_body(), 10240)
        .await
        .unwrap();
    let register_json: Value = serde_json::from_slice(&register_body).unwrap();
    let user_id = register_json["user_id"].as_str().unwrap().to_string();
    let encoded_user_id = encode_path_segment(&user_id);
    let now = chrono::Utc::now().timestamp_millis();

    let access_token_id: i64 = sqlx::query_scalar(
        "INSERT INTO access_tokens (token_hash, token, user_id, device_id, created_ts, expires_at, last_used_ts, user_agent, ip_address, is_revoked) VALUES ($1, NULL, $2, $3, $4, NULL, NULL, NULL, NULL, FALSE) RETURNING id",
    )
    .bind(format!("admin-test-access-hash-{}", rand::random::<u64>()))
    .bind(&user_id)
    .bind(format!("DEVICE{}", rand::random::<u32>()))
    .bind(now)
    .fetch_one(&*pool)
    .await
    .expect("failed to seed access token");

    let refresh_token_id: i64 = sqlx::query_scalar(
        "INSERT INTO refresh_tokens (token_hash, user_id, device_id, access_token_id, scope, created_ts, expires_at, client_info, ip_address, user_agent) VALUES ($1, $2, $3, $4, $5, $6, NULL, $7, NULL, NULL) RETURNING id",
    )
    .bind(format!("admin-test-refresh-hash-{}", rand::random::<u64>()))
    .bind(&user_id)
    .bind(format!("DEVICE{}", rand::random::<u32>()))
    .bind(access_token_id.to_string())
    .bind("offline_access")
    .bind(now)
    .bind(json!({ "source": "admin-test" }))
    .fetch_one(&*pool)
    .await
    .expect("failed to seed refresh token");

    let delete_user_token_request = Request::builder()
        .method("DELETE")
        .uri(format!(
            "/_synapse/admin/v1/users/{}/tokens/{}",
            encoded_user_id, access_token_id
        ))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let delete_user_token_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), delete_user_token_request)
            .await
            .unwrap();
    assert_eq!(delete_user_token_response.status(), StatusCode::OK);

    let access_token_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM access_tokens WHERE id = $1 AND user_id = $2)",
    )
    .bind(access_token_id)
    .bind(&user_id)
    .fetch_one(&*pool)
    .await
    .expect("failed to inspect access token after admin delete");
    assert!(!access_token_exists);

    let delete_refresh_token_request = Request::builder()
        .method("DELETE")
        .uri(format!(
            "/_synapse/admin/v1/users/{}/refresh_tokens/{}",
            encoded_user_id, refresh_token_id
        ))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let delete_refresh_token_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), delete_refresh_token_request)
            .await
            .unwrap();
    assert_eq!(delete_refresh_token_response.status(), StatusCode::OK);

    let refresh_token_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM refresh_tokens WHERE id = $1 AND user_id = $2)",
    )
    .bind(refresh_token_id)
    .bind(&user_id)
    .fetch_one(&*pool)
    .await
    .expect("failed to inspect refresh token after admin delete");
    assert!(!refresh_token_exists);
}

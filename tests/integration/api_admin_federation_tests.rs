use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use hmac::{Hmac, Mac};
use serde_json::{json, Value};
use sqlx::PgPool;
use std::sync::Arc;
use synapse_rust::cache::{CacheConfig, CacheManager};
use synapse_rust::services::ServiceContainer;
use synapse_rust::web::routes::create_router;
use synapse_rust::web::AppState;
use tower::ServiceExt;

type HmacSha256 = Hmac<sha2::Sha256>;

async fn setup_test_context() -> Option<(axum::Router, Arc<PgPool>)> {
    if !super::init_test_database().await {
        return None;
    }

    let container = ServiceContainer::new_test();
    let pool = container.user_storage.pool.clone();
    let cache = Arc::new(CacheManager::new(CacheConfig::default()));
    let state = AppState::new(container, cache);

    Some((create_router(state), pool))
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
    let username = format!("admin_fed_{}", rand::random::<u32>());
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

#[tokio::test]
async fn test_admin_federation_destinations_routes_work() {
    let Some((app, pool)) = setup_test_context().await else {
        return;
    };
    let admin_token = get_admin_token(&app).await;
    let suffix = rand::random::<u32>();
    let server_name = format!("fed-{}.example.com", suffix);
    let replacement = format!("target-{}.example.com", suffix);
    let room_a = format!("!roomA{}:localhost", suffix);
    let room_b = format!("!roomB{}:localhost", suffix);

    sqlx::query(
        "INSERT INTO federation_servers (server_name, last_successful_connect_at, last_failed_connect_at, failure_count) VALUES ($1, $2, $3, $4)",
    )
    .bind(&server_name)
    .bind(1000_i64)
    .bind(2000_i64)
    .bind(3_i32)
    .execute(&*pool)
    .await
    .unwrap();

    for room_id in [&room_a, &room_b] {
        sqlx::query(
            "INSERT INTO federation_queue (destination, event_id, event_type, room_id, content, created_ts) VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(&server_name)
        .bind(format!("${}${}", room_id, suffix))
        .bind("m.room.message")
        .bind(room_id)
        .bind(json!({ "body": "hello" }))
        .bind(3000_i64)
        .execute(&*pool)
        .await
        .unwrap();
    }

    let list_request = Request::builder()
        .uri("/_synapse/admin/v1/federation/destinations")
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let list_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), list_request)
        .await
        .unwrap();
    assert_eq!(list_response.status(), StatusCode::OK);
    let list_body = axum::body::to_bytes(list_response.into_body(), 4096)
        .await
        .unwrap();
    let list_json: Value = serde_json::from_slice(&list_body).unwrap();
    assert!(list_json["destinations"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry["destination"] == server_name));

    let get_request = Request::builder()
        .uri(format!(
            "/_synapse/admin/v1/federation/destinations/{}",
            server_name
        ))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let get_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), get_request)
        .await
        .unwrap();
    assert_eq!(get_response.status(), StatusCode::OK);
    let get_body = axum::body::to_bytes(get_response.into_body(), 2048)
        .await
        .unwrap();
    let get_json: Value = serde_json::from_slice(&get_body).unwrap();
    assert_eq!(get_json["destination"], server_name);
    assert_eq!(get_json["failure_count"], 3);

    let rooms_request = Request::builder()
        .uri(format!(
            "/_synapse/admin/v1/federation/destinations/{}/rooms",
            server_name
        ))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let rooms_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), rooms_request)
        .await
        .unwrap();
    assert_eq!(rooms_response.status(), StatusCode::OK);
    let rooms_body = axum::body::to_bytes(rooms_response.into_body(), 2048)
        .await
        .unwrap();
    let rooms_json: Value = serde_json::from_slice(&rooms_body).unwrap();
    assert_eq!(rooms_json["total"], 2);

    let resolve_request = Request::builder()
        .method("POST")
        .uri("/_synapse/admin/v1/federation/resolve")
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({ "server_name": server_name }).to_string(),
        ))
        .unwrap();
    let resolve_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), resolve_request)
        .await
        .unwrap();
    assert_eq!(resolve_response.status(), StatusCode::OK);
    let resolve_body = axum::body::to_bytes(resolve_response.into_body(), 2048)
        .await
        .unwrap();
    let resolve_json: Value = serde_json::from_slice(&resolve_body).unwrap();
    assert_eq!(resolve_json["resolved"], true);
    assert_eq!(resolve_json["in_destinations"], true);

    let rewrite_request = Request::builder()
        .method("POST")
        .uri("/_synapse/admin/v1/federation/rewrite")
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({ "from": server_name, "to": replacement }).to_string(),
        ))
        .unwrap();
    let rewrite_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), rewrite_request)
        .await
        .unwrap();
    assert_eq!(rewrite_response.status(), StatusCode::OK);

    let reset_request = Request::builder()
        .method("POST")
        .uri(format!(
            "/_synapse/admin/v1/federation/destinations/{}/reset_connection",
            server_name
        ))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let reset_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), reset_request)
        .await
        .unwrap();
    assert_eq!(reset_response.status(), StatusCode::OK);

    let verify_request = Request::builder()
        .uri(format!(
            "/_synapse/admin/v1/federation/destinations/{}",
            server_name
        ))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let verify_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), verify_request)
        .await
        .unwrap();
    let verify_body = axum::body::to_bytes(verify_response.into_body(), 2048)
        .await
        .unwrap();
    let verify_json: Value = serde_json::from_slice(&verify_body).unwrap();
    assert_eq!(verify_json["failure_count"], 0);
    assert_eq!(verify_json["retry_last_ts"], Value::Null);

    let delete_request = Request::builder()
        .method("DELETE")
        .uri(format!(
            "/_synapse/admin/v1/federation/destinations/{}",
            server_name
        ))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let delete_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), delete_request)
        .await
        .unwrap();
    assert_eq!(delete_response.status(), StatusCode::OK);

    let missing_request = Request::builder()
        .uri(format!(
            "/_synapse/admin/v1/federation/destinations/{}",
            server_name
        ))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let missing_response = ServiceExt::<Request<Body>>::oneshot(app, missing_request)
        .await
        .unwrap();
    assert_eq!(missing_response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_admin_federation_blacklist_cache_and_confirm_routes_work() {
    let Some((app, pool)) = setup_test_context().await else {
        return;
    };
    let admin_token = get_admin_token(&app).await;
    let suffix = rand::random::<u32>();
    let server_name = format!("blocked-{}.example.com", suffix);
    let cache_key_one = format!("fed-cache-one-{}", suffix);
    let cache_key_two = format!("fed-cache-two-{}", suffix);

    let add_request = Request::builder()
        .method("POST")
        .uri(format!(
            "/_synapse/admin/v1/federation/blacklist/{}",
            server_name
        ))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let add_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), add_request)
        .await
        .unwrap();
    assert_eq!(add_response.status(), StatusCode::OK);

    let list_request = Request::builder()
        .uri("/_synapse/admin/v1/federation/blacklist")
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let list_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), list_request)
        .await
        .unwrap();
    assert_eq!(list_response.status(), StatusCode::OK);
    let list_body = axum::body::to_bytes(list_response.into_body(), 4096)
        .await
        .unwrap();
    let list_json: Value = serde_json::from_slice(&list_body).unwrap();
    assert!(list_json["blacklist"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry["server_name"] == server_name));

    let confirm_request = Request::builder()
        .method("POST")
        .uri("/_synapse/admin/v1/federation/confirm")
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({ "server_name": server_name, "accept": true }).to_string(),
        ))
        .unwrap();
    let confirm_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), confirm_request)
        .await
        .unwrap();
    assert_eq!(confirm_response.status(), StatusCode::OK);
    let confirm_body = axum::body::to_bytes(confirm_response.into_body(), 2048)
        .await
        .unwrap();
    let confirm_json: Value = serde_json::from_slice(&confirm_body).unwrap();
    assert_eq!(confirm_json["confirmed"], true);

    for key in [&cache_key_one, &cache_key_two] {
        sqlx::query(
            "INSERT INTO federation_cache (key, value, expiry_ts, created_ts) VALUES ($1, $2, $3, $4)",
        )
        .bind(key)
        .bind(format!("value-{}", key))
        .bind(9999_i64)
        .bind(1111_i64)
        .execute(&*pool)
        .await
        .unwrap();
    }

    let cache_request = Request::builder()
        .uri("/_synapse/admin/v1/federation/cache")
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let cache_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), cache_request)
        .await
        .unwrap();
    assert_eq!(cache_response.status(), StatusCode::OK);
    let cache_body = axum::body::to_bytes(cache_response.into_body(), 4096)
        .await
        .unwrap();
    let cache_json: Value = serde_json::from_slice(&cache_body).unwrap();
    assert!(cache_json["cache"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry["key"] == cache_key_one));

    let delete_cache_request = Request::builder()
        .method("DELETE")
        .uri(format!(
            "/_synapse/admin/v1/federation/cache/{}",
            cache_key_one
        ))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let delete_cache_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), delete_cache_request)
            .await
            .unwrap();
    assert_eq!(delete_cache_response.status(), StatusCode::OK);

    let clear_cache_request = Request::builder()
        .method("POST")
        .uri("/_synapse/admin/v1/federation/cache/clear")
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let clear_cache_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), clear_cache_request)
            .await
            .unwrap();
    assert_eq!(clear_cache_response.status(), StatusCode::OK);
    let clear_cache_body = axum::body::to_bytes(clear_cache_response.into_body(), 2048)
        .await
        .unwrap();
    let clear_cache_json: Value = serde_json::from_slice(&clear_cache_body).unwrap();
    assert_eq!(clear_cache_json["deleted"], 1);

    let remove_request = Request::builder()
        .method("DELETE")
        .uri(format!(
            "/_synapse/admin/v1/federation/blacklist/{}",
            server_name
        ))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let remove_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), remove_request)
        .await
        .unwrap();
    assert_eq!(remove_response.status(), StatusCode::OK);

    let verify_request = Request::builder()
        .uri("/_synapse/admin/v1/federation/blacklist")
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let verify_response = ServiceExt::<Request<Body>>::oneshot(app, verify_request)
        .await
        .unwrap();
    let verify_body = axum::body::to_bytes(verify_response.into_body(), 4096)
        .await
        .unwrap();
    let verify_json: Value = serde_json::from_slice(&verify_body).unwrap();
    assert!(!verify_json["blacklist"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry["server_name"] == server_name));
}

#[tokio::test]
async fn test_admin_federation_routes_require_admin() {
    let Some((app, _pool)) = setup_test_context().await else {
        return;
    };
    let user_token = register_user(&app, &format!("nonadmin_{}", rand::random::<u32>())).await;

    let request = Request::builder()
        .uri("/_synapse/admin/v1/federation/destinations")
        .header("Authorization", format!("Bearer {}", user_token))
        .body(Body::empty())
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app, request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

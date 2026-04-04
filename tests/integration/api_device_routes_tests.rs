use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use synapse_rust::cache::{CacheConfig, CacheManager};
use synapse_rust::services::ServiceContainer;
use synapse_rust::web::routes::create_router;
use synapse_rust::web::AppState;
use tower::ServiceExt;

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

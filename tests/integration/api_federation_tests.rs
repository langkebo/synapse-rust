use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::Value;
use std::sync::Arc;
use synapse_rust::cache::{CacheConfig, CacheManager};
use synapse_rust::services::ServiceContainer;
use synapse_rust::web::routes::create_router;
use synapse_rust::web::AppState;
use tower::ServiceExt;
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};

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
        .uri("/_matrix/client/v3/register")
        .header("Content-Type", "application/json")
        .body(Body::from(
            serde_json::json!({
                "username": format!("{}_{}", username, rand::random::<u32>()),
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

    let body = axum::body::to_bytes(response.into_body(), 2048)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    (
        json["access_token"].as_str().unwrap().to_string(),
        json["user_id"].as_str().unwrap().to_string(),
    )
}

async fn request_openid_token(app: &axum::Router, token: &str, user_id: &str) -> String {
    let request = Request::builder()
        .method("GET")
        .uri(format!(
            "/_matrix/client/v3/user/{}/openid/request_token",
            user_id
        ))
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 2048)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    json["access_token"].as_str().unwrap().to_string()
}


#[tokio::test]
async fn test_federation_version() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let request = Request::builder()
        .uri("/_matrix/federation/v1/version")
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
    assert!(json["server"]["version"].is_string());
}

#[tokio::test]
async fn test_federation_queries() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    // 1. Query Profile
    let request = Request::builder()
        .uri("/_matrix/federation/v1/query/profile/@alice:localhost?field=displayname")
        .body(Body::empty())
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    // Might be 404 if user doesn't exist, but the endpoint should exist
    assert!(response.status() == StatusCode::OK || response.status() == StatusCode::NOT_FOUND);

    // 2. Query Directory
    let request = Request::builder()
        .uri("/_matrix/federation/v1/query/directory?room_alias=#test:localhost")
        .body(Body::empty())
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert!(response.status() == StatusCode::OK || response.status() == StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_federation_public_rooms() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let request = Request::builder()
        .uri("/_matrix/federation/v1/publicRooms")
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_server_keys_endpoint_returns_verify_keys_without_config_signing_key() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let request = Request::builder()
        .uri("/_matrix/key/v2/server")
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 4096)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["server_name"], "test.example.com");
    assert!(json["verify_keys"]
        .as_object()
        .is_some_and(|keys| !keys.is_empty()));
}

#[tokio::test]
async fn test_local_key_query_reuses_server_key_response() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let request = Request::builder()
        .uri("/_matrix/key/v2/server")
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    let body = axum::body::to_bytes(response.into_body(), 4096)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    let key_id = json["verify_keys"]
        .as_object()
        .and_then(|keys| keys.keys().next().cloned())
        .unwrap();

    let request = Request::builder()
        .uri(format!("/_matrix/key/v2/query/test.example.com/{}", key_id))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app, request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 4096)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["server_name"], "test.example.com");
    assert!(json["verify_keys"].get(&key_id).is_some());
}

#[tokio::test]
async fn test_remote_key_query_fetches_real_remote_server_response() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let mock_server = MockServer::start().await;
    let key_id = "ed25519:test";
    let server_name = mock_server.address().to_string();

    Mock::given(method("GET"))
        .and(path("/_matrix/key/v2/server"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "server_name": server_name,
            "valid_until_ts": 4_102_444_800_000_i64,
            "verify_keys": {
                key_id: {
                    "key": "ZmFrZV9yZW1vdGVfa2V5"
                }
            },
            "old_verify_keys": {},
            "signatures": {}
        })))
        .mount(&mock_server)
        .await;

    let request = Request::builder()
        .uri(format!("/_matrix/key/v2/query/{}/{}", server_name, key_id))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app, request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 4096)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["server_name"], server_name);
    assert_eq!(json["verify_keys"][key_id]["key"], "ZmFrZV9yZW1vdGVfa2V5");
}

#[tokio::test]
async fn test_federation_openid_userinfo_validates_openid_token_without_placeholder() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let (access_token, user_id) = register_user(&app, "federation_openid").await;
    let openid_token = request_openid_token(&app, &access_token, &user_id).await;

    let request = Request::builder()
        .uri(format!(
            "/_matrix/federation/v1/openid/userinfo?access_token={}",
            openid_token
        ))
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
    assert_eq!(json["sub"], user_id);

    let invalid_request = Request::builder()
        .uri("/_matrix/federation/v1/openid/userinfo?access_token=invalid_token")
        .body(Body::empty())
        .unwrap();

    let invalid_response = ServiceExt::<Request<Body>>::oneshot(app, invalid_request)
        .await
        .unwrap();
    assert_eq!(invalid_response.status(), StatusCode::UNAUTHORIZED);
}

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use hmac::{Hmac, Mac};
use serde_json::{json, Value};
use std::sync::Arc;
use synapse_rust::cache::{CacheConfig, CacheManager};
use synapse_rust::services::ServiceContainer;
use synapse_rust::web::routes::create_router;
use synapse_rust::web::AppState;
use tower::ServiceExt;

type HmacSha256 = Hmac<sha2::Sha256>;

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

    let username = format!("audit_admin_{}", rand::random::<u32>());
    let password = "password123";
    let shared_secret = "test_shared_secret";

    let mut mac = HmacSha256::new_from_slice(shared_secret.as_bytes()).unwrap();
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

    let body = axum::body::to_bytes(response.into_body(), 4096)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    (
        json["access_token"].as_str().unwrap().to_string(),
        json["user_id"].as_str().unwrap().to_string(),
    )
}

async fn create_test_user(app: &axum::Router) -> (String, String) {
    let username = format!("audit_user_{}", rand::random::<u32>());
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
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 4096)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    (
        json["access_token"].as_str().unwrap().to_string(),
        json["user_id"].as_str().unwrap().to_string(),
    )
}

#[tokio::test]
async fn test_admin_audit_write_query_and_trace() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let (admin_token, _) = get_admin_token(&app).await;
    let unique = rand::random::<u32>();
    let action = format!("admin.audit.manual_{}", unique);
    let request_id = format!("req-audit-manual-{}", unique);

    let request = Request::builder()
        .method("POST")
        .uri("/_synapse/admin/v1/audit/events")
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "actor_id": format!("@manual_admin_{}:localhost", unique),
                "action": action,
                "resource_type": "user",
                "resource_id": format!("@manual_user_{}:localhost", unique),
                "result": "success",
                "request_id": request_id,
                "details": { "source": "integration_test" }
            })
            .to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 8192)
        .await
        .unwrap();
    let created_event: Value = serde_json::from_slice(&body).unwrap();
    let event_id = created_event["event_id"].as_str().unwrap().to_string();
    assert_eq!(created_event["action"], action);

    let request = Request::builder()
        .uri(format!("/_synapse/admin/v1/audit/events?action={}", action))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 8192)
        .await
        .unwrap();
    let list_response: Value = serde_json::from_slice(&body).unwrap();
    let events = list_response["events"].as_array().unwrap();
    assert!(events.iter().any(|event| event["event_id"] == event_id));

    let request = Request::builder()
        .uri(format!("/_synapse/admin/v1/audit/events/{}", event_id))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 8192)
        .await
        .unwrap();
    let detail_response: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(detail_response["request_id"], created_event["request_id"]);
    assert_eq!(detail_response["details"]["source"], "integration_test");
}

#[tokio::test]
async fn test_shadow_ban_writes_audit_event() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let (admin_token, admin_user_id) = get_admin_token(&app).await;
    let (_, user_id) = create_test_user(&app).await;
    let request_id = format!("req-shadow-ban-{}", rand::random::<u32>());

    let request = Request::builder()
        .method("POST")
        .uri(format!("/_synapse/admin/v1/users/{}/shadow_ban", user_id))
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("x-request-id", &request_id)
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let request = Request::builder()
        .uri("/_synapse/admin/v1/audit/events?action=admin.user.shadow_ban")
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 8192)
        .await
        .unwrap();
    let list_response: Value = serde_json::from_slice(&body).unwrap();
    let events = list_response["events"].as_array().unwrap();
    let audit_event = events
        .iter()
        .find(|event| {
            event["request_id"] == request_id
                && event["resource_id"] == user_id
                && event["actor_id"] == admin_user_id
        })
        .expect("shadow ban audit event should exist");

    let event_id = audit_event["event_id"].as_str().unwrap();

    let request = Request::builder()
        .uri(format!("/_synapse/admin/v1/audit/events/{}", event_id))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 8192)
        .await
        .unwrap();
    let detail_response: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(detail_response["details"]["is_shadow_banned"], true);
}

#[tokio::test]
async fn test_audit_query_requires_admin() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let (user_token, _) = create_test_user(&app).await;

    let request = Request::builder()
        .uri("/_synapse/admin/v1/audit/events")
        .header("Authorization", format!("Bearer {}", user_token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

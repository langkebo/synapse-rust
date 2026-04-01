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

    let username = format!("feature_admin_{}", rand::random::<u32>());
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
    json["access_token"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn test_feature_flag_create_get_list_and_update() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let admin_token = get_admin_token(&app).await;
    let flag_key = format!("room.summary.realtime_sync.{}", rand::random::<u32>());

    let request = Request::builder()
        .method("POST")
        .uri("/_synapse/admin/v1/feature-flags")
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("Content-Type", "application/json")
        .header("x-request-id", "feature-flags-create")
        .body(Body::from(
            json!({
                "flag_key": flag_key,
                "target_scope": "room",
                "rollout_percent": 25,
                "reason": "enable staged rollout",
                "status": "active",
                "expires_at": chrono::Utc::now().timestamp_millis() + 600000,
                "targets": [
                    { "subject_type": "room", "subject_id": "!room123:localhost" },
                    { "subject_type": "user", "subject_id": "@tester:localhost" }
                ]
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
    let created: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(created["flag_key"], flag_key);
    assert_eq!(created["rollout_percent"], 25);
    assert_eq!(created["targets"].as_array().unwrap().len(), 2);

    let request = Request::builder()
        .uri(format!("/_synapse/admin/v1/feature-flags/{}", flag_key))
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
    let fetched: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(fetched["status"], "active");

    let request = Request::builder()
        .uri("/_synapse/admin/v1/feature-flags?target_scope=room&status=active")
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
    let listed: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(listed["total"], 1);
    assert_eq!(listed["flags"].as_array().unwrap().len(), 1);

    let request = Request::builder()
        .method("PATCH")
        .uri(format!("/_synapse/admin/v1/feature-flags/{}", flag_key))
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("Content-Type", "application/json")
        .header("x-request-id", "feature-flags-update")
        .body(Body::from(
            json!({
                "rollout_percent": 100,
                "status": "fully_enabled",
                "reason": "guardrails passed"
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
    let updated: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(updated["rollout_percent"], 100);
    assert_eq!(updated["status"], "fully_enabled");

    let request = Request::builder()
        .uri("/_synapse/admin/v1/audit/events?action=admin.feature_flag.create")
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
    let audit_list: Value = serde_json::from_slice(&body).unwrap();
    let events = audit_list["events"].as_array().unwrap();
    assert!(events.iter().any(|event| event["resource_id"] == flag_key));
}

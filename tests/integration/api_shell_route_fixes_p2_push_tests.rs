// Integration tests for P2 shell route fixes - Push notifications
// Tests verify push notification operations return confirmation data

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;

async fn setup_test_app() -> axum::Router {
    super::setup_test_app()
        .await
        .unwrap_or_else(|| {
            panic!(
                "Shell route fix P2 push tests require isolated schema setup. Start PostgreSQL and apply migrations for local runs."
            )
        })
}

async fn register_user(app: &axum::Router, username: &str) -> (String, String, String) {
    let username = format!("{}_{}", username, rand::random::<u32>());
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

    (
        json["access_token"].as_str().unwrap().to_string(),
        json["user_id"].as_str().unwrap().to_string(),
        json["device_id"].as_str().unwrap().to_string(),
    )
}

#[tokio::test]
async fn test_set_pusher_returns_confirmation() {
    let app = setup_test_app().await;
    let (token, _user_id, _) = register_user(&app, "alice").await;

    // Set pusher
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/pushers/set")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::from(
            json!({
                "pushkey": "test_pushkey_123",
                "kind": "http",
                "app_id": "com.example.app",
                "app_display_name": "Example App",
                "device_display_name": "My Device",
                "lang": "en",
                "data": {
                    "url": "https://example.com/push"
                }
            })
            .to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .expect("Failed to set pusher");

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .expect("Failed to read response body");
    let body: Value = serde_json::from_slice(&body).expect("Failed to parse response");

    // Verify response contains confirmation data
    assert!(
        body.get("pushkey").is_some(),
        "Response should contain pushkey"
    );
    assert_eq!(body["pushkey"], "test_pushkey_123");
    assert!(body.get("kind").is_some(), "Response should contain kind");
    assert_eq!(body["kind"], "http");
    assert!(
        body.get("app_id").is_some(),
        "Response should contain app_id"
    );
    assert_eq!(body["app_id"], "com.example.app");
    assert!(
        body.get("created_ts").is_some(),
        "Response should contain created_ts"
    );
    assert!(
        body["created_ts"].is_number(),
        "created_ts should be a number"
    );
}

#[tokio::test]
async fn test_delete_pusher_returns_confirmation() {
    let app = setup_test_app().await;
    let (token, _user_id, _) = register_user(&app, "bob").await;

    // Set pusher first
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/pushers/set")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::from(
            json!({
                "pushkey": "test_pushkey_456",
                "kind": "http",
                "app_id": "com.example.app",
                "app_display_name": "Example App",
                "device_display_name": "My Device",
                "lang": "en",
                "data": {
                    "url": "https://example.com/push"
                }
            })
            .to_string(),
        ))
        .unwrap();

    ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .expect("Failed to set pusher");

    // Delete pusher
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/pushers/set")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::from(
            json!({
                "pushkey": "test_pushkey_456",
                "kind": "null",
                "app_id": "com.example.app",
                "app_display_name": "Example App",
                "device_display_name": "My Device",
                "lang": "en"
            })
            .to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .expect("Failed to delete pusher");

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .expect("Failed to read response body");
    let body: Value = serde_json::from_slice(&body).expect("Failed to parse response");

    // Verify response contains confirmation data
    assert!(
        body.get("deleted").is_some(),
        "Response should contain deleted flag"
    );
    assert_eq!(body["deleted"], true);
    assert!(
        body.get("pushkey").is_some(),
        "Response should contain pushkey"
    );
    assert_eq!(body["pushkey"], "test_pushkey_456");
}

#[tokio::test]
async fn test_set_push_rule_returns_confirmation() {
    let app = setup_test_app().await;
    let (token, _user_id, _) = register_user(&app, "charlie").await;

    // Set push rule
    let request = Request::builder()
        .method("PUT")
        .uri("/_matrix/client/v3/pushrules/global/override/test_rule")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::from(
            json!({
                "actions": ["notify"],
                "conditions": [
                    {
                        "kind": "event_match",
                        "key": "type",
                        "pattern": "m.room.message"
                    }
                ]
            })
            .to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .expect("Failed to set push rule");

    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 4096)
        .await
        .expect("Failed to read response body");
    if status != StatusCode::OK {
        panic!(
            "set push rule expected 200, got {} body={}",
            status.as_u16(),
            String::from_utf8_lossy(&body)
        );
    }
    let body: Value = serde_json::from_slice(&body).expect("Failed to parse response");

    // Verify response contains confirmation data
    assert!(
        body.get("rule_id").is_some(),
        "Response should contain rule_id"
    );
    assert_eq!(body["rule_id"], "test_rule");
    assert!(body.get("scope").is_some(), "Response should contain scope");
    assert_eq!(body["scope"], "global");
    assert!(body.get("kind").is_some(), "Response should contain kind");
    assert_eq!(body["kind"], "override");
    assert!(
        body.get("created_ts").is_some(),
        "Response should contain created_ts"
    );
    assert!(
        body["created_ts"].is_number(),
        "created_ts should be a number"
    );
}

#[tokio::test]
async fn test_create_push_rule_returns_confirmation() {
    let app = setup_test_app().await;
    let (token, _user_id, _) = register_user(&app, "dave").await;

    // Create push rule
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/pushrules/global/content/test_rule_2")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::from(
            json!({
                "actions": ["notify", {"set_tweak": "sound", "value": "default"}],
                "pattern": "alice"
            })
            .to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .expect("Failed to create push rule");

    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 4096)
        .await
        .expect("Failed to read response body");
    if status != StatusCode::OK {
        panic!(
            "create push rule expected 200, got {} body={}",
            status.as_u16(),
            String::from_utf8_lossy(&body)
        );
    }
    let body: Value = serde_json::from_slice(&body).expect("Failed to parse response");

    // Verify response contains confirmation data
    assert!(
        body.get("rule_id").is_some(),
        "Response should contain rule_id"
    );
    assert_eq!(body["rule_id"], "test_rule_2");
    assert!(body.get("scope").is_some(), "Response should contain scope");
    assert_eq!(body["scope"], "global");
    assert!(body.get("kind").is_some(), "Response should contain kind");
    assert_eq!(body["kind"], "content");
    assert!(
        body.get("created_ts").is_some(),
        "Response should contain created_ts"
    );
    assert!(
        body["created_ts"].is_number(),
        "created_ts should be a number"
    );
}

#[tokio::test]
async fn test_set_push_rule_actions_returns_confirmation() {
    let app = setup_test_app().await;
    let (token, _user_id, _) = register_user(&app, "eve").await;

    // Create push rule first
    let request = Request::builder()
        .method("PUT")
        .uri("/_matrix/client/v3/pushrules/global/override/test_rule_3")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::from(
            json!({
                "actions": ["notify"],
                "conditions": []
            })
            .to_string(),
        ))
        .unwrap();

    ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .expect("Failed to create push rule");

    // Update push rule actions
    let request = Request::builder()
        .method("PUT")
        .uri("/_matrix/client/v3/pushrules/global/override/test_rule_3/actions")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::from(
            json!({
                "actions": ["notify", {"set_tweak": "highlight"}]
            })
            .to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .expect("Failed to set push rule actions");

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .expect("Failed to read response body");
    let body: Value = serde_json::from_slice(&body).expect("Failed to parse response");

    // Verify response contains confirmation data
    assert!(
        body.get("rule_id").is_some(),
        "Response should contain rule_id"
    );
    assert_eq!(body["rule_id"], "test_rule_3");
    assert!(
        body.get("actions").is_some(),
        "Response should contain actions"
    );
    assert!(body["actions"].is_array(), "actions should be an array");
    assert!(
        body.get("updated_ts").is_some(),
        "Response should contain updated_ts"
    );
    assert!(
        body["updated_ts"].is_number(),
        "updated_ts should be a number"
    );
}

#[tokio::test]
async fn test_set_push_rule_enabled_returns_confirmation() {
    let app = setup_test_app().await;
    let (token, _user_id, _) = register_user(&app, "frank").await;

    // Create push rule first
    let request = Request::builder()
        .method("PUT")
        .uri("/_matrix/client/v3/pushrules/global/override/test_rule_4")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::from(
            json!({
                "actions": ["notify"],
                "conditions": []
            })
            .to_string(),
        ))
        .unwrap();

    ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .expect("Failed to create push rule");

    // Update push rule enabled state
    let request = Request::builder()
        .method("PUT")
        .uri("/_matrix/client/v3/pushrules/global/override/test_rule_4/enabled")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::from(
            json!({
                "enabled": false
            })
            .to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .expect("Failed to set push rule enabled");

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .expect("Failed to read response body");
    let body: Value = serde_json::from_slice(&body).expect("Failed to parse response");

    // Verify response contains confirmation data
    assert!(
        body.get("rule_id").is_some(),
        "Response should contain rule_id"
    );
    assert_eq!(body["rule_id"], "test_rule_4");
    assert!(
        body.get("enabled").is_some(),
        "Response should contain enabled"
    );
    assert_eq!(body["enabled"], false);
    assert!(
        body.get("updated_ts").is_some(),
        "Response should contain updated_ts"
    );
    assert!(
        body["updated_ts"].is_number(),
        "updated_ts should be a number"
    );
}

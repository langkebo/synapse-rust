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
use tokio::runtime::Runtime;
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

async fn register_user(app: &axum::Router, username: &str) -> String {
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/r0/register")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "username": username,
                "password": "Password123!",
                "auth": {"type": "m.login.dummy"}
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

#[test]
fn test_friend_system_extended() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let app = match setup_test_app().await {
            Some(app) => app,
            None => return,
        };
        let alice_token = register_user(&app, &format!("alice_{}", rand::random::<u32>())).await;

        // 1. Get Friend List
        let request = Request::builder()
            .uri("/_matrix/client/v1/friends")
            .header("Authorization", format!("Bearer {}", alice_token))
            .body(Body::empty())
            .unwrap();
        let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // 2. Get Friend Requests
        let request = Request::builder()
            .uri("/_matrix/client/v1/friends/requests/incoming")
            .header("Authorization", format!("Bearer {}", alice_token))
            .body(Body::empty())
            .unwrap();
        let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // 3. Get Outgoing Friend Requests
        let request = Request::builder()
            .uri("/_matrix/client/v1/friends/requests/outgoing")
            .header("Authorization", format!("Bearer {}", alice_token))
            .body(Body::empty())
            .unwrap();
        let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    });
}

#[test]
fn test_trusted_private_chat_preset() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let app = match setup_test_app().await {
            Some(app) => app,
            None => return,
        };
        let alice_token = register_user(&app, &format!("alice_{}", rand::random::<u32>())).await;

        // Create a trusted private chat room using the standard Matrix API
        let request = Request::builder()
            .method("POST")
            .uri("/_matrix/client/r0/createRoom")
            .header("Authorization", format!("Bearer {}", alice_token))
            .header("Content-Type", "application/json")
            .body(Body::from(
                json!({
                    "preset": "trusted_private_chat",
                    "visibility": "private",
                    "name": "Private Chat",
                    "topic": "Encrypted & Secure",
                    "is_direct": true
                })
                .to_string(),
            ))
            .unwrap();
        let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // Verify room was created
        let body = axum::body::to_bytes(response.into_body(), 4096)
            .await
            .unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();
        assert!(json["room_id"].is_string());

        // Get room state to verify privacy settings were applied
        let room_id = json["room_id"].as_str().unwrap();
        let request = Request::builder()
            .uri(format!("/_matrix/client/r0/rooms/{}/state", room_id))
            .header("Authorization", format!("Bearer {}", alice_token))
            .body(Body::empty())
            .unwrap();
        let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    });
}

#[test]
fn test_voice_messages() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let app = match setup_test_app().await {
            Some(app) => app,
            None => return,
        };
        let _alice_token = register_user(&app, &format!("alice_{}", rand::random::<u32>())).await;

        // 1. Get Voice Config (this doesn't require database)
        let request = Request::builder()
            .uri("/_matrix/client/r0/voice/config")
            .header("Authorization", format!("Bearer {}", _alice_token))
            .body(Body::empty())
            .unwrap();
        let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    });
}

#[test]
fn test_thirdparty_routes_share_across_r0_and_v3() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let app = match setup_test_app().await {
            Some(app) => app,
            None => return,
        };
        let alice_token =
            register_user(&app, &format!("thirdparty_{}", rand::random::<u32>())).await;

        let v3_protocols_request = Request::builder()
            .uri("/_matrix/client/v3/thirdparty/protocols")
            .header("Authorization", format!("Bearer {}", alice_token))
            .body(Body::empty())
            .unwrap();
        let v3_protocols_response =
            ServiceExt::<Request<Body>>::oneshot(app.clone(), v3_protocols_request)
                .await
                .unwrap();
        assert_eq!(v3_protocols_response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(v3_protocols_response.into_body(), 1024)
            .await
            .unwrap();
        let v3_protocols_json: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(v3_protocols_json, json!({}));

        let r0_protocols_request = Request::builder()
            .uri("/_matrix/client/r0/thirdparty/protocols")
            .header("Authorization", format!("Bearer {}", alice_token))
            .body(Body::empty())
            .unwrap();
        let r0_protocols_response =
            ServiceExt::<Request<Body>>::oneshot(app.clone(), r0_protocols_request)
                .await
                .unwrap();
        assert_eq!(r0_protocols_response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(r0_protocols_response.into_body(), 1024)
            .await
            .unwrap();
        let r0_protocols_json: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(r0_protocols_json, json!({}));

        let v3_protocol_request = Request::builder()
            .uri("/_matrix/client/v3/thirdparty/protocol/test")
            .header("Authorization", format!("Bearer {}", alice_token))
            .body(Body::empty())
            .unwrap();
        let v3_protocol_response =
            ServiceExt::<Request<Body>>::oneshot(app.clone(), v3_protocol_request)
                .await
                .unwrap();
        assert_eq!(v3_protocol_response.status(), StatusCode::NOT_FOUND);

        let r0_protocol_request = Request::builder()
            .uri("/_matrix/client/r0/thirdparty/protocol/test")
            .header("Authorization", format!("Bearer {}", alice_token))
            .body(Body::empty())
            .unwrap();
        let r0_protocol_response =
            ServiceExt::<Request<Body>>::oneshot(app.clone(), r0_protocol_request)
                .await
                .unwrap();
        assert_eq!(r0_protocol_response.status(), StatusCode::NOT_FOUND);

        let v3_location_request = Request::builder()
            .uri("/_matrix/client/v3/thirdparty/location?alias=%23demo:localhost")
            .header("Authorization", format!("Bearer {}", alice_token))
            .body(Body::empty())
            .unwrap();
        let v3_location_response =
            ServiceExt::<Request<Body>>::oneshot(app.clone(), v3_location_request)
                .await
                .unwrap();
        assert_eq!(v3_location_response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(v3_location_response.into_body(), 1024)
            .await
            .unwrap();
        let v3_location_json: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(v3_location_json, json!([]));

        let r0_location_request = Request::builder()
            .uri("/_matrix/client/r0/thirdparty/location?alias=%23demo:localhost")
            .header("Authorization", format!("Bearer {}", alice_token))
            .body(Body::empty())
            .unwrap();
        let r0_location_response = ServiceExt::<Request<Body>>::oneshot(app, r0_location_request)
            .await
            .unwrap();
        assert_eq!(r0_location_response.status(), StatusCode::NOT_FOUND);
    });
}

#[test]
fn test_push_routes_share_across_r0_and_v3() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let app = match setup_test_app().await {
            Some(app) => app,
            None => return,
        };
        let alice_token = register_user(&app, &format!("push_{}", rand::random::<u32>())).await;

        let set_pusher_request = Request::builder()
            .method("POST")
            .uri("/_matrix/client/v3/pushers/set")
            .header("Authorization", format!("Bearer {}", alice_token))
            .header("Content-Type", "application/json")
            .body(Body::from(
                json!({
                    "pushkey": "pushkey-1",
                    "kind": "http",
                    "app_id": "com.example.push",
                    "app_display_name": "Example Push",
                    "device_display_name": "Alice Device",
                    "lang": "en",
                    "data": {
                        "url": "https://push.example.test"
                    }
                })
                .to_string(),
            ))
            .unwrap();
        let set_pusher_response =
            ServiceExt::<Request<Body>>::oneshot(app.clone(), set_pusher_request)
                .await
                .unwrap();
        assert_eq!(set_pusher_response.status(), StatusCode::OK);

        let r0_pushers_request = Request::builder()
            .uri("/_matrix/client/r0/pushers")
            .header("Authorization", format!("Bearer {}", alice_token))
            .body(Body::empty())
            .unwrap();
        let r0_pushers_response =
            ServiceExt::<Request<Body>>::oneshot(app.clone(), r0_pushers_request)
                .await
                .unwrap();
        assert_eq!(r0_pushers_response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(r0_pushers_response.into_body(), 2048)
            .await
            .unwrap();
        let r0_pushers_json: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(r0_pushers_json["pushers"][0]["pushkey"], json!("pushkey-1"));

        let v3_pushrules_request = Request::builder()
            .uri("/_matrix/client/v3/pushrules")
            .header("Authorization", format!("Bearer {}", alice_token))
            .body(Body::empty())
            .unwrap();
        let v3_pushrules_response =
            ServiceExt::<Request<Body>>::oneshot(app.clone(), v3_pushrules_request)
                .await
                .unwrap();
        assert_eq!(v3_pushrules_response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(v3_pushrules_response.into_body(), 4096)
            .await
            .unwrap();
        let v3_pushrules_json: Value = serde_json::from_slice(&body).unwrap();
        assert!(v3_pushrules_json.get("global").is_some());

        let r0_notifications_request = Request::builder()
            .uri("/_matrix/client/r0/notifications")
            .header("Authorization", format!("Bearer {}", alice_token))
            .body(Body::empty())
            .unwrap();
        let r0_notifications_response =
            ServiceExt::<Request<Body>>::oneshot(app.clone(), r0_notifications_request)
                .await
                .unwrap();
        assert_eq!(r0_notifications_response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(r0_notifications_response.into_body(), 2048)
            .await
            .unwrap();
        let r0_notifications_json: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(r0_notifications_json["notifications"], json!([]));

        let r0_enabled_request = Request::builder()
            .uri("/_matrix/client/r0/pushrules/global/override/.m.rule.master/enabled")
            .header("Authorization", format!("Bearer {}", alice_token))
            .body(Body::empty())
            .unwrap();
        let r0_enabled_response = ServiceExt::<Request<Body>>::oneshot(app, r0_enabled_request)
            .await
            .unwrap();
        assert_eq!(r0_enabled_response.status(), StatusCode::NOT_FOUND);
    });
}

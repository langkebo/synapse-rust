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

    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 1024)
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
    (
        json["access_token"].as_str().unwrap().to_string(),
        json["user_id"].as_str().unwrap().to_string(),
    )
}

#[tokio::test]
async fn test_e2ee_keys() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let (token, user_id) = register_user(&app, &format!("user_{}", rand::random::<u32>())).await;

    // 1. Upload Keys
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/r0/keys/upload")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "device_keys": {
                    "user_id": user_id.clone(),
                    "device_id": "MY_DEVICE",
                    "algorithms": ["m.olm.v1.curve25519-aes-sha2", "m.megolm.v1.aes-sha2"],
                    "keys": {
                        "curve25519:MY_DEVICE": "xyz...",
                        "ed25519:MY_DEVICE": "abc..."
                    },
                    "signatures": {
                        user_id.clone(): {
                            "ed25519:MY_DEVICE": "sig..."
                        }
                    }
                },
                "one_time_keys": {
                    "curve25519:key1": "key1...",
                    "curve25519:key2": "key2..."
                }
            })
            .to_string(),
        ))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();

    let status = response.status();
    if status != StatusCode::OK {
        let body = axum::body::to_bytes(response.into_body(), 10240)
            .await
            .unwrap();
        panic!(
            "Upload keys failed with status {}: {:?}",
            status,
            String::from_utf8_lossy(&body)
        );
    }

    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert!(
        json["one_time_key_counts"]["signed_curve25519"]
            .as_i64()
            .unwrap()
            >= 2
    );

    // 2. Query Keys
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/r0/keys/query")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "device_keys": {
                    user_id.clone(): []
                }
            })
            .to_string(),
        ))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // 3. Claim Keys
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/r0/keys/claim")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "one_time_keys": {
                    user_id: {
                        "MY_DEVICE": "curve25519"
                    }
                }
            })
            .to_string(),
        ))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // 4. Get Key Changes
    let request = Request::builder()
        .uri("/_matrix/client/r0/keys/changes?from=0&to=100")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_e2ee_shared_routes_across_versions() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let (token, user_id) =
        register_user(&app, &format!("e2ee_shared_{}", rand::random::<u32>())).await;

    let upload_request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/keys/upload")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "device_keys": {
                    "user_id": user_id.clone(),
                    "device_id": "DEVICE_SHARED",
                    "algorithms": ["m.olm.v1.curve25519-aes-sha2"],
                    "keys": {
                        "curve25519:DEVICE_SHARED": "shared-curve",
                        "ed25519:DEVICE_SHARED": "shared-ed"
                    },
                    "signatures": {
                        user_id.clone(): {
                            "ed25519:DEVICE_SHARED": "shared-signature"
                        }
                    }
                },
                "one_time_keys": {
                    "curve25519:shared1": "shared-key"
                }
            })
            .to_string(),
        ))
        .unwrap();
    let upload_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), upload_request)
        .await
        .unwrap();
    assert_eq!(upload_response.status(), StatusCode::OK);

    let query_request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/r0/keys/query")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "device_keys": {
                    user_id.clone(): []
                }
            })
            .to_string(),
        ))
        .unwrap();
    let query_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), query_request)
        .await
        .unwrap();
    assert_eq!(query_response.status(), StatusCode::OK);

    let device_signing_request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/r0/keys/device_signing/upload")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({}).to_string()))
        .unwrap();
    let device_signing_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), device_signing_request)
            .await
            .unwrap();
    assert_eq!(device_signing_response.status(), StatusCode::OK);

    let changes_request = Request::builder()
        .method("GET")
        .uri("/_matrix/client/v3/keys/changes?from=0&to=100")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let changes_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), changes_request)
        .await
        .unwrap();
    assert_eq!(changes_response.status(), StatusCode::OK);

    let summary_request = Request::builder()
        .method("GET")
        .uri("/_matrix/client/v3/security/summary")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let summary_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), summary_request)
        .await
        .unwrap();
    assert_eq!(summary_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(summary_response.into_body(), 2048)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert!(json.get("security_score").is_some());

    let missing_r0_only_request = Request::builder()
        .method("GET")
        .uri("/_matrix/client/r0/security/summary")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let missing_r0_only_response =
        ServiceExt::<Request<Body>>::oneshot(app, missing_r0_only_request)
            .await
            .unwrap();
    assert_eq!(missing_r0_only_response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_verification_routes_work_across_v1_and_r0() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let (token, _) = register_user(&app, &format!("verify_shared_{}", rand::random::<u32>())).await;

    let v1_show_request = Request::builder()
        .method("GET")
        .uri("/_matrix/client/v1/keys/qr_code/show")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let v1_show_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), v1_show_request)
        .await
        .unwrap();
    assert_eq!(v1_show_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(v1_show_response.into_body(), 2048)
        .await
        .unwrap();
    let v1_show_json: Value = serde_json::from_slice(&body).unwrap();

    let r0_show_request = Request::builder()
        .method("GET")
        .uri("/_matrix/client/r0/keys/qr_code/show")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let r0_show_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), r0_show_request)
        .await
        .unwrap();
    assert_eq!(r0_show_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(r0_show_response.into_body(), 2048)
        .await
        .unwrap();
    let r0_show_json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v1_show_json["user_id"], r0_show_json["user_id"]);
    assert_eq!(v1_show_json["device_id"], r0_show_json["device_id"]);

    let v1_start_request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v1/keys/device_signing/verify_start")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "from_device": "DEVICE",
                "to_user": "@nobody:localhost"
            })
            .to_string(),
        ))
        .unwrap();
    let v1_start_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), v1_start_request)
        .await
        .unwrap();

    let r0_start_request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/r0/keys/device_signing/verify_start")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "from_device": "DEVICE",
                "to_user": "@nobody:localhost"
            })
            .to_string(),
        ))
        .unwrap();
    let r0_start_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), r0_start_request)
        .await
        .unwrap();
    assert_eq!(v1_start_response.status(), r0_start_response.status());

    let v3_start_request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/keys/device_signing/verify_start")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "from_device": "DEVICE",
                "to_user": "@nobody:localhost"
            })
            .to_string(),
        ))
        .unwrap();
    let v3_start_response = ServiceExt::<Request<Body>>::oneshot(app, v3_start_request)
        .await
        .unwrap();
    assert_eq!(v3_start_response.status(), StatusCode::NOT_FOUND);
}

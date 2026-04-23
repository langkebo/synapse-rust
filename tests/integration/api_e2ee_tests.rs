use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;

async fn setup_test_app() -> Option<axum::Router> {
    super::setup_test_app().await
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

async fn create_room(app: &axum::Router, token: &str, name: &str) -> String {
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/createRoom")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "name": name }).to_string()))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    json["room_id"].as_str().unwrap().to_string()
}

async fn invite_user_to_room(
    app: &axum::Router,
    inviter_token: &str,
    room_id: &str,
    invitee_user_id: &str,
) {
    let request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/r0/rooms/{}/invite", room_id))
        .header("Authorization", format!("Bearer {}", inviter_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({ "user_id": invitee_user_id }).to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

async fn join_room(app: &axum::Router, token: &str, room_id: &str) {
    let request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/r0/rooms/{}/join", room_id))
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

async fn upload_test_device_keys(
    app: &axum::Router,
    token: &str,
    user_id: &str,
    device_id: &str,
    include_one_time_key: bool,
) {
    let one_time_keys = if include_one_time_key {
        json!({
            format!("curve25519:{}_otk", device_id): format!("{}_otk_value", device_id)
        })
    } else {
        json!({})
    };

    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/r0/keys/upload")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "device_keys": {
                    "user_id": user_id,
                    "device_id": device_id,
                    "algorithms": ["m.olm.v1.curve25519-aes-sha2"],
                    "keys": {
                        format!("curve25519:{}", device_id): format!("{}_curve", device_id),
                        format!("ed25519:{}", device_id): format!("{}_ed", device_id)
                    },
                    "signatures": {
                        user_id: {
                            format!("ed25519:{}", device_id): format!("{}_sig", device_id)
                        }
                    }
                },
                "one_time_keys": one_time_keys
            })
            .to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
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
        json["one_time_key_counts"]["curve25519"]
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
async fn test_sync_returns_device_one_time_keys_count() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let (token, user_id) =
        register_user(&app, &format!("sync_e2ee_{}", rand::random::<u32>())).await;

    let upload_request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/r0/keys/upload")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "device_keys": {
                    "user_id": user_id.clone(),
                    "device_id": "SYNC_DEVICE",
                    "algorithms": ["m.olm.v1.curve25519-aes-sha2"],
                    "keys": {
                        "curve25519:SYNC_DEVICE": "sync-curve",
                        "ed25519:SYNC_DEVICE": "sync-ed"
                    },
                    "signatures": {
                        user_id.clone(): {
                            "ed25519:SYNC_DEVICE": "sync-signature"
                        }
                    }
                },
                "one_time_keys": {
                    "curve25519:sync1": "sync-key-1"
                }
            })
            .to_string(),
        ))
        .unwrap();
    let upload_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), upload_request)
        .await
        .unwrap();
    assert_eq!(upload_response.status(), StatusCode::OK);

    let sync_request = Request::builder()
        .method("GET")
        .uri("/_matrix/client/v3/sync")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let sync_response = ServiceExt::<Request<Body>>::oneshot(app, sync_request)
        .await
        .unwrap();
    assert_eq!(sync_response.status(), StatusCode::OK);

    let sync_body = axum::body::to_bytes(sync_response.into_body(), 32 * 1024)
        .await
        .unwrap();
    let sync_json: Value = serde_json::from_slice(&sync_body).unwrap();
    assert!(
        sync_json["device_one_time_keys_count"]["curve25519"]
            .as_i64()
            .unwrap_or_default()
            >= 1
    );
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

#[tokio::test]
async fn test_keys_query_filters_users_without_shared_rooms() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let (alice_token, alice_user_id) =
        register_user(&app, &format!("e2ee_query_alice_{}", rand::random::<u32>())).await;
    let (bob_token, bob_user_id) =
        register_user(&app, &format!("e2ee_query_bob_{}", rand::random::<u32>())).await;

    upload_test_device_keys(&app, &bob_token, &bob_user_id, "BOB_QUERY_DEVICE", false).await;

    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/r0/keys/query")
        .header("Authorization", format!("Bearer {}", alice_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "device_keys": {
                    alice_user_id.clone(): [],
                    bob_user_id.clone(): []
                }
            })
            .to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 32 * 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert!(json["device_keys"].get(&alice_user_id).is_some());
    assert!(json["device_keys"].get(&bob_user_id).is_none());
}

#[tokio::test]
async fn test_keys_query_allows_users_with_shared_rooms() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let (alice_token, _) =
        register_user(&app, &format!("e2ee_room_alice_{}", rand::random::<u32>())).await;
    let (bob_token, bob_user_id) =
        register_user(&app, &format!("e2ee_room_bob_{}", rand::random::<u32>())).await;

    upload_test_device_keys(&app, &bob_token, &bob_user_id, "BOB_SHARED_DEVICE", false).await;

    let room_id = create_room(&app, &alice_token, "E2EE Shared Query Room").await;
    invite_user_to_room(&app, &alice_token, &room_id, &bob_user_id).await;
    join_room(&app, &bob_token, &room_id).await;

    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/r0/keys/query")
        .header("Authorization", format!("Bearer {}", alice_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "device_keys": {
                    bob_user_id.clone(): []
                }
            })
            .to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 32 * 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert!(json["device_keys"].get(&bob_user_id).is_some());
}

#[tokio::test]
async fn test_keys_claim_filters_users_without_shared_rooms() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let (alice_token, _) =
        register_user(&app, &format!("e2ee_claim_alice_{}", rand::random::<u32>())).await;
    let (bob_token, bob_user_id) =
        register_user(&app, &format!("e2ee_claim_bob_{}", rand::random::<u32>())).await;

    upload_test_device_keys(&app, &bob_token, &bob_user_id, "BOB_CLAIM_DEVICE", true).await;

    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/r0/keys/claim")
        .header("Authorization", format!("Bearer {}", alice_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "one_time_keys": {
                    bob_user_id.clone(): {
                        "BOB_CLAIM_DEVICE": "curve25519"
                    }
                }
            })
            .to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app, request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 32 * 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert!(json["one_time_keys"].get(&bob_user_id).is_none());
}

#[tokio::test]
async fn test_key_changes_filters_users_without_shared_rooms() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let (alice_token, _) = register_user(
        &app,
        &format!("e2ee_changes_alice_{}", rand::random::<u32>()),
    )
    .await;
    let (bob_token, bob_user_id) =
        register_user(&app, &format!("e2ee_changes_bob_{}", rand::random::<u32>())).await;

    upload_test_device_keys(&app, &bob_token, &bob_user_id, "BOB_CHANGE_DEVICE", false).await;

    let request = Request::builder()
        .method("GET")
        .uri("/_matrix/client/r0/keys/changes?from=0&to=1000")
        .header("Authorization", format!("Bearer {}", alice_token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 32 * 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let changed = json["changed"].as_array().unwrap();
    let left = json["left"].as_array().unwrap();

    assert!(!changed.iter().any(|entry| entry == &json!(bob_user_id)));
    assert!(!left.iter().any(|entry| entry == &json!(bob_user_id)));
}

#[tokio::test]
async fn test_key_changes_allows_users_with_shared_rooms() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let (alice_token, _) = register_user(
        &app,
        &format!("e2ee_shared_changes_alice_{}", rand::random::<u32>()),
    )
    .await;
    let (bob_token, bob_user_id) = register_user(
        &app,
        &format!("e2ee_shared_changes_bob_{}", rand::random::<u32>()),
    )
    .await;

    let room_id = create_room(&app, &alice_token, "E2EE Shared Changes Room").await;
    invite_user_to_room(&app, &alice_token, &room_id, &bob_user_id).await;
    join_room(&app, &bob_token, &room_id).await;
    upload_test_device_keys(
        &app,
        &bob_token,
        &bob_user_id,
        "BOB_SHARED_CHANGE_DEVICE",
        false,
    )
    .await;

    let request = Request::builder()
        .method("GET")
        .uri("/_matrix/client/r0/keys/changes?from=0&to=1000")
        .header("Authorization", format!("Bearer {}", alice_token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 32 * 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let changed = json["changed"].as_array().unwrap();
    let left = json["left"].as_array().unwrap();

    assert!(changed.iter().any(|entry| entry == &json!(bob_user_id)));
    assert!(!left.iter().any(|entry| entry == &json!(bob_user_id)));
}

#[tokio::test]
async fn test_device_verification_v3_accepts_alias_fields_and_round_trips_status() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let (token, _) = register_user(&app, &format!("device_verify_{}", rand::random::<u32>())).await;

    let request_verification_request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/device_verification/request")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "device_id": "SECOND_DEVICE",
                "method": "sas"
            })
            .to_string(),
        ))
        .unwrap();
    let request_verification_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), request_verification_request)
            .await
            .unwrap();
    if request_verification_response.status() != StatusCode::OK {
        let body = axum::body::to_bytes(request_verification_response.into_body(), 8192)
            .await
            .unwrap();
        panic!(
            "request_device_verification failed: {:?}",
            String::from_utf8_lossy(&body)
        );
    }

    let request_verification_body =
        axum::body::to_bytes(request_verification_response.into_body(), 4096)
            .await
            .unwrap();
    let request_verification_json: Value =
        serde_json::from_slice(&request_verification_body).unwrap();
    let verification_token = request_verification_json["token"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        request_verification_json["request_token"],
        request_verification_json["token"]
    );
    assert_eq!(request_verification_json["status"], "pending");

    let status_request = Request::builder()
        .method("GET")
        .uri(format!(
            "/_matrix/client/v3/device_verification/status/{}",
            verification_token
        ))
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let status_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), status_request)
        .await
        .unwrap();
    assert_eq!(status_response.status(), StatusCode::OK);

    let status_body = axum::body::to_bytes(status_response.into_body(), 4096)
        .await
        .unwrap();
    let status_json: Value = serde_json::from_slice(&status_body).unwrap();
    assert_eq!(status_json["request_token"], status_json["token"]);
    assert_eq!(status_json["status"], "pending");

    let respond_request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/device_verification/respond")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "token": verification_token,
                "approved": true
            })
            .to_string(),
        ))
        .unwrap();
    let respond_response = ServiceExt::<Request<Body>>::oneshot(app, respond_request)
        .await
        .unwrap();
    if respond_response.status() != StatusCode::OK {
        let body = axum::body::to_bytes(respond_response.into_body(), 8192)
            .await
            .unwrap();
        panic!(
            "respond_device_verification failed: {:?}",
            String::from_utf8_lossy(&body)
        );
    }

    let respond_body = axum::body::to_bytes(respond_response.into_body(), 4096)
        .await
        .unwrap();
    let respond_json: Value = serde_json::from_slice(&respond_body).unwrap();
    assert_eq!(respond_json["success"], true);
    assert_eq!(respond_json["trust_level"], "verified");
}

#[tokio::test]
async fn test_verification_request_listing_and_cancellation_flow() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let (alice_token, alice_user_id) =
        register_user(&app, &format!("verify_alice_{}", rand::random::<u32>())).await;
    let (bob_token, bob_user_id) =
        register_user(&app, &format!("verify_bob_{}", rand::random::<u32>())).await;
    let (mallory_token, _) =
        register_user(&app, &format!("verify_mallory_{}", rand::random::<u32>())).await;

    let start_request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v1/keys/device_signing/verify_start")
        .header("Authorization", format!("Bearer {}", alice_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "from_device": "ALICE",
                "to_user": bob_user_id,
                "to_device": "BOB",
                "method": "m.sas.v1"
            })
            .to_string(),
        ))
        .unwrap();
    let start_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), start_request)
        .await
        .unwrap();
    assert_eq!(start_response.status(), StatusCode::OK);

    let start_body = axum::body::to_bytes(start_response.into_body(), 4096)
        .await
        .unwrap();
    let start_json: Value = serde_json::from_slice(&start_body).unwrap();
    let transaction_id = start_json["transaction_id"].as_str().unwrap().to_string();

    let pending_request = Request::builder()
        .method("GET")
        .uri("/_matrix/client/v1/keys/device_signing/requests")
        .header("Authorization", format!("Bearer {}", bob_token))
        .body(Body::empty())
        .unwrap();
    let pending_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), pending_request)
        .await
        .unwrap();
    assert_eq!(pending_response.status(), StatusCode::OK);

    let pending_body = axum::body::to_bytes(pending_response.into_body(), 4096)
        .await
        .unwrap();
    let pending_json: Value = serde_json::from_slice(&pending_body).unwrap();
    let requests = pending_json["requests"].as_array().unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0]["transaction_id"], transaction_id);
    assert_eq!(requests[0]["from_user"], alice_user_id);
    assert_eq!(requests[0]["to_user"], bob_user_id);
    assert_eq!(requests[0]["state"], "requested");

    let forbidden_cancel_request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v1/keys/device_signing/verify_cancel")
        .header("Authorization", format!("Bearer {}", mallory_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "transaction_id": transaction_id,
                "code": "m.user",
                "reason": "Mallory should not cancel"
            })
            .to_string(),
        ))
        .unwrap();
    let forbidden_cancel_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), forbidden_cancel_request)
            .await
            .unwrap();
    assert_eq!(forbidden_cancel_response.status(), StatusCode::FORBIDDEN);

    let cancel_request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v1/keys/device_signing/verify_cancel")
        .header("Authorization", format!("Bearer {}", bob_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "transaction_id": transaction_id,
                "code": "m.user",
                "reason": "Cancelled by receiver"
            })
            .to_string(),
        ))
        .unwrap();
    let cancel_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), cancel_request)
        .await
        .unwrap();
    assert_eq!(cancel_response.status(), StatusCode::OK);

    let cancel_body = axum::body::to_bytes(cancel_response.into_body(), 4096)
        .await
        .unwrap();
    let cancel_json: Value = serde_json::from_slice(&cancel_body).unwrap();
    assert_eq!(cancel_json["transaction_id"], transaction_id);
    assert_eq!(cancel_json["state"], "cancelled");
    assert_eq!(cancel_json["code"], "m.user");

    let post_cancel_list_request = Request::builder()
        .method("GET")
        .uri("/_matrix/client/v1/keys/device_signing/requests")
        .header("Authorization", format!("Bearer {}", bob_token))
        .body(Body::empty())
        .unwrap();
    let post_cancel_list_response =
        ServiceExt::<Request<Body>>::oneshot(app, post_cancel_list_request)
            .await
            .unwrap();
    assert_eq!(post_cancel_list_response.status(), StatusCode::OK);

    let post_cancel_body = axum::body::to_bytes(post_cancel_list_response.into_body(), 4096)
        .await
        .unwrap();
    let post_cancel_json: Value = serde_json::from_slice(&post_cancel_body).unwrap();
    assert_eq!(post_cancel_json["requests"], json!([]));
}

#[tokio::test]
async fn test_room_key_forward_and_backward_routes() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let (token, _) = register_user(&app, &format!("room_keys_{}", rand::random::<u32>())).await;
    let room_id = create_room(&app, &token, "Room Keys Test").await;

    let create_backup_request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/room_keys/version")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "algorithm": "m.megolm.v1.aes-sha2",
                "auth_data": {}
            })
            .to_string(),
        ))
        .unwrap();
    let create_backup_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), create_backup_request)
            .await
            .unwrap();
    assert_eq!(create_backup_response.status(), StatusCode::OK);

    let forward_request = Request::builder()
        .method("PUT")
        .uri(format!(
            "/_matrix/client/v3/rooms/{}/room_keys/keys",
            room_id
        ))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "sessions": {
                    "sess1": {
                        "first_message_index": 0,
                        "forwarded_count": 0,
                        "is_verified": true,
                        "session_data": {
                            "ciphertext": "abc123"
                        }
                    }
                }
            })
            .to_string(),
        ))
        .unwrap();
    let forward_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), forward_request)
        .await
        .unwrap();
    let forward_status = forward_response.status();
    let body = axum::body::to_bytes(forward_response.into_body(), 4096)
        .await
        .unwrap();
    if forward_status != StatusCode::OK {
        panic!(
            "forward_room_keys failed with status {}: {:?}",
            forward_status,
            String::from_utf8_lossy(&body)
        );
    }
    let forward_json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(forward_json["count"], json!(1));
    assert!(forward_json["version"].as_str().is_some());

    let version_request = Request::builder()
        .method("GET")
        .uri(format!("/_matrix/client/v3/rooms/{}/keys/version", room_id))
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let version_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), version_request)
        .await
        .unwrap();
    assert_eq!(version_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(version_response.into_body(), 2048)
        .await
        .unwrap();
    let version_json: Value = serde_json::from_slice(&body).unwrap();
    assert_ne!(version_json["version"], json!("0"));

    let count_request = Request::builder()
        .method("GET")
        .uri(format!("/_matrix/client/v3/rooms/{}/keys/count", room_id))
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let count_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), count_request)
        .await
        .unwrap();
    let count_status = count_response.status();
    let body = axum::body::to_bytes(count_response.into_body(), 4096)
        .await
        .unwrap();
    if count_status != StatusCode::OK {
        panic!(
            "get_room_key_count failed with status {}: {:?}",
            count_status,
            String::from_utf8_lossy(&body)
        );
    }
    let count_json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(count_json["count"], json!(1));

    let get_request = Request::builder()
        .method("GET")
        .uri(format!("/_matrix/client/v3/rooms/{}/keys", room_id))
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let get_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), get_request)
        .await
        .unwrap();
    assert_eq!(get_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(get_response.into_body(), 2048)
        .await
        .unwrap();
    let get_json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(get_json["room_id"], json!(room_id));
    assert_eq!(get_json["keys"][0]["session_id"], json!("sess1"));

    let claim_request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/v3/rooms/{}/keys/claim", room_id))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "session_ids": ["sess1"] }).to_string()))
        .unwrap();
    let claim_response = ServiceExt::<Request<Body>>::oneshot(app, claim_request)
        .await
        .unwrap();
    assert_eq!(claim_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(claim_response.into_body(), 2048)
        .await
        .unwrap();
    let claim_json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        claim_json["one_time_keys"][room_id]["sess1"]["session_data"]["ciphertext"],
        json!("abc123")
    );
}

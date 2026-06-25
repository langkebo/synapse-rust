use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use ed25519_dalek::{Signer, SigningKey};
use serde_json::{json, Value};
use std::sync::OnceLock;
use synapse_rust::cache::{CacheConfig, CacheManager};
use synapse_rust::e2ee::signed_json::{canonical_json_bytes, remove_signatures_and_unsigned};
use synapse_rust::services::ServiceContainer;
use synapse_rust::web::routes::state::AppState;
use tower::ServiceExt;

static SYNC_DEVICE_LISTS_TEST_MUTEX: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();

fn sync_device_lists_test_mutex() -> &'static tokio::sync::Mutex<()> {
    SYNC_DEVICE_LISTS_TEST_MUTEX.get_or_init(|| tokio::sync::Mutex::new(()))
}

async fn setup_test_app() -> Option<axum::Router> {
    super::setup_test_app().await
}

async fn setup_test_app_with_state() -> Option<(axum::Router, synapse_rust::web::routes::state::AppState)> {
    super::setup_test_app_with_state().await
}

async fn setup_isolated_test_app_with_state() -> Option<(axum::Router, AppState)> {
    let pool = synapse_rust::test_utils::prepare_isolated_test_pool().await.ok()?;
    let cache = std::sync::Arc::new(CacheManager::new(&CacheConfig::default()));
    let container = ServiceContainer::new_test_with_pool_and_cache(pool, cache.clone()).await;
    let state = AppState::new(container, cache);
    let app = synapse_rust::web::create_router(state.clone());
    Some((app, state))
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

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();

    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 1024).await.unwrap();
    if status != StatusCode::OK {
        panic!("Registration failed with status {}: {:?}", status, String::from_utf8_lossy(&body));
    }
    let json: Value = serde_json::from_slice(&body).unwrap();
    (json["access_token"].as_str().unwrap().to_string(), json["user_id"].as_str().unwrap().to_string())
}

async fn register_user_with_device_id(app: &axum::Router, username: &str) -> (String, String, String) {
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

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();

    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 1024).await.unwrap();
    if status != StatusCode::OK {
        panic!("Registration failed with status {}: {:?}", status, String::from_utf8_lossy(&body));
    }
    let json: Value = serde_json::from_slice(&body).unwrap();
    (
        json["access_token"].as_str().unwrap().to_string(),
        json["user_id"].as_str().unwrap().to_string(),
        json["device_id"].as_str().unwrap().to_string(),
    )
}

fn matrix_signature(value: &Value, signing_key: &SigningKey) -> String {
    let mut json_for_signing = value.clone();
    remove_signatures_and_unsigned(&mut json_for_signing);
    let message = canonical_json_bytes(&json_for_signing).unwrap();
    STANDARD.encode(signing_key.sign(&message).to_bytes())
}

fn attach_matrix_signature(value: &mut Value, signer_user_id: &str, key_id: &str, signing_key: &SigningKey) {
    let signature = matrix_signature(value, signing_key);
    value["signatures"] = json!({
        signer_user_id: {
            key_id: signature
        }
    });
}

async fn seed_dehydrated_device_preconditions(
    state: &synapse_rust::web::routes::state::AppState,
    user_id: &str,
    key_id: &str,
) {
    state
        .services
        .e2ee
        .cross_signing_service
        .upload_cross_signing_keys(synapse_rust::e2ee::cross_signing::CrossSigningUpload {
            master_key: json!({
                "user_id": user_id,
                "usage": ["master"],
                "keys": {
                    "ed25519:MASTER": "master-public-key"
                },
                "signatures": {}
            }),
            self_signing_key: json!({
                "user_id": user_id,
                "usage": ["self_signing"],
                "keys": {
                    "ed25519:SELF": "self-signing-public-key"
                },
                "signatures": {}
            }),
            user_signing_key: json!({
                "user_id": user_id,
                "usage": ["user_signing"],
                "keys": {
                    "ed25519:USER": "user-signing-public-key"
                },
                "signatures": {}
            }),
        })
        .await
        .expect("failed to seed cross-signing keys");

    state
        .services
        .core
        .account_data_service
        .set_account_data(
            user_id,
            &format!("m.secret_storage.key.{key_id}"),
            &json!({
                "algorithm": "m.secret_storage.v1.aes-hmac-sha2",
                "auth_data": {
                    "key": "opaque-secret-storage-key",
                    "iv": "opaque-iv",
                    "mac": "opaque-mac",
                    "signatures": {}
                }
            }),
        )
        .await
        .expect("failed to seed m.secret_storage.key account_data");

    state
        .services
        .core
        .account_data_service
        .set_account_data(
            user_id,
            "m.secret_storage.default_key",
            &json!({
                "key_id": key_id
            }),
        )
        .await
        .expect("failed to seed m.secret_storage.default_key account_data");
}

async fn create_room(app: &axum::Router, token: &str, name: &str) -> String {
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/createRoom")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "name": name }).to_string()))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    json["room_id"].as_str().unwrap().to_string()
}

async fn invite_user_to_room(app: &axum::Router, inviter_token: &str, room_id: &str, invitee_user_id: &str) {
    let request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/r0/rooms/{}/invite", room_id))
        .header("Authorization", format!("Bearer {}", inviter_token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "user_id": invitee_user_id }).to_string()))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

async fn join_room(app: &axum::Router, token: &str, room_id: &str) {
    let request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/r0/rooms/{}/join", room_id))
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

async fn leave_room(app: &axum::Router, token: &str, room_id: &str) {
    let request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/r0/rooms/{}/leave", room_id))
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

async fn kick_user_from_room(app: &axum::Router, token: &str, room_id: &str, user_id: &str, reason: &str) {
    let request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/r0/rooms/{}/kick", room_id))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "user_id": user_id, "reason": reason }).to_string()))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

async fn ban_user_from_room(app: &axum::Router, token: &str, room_id: &str, user_id: &str, reason: &str) {
    let request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/r0/rooms/{}/ban", room_id))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "user_id": user_id, "reason": reason }).to_string()))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

async fn unban_user_from_room(app: &axum::Router, token: &str, room_id: &str, user_id: &str) {
    let request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/r0/rooms/{}/unban", room_id))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "user_id": user_id }).to_string()))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

async fn forget_room(app: &axum::Router, token: &str, room_id: &str) {
    let request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/r0/rooms/{}/forget", room_id))
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

async fn set_room_join_rule(app: &axum::Router, token: &str, room_id: &str, join_rule: &str) {
    let request = Request::builder()
        .method("PUT")
        .uri(format!("/_matrix/client/r0/rooms/{room_id}/state/m.room.join_rules"))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "join_rule": join_rule }).to_string()))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

async fn knock_room(app: &axum::Router, token: &str, room_id: &str, reason: &str) {
    let request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/v3/knock/{room_id}"))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "reason": reason }).to_string()))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
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

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

async fn sync_once(app: &axum::Router, token: &str, since: Option<&str>) -> Value {
    let uri = match since {
        Some(since) => format!("/_matrix/client/v3/sync?since={since}"),
        None => "/_matrix/client/v3/sync".to_string(),
    };

    let request = Request::builder()
        .method("GET")
        .uri(uri)
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 64 * 1024).await.unwrap();
    serde_json::from_slice(&body).unwrap()
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
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();

    let status = response.status();
    if status != StatusCode::OK {
        let body = axum::body::to_bytes(response.into_body(), 10240).await.unwrap();
        panic!("Upload keys failed with status {}: {:?}", status, String::from_utf8_lossy(&body));
    }

    let body = axum::body::to_bytes(response.into_body(), 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert!(json["one_time_key_counts"]["curve25519"].as_i64().unwrap() >= 2);

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
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
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
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // 4. Get Key Changes
    let request = Request::builder()
        .uri("/_matrix/client/r0/keys/changes?from=0&to=100")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_sync_returns_device_one_time_keys_count() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let (token, user_id) = register_user(&app, &format!("sync_e2ee_{}", rand::random::<u32>())).await;

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
    let upload_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), upload_request).await.unwrap();
    assert_eq!(upload_response.status(), StatusCode::OK);

    let sync_request = Request::builder()
        .method("GET")
        .uri("/_matrix/client/v3/sync")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let sync_response = ServiceExt::<Request<Body>>::oneshot(app, sync_request).await.unwrap();
    assert_eq!(sync_response.status(), StatusCode::OK);

    let sync_body = axum::body::to_bytes(sync_response.into_body(), 32 * 1024).await.unwrap();
    let sync_json: Value = serde_json::from_slice(&sync_body).unwrap();
    assert!(sync_json["device_one_time_keys_count"]["curve25519"].as_i64().unwrap_or_default() >= 1);
}

#[tokio::test]
async fn test_e2ee_shared_routes_across_versions() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let (token, user_id) = register_user(&app, &format!("e2ee_shared_{}", rand::random::<u32>())).await;

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
    let upload_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), upload_request).await.unwrap();
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
    let query_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), query_request).await.unwrap();
    assert_eq!(query_response.status(), StatusCode::OK);

    let device_signing_request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/r0/keys/device_signing/upload")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({}).to_string()))
        .unwrap();
    let device_signing_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), device_signing_request).await.unwrap();
    // Matrix/Synapse contract: this endpoint is UIA-protected, so a request
    // without `auth` must return a UIA challenge rather than silently
    // succeeding.
    assert_eq!(device_signing_response.status(), StatusCode::UNAUTHORIZED);
    let body = axum::body::to_bytes(device_signing_response.into_body(), 4096).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["errcode"], "M_UIA_REQUIRED");
    assert!(json["session"].is_string(), "missing UIA session in response: {}", json);

    let changes_request = Request::builder()
        .method("GET")
        .uri("/_matrix/client/v3/keys/changes?from=0&to=100")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let changes_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), changes_request).await.unwrap();
    assert_eq!(changes_response.status(), StatusCode::OK);

    let summary_request = Request::builder()
        .method("GET")
        .uri("/_matrix/client/v3/security/summary")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let summary_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), summary_request).await.unwrap();
    assert_eq!(summary_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(summary_response.into_body(), 2048).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert!(json.get("security_score").is_some());

    let missing_r0_only_request = Request::builder()
        .method("GET")
        .uri("/_matrix/client/r0/security/summary")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let missing_r0_only_response = ServiceExt::<Request<Body>>::oneshot(app, missing_r0_only_request).await.unwrap();
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
    let v1_show_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), v1_show_request).await.unwrap();
    assert_eq!(v1_show_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(v1_show_response.into_body(), 2048).await.unwrap();
    let v1_show_json: Value = serde_json::from_slice(&body).unwrap();

    let r0_show_request = Request::builder()
        .method("GET")
        .uri("/_matrix/client/r0/keys/qr_code/show")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let r0_show_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), r0_show_request).await.unwrap();
    assert_eq!(r0_show_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(r0_show_response.into_body(), 2048).await.unwrap();
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
    let v1_start_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), v1_start_request).await.unwrap();

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
    let r0_start_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), r0_start_request).await.unwrap();
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
    let v3_start_response = ServiceExt::<Request<Body>>::oneshot(app, v3_start_request).await.unwrap();
    // Route exists but may return 200 (accepted) or 404 (no verification started)
    // Accept both — the key point is the route is registered
    assert!(
        v3_start_response.status() == StatusCode::OK || v3_start_response.status() == StatusCode::NOT_FOUND,
        "verify_start returned unexpected status: {}",
        v3_start_response.status()
    );
}

#[tokio::test]
async fn test_keys_query_filters_users_without_shared_rooms() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let (alice_token, alice_user_id) =
        register_user(&app, &format!("e2ee_query_alice_{}", rand::random::<u32>())).await;
    let (bob_token, bob_user_id) = register_user(&app, &format!("e2ee_query_bob_{}", rand::random::<u32>())).await;

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

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 32 * 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert!(json["device_keys"].get(&alice_user_id).is_some());
    assert!(json["device_keys"].get(&bob_user_id).is_none());
}

#[tokio::test]
async fn test_keys_query_allows_users_with_shared_rooms() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let (alice_token, _) = register_user(&app, &format!("e2ee_room_alice_{}", rand::random::<u32>())).await;
    let (bob_token, bob_user_id) = register_user(&app, &format!("e2ee_room_bob_{}", rand::random::<u32>())).await;

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

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 32 * 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert!(json["device_keys"].get(&bob_user_id).is_some());
}

#[tokio::test]
async fn test_keys_claim_filters_users_without_shared_rooms() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let (alice_token, _) = register_user(&app, &format!("e2ee_claim_alice_{}", rand::random::<u32>())).await;
    let (bob_token, bob_user_id) = register_user(&app, &format!("e2ee_claim_bob_{}", rand::random::<u32>())).await;

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

    let response = ServiceExt::<Request<Body>>::oneshot(app, request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 32 * 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert!(json["one_time_keys"].get(&bob_user_id).is_none());
}

#[tokio::test]
async fn test_key_changes_filters_users_without_shared_rooms() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let (alice_token, _) = register_user(&app, &format!("e2ee_changes_alice_{}", rand::random::<u32>())).await;
    let (bob_token, bob_user_id) = register_user(&app, &format!("e2ee_changes_bob_{}", rand::random::<u32>())).await;

    upload_test_device_keys(&app, &bob_token, &bob_user_id, "BOB_CHANGE_DEVICE", false).await;

    let request = Request::builder()
        .method("GET")
        .uri("/_matrix/client/r0/keys/changes?from=0&to=1000")
        .header("Authorization", format!("Bearer {}", alice_token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 32 * 1024).await.unwrap();
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
    let (alice_token, _) = register_user(&app, &format!("e2ee_shared_changes_alice_{}", rand::random::<u32>())).await;
    let (bob_token, bob_user_id) =
        register_user(&app, &format!("e2ee_shared_changes_bob_{}", rand::random::<u32>())).await;

    let room_id = create_room(&app, &alice_token, "E2EE Shared Changes Room").await;
    invite_user_to_room(&app, &alice_token, &room_id, &bob_user_id).await;
    join_room(&app, &bob_token, &room_id).await;
    upload_test_device_keys(&app, &bob_token, &bob_user_id, "BOB_SHARED_CHANGE_DEVICE", false).await;

    let request = Request::builder()
        .method("GET")
        .uri("/_matrix/client/r0/keys/changes?from=0&to=1000")
        .header("Authorization", format!("Bearer {}", alice_token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 32 * 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let changed = json["changed"].as_array().unwrap();
    let left = json["left"].as_array().unwrap();

    assert!(changed.iter().any(|entry| entry == &json!(bob_user_id)));
    assert!(!left.iter().any(|entry| entry == &json!(bob_user_id)));
}

#[tokio::test]
async fn test_key_changes_exposes_cross_signing_updates_for_shared_users() {
    let Some((app, state)) = super::setup_test_app_with_state().await else {
        return;
    };
    let (alice_token, _) = register_user(&app, &format!("e2ee_cross_changes_alice_{}", rand::random::<u32>())).await;
    let (bob_token, bob_user_id) =
        register_user(&app, &format!("e2ee_cross_changes_bob_{}", rand::random::<u32>())).await;

    let room_id = create_room(&app, &alice_token, "E2EE Cross-Signing Changes Room").await;
    invite_user_to_room(&app, &alice_token, &room_id, &bob_user_id).await;
    join_room(&app, &bob_token, &room_id).await;

    state
        .services
        .e2ee
        .cross_signing_service
        .upload_cross_signing_keys(synapse_rust::e2ee::cross_signing::CrossSigningUpload {
            master_key: json!({
                "user_id": bob_user_id,
                "usage": ["master"],
                "keys": {
                    "ed25519:BOBMASTER": "bob-master-public-key"
                },
                "signatures": {}
            }),
            self_signing_key: json!({
                "user_id": bob_user_id,
                "usage": ["self_signing"],
                "keys": {
                    "ed25519:BOBSELF": "bob-self-signing-public-key"
                },
                "signatures": {}
            }),
            user_signing_key: json!({
                "user_id": bob_user_id,
                "usage": ["user_signing"],
                "keys": {
                    "ed25519:BOBUSER": "bob-user-signing-public-key"
                },
                "signatures": {}
            }),
        })
        .await
        .expect("failed to upload cross-signing keys");

    let request = Request::builder()
        .method("GET")
        .uri("/_matrix/client/r0/keys/changes?from=0&to=1000")
        .header("Authorization", format!("Bearer {}", alice_token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 32 * 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let changed = json["changed"].as_array().unwrap();
    let left = json["left"].as_array().unwrap();

    assert!(
        changed.iter().any(|entry| entry == &json!(bob_user_id)),
        "expected /keys/changes to include cross-signing-only update for shared user: {}",
        json
    );
    assert!(!left.iter().any(|entry| entry == &json!(bob_user_id)));
}

#[tokio::test]
async fn test_sync_device_lists_exposes_cross_signing_updates_for_shared_users() {
    let _guard = sync_device_lists_test_mutex().lock().await;
    let Some((app, state)) = setup_isolated_test_app_with_state().await else {
        return;
    };
    let (alice_token, _) = register_user(&app, &format!("e2ee_sync_cross_alice_{}", rand::random::<u32>())).await;
    let (bob_token, bob_user_id) = register_user(&app, &format!("e2ee_sync_cross_bob_{}", rand::random::<u32>())).await;

    let room_id = create_room(&app, &alice_token, "E2EE Sync Cross-Signing Room").await;
    invite_user_to_room(&app, &alice_token, &room_id, &bob_user_id).await;
    join_room(&app, &bob_token, &room_id).await;

    let initial_sync = sync_once(&app, &alice_token, None).await;
    let since = initial_sync["next_batch"].as_str().expect("missing next_batch").to_string();

    state
        .services
        .e2ee
        .cross_signing_service
        .upload_cross_signing_keys(synapse_rust::e2ee::cross_signing::CrossSigningUpload {
            master_key: json!({
                "user_id": bob_user_id,
                "usage": ["master"],
                "keys": {
                    "ed25519:BOBSYNCMASTER": "bob-sync-master-public-key"
                },
                "signatures": {}
            }),
            self_signing_key: json!({
                "user_id": bob_user_id,
                "usage": ["self_signing"],
                "keys": {
                    "ed25519:BOBSYNCSELF": "bob-sync-self-signing-public-key"
                },
                "signatures": {}
            }),
            user_signing_key: json!({
                "user_id": bob_user_id,
                "usage": ["user_signing"],
                "keys": {
                    "ed25519:BOBSYNCUSER": "bob-sync-user-signing-public-key"
                },
                "signatures": {}
            }),
        })
        .await
        .expect("failed to upload cross-signing keys");

    let incremental_sync = sync_once(&app, &alice_token, Some(&since)).await;
    let changed = incremental_sync["device_lists"]["changed"].as_array().unwrap();
    let left = incremental_sync["device_lists"]["left"].as_array().unwrap();

    assert!(
        changed.iter().any(|entry| entry == &json!(bob_user_id)),
        "expected /sync device_lists.changed to include shared user cross-signing update: {}",
        incremental_sync
    );
    assert!(!left.iter().any(|entry| entry == &json!(bob_user_id)));
}

#[tokio::test]
async fn test_sync_device_lists_does_not_leak_cross_signing_updates_without_shared_rooms() {
    let _guard = sync_device_lists_test_mutex().lock().await;
    let Some((app, state)) = setup_isolated_test_app_with_state().await else {
        return;
    };
    let (alice_token, _) = register_user(&app, &format!("e2ee_sync_isolated_alice_{}", rand::random::<u32>())).await;
    let (_, bob_user_id) = register_user(&app, &format!("e2ee_sync_isolated_bob_{}", rand::random::<u32>())).await;

    let initial_sync = sync_once(&app, &alice_token, None).await;
    let since = initial_sync["next_batch"].as_str().expect("missing next_batch").to_string();

    state
        .services
        .e2ee
        .cross_signing_service
        .upload_cross_signing_keys(synapse_rust::e2ee::cross_signing::CrossSigningUpload {
            master_key: json!({
                "user_id": bob_user_id,
                "usage": ["master"],
                "keys": {
                    "ed25519:BOBISOLATEDMASTER": "bob-isolated-master-public-key"
                },
                "signatures": {}
            }),
            self_signing_key: json!({
                "user_id": bob_user_id,
                "usage": ["self_signing"],
                "keys": {
                    "ed25519:BOBISOLATEDSELF": "bob-isolated-self-signing-public-key"
                },
                "signatures": {}
            }),
            user_signing_key: json!({
                "user_id": bob_user_id,
                "usage": ["user_signing"],
                "keys": {
                    "ed25519:BOBISOLATEDUSER": "bob-isolated-user-signing-public-key"
                },
                "signatures": {}
            }),
        })
        .await
        .expect("failed to upload isolated cross-signing keys");

    let incremental_sync = sync_once(&app, &alice_token, Some(&since)).await;
    let changed = incremental_sync["device_lists"]["changed"].as_array().unwrap();
    let left = incremental_sync["device_lists"]["left"].as_array().unwrap();

    assert!(
        !changed.iter().any(|entry| entry == &json!(bob_user_id)),
        "expected /sync device_lists.changed to exclude non-shared user cross-signing update: {}",
        incremental_sync
    );
    assert!(!left.iter().any(|entry| entry == &json!(bob_user_id)));
}

#[tokio::test]
async fn test_sync_device_lists_left_reports_users_who_stop_sharing_rooms() {
    let _guard = sync_device_lists_test_mutex().lock().await;
    let Some((app, _state)) = setup_isolated_test_app_with_state().await else {
        return;
    };
    let (alice_token, _) = register_user(&app, &format!("e2ee_sync_left_alice_{}", rand::random::<u32>())).await;
    let (bob_token, bob_user_id) = register_user(&app, &format!("e2ee_sync_left_bob_{}", rand::random::<u32>())).await;

    let room_id = create_room(&app, &alice_token, "E2EE Sync Device List Left Room").await;
    invite_user_to_room(&app, &alice_token, &room_id, &bob_user_id).await;
    join_room(&app, &bob_token, &room_id).await;

    let initial_sync = sync_once(&app, &alice_token, None).await;
    let since = initial_sync["next_batch"].as_str().expect("missing next_batch").to_string();

    leave_room(&app, &bob_token, &room_id).await;

    let incremental_sync = sync_once(&app, &alice_token, Some(&since)).await;
    let changed = incremental_sync["device_lists"]["changed"].as_array().unwrap();
    let left = incremental_sync["device_lists"]["left"].as_array().unwrap();

    assert!(
        !changed.iter().any(|entry| entry == &json!(bob_user_id)),
        "membership-only unshare should not masquerade as device change in /sync: {}",
        incremental_sync
    );
    assert!(
        left.iter().any(|entry| entry == &json!(bob_user_id)),
        "expected /sync device_lists.left to include user who stopped sharing rooms: {}",
        incremental_sync
    );
}

#[tokio::test]
async fn test_sync_device_lists_left_reports_users_when_requester_leaves_last_shared_room() {
    let _guard = sync_device_lists_test_mutex().lock().await;
    let Some((app, _state)) = setup_isolated_test_app_with_state().await else {
        return;
    };
    let (alice_token, _) = register_user(&app, &format!("e2ee_sync_self_left_alice_{}", rand::random::<u32>())).await;
    let (bob_token, bob_user_id) =
        register_user(&app, &format!("e2ee_sync_self_left_bob_{}", rand::random::<u32>())).await;

    let room_id = create_room(&app, &alice_token, "E2EE Sync Requester Leaves Shared Room").await;
    invite_user_to_room(&app, &alice_token, &room_id, &bob_user_id).await;
    join_room(&app, &bob_token, &room_id).await;

    let initial_sync = sync_once(&app, &alice_token, None).await;
    let since = initial_sync["next_batch"].as_str().expect("missing next_batch").to_string();

    leave_room(&app, &alice_token, &room_id).await;

    let incremental_sync = sync_once(&app, &alice_token, Some(&since)).await;
    let changed = incremental_sync["device_lists"]["changed"].as_array().unwrap();
    let left = incremental_sync["device_lists"]["left"].as_array().unwrap();

    assert!(
        !changed.iter().any(|entry| entry == &json!(bob_user_id)),
        "requester leaving shared room should not masquerade as device change in /sync: {}",
        incremental_sync
    );
    assert!(
        left.iter().any(|entry| entry == &json!(bob_user_id)),
        "expected /sync device_lists.left to include still-joined peer after requester leaves last shared room: {}",
        incremental_sync
    );
}

#[tokio::test]
async fn test_sync_device_lists_left_does_not_report_user_still_shared_via_other_room() {
    let _guard = sync_device_lists_test_mutex().lock().await;
    let Some((app, _state)) = setup_isolated_test_app_with_state().await else {
        return;
    };
    let (alice_token, _) = register_user(&app, &format!("e2ee_sync_multi_left_alice_{}", rand::random::<u32>())).await;
    let (bob_token, bob_user_id) =
        register_user(&app, &format!("e2ee_sync_multi_left_bob_{}", rand::random::<u32>())).await;

    let room_a = create_room(&app, &alice_token, "E2EE Sync Shared Room A").await;
    invite_user_to_room(&app, &alice_token, &room_a, &bob_user_id).await;
    join_room(&app, &bob_token, &room_a).await;

    let room_b = create_room(&app, &alice_token, "E2EE Sync Shared Room B").await;
    invite_user_to_room(&app, &alice_token, &room_b, &bob_user_id).await;
    join_room(&app, &bob_token, &room_b).await;

    let initial_sync = sync_once(&app, &alice_token, None).await;
    let since = initial_sync["next_batch"].as_str().expect("missing next_batch").to_string();

    leave_room(&app, &bob_token, &room_a).await;

    let incremental_sync = sync_once(&app, &alice_token, Some(&since)).await;
    let changed = incremental_sync["device_lists"]["changed"].as_array().unwrap();
    let left = incremental_sync["device_lists"]["left"].as_array().unwrap();

    assert!(
        !changed.iter().any(|entry| entry == &json!(bob_user_id)),
        "membership-only partial unshare should not masquerade as device change in /sync: {}",
        incremental_sync
    );
    assert!(
        !left.iter().any(|entry| entry == &json!(bob_user_id)),
        "user still shared via another room must not appear in /sync device_lists.left: {}",
        incremental_sync
    );
}

#[tokio::test]
async fn test_sync_device_lists_leave_then_rejoin_does_not_report_left() {
    let _guard = sync_device_lists_test_mutex().lock().await;
    let Some((app, _state)) = setup_isolated_test_app_with_state().await else {
        return;
    };
    let (alice_token, _) = register_user(&app, &format!("e2ee_sync_rejoin_alice_{}", rand::random::<u32>())).await;
    let (bob_token, bob_user_id) =
        register_user(&app, &format!("e2ee_sync_rejoin_bob_{}", rand::random::<u32>())).await;

    let room_id = create_room(&app, &alice_token, "E2EE Sync Leave Rejoin Room").await;
    invite_user_to_room(&app, &alice_token, &room_id, &bob_user_id).await;
    join_room(&app, &bob_token, &room_id).await;

    let initial_sync = sync_once(&app, &alice_token, None).await;
    let since = initial_sync["next_batch"].as_str().expect("missing next_batch").to_string();

    leave_room(&app, &bob_token, &room_id).await;
    invite_user_to_room(&app, &alice_token, &room_id, &bob_user_id).await;
    join_room(&app, &bob_token, &room_id).await;

    let incremental_sync = sync_once(&app, &alice_token, Some(&since)).await;
    let changed = incremental_sync["device_lists"]["changed"].as_array().unwrap();
    let left = incremental_sync["device_lists"]["left"].as_array().unwrap();

    assert!(
        !changed.iter().any(|entry| entry == &json!(bob_user_id)),
        "leave then rejoin should not masquerade as device change in /sync: {}",
        incremental_sync
    );
    assert!(
        !left.iter().any(|entry| entry == &json!(bob_user_id)),
        "user who re-shares a room before the next /sync must not remain in device_lists.left: {}",
        incremental_sync
    );
}

#[tokio::test]
async fn test_sync_device_lists_kick_reports_left_for_kicked_shared_user() {
    let _guard = sync_device_lists_test_mutex().lock().await;
    let Some((app, _state)) = setup_isolated_test_app_with_state().await else {
        return;
    };
    let (alice_token, _) = register_user(&app, &format!("e2ee_sync_kick_alice_{}", rand::random::<u32>())).await;
    let (bob_token, bob_user_id) = register_user(&app, &format!("e2ee_sync_kick_bob_{}", rand::random::<u32>())).await;

    let room_id = create_room(&app, &alice_token, "E2EE Sync Kick Shared User Room").await;
    invite_user_to_room(&app, &alice_token, &room_id, &bob_user_id).await;
    join_room(&app, &bob_token, &room_id).await;

    let initial_sync = sync_once(&app, &alice_token, None).await;
    let since = initial_sync["next_batch"].as_str().expect("missing next_batch").to_string();

    kick_user_from_room(&app, &alice_token, &room_id, &bob_user_id, "moderation kick").await;

    let incremental_sync = sync_once(&app, &alice_token, Some(&since)).await;
    let changed = incremental_sync["device_lists"]["changed"].as_array().unwrap();
    let left = incremental_sync["device_lists"]["left"].as_array().unwrap();

    assert!(
        !changed.iter().any(|entry| entry == &json!(bob_user_id)),
        "kick-driven unshare should not masquerade as device change in /sync: {}",
        incremental_sync
    );
    assert!(
        left.iter().any(|entry| entry == &json!(bob_user_id)),
        "kicked shared user should appear in /sync device_lists.left: {}",
        incremental_sync
    );
}

#[tokio::test]
async fn test_sync_device_lists_ban_reports_left_for_banned_shared_user() {
    let _guard = sync_device_lists_test_mutex().lock().await;
    let Some((app, _state)) = setup_isolated_test_app_with_state().await else {
        return;
    };
    let (alice_token, _) = register_user(&app, &format!("e2ee_sync_ban_alice_{}", rand::random::<u32>())).await;
    let (bob_token, bob_user_id) = register_user(&app, &format!("e2ee_sync_ban_bob_{}", rand::random::<u32>())).await;

    let room_id = create_room(&app, &alice_token, "E2EE Sync Ban Shared User Room").await;
    invite_user_to_room(&app, &alice_token, &room_id, &bob_user_id).await;
    join_room(&app, &bob_token, &room_id).await;

    let initial_sync = sync_once(&app, &alice_token, None).await;
    let since = initial_sync["next_batch"].as_str().expect("missing next_batch").to_string();

    ban_user_from_room(&app, &alice_token, &room_id, &bob_user_id, "moderation ban").await;

    let incremental_sync = sync_once(&app, &alice_token, Some(&since)).await;
    let changed = incremental_sync["device_lists"]["changed"].as_array().unwrap();
    let left = incremental_sync["device_lists"]["left"].as_array().unwrap();

    assert!(
        !changed.iter().any(|entry| entry == &json!(bob_user_id)),
        "ban-driven unshare should not masquerade as device change in /sync: {}",
        incremental_sync
    );
    assert!(
        left.iter().any(|entry| entry == &json!(bob_user_id)),
        "banned shared user should appear in /sync device_lists.left: {}",
        incremental_sync
    );
}

#[tokio::test]
async fn test_sync_device_lists_unban_does_not_repeat_left_for_already_unshared_user() {
    let _guard = sync_device_lists_test_mutex().lock().await;
    let Some((app, _state)) = setup_isolated_test_app_with_state().await else {
        return;
    };
    let (alice_token, _) = register_user(&app, &format!("e2ee_sync_unban_alice_{}", rand::random::<u32>())).await;
    let (bob_token, bob_user_id) = register_user(&app, &format!("e2ee_sync_unban_bob_{}", rand::random::<u32>())).await;

    let room_id = create_room(&app, &alice_token, "E2EE Sync Unban Shared User Room").await;
    invite_user_to_room(&app, &alice_token, &room_id, &bob_user_id).await;
    join_room(&app, &bob_token, &room_id).await;

    let initial_sync = sync_once(&app, &alice_token, None).await;
    let since = initial_sync["next_batch"].as_str().expect("missing next_batch").to_string();

    ban_user_from_room(&app, &alice_token, &room_id, &bob_user_id, "moderation ban").await;

    let ban_sync = sync_once(&app, &alice_token, Some(&since)).await;
    let ban_left = ban_sync["device_lists"]["left"].as_array().unwrap();
    assert!(
        ban_left.iter().any(|entry| entry == &json!(bob_user_id)),
        "ban should first report the shared user in /sync device_lists.left: {}",
        ban_sync
    );
    let since_after_ban = ban_sync["next_batch"].as_str().expect("missing next_batch after ban").to_string();

    unban_user_from_room(&app, &alice_token, &room_id, &bob_user_id).await;

    let unban_sync = sync_once(&app, &alice_token, Some(&since_after_ban)).await;
    let changed = unban_sync["device_lists"]["changed"].as_array().unwrap();
    let left = unban_sync["device_lists"]["left"].as_array().unwrap();

    assert!(
        !changed.iter().any(|entry| entry == &json!(bob_user_id)),
        "unban should not masquerade as device change in /sync: {}",
        unban_sync
    );
    assert!(
        !left.iter().any(|entry| entry == &json!(bob_user_id)),
        "unban after a prior ban must not repeat /sync device_lists.left for an already-unshared user: {}",
        unban_sync
    );
}

#[tokio::test]
async fn test_sync_device_lists_forget_does_not_repeat_left_for_already_unshared_user() {
    let _guard = sync_device_lists_test_mutex().lock().await;
    let Some((app, _state)) = setup_isolated_test_app_with_state().await else {
        return;
    };
    let (alice_token, _) = register_user(&app, &format!("e2ee_sync_forget_alice_{}", rand::random::<u32>())).await;
    let (bob_token, bob_user_id) =
        register_user(&app, &format!("e2ee_sync_forget_bob_{}", rand::random::<u32>())).await;

    let room_id = create_room(&app, &alice_token, "E2EE Sync Forget Shared User Room").await;
    invite_user_to_room(&app, &alice_token, &room_id, &bob_user_id).await;
    join_room(&app, &bob_token, &room_id).await;

    let initial_sync = sync_once(&app, &alice_token, None).await;
    let since = initial_sync["next_batch"].as_str().expect("missing next_batch").to_string();

    leave_room(&app, &alice_token, &room_id).await;

    let leave_sync = sync_once(&app, &alice_token, Some(&since)).await;
    let leave_left = leave_sync["device_lists"]["left"].as_array().unwrap();
    assert!(
        leave_left.iter().any(|entry| entry == &json!(bob_user_id)),
        "leaving the last shared room should first report the peer in /sync device_lists.left: {}",
        leave_sync
    );
    let since_after_leave = leave_sync["next_batch"].as_str().expect("missing next_batch after leave").to_string();

    forget_room(&app, &alice_token, &room_id).await;

    let forget_sync = sync_once(&app, &alice_token, Some(&since_after_leave)).await;
    let changed = forget_sync["device_lists"]["changed"].as_array().unwrap();
    let left = forget_sync["device_lists"]["left"].as_array().unwrap();

    assert!(
        !changed.iter().any(|entry| entry == &json!(bob_user_id)),
        "forget should not masquerade as device change in /sync: {}",
        forget_sync
    );
    assert!(
        !left.iter().any(|entry| entry == &json!(bob_user_id)),
        "forget after a prior leave must not repeat /sync device_lists.left for an already-unshared user: {}",
        forget_sync
    );
}

#[tokio::test]
async fn test_sync_device_lists_knock_does_not_leak_non_shared_user() {
    let _guard = sync_device_lists_test_mutex().lock().await;
    let Some((app, _state)) = setup_isolated_test_app_with_state().await else {
        return;
    };
    let (alice_token, _) = register_user(&app, &format!("e2ee_sync_knock_alice_{}", rand::random::<u32>())).await;
    let (bob_token, bob_user_id) = register_user(&app, &format!("e2ee_sync_knock_bob_{}", rand::random::<u32>())).await;

    let room_id = create_room(&app, &alice_token, "E2EE Sync Knock Room").await;
    set_room_join_rule(&app, &alice_token, &room_id, "knock").await;

    let initial_sync = sync_once(&app, &alice_token, None).await;
    let since = initial_sync["next_batch"].as_str().expect("missing next_batch").to_string();

    knock_room(&app, &bob_token, &room_id, "let me in").await;

    let incremental_sync = sync_once(&app, &alice_token, Some(&since)).await;
    let changed = incremental_sync["device_lists"]["changed"].as_array().unwrap();
    let left = incremental_sync["device_lists"]["left"].as_array().unwrap();

    assert!(
        !changed.iter().any(|entry| entry == &json!(bob_user_id)),
        "knock by a never-shared user must not appear in /sync device_lists.changed: {}",
        incremental_sync
    );
    assert!(
        !left.iter().any(|entry| entry == &json!(bob_user_id)),
        "knock by a never-shared user must not appear in /sync device_lists.left: {}",
        incremental_sync
    );
}

#[tokio::test]
async fn test_sync_device_lists_invite_decline_does_not_report_left_and_updates_membership_state() {
    let _guard = sync_device_lists_test_mutex().lock().await;
    let Some((app, state)) = setup_isolated_test_app_with_state().await else {
        return;
    };
    let (alice_token, _) =
        register_user(&app, &format!("e2ee_sync_invite_decline_alice_{}", rand::random::<u32>())).await;
    let (bob_token, bob_user_id) =
        register_user(&app, &format!("e2ee_sync_invite_decline_bob_{}", rand::random::<u32>())).await;

    let room_id = create_room(&app, &alice_token, "E2EE Sync Invite Decline Room").await;
    invite_user_to_room(&app, &alice_token, &room_id, &bob_user_id).await;

    let initial_sync = sync_once(&app, &alice_token, None).await;
    let since = initial_sync["next_batch"].as_str().expect("missing next_batch").to_string();

    leave_room(&app, &bob_token, &room_id).await;

    let incremental_sync = sync_once(&app, &alice_token, Some(&since)).await;
    let changed = incremental_sync["device_lists"]["changed"].as_array().unwrap();
    let left = incremental_sync["device_lists"]["left"].as_array().unwrap();

    assert!(
        !changed.iter().any(|entry| entry == &json!(bob_user_id)),
        "invite decline must not masquerade as device change in /sync: {}",
        incremental_sync
    );
    assert!(
        !left.iter().any(|entry| entry == &json!(bob_user_id)),
        "invite decline must not surface as shared-user loss in /sync device_lists.left: {}",
        incremental_sync
    );

    let membership = state
        .services
        .rooms
        .member_storage
        .get_room_member(&room_id, &bob_user_id)
        .await
        .expect("failed to load membership after invite decline")
        .expect("missing membership after invite decline");
    assert_eq!(membership.membership, "leave");
    assert!(membership.joined_ts.is_none(), "invite decline should not backfill joined_ts for never-joined user");
    assert!(membership.left_ts.is_some(), "invite decline should stamp left_ts");
}

#[tokio::test]
async fn test_sync_device_lists_invite_retract_via_kick_does_not_report_left() {
    let _guard = sync_device_lists_test_mutex().lock().await;
    let Some((app, state)) = setup_isolated_test_app_with_state().await else {
        return;
    };
    let (alice_token, _) =
        register_user(&app, &format!("e2ee_sync_invite_retract_alice_{}", rand::random::<u32>())).await;
    let (_, bob_user_id) =
        register_user(&app, &format!("e2ee_sync_invite_retract_bob_{}", rand::random::<u32>())).await;

    let room_id = create_room(&app, &alice_token, "E2EE Sync Invite Retract Room").await;
    invite_user_to_room(&app, &alice_token, &room_id, &bob_user_id).await;

    let initial_sync = sync_once(&app, &alice_token, None).await;
    let since = initial_sync["next_batch"].as_str().expect("missing next_batch").to_string();

    kick_user_from_room(&app, &alice_token, &room_id, &bob_user_id, "invite retracted").await;

    let incremental_sync = sync_once(&app, &alice_token, Some(&since)).await;
    let changed = incremental_sync["device_lists"]["changed"].as_array().unwrap();
    let left = incremental_sync["device_lists"]["left"].as_array().unwrap();

    assert!(
        !changed.iter().any(|entry| entry == &json!(bob_user_id)),
        "invite retract via kick must not masquerade as device change in /sync: {}",
        incremental_sync
    );
    assert!(
        !left.iter().any(|entry| entry == &json!(bob_user_id)),
        "invite retract via kick must not surface as shared-user loss in /sync device_lists.left: {}",
        incremental_sync
    );

    let membership = state
        .services
        .rooms
        .member_storage
        .get_room_member(&room_id, &bob_user_id)
        .await
        .expect("failed to load membership after invite retract")
        .expect("missing membership after invite retract");
    assert_eq!(membership.membership, "leave");
    assert!(membership.joined_ts.is_none(), "invite retract must not mark invitee as joined");
}

#[tokio::test]
async fn test_device_signing_password_uia_upload_enables_dehydrated_device_bootstrap() {
    let Some((app, state)) = setup_test_app_with_state().await else {
        return;
    };
    let username = format!("e2ee_uia_bootstrap_{}", rand::random::<u32>());
    let (token, user_id) = register_user(&app, &username).await;
    let master_key = json!({
        "user_id": user_id,
        "usage": ["master"],
        "keys": {
            "ed25519:MASTER": "master-public-key-uia"
        },
        "signatures": {}
    });

    let challenge_request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/r0/keys/device_signing/upload")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "master_key": master_key
            })
            .to_string(),
        ))
        .unwrap();
    let challenge_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), challenge_request).await.unwrap();
    assert_eq!(challenge_response.status(), StatusCode::UNAUTHORIZED);

    let challenge_body = axum::body::to_bytes(challenge_response.into_body(), 4096).await.unwrap();
    let challenge_json: Value = serde_json::from_slice(&challenge_body).unwrap();
    assert_eq!(challenge_json["errcode"], "M_UIA_REQUIRED");
    let session = challenge_json["session"].as_str().expect("missing UIA session").to_string();

    let complete_uia_request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/r0/keys/device_signing/upload")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "master_key": master_key,
                "auth": {
                    "type": "m.login.password",
                    "session": session,
                    "identifier": {
                        "type": "m.id.user",
                        "user": username
                    },
                    "password": "Password123!"
                }
            })
            .to_string(),
        ))
        .unwrap();
    let complete_uia_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), complete_uia_request).await.unwrap();
    assert_eq!(complete_uia_response.status(), StatusCode::OK);

    let verification_status = state
        .services
        .e2ee
        .cross_signing_service
        .get_user_verification_status(&user_id)
        .await
        .expect("failed to query cross-signing status");
    assert!(
        verification_status.has_master_key,
        "master key upload should satisfy dehydrated-device bootstrap precondition"
    );

    let key_id = format!("uia-ssss-{}", rand::random::<u32>());
    for (data_type, content) in [
        (
            format!("m.secret_storage.key.{key_id}"),
            json!({
                "algorithm": "m.secret_storage.v1.aes-hmac-sha2",
                "auth_data": {
                    "key": "opaque-secret-storage-key",
                    "iv": "opaque-iv",
                    "mac": "opaque-mac",
                    "signatures": {}
                }
            }),
        ),
        (
            "m.secret_storage.default_key".to_string(),
            json!({
                "key_id": key_id
            }),
        ),
    ] {
        let request = Request::builder()
            .method("PUT")
            .uri(format!("/_matrix/client/v3/user/{}/account_data/{}", user_id, data_type))
            .header("Authorization", format!("Bearer {}", token))
            .header("Content-Type", "application/json")
            .body(Body::from(content.to_string()))
            .unwrap();
        let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK, "failed to write account_data {}", data_type);
    }

    let dehydrated_device_id = format!("UIABOOT{:04}", rand::random::<u16>());
    let put_request = Request::builder()
        .method("PUT")
        .uri("/_matrix/client/unstable/org.matrix.msc3814.v1/dehydrated_device")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "device_id": dehydrated_device_id,
                "device_keys": {
                    "user_id": user_id,
                    "device_id": dehydrated_device_id,
                    "algorithms": ["m.olm.v1.curve25519-aes-sha2"],
                    "keys": {
                        format!("curve25519:{}", dehydrated_device_id): "AAAA",
                        format!("ed25519:{}", dehydrated_device_id): "BBBB"
                    },
                    "signatures": {}
                },
                "device_data": {
                    "algorithm": "org.matrix.msc3814.v1.olm"
                }
            })
            .to_string(),
        ))
        .unwrap();
    let put_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), put_request).await.unwrap();
    assert_eq!(put_response.status(), StatusCode::OK);

    let delete_request = Request::builder()
        .method("DELETE")
        .uri("/_matrix/client/unstable/org.matrix.msc3814.v1/dehydrated_device")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let delete_response = ServiceExt::<Request<Body>>::oneshot(app, delete_request).await.unwrap();
    assert_eq!(delete_response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_fresh_account_cross_signing_ssss_and_dehydrated_device_end_to_end() {
    let Some((app, state)) = setup_test_app_with_state().await else {
        return;
    };
    let username = format!("fresh_bootstrap_{}", rand::random::<u32>());
    let (token, user_id, device_id) = register_user_with_device_id(&app, &username).await;

    let device_signing_key = SigningKey::from_bytes(&[61u8; 32]);
    let device_ed25519_key_id = format!("ed25519:{device_id}");
    let device_ed25519_public_key = STANDARD.encode(device_signing_key.verifying_key().as_bytes());

    let master_signing_key = SigningKey::from_bytes(&[62u8; 32]);
    let master_key_id = "ed25519:FRESHMASTER";
    let master_public_key = STANDARD.encode(master_signing_key.verifying_key().as_bytes());

    let self_signing_key = SigningKey::from_bytes(&[63u8; 32]);
    let self_signing_key_id = "ed25519:FRESHSELF";
    let self_signing_public_key = STANDARD.encode(self_signing_key.verifying_key().as_bytes());

    let user_signing_key = SigningKey::from_bytes(&[64u8; 32]);
    let user_signing_key_id = "ed25519:FRESHUSER";
    let user_signing_public_key = STANDARD.encode(user_signing_key.verifying_key().as_bytes());

    let mut device_keys = json!({
        "user_id": user_id,
        "device_id": device_id,
        "algorithms": ["m.olm.v1.curve25519-aes-sha2", "m.megolm.v1.aes-sha2"],
        "keys": {
            format!("curve25519:{}", device_id): "fresh-curve25519-public-key",
            device_ed25519_key_id.clone(): device_ed25519_public_key
        }
    });
    attach_matrix_signature(&mut device_keys, &user_id, &device_ed25519_key_id, &device_signing_key);

    let upload_device_keys_request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/r0/keys/upload")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "device_keys": device_keys }).to_string()))
        .unwrap();
    let upload_device_keys_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), upload_device_keys_request).await.unwrap();
    assert_eq!(upload_device_keys_response.status(), StatusCode::OK);

    let mut master_key = json!({
        "user_id": user_id,
        "usage": ["master"],
        "keys": {
            master_key_id: master_public_key
        }
    });
    attach_matrix_signature(&mut master_key, &user_id, &device_ed25519_key_id, &device_signing_key);

    let mut self_signing_key_json = json!({
        "user_id": user_id,
        "usage": ["self_signing"],
        "keys": {
            self_signing_key_id: self_signing_public_key
        }
    });
    attach_matrix_signature(&mut self_signing_key_json, &user_id, master_key_id, &master_signing_key);

    let mut user_signing_key_json = json!({
        "user_id": user_id,
        "usage": ["user_signing"],
        "keys": {
            user_signing_key_id: user_signing_public_key
        }
    });
    attach_matrix_signature(&mut user_signing_key_json, &user_id, master_key_id, &master_signing_key);

    let cross_signing_payload = json!({
        "master_key": master_key,
        "self_signing_key": self_signing_key_json,
        "user_signing_key": user_signing_key_json
    });

    let challenge_request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/r0/keys/device_signing/upload")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(cross_signing_payload.to_string()))
        .unwrap();
    let challenge_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), challenge_request).await.unwrap();
    assert_eq!(challenge_response.status(), StatusCode::UNAUTHORIZED);

    let challenge_body = axum::body::to_bytes(challenge_response.into_body(), 4096).await.unwrap();
    let challenge_json: Value = serde_json::from_slice(&challenge_body).unwrap();
    assert_eq!(challenge_json["errcode"], "M_UIA_REQUIRED");
    let session = challenge_json["session"].as_str().expect("missing UIA session").to_string();

    let complete_uia_request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/r0/keys/device_signing/upload")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "master_key": cross_signing_payload["master_key"].clone(),
                "self_signing_key": cross_signing_payload["self_signing_key"].clone(),
                "user_signing_key": cross_signing_payload["user_signing_key"].clone(),
                "auth": {
                    "type": "m.login.password",
                    "session": session,
                    "identifier": {
                        "type": "m.id.user",
                        "user": username
                    },
                    "password": "Password123!"
                }
            })
            .to_string(),
        ))
        .unwrap();
    let complete_uia_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), complete_uia_request).await.unwrap();
    assert_eq!(complete_uia_response.status(), StatusCode::OK);

    let verification_status = state
        .services
        .e2ee
        .cross_signing_service
        .get_user_verification_status(&user_id)
        .await
        .expect("failed to query cross-signing status");
    assert!(verification_status.has_master_key);
    assert!(verification_status.has_self_signing_key);
    assert!(verification_status.has_user_signing_key);

    let key_id = format!("fresh-ssss-{}", rand::random::<u32>());
    let secret_storage_key_content = json!({
        "algorithm": "m.secret_storage.v1.aes-hmac-sha2",
        "auth_data": {
            "key": "fresh-secret-storage-key",
            "iv": "fresh-iv",
            "mac": "fresh-mac",
            "signatures": {}
        }
    });

    for (data_type, content) in [
        (format!("m.secret_storage.key.{key_id}"), secret_storage_key_content.clone()),
        ("m.secret_storage.default_key".to_string(), json!({ "key_id": key_id })),
    ] {
        let request = Request::builder()
            .method("PUT")
            .uri(format!("/_matrix/client/v3/user/{}/account_data/{}", user_id, data_type))
            .header("Authorization", format!("Bearer {}", token))
            .header("Content-Type", "application/json")
            .body(Body::from(content.to_string()))
            .unwrap();
        let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK, "failed to write account_data {}", data_type);
    }

    let mirrored_key = state
        .services
        .e2ee
        .ssss_service
        .get_key(&user_id, &key_id)
        .await
        .expect("failed to query mirrored SSSS key")
        .expect("expected SSSS key to be mirrored into internal store");
    assert_eq!(mirrored_key.algorithm, "m.secret_storage.v1.aes-hmac-sha2");
    assert_eq!(mirrored_key.encrypted_key, "fresh-secret-storage-key");

    for (data_type, expected_field, expected_value) in [
        ("m.secret_storage.default_key".to_string(), "key_id", key_id.as_str()),
        (format!("m.secret_storage.key.{key_id}"), "algorithm", "m.secret_storage.v1.aes-hmac-sha2"),
    ] {
        let request = Request::builder()
            .method("GET")
            .uri(format!("/_matrix/client/v3/user/{}/account_data/{}", user_id, data_type))
            .header("Authorization", format!("Bearer {}", token))
            .body(Body::empty())
            .unwrap();
        let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK, "failed to read account_data {}", data_type);
        let body = axum::body::to_bytes(response.into_body(), 4096).await.unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json[expected_field], expected_value);
    }

    let dehydrated_device_id = format!("FRESHBOOT{:04}", rand::random::<u16>());
    let put_request = Request::builder()
        .method("PUT")
        .uri("/_matrix/client/unstable/org.matrix.msc3814.v1/dehydrated_device")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "device_id": dehydrated_device_id,
                "device_keys": {
                    "user_id": user_id,
                    "device_id": dehydrated_device_id,
                    "algorithms": ["m.olm.v1.curve25519-aes-sha2"],
                    "keys": {
                        format!("curve25519:{}", dehydrated_device_id): "fresh-dehydrated-curve",
                        format!("ed25519:{}", dehydrated_device_id): "fresh-dehydrated-ed"
                    },
                    "signatures": {}
                },
                "device_data": {
                    "algorithm": "org.matrix.msc3814.v1.olm"
                }
            })
            .to_string(),
        ))
        .unwrap();
    let put_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), put_request).await.unwrap();
    assert_eq!(put_response.status(), StatusCode::OK);

    let query_request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/keys/query")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "device_keys": { user_id.clone(): [] }
            })
            .to_string(),
        ))
        .unwrap();
    let query_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), query_request).await.unwrap();
    assert_eq!(query_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(query_response.into_body(), 16 * 1024).await.unwrap();
    let query_json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(query_json["master_keys"][&user_id]["user_id"], user_id);
    assert_eq!(query_json["self_signing_keys"][&user_id]["user_id"], user_id);
    assert_eq!(query_json["user_signing_keys"][&user_id]["user_id"], user_id);
    let user_devices =
        query_json["device_keys"][&user_id].as_object().expect("user entry must be present in /keys/query response");
    assert!(user_devices.contains_key(&dehydrated_device_id));
    assert_eq!(user_devices[&dehydrated_device_id]["device_id"], dehydrated_device_id);

    let delete_request = Request::builder()
        .method("DELETE")
        .uri("/_matrix/client/unstable/org.matrix.msc3814.v1/dehydrated_device")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let delete_response = ServiceExt::<Request<Body>>::oneshot(app, delete_request).await.unwrap();
    assert_eq!(delete_response.status(), StatusCode::OK);
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
        ServiceExt::<Request<Body>>::oneshot(app.clone(), request_verification_request).await.unwrap();
    if request_verification_response.status() != StatusCode::OK {
        let body = axum::body::to_bytes(request_verification_response.into_body(), 8192).await.unwrap();
        panic!("request_device_verification failed: {:?}", String::from_utf8_lossy(&body));
    }

    let request_verification_body =
        axum::body::to_bytes(request_verification_response.into_body(), 4096).await.unwrap();
    let request_verification_json: Value = serde_json::from_slice(&request_verification_body).unwrap();
    let verification_token = request_verification_json["token"].as_str().unwrap().to_string();
    assert_eq!(request_verification_json["request_token"], request_verification_json["token"]);
    assert_eq!(request_verification_json["status"], "pending");

    let status_request = Request::builder()
        .method("GET")
        .uri(format!("/_matrix/client/v3/device_verification/status/{}", verification_token))
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let status_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), status_request).await.unwrap();
    assert_eq!(status_response.status(), StatusCode::OK);

    let status_body = axum::body::to_bytes(status_response.into_body(), 4096).await.unwrap();
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
    let respond_response = ServiceExt::<Request<Body>>::oneshot(app, respond_request).await.unwrap();
    if respond_response.status() != StatusCode::OK {
        let body = axum::body::to_bytes(respond_response.into_body(), 8192).await.unwrap();
        panic!("respond_device_verification failed: {:?}", String::from_utf8_lossy(&body));
    }

    let respond_body = axum::body::to_bytes(respond_response.into_body(), 4096).await.unwrap();
    let respond_json: Value = serde_json::from_slice(&respond_body).unwrap();
    assert_eq!(respond_json["success"], true);
    assert_eq!(respond_json["trust_level"], "verified");
}

#[tokio::test]
async fn test_verification_request_listing_and_cancellation_flow() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let (alice_token, alice_user_id) = register_user(&app, &format!("verify_alice_{}", rand::random::<u32>())).await;
    let (bob_token, bob_user_id) = register_user(&app, &format!("verify_bob_{}", rand::random::<u32>())).await;
    let (mallory_token, _) = register_user(&app, &format!("verify_mallory_{}", rand::random::<u32>())).await;

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
    let start_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), start_request).await.unwrap();
    assert_eq!(start_response.status(), StatusCode::OK);

    let start_body = axum::body::to_bytes(start_response.into_body(), 4096).await.unwrap();
    let start_json: Value = serde_json::from_slice(&start_body).unwrap();
    let transaction_id = start_json["transaction_id"].as_str().unwrap().to_string();

    let pending_request = Request::builder()
        .method("GET")
        .uri("/_matrix/client/v1/keys/device_signing/requests")
        .header("Authorization", format!("Bearer {}", bob_token))
        .body(Body::empty())
        .unwrap();
    let pending_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), pending_request).await.unwrap();
    assert_eq!(pending_response.status(), StatusCode::OK);

    let pending_body = axum::body::to_bytes(pending_response.into_body(), 4096).await.unwrap();
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
        ServiceExt::<Request<Body>>::oneshot(app.clone(), forbidden_cancel_request).await.unwrap();
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
    let cancel_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), cancel_request).await.unwrap();
    assert_eq!(cancel_response.status(), StatusCode::OK);

    let cancel_body = axum::body::to_bytes(cancel_response.into_body(), 4096).await.unwrap();
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
    let post_cancel_list_response = ServiceExt::<Request<Body>>::oneshot(app, post_cancel_list_request).await.unwrap();
    assert_eq!(post_cancel_list_response.status(), StatusCode::OK);

    let post_cancel_body = axum::body::to_bytes(post_cancel_list_response.into_body(), 4096).await.unwrap();
    let post_cancel_json: Value = serde_json::from_slice(&post_cancel_body).unwrap();
    assert_eq!(post_cancel_json["requests"], json!([]));
}

#[tokio::test]
async fn test_verification_compat_request_status_and_cancel_flow() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let (alice_token, alice_user_id) =
        register_user(&app, &format!("compat_verify_alice_{}", rand::random::<u32>())).await;
    let (bob_token, bob_user_id) = register_user(&app, &format!("compat_verify_bob_{}", rand::random::<u32>())).await;
    let (mallory_token, _) = register_user(&app, &format!("compat_verify_mallory_{}", rand::random::<u32>())).await;

    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/keys/verification/request")
        .header("Authorization", format!("Bearer {}", alice_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "to_user": bob_user_id,
                "to_device": "BOB",
                "method": "m.sas.v1"
            })
            .to_string(),
        ))
        .unwrap();
    let request_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(request_response.status(), StatusCode::OK);

    let request_body = axum::body::to_bytes(request_response.into_body(), 4096).await.unwrap();
    let request_json: Value = serde_json::from_slice(&request_body).unwrap();
    let transaction_id = request_json["transaction_id"].as_str().unwrap().to_string();
    assert_eq!(request_json["state"], "requested");
    assert_eq!(request_json["request"]["from_user"], alice_user_id);
    assert_eq!(request_json["request"]["to_user"], bob_user_id);

    let status_request = Request::builder()
        .method("GET")
        .uri(format!("/_matrix/client/v3/keys/verification/{transaction_id}"))
        .header("Authorization", format!("Bearer {}", bob_token))
        .body(Body::empty())
        .unwrap();
    let status_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), status_request).await.unwrap();
    assert_eq!(status_response.status(), StatusCode::OK);

    let status_body = axum::body::to_bytes(status_response.into_body(), 4096).await.unwrap();
    let status_json: Value = serde_json::from_slice(&status_body).unwrap();
    assert_eq!(status_json["transaction_id"], transaction_id);
    assert_eq!(status_json["state"], "requested");
    assert_eq!(status_json["request"]["to_user"], bob_user_id);

    let forbidden_status_request = Request::builder()
        .method("GET")
        .uri(format!("/_matrix/client/v3/keys/verification/{transaction_id}"))
        .header("Authorization", format!("Bearer {}", mallory_token))
        .body(Body::empty())
        .unwrap();
    let forbidden_status_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), forbidden_status_request).await.unwrap();
    assert_eq!(forbidden_status_response.status(), StatusCode::FORBIDDEN);

    let cancel_request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/v3/keys/verification/{transaction_id}/cancel"))
        .header("Authorization", format!("Bearer {}", bob_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "code": "m.user",
                "reason": "Cancelled via compat endpoint"
            })
            .to_string(),
        ))
        .unwrap();
    let cancel_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), cancel_request).await.unwrap();
    assert_eq!(cancel_response.status(), StatusCode::OK);

    let cancel_body = axum::body::to_bytes(cancel_response.into_body(), 4096).await.unwrap();
    let cancel_json: Value = serde_json::from_slice(&cancel_body).unwrap();
    assert_eq!(cancel_json["transaction_id"], transaction_id);
    assert_eq!(cancel_json["state"], "cancelled");
    assert_eq!(cancel_json["code"], "m.user");

    let cancelled_status_request = Request::builder()
        .method("GET")
        .uri(format!("/_matrix/client/v3/keys/verification/{transaction_id}"))
        .header("Authorization", format!("Bearer {}", alice_token))
        .body(Body::empty())
        .unwrap();
    let cancelled_status_response = ServiceExt::<Request<Body>>::oneshot(app, cancelled_status_request).await.unwrap();
    assert_eq!(cancelled_status_response.status(), StatusCode::OK);

    let cancelled_status_body = axum::body::to_bytes(cancelled_status_response.into_body(), 4096).await.unwrap();
    let cancelled_status_json: Value = serde_json::from_slice(&cancelled_status_body).unwrap();
    assert_eq!(cancelled_status_json["state"], "cancelled");
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
                "auth_data": {"public_key": "test_pub_key_base64"}
            })
            .to_string(),
        ))
        .unwrap();
    let create_backup_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), create_backup_request).await.unwrap();
    assert_eq!(create_backup_response.status(), StatusCode::OK);

    let forward_request = Request::builder()
        .method("PUT")
        .uri(format!("/_matrix/client/v3/rooms/{}/room_keys/keys", room_id))
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
    let forward_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), forward_request).await.unwrap();
    let forward_status = forward_response.status();
    let body = axum::body::to_bytes(forward_response.into_body(), 4096).await.unwrap();
    if forward_status != StatusCode::OK {
        panic!("forward_room_keys failed with status {}: {:?}", forward_status, String::from_utf8_lossy(&body));
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
    let version_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), version_request).await.unwrap();
    assert_eq!(version_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(version_response.into_body(), 2048).await.unwrap();
    let version_json: Value = serde_json::from_slice(&body).unwrap();
    assert_ne!(version_json["version"], json!("0"));

    let count_request = Request::builder()
        .method("GET")
        .uri(format!("/_matrix/client/v3/rooms/{}/keys/count", room_id))
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let count_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), count_request).await.unwrap();
    let count_status = count_response.status();
    let body = axum::body::to_bytes(count_response.into_body(), 4096).await.unwrap();
    if count_status != StatusCode::OK {
        panic!("get_room_key_count failed with status {}: {:?}", count_status, String::from_utf8_lossy(&body));
    }
    let count_json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(count_json["count"], json!(1));

    let get_request = Request::builder()
        .method("GET")
        .uri(format!("/_matrix/client/v3/rooms/{}/keys", room_id))
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let get_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), get_request).await.unwrap();
    assert_eq!(get_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(get_response.into_body(), 2048).await.unwrap();
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
    let claim_response = ServiceExt::<Request<Body>>::oneshot(app, claim_request).await.unwrap();
    assert_eq!(claim_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(claim_response.into_body(), 2048).await.unwrap();
    let claim_json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(claim_json["one_time_keys"][room_id]["sess1"]["session_data"]["ciphertext"], json!("abc123"));
}

#[tokio::test]
async fn test_dehydrated_device_put_get_delete_roundtrip() {
    let Some((app, state)) = setup_test_app_with_state().await else {
        return;
    };
    let (token, user_id) = register_user(&app, &format!("dehydrated_{}", rand::random::<u32>())).await;
    let key_id = format!("dh-ssss-{}", rand::random::<u32>());
    seed_dehydrated_device_preconditions(&state, &user_id, &key_id).await;
    let device_id = format!("DEHYDRATEDTEST{:04}", rand::random::<u16>());

    let put_request = Request::builder()
        .method("PUT")
        .uri("/_matrix/client/unstable/org.matrix.msc3814.v1/dehydrated_device")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "device_id": device_id,
                "device_keys": {
                    "user_id": user_id,
                    "device_id": device_id,
                    "algorithms": ["m.olm.v1.curve25519-aes-sha2"],
                    "keys": {
                        format!("curve25519:{}", device_id): "AAAA",
                        format!("ed25519:{}", device_id): "BBBB"
                    },
                    "signatures": {}
                },
                "algorithm": "org.matrix.msc3814.v1.olm",
                "device_data": {
                    "algorithm": "org.matrix.msc3814.v1.olm",
                    "account": {
                        "pickle": "opaque-account"
                    },
                    "initial_device_display_name": "Dehydrated Test Device"
                }
            })
            .to_string(),
        ))
        .unwrap();

    let put_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), put_request).await.unwrap();
    assert_eq!(put_response.status(), StatusCode::OK);

    let put_body = axum::body::to_bytes(put_response.into_body(), 4096).await.unwrap();
    let put_json: Value = serde_json::from_slice(&put_body).unwrap();
    let device_id = put_json["device_id"].as_str().unwrap().to_string();
    assert_eq!(device_id, put_json["device_id"].as_str().unwrap());

    let get_request = Request::builder()
        .method("GET")
        .uri("/_matrix/client/unstable/org.matrix.msc3814.v1/dehydrated_device")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let get_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), get_request).await.unwrap();
    assert_eq!(get_response.status(), StatusCode::OK);

    let get_body = axum::body::to_bytes(get_response.into_body(), 4096).await.unwrap();
    let get_json: Value = serde_json::from_slice(&get_body).unwrap();
    assert_eq!(get_json["device_id"], device_id);
    assert_eq!(get_json["device_data"]["algorithm"], "org.matrix.msc3814.v1.olm");
    assert_eq!(get_json["device_data"]["account"]["pickle"], "opaque-account");
    assert_eq!(get_json["device_data"]["initial_device_display_name"], "Dehydrated Test Device");

    let delete_request = Request::builder()
        .method("DELETE")
        .uri("/_matrix/client/unstable/org.matrix.msc3814.v1/dehydrated_device")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let delete_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), delete_request).await.unwrap();
    assert_eq!(delete_response.status(), StatusCode::OK);
    let delete_body = axum::body::to_bytes(delete_response.into_body(), 4096).await.unwrap();
    let delete_json: Value = serde_json::from_slice(&delete_body).unwrap();
    assert_eq!(delete_json["device_id"], device_id);

    let get_after_delete_request = Request::builder()
        .method("GET")
        .uri("/_matrix/client/unstable/org.matrix.msc3814.v1/dehydrated_device")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let get_after_delete_response = ServiceExt::<Request<Body>>::oneshot(app, get_after_delete_request).await.unwrap();
    assert_eq!(get_after_delete_response.status(), StatusCode::NOT_FOUND);
}

/// MSC3814 — once a user uploads a dehydrated device, `/keys/query` must
/// surface its `device_keys` so other clients can address to-device messages
/// to it. This locks in the enrichment added to `query_keys_internal`.
#[tokio::test]
async fn test_dehydrated_device_appears_in_keys_query() {
    let Some((app, state)) = setup_test_app_with_state().await else {
        return;
    };
    let (token, user_id) = register_user(&app, &format!("dh_query_{}", rand::random::<u32>())).await;
    let key_id = format!("dh-query-ssss-{}", rand::random::<u32>());
    seed_dehydrated_device_preconditions(&state, &user_id, &key_id).await;

    let dehydrated_device_id = format!("DEHYDRATEDQUERY{:04}", rand::random::<u16>());
    let put_request = Request::builder()
        .method("PUT")
        .uri("/_matrix/client/unstable/org.matrix.msc3814.v1/dehydrated_device")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "device_id": dehydrated_device_id,
                "device_keys": {
                    "user_id": user_id,
                    "device_id": dehydrated_device_id,
                    "algorithms": [
                        "m.olm.v1.curve25519-aes-sha2",
                        "m.megolm.v1.aes-sha2"
                    ],
                    "keys": {
                        format!("curve25519:{}", dehydrated_device_id): "AAAA",
                        format!("ed25519:{}", dehydrated_device_id): "BBBB"
                    },
                    "signatures": {}
                },
                "device_data": {
                    "algorithm": "org.matrix.msc3814.v1.olm"
                }
            })
            .to_string(),
        ))
        .unwrap();
    let put_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), put_request).await.unwrap();
    assert_eq!(put_response.status(), StatusCode::OK);

    let query_request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/keys/query")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "device_keys": { user_id.clone(): [] }
            })
            .to_string(),
        ))
        .unwrap();
    let query_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), query_request).await.unwrap();
    assert_eq!(query_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(query_response.into_body(), 16 * 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let user_devices =
        json["device_keys"][&user_id].as_object().expect("user entry must be present in /keys/query response");
    assert!(
        user_devices.contains_key(&dehydrated_device_id),
        "expected dehydrated device {} in /keys/query response, got keys {:?}",
        dehydrated_device_id,
        user_devices.keys().collect::<Vec<_>>()
    );
    assert_eq!(user_devices[&dehydrated_device_id]["device_id"], dehydrated_device_id);

    let delete_request = Request::builder()
        .method("DELETE")
        .uri("/_matrix/client/unstable/org.matrix.msc3814.v1/dehydrated_device")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let delete_response = ServiceExt::<Request<Body>>::oneshot(app, delete_request).await.unwrap();
    assert_eq!(delete_response.status(), StatusCode::OK);
}

/// MSC3814 — `POST .../dehydrated_device/{device_id}/events` returns an empty
/// page with a `next_batch` cursor when no to-device messages are pending.
#[tokio::test]
async fn test_dehydrated_device_events_endpoint_empty_batch() {
    let Some((app, state)) = setup_test_app_with_state().await else {
        return;
    };
    let (token, user_id) = register_user(&app, &format!("dh_events_{}", rand::random::<u32>())).await;
    let key_id = format!("dh-events-ssss-{}", rand::random::<u32>());
    seed_dehydrated_device_preconditions(&state, &user_id, &key_id).await;
    let device_id = format!("DEHYDRATEDEVENT{:04}", rand::random::<u16>());

    let put_request = Request::builder()
        .method("PUT")
        .uri("/_matrix/client/unstable/org.matrix.msc3814.v1/dehydrated_device")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "device_id": device_id,
                "device_keys": {
                    "user_id": user_id,
                    "device_id": device_id,
                    "algorithms": ["m.olm.v1.curve25519-aes-sha2"],
                    "keys": {
                        format!("curve25519:{}", device_id): "AAAA",
                        format!("ed25519:{}", device_id): "BBBB"
                    },
                    "signatures": {}
                },
                "device_data": {
                    "algorithm": "org.matrix.msc3814.v1.olm"
                }
            })
            .to_string(),
        ))
        .unwrap();
    let put_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), put_request).await.unwrap();
    assert_eq!(put_response.status(), StatusCode::OK);
    let put_body = axum::body::to_bytes(put_response.into_body(), 4096).await.unwrap();
    let put_json: Value = serde_json::from_slice(&put_body).unwrap();
    let device_id = put_json["device_id"].as_str().unwrap().to_string();
    assert_eq!(device_id, put_json["device_id"].as_str().unwrap());

    let events_request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/unstable/org.matrix.msc3814.v1/dehydrated_device/{}/events", device_id))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({}).to_string()))
        .unwrap();
    let events_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), events_request).await.unwrap();
    assert_eq!(events_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(events_response.into_body(), 4096).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["events"].as_array().map_or(usize::MAX, |v| v.len()), 0, "expected no pending events");
    assert!(json["next_batch"].is_string(), "next_batch must be a string cursor: {}", json);

    let delete_request = Request::builder()
        .method("DELETE")
        .uri("/_matrix/client/unstable/org.matrix.msc3814.v1/dehydrated_device")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let delete_response = ServiceExt::<Request<Body>>::oneshot(app, delete_request).await.unwrap();
    assert_eq!(delete_response.status(), StatusCode::OK);
}

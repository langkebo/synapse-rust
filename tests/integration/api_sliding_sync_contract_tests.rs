use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use base64::Engine;
use ed25519_dalek::{Signer, SigningKey};
use serde_json::{json, Value};
use std::sync::Arc;
use synapse_rust::cache::{CacheConfig, CacheManager};
use synapse_rust::common::config::RateLimitRule;
use synapse_rust::services::ServiceContainer;
use synapse_rust::storage::sliding_sync::SlidingSyncStorage;
use synapse_rust::web::routes::state::AppState;
use tower::ServiceExt;

async fn setup_test_app_with_pool() -> Option<(axum::Router, Arc<sqlx::PgPool>)> {
    let pool = super::get_test_pool().await?;
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let container = ServiceContainer::new_test_with_pool_and_cache(pool.clone(), cache.clone()).await;
    let state = AppState::new(container, cache);
    Some((synapse_rust::web::create_router(state), pool))
}

async fn setup_isolated_test_app_with_pool() -> Option<(axum::Router, Arc<sqlx::PgPool>)> {
    let pool = synapse_rust::test_utils::prepare_isolated_test_pool().await.ok()?;
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let container = ServiceContainer::new_test_with_pool_and_cache(pool.clone(), cache.clone()).await;
    let state = AppState::new(container, cache);
    Some((synapse_rust::web::create_router(state), pool))
}

async fn setup_isolated_test_app_with_state_and_pool() -> Option<(axum::Router, AppState, Arc<sqlx::PgPool>)> {
    let pool = synapse_rust::test_utils::prepare_isolated_test_pool().await.ok()?;
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let container = ServiceContainer::new_test_with_pool_and_cache(pool.clone(), cache.clone()).await;
    let state = AppState::new(container, cache);
    let app = synapse_rust::web::create_router(state.clone());
    Some((app, state, pool))
}

async fn setup_test_app_with_sliding_sync_rate_limit(
    initial: RateLimitRule,
    incremental: RateLimitRule,
) -> Option<(axum::Router, Arc<sqlx::PgPool>)> {
    let pool = super::get_test_pool().await?;
    let mut container = ServiceContainer::new_test_with_pool(pool.clone()).await;
    container.core.config.rate_limit.enabled = false;
    container.core.config.rate_limit.sync.enabled = true;
    container.core.config.rate_limit.sync.initial = initial;
    container.core.config.rate_limit.sync.incremental = incremental;

    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let state = AppState::new(container, cache);
    Some((synapse_rust::web::create_router(state), pool))
}

async fn setup_two_test_apps_with_shared_pool() -> Option<((axum::Router, axum::Router), Arc<sqlx::PgPool>)> {
    let pool = super::get_test_pool().await?;

    let cache_a = Arc::new(CacheManager::new(&CacheConfig::default()));
    let container_a = ServiceContainer::new_test_with_pool_and_cache(pool.clone(), cache_a.clone()).await;
    let state_a = AppState::new(container_a, cache_a);
    let app_a = synapse_rust::web::create_router(state_a);

    let cache_b = Arc::new(CacheManager::new(&CacheConfig::default()));
    let container_b = ServiceContainer::new_test_with_pool_and_cache(pool.clone(), cache_b.clone()).await;
    let state_b = AppState::new(container_b, cache_b);
    let app_b = synapse_rust::web::create_router(state_b);

    Some(((app_a, app_b), pool))
}

async fn whoami(app: &axum::Router, token: &str) -> (String, Option<String>) {
    let request = Request::builder()
        .method("GET")
        .uri("/_matrix/client/v3/account/whoami")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(super::with_local_connect_info(request)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let user_id = json["user_id"].as_str().unwrap().to_string();
    let device_id = json.get("device_id").and_then(|v| v.as_str()).map(|v| v.to_string());
    (user_id, device_id)
}

async fn create_room(app: &axum::Router, token: &str) -> String {
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/createRoom")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "name": "Sliding Sync Room" }).to_string()))
        .unwrap();

    let response = app.clone().oneshot(super::with_local_connect_info(request)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    json["room_id"].as_str().unwrap().to_string()
}

async fn invite_user_to_room(app: &axum::Router, inviter_token: &str, room_id: &str, invitee_user_id: &str) {
    let request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/v3/rooms/{room_id}/invite"))
        .header("Authorization", format!("Bearer {}", inviter_token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "user_id": invitee_user_id }).to_string()))
        .unwrap();

    let response = app.clone().oneshot(super::with_local_connect_info(request)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

async fn join_room(app: &axum::Router, token: &str, room_id: &str) {
    let request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/v3/rooms/{room_id}/join"))
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(super::with_local_connect_info(request)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

async fn leave_room(app: &axum::Router, token: &str, room_id: &str) {
    let request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/v3/rooms/{room_id}/leave"))
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(super::with_local_connect_info(request)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

async fn kick_user_from_room(app: &axum::Router, token: &str, room_id: &str, user_id: &str, reason: &str) {
    let request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/v3/rooms/{room_id}/kick"))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "user_id": user_id, "reason": reason }).to_string()))
        .unwrap();

    let response = app.clone().oneshot(super::with_local_connect_info(request)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

async fn ban_user_from_room(app: &axum::Router, token: &str, room_id: &str, user_id: &str, reason: &str) {
    let request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/v3/rooms/{room_id}/ban"))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "user_id": user_id, "reason": reason }).to_string()))
        .unwrap();

    let response = app.clone().oneshot(super::with_local_connect_info(request)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

async fn unban_user_from_room(app: &axum::Router, token: &str, room_id: &str, user_id: &str) {
    let request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/v3/rooms/{room_id}/unban"))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "user_id": user_id }).to_string()))
        .unwrap();

    let response = app.clone().oneshot(super::with_local_connect_info(request)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

async fn forget_room(app: &axum::Router, token: &str, room_id: &str) {
    let request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/v3/rooms/{room_id}/forget"))
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(super::with_local_connect_info(request)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

async fn set_room_join_rule(app: &axum::Router, token: &str, room_id: &str, join_rule: &str) {
    let request = Request::builder()
        .method("PUT")
        .uri(format!("/_matrix/client/v3/rooms/{room_id}/state/m.room.join_rules"))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "join_rule": join_rule }).to_string()))
        .unwrap();

    let response = app.clone().oneshot(super::with_local_connect_info(request)).await.unwrap();
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

    let response = app.clone().oneshot(super::with_local_connect_info(request)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

async fn add_join_membership(pool: &Arc<sqlx::PgPool>, room_id: &str, user_id: &str) {
    sqlx::query(
        r#"
        INSERT INTO room_memberships (room_id, user_id, membership, joined_ts)
        VALUES ($1, $2, 'join', $3)
        ON CONFLICT DO NOTHING
        "#,
    )
    .bind(room_id)
    .bind(user_id)
    .bind(chrono::Utc::now().timestamp_millis())
    .execute(pool.as_ref())
    .await
    .unwrap();
}

async fn put_global_account_data(app: &axum::Router, token: &str, user_id: &str, data_type: &str, content: Value) {
    let request = Request::builder()
        .method("PUT")
        .uri(format!("/_matrix/client/v3/user/{}/account_data/{}", user_id, data_type))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(content.to_string()))
        .unwrap();

    let response = app.clone().oneshot(super::with_local_connect_info(request)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

async fn send_room_message(app: &axum::Router, token: &str, room_id: &str) -> String {
    let request = Request::builder()
        .method("PUT")
        .uri(format!("/_matrix/client/v3/rooms/{}/send/m.room.message/{}", room_id, rand::random::<u32>()))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "msgtype": "m.text", "body": "hello" }).to_string()))
        .unwrap();

    let response = app.clone().oneshot(super::with_local_connect_info(request)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    json["event_id"].as_str().unwrap().to_string()
}

async fn upload_device_keys(app: &axum::Router, token: &str, user_id: &str, device_id: &str, key_suffix: &str) {
    let mut seed = [0u8; 32];
    let suffix_bytes = key_suffix.as_bytes();
    for (i, b) in suffix_bytes.iter().take(32).enumerate() {
        seed[i] = *b;
    }
    let signing_key = SigningKey::from_bytes(&seed);
    let verifying_key = signing_key.verifying_key();
    let ed25519_pk_b64 = base64::engine::general_purpose::STANDARD.encode(verifying_key.as_bytes());
    let curve25519_pk = format!("curve25519-{}", key_suffix);
    let otk_pk = format!("otk-{}", key_suffix);

    let mut device_keys = json!({
        "user_id": user_id,
        "device_id": device_id,
        "algorithms": ["m.olm.v1.curve25519-aes-sha2"],
        "keys": {
            format!("curve25519:{}", device_id): curve25519_pk,
            format!("ed25519:{}", device_id): ed25519_pk_b64,
        }
    });
    let dk_canonical = synapse_rust::e2ee::signed_json::canonical_json_bytes(&device_keys).unwrap();
    let dk_signature = base64::engine::general_purpose::STANDARD.encode(signing_key.sign(&dk_canonical).to_bytes());
    device_keys["signatures"] = json!({
        user_id: { format!("ed25519:{}", device_id): dk_signature }
    });

    let otk_id = format!("signed_curve25519:{}", key_suffix);
    let mut otk_payload = json!({ "key": otk_pk });
    let otk_canonical = synapse_rust::e2ee::signed_json::canonical_json_bytes(&otk_payload).unwrap();
    let otk_signature = base64::engine::general_purpose::STANDARD.encode(signing_key.sign(&otk_canonical).to_bytes());
    otk_payload["signatures"] = json!({
        user_id: { format!("ed25519:{}", device_id): otk_signature }
    });

    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/r0/keys/upload")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "device_keys": device_keys,
                "one_time_keys": {
                    otk_id: otk_payload
                }
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.clone().oneshot(super::with_local_connect_info(request)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

async fn upload_cross_signing_keys(state: &AppState, user_id: &str, key_suffix: &str) {
    state
        .services
        .e2ee
        .cross_signing_service
        .upload_cross_signing_keys(synapse_rust::e2ee::cross_signing::CrossSigningUpload {
            master_key: json!({
                "user_id": user_id,
                "usage": ["master"],
                "keys": {
                    format!("ed25519:{key_suffix}MASTER"): format!("{key_suffix}-master-public-key")
                },
                "signatures": {}
            }),
            self_signing_key: json!({
                "user_id": user_id,
                "usage": ["self_signing"],
                "keys": {
                    format!("ed25519:{key_suffix}SELF"): format!("{key_suffix}-self-signing-public-key")
                },
                "signatures": {}
            }),
            user_signing_key: json!({
                "user_id": user_id,
                "usage": ["user_signing"],
                "keys": {
                    format!("ed25519:{key_suffix}USER"): format!("{key_suffix}-user-signing-public-key")
                },
                "signatures": {}
            }),
        })
        .await
        .expect("failed to upload cross-signing keys");
}

async fn send_to_device_message(
    app: &axum::Router,
    token: &str,
    event_type: &str,
    txn_id: &str,
    recipient_user_id: &str,
    recipient_device_id: &str,
    content: Value,
) {
    let request = Request::builder()
        .method("PUT")
        .uri(format!("/_matrix/client/v3/sendToDevice/{}/{}", event_type, txn_id))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "messages": {
                    recipient_user_id: {
                        recipient_device_id: content
                    }
                }
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.clone().oneshot(super::with_local_connect_info(request)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

async fn send_read_receipt(app: &axum::Router, token: &str, room_id: &str, event_id: &str) -> (StatusCode, Value) {
    let request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/v3/rooms/{}/receipt/m.read/{}", room_id, event_id))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({}).to_string()))
        .unwrap();

    let response = app.clone().oneshot(super::with_local_connect_info(request)).await.unwrap();
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap_or_else(|_| json!({}));
    (status, json)
}

async fn set_typing(
    app: &axum::Router,
    token: &str,
    room_id: &str,
    user_id: &str,
    typing: bool,
) -> (StatusCode, Value) {
    let request = Request::builder()
        .method("PUT")
        .uri(format!("/_matrix/client/v3/rooms/{}/typing/{}", room_id, user_id))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "typing": typing,
                "timeout": 30000
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.clone().oneshot(super::with_local_connect_info(request)).await.unwrap();
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap_or_else(|_| json!({}));
    (status, json)
}

async fn put_beacon_info(app: &axum::Router, token: &str, room_id: &str, state_key: &str) -> String {
    let request = Request::builder()
        .method("PUT")
        .uri(format!("/_matrix/client/v3/rooms/{}/state/m.beacon_info/{}", room_id, state_key))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "m.beacon_info": {
                    "description": "Beacon in sliding sync",
                    "timeout": 60_000,
                    "live": true
                },
                "m.ts": chrono::Utc::now().timestamp_millis(),
                "m.asset": { "type": "m.self" }
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.clone().oneshot(super::with_local_connect_info(request)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    json["event_id"].as_str().unwrap().to_string()
}

async fn send_beacon(app: &axum::Router, token: &str, room_id: &str, beacon_info_id: &str) -> (StatusCode, Value) {
    let request = Request::builder()
        .method("PUT")
        .uri(format!("/_matrix/client/v3/rooms/{}/send/m.beacon/{}", room_id, rand::random::<u32>()))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "m.relates_to": {
                    "rel_type": "m.reference",
                    "event_id": beacon_info_id
                },
                "m.location": {
                    "uri": "geo:51.5008,0.1247;u=35",
                    "description": "London"
                },
                "m.ts": chrono::Utc::now().timestamp_millis(),
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.clone().oneshot(super::with_local_connect_info(request)).await.unwrap();
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap_or_else(|_| json!({}));
    (status, json)
}

async fn post_sliding_sync(app: &axum::Router, token: Option<&str>, body: Value) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method("POST")
        .uri("/_matrix/client/unstable/org.matrix.msc3575/sync")
        .header("Content-Type", "application/json");

    if let Some(token) = token {
        builder = builder.header("Authorization", format!("Bearer {}", token));
    }

    let request = builder.body(Body::from(body.to_string())).unwrap();

    let response = app.clone().oneshot(super::with_local_connect_info(request)).await.unwrap();

    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024).await.unwrap();

    let json = if body.is_empty() {
        json!({})
    } else {
        serde_json::from_slice(&body).unwrap_or_else(|_| json!({ "raw": String::from_utf8_lossy(&body) }))
    };

    (status, json)
}

async fn get_sync(app: &axum::Router, token: &str, uri: &str) -> (StatusCode, Value) {
    let request = Request::builder()
        .method("GET")
        .uri(uri)
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(super::with_local_connect_info(request)).await.unwrap();

    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    let json = if body.is_empty() {
        json!({})
    } else {
        serde_json::from_slice(&body).unwrap_or_else(|_| json!({ "raw": String::from_utf8_lossy(&body) }))
    };

    (status, json)
}

#[tokio::test]
async fn test_sliding_sync_requires_authentication() {
    let Some((app, _pool)) = setup_test_app_with_pool().await else {
        eprintln!("Skipping test: database not available");
        return;
    };

    let (status, _body) = post_sliding_sync(&app, None, json!({ "lists": {} })).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_sliding_sync_rate_limit_returns_backoff() {
    let Some((app, _pool)) = setup_test_app_with_sliding_sync_rate_limit(
        RateLimitRule { per_second: 1, burst_size: 1 },
        RateLimitRule { per_second: 10, burst_size: 10 },
    )
    .await
    else {
        eprintln!("Skipping test: database not available");
        return;
    };

    let token = super::create_test_user(&app).await;
    let mut limited_body: Option<Value> = None;

    for _ in 0..3 {
        let (status, body) = post_sliding_sync(&app, Some(&token), json!({ "lists": {} })).await;
        if status == StatusCode::TOO_MANY_REQUESTS {
            limited_body = Some(body);
            break;
        }
    }

    let body = limited_body.expect("expected at least one sliding sync 429 response");
    assert_eq!(body["errcode"], "M_LIMIT_EXCEEDED");
    assert!(body.get("retry_after_ms").is_some());
}

#[tokio::test]
async fn test_sliding_sync_pos_roundtrip_and_validation() {
    let Some((app, _pool)) = setup_test_app_with_pool().await else {
        eprintln!("Skipping test: database not available");
        return;
    };

    let token = super::create_test_user(&app).await;

    let (status_1, body_1) = post_sliding_sync(&app, Some(&token), json!({ "lists": {} })).await;
    assert_eq!(status_1, StatusCode::OK, "{:?}", body_1);
    let pos_1 = body_1["pos"].as_str().unwrap().to_string();
    assert!(!pos_1.is_empty());

    let (status_2, body_2) = post_sliding_sync(
        &app,
        Some(&token),
        json!({
            "lists": {},
            "pos": pos_1.clone()
        }),
    )
    .await;
    assert_eq!(status_2, StatusCode::OK, "{:?}", body_2);
    let pos_2 = body_2["pos"].as_str().unwrap().to_string();
    assert!(!pos_2.is_empty());
    assert_ne!(pos_2, pos_1);

    let (status_3, body_3) = post_sliding_sync(
        &app,
        Some(&token),
        json!({
            "lists": {},
            "pos": pos_1
        }),
    )
    .await;
    assert_eq!(status_3, StatusCode::BAD_REQUEST, "{:?}", body_3);
}

#[tokio::test]
async fn test_sliding_sync_lists_ranges_returns_rooms_in_order() {
    let Some((app, pool)) = setup_test_app_with_pool().await else {
        eprintln!("Skipping test: database not available");
        return;
    };

    let token = super::create_test_user(&app).await;
    let (user_id, device_id) = whoami(&app, &token).await;
    let device_id = device_id.unwrap_or_else(|| "default".to_string());

    let room_a = create_room(&app, &token).await;
    let room_b = create_room(&app, &token).await;
    let room_c = create_room(&app, &token).await;
    let base = chrono::Utc::now().timestamp_millis() + 10_000;

    let storage = SlidingSyncStorage::new(pool.clone());
    storage
        .upsert_room(
            &user_id,
            &device_id,
            &room_a,
            None,
            Some("main"),
            base + 3000,
            0,
            0,
            false,
            false,
            false,
            false,
            Some("A"),
            None,
            base + 3000,
        )
        .await
        .unwrap();
    storage
        .upsert_room(
            &user_id,
            &device_id,
            &room_b,
            None,
            Some("main"),
            base + 2000,
            0,
            0,
            false,
            false,
            false,
            false,
            Some("B"),
            None,
            base + 2000,
        )
        .await
        .unwrap();
    storage
        .upsert_room(
            &user_id,
            &device_id,
            &room_c,
            None,
            Some("main"),
            base + 1000,
            0,
            0,
            false,
            false,
            false,
            false,
            Some("C"),
            None,
            base + 1000,
        )
        .await
        .unwrap();

    let (status, body) = post_sliding_sync(
        &app,
        Some(&token),
        json!({
            "lists": {
                "main": {
                    "ranges": [[0, 1]],
                    "sort": ["by_recency"]
                }
            }
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{:?}", body);

    assert_eq!(body["lists"]["main"]["count"].as_u64().unwrap(), 3);
    assert_eq!(body["lists"]["main"]["ops"][0]["op"], "SYNC");
    assert_eq!(body["lists"]["main"]["ops"][0]["range"], json!([0, 1]));
    assert_eq!(body["lists"]["main"]["ops"][0]["room_ids"], json!([room_a, room_b]));

    assert!(body["rooms"].get(&room_a).is_some());
    assert!(body["rooms"].get(&room_b).is_some());
    assert!(body["rooms"].get(&room_c).is_none());
}

#[tokio::test]
async fn test_sliding_sync_room_subscriptions_includes_room() {
    let Some((app, pool)) = setup_test_app_with_pool().await else {
        eprintln!("Skipping test: database not available");
        return;
    };

    let token = super::create_test_user(&app).await;
    let (user_id, device_id) = whoami(&app, &token).await;
    let device_id = device_id.unwrap_or_else(|| "default".to_string());

    let room_id = create_room(&app, &token).await;

    let storage = SlidingSyncStorage::new(pool.clone());
    storage
        .upsert_room(
            &user_id,
            &device_id,
            &room_id,
            None,
            None,
            1000,
            0,
            0,
            false,
            false,
            false,
            false,
            Some("Room"),
            None,
            1000,
        )
        .await
        .unwrap();

    let mut room_subscriptions = serde_json::Map::new();
    room_subscriptions.insert(room_id.clone(), json!({}));

    let (status, body) = post_sliding_sync(
        &app,
        Some(&token),
        json!({
            "lists": {},
            "room_subscriptions": room_subscriptions
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{:?}", body);

    assert!(body["rooms"].get(&room_id).is_some());
}

#[tokio::test]
async fn test_sliding_sync_list_filters_apply_to_query_results() {
    let Some((app, pool)) = setup_test_app_with_pool().await else {
        eprintln!("Skipping test: database not available");
        return;
    };

    let token = super::create_test_user(&app).await;
    let (user_id, device_id) = whoami(&app, &token).await;
    let device_id = device_id.unwrap_or_else(|| "default".to_string());

    let room_dm = create_room(&app, &token).await;
    let room_group = create_room(&app, &token).await;
    let base = chrono::Utc::now().timestamp_millis() + 10_000;

    let storage = SlidingSyncStorage::new(pool.clone());
    storage
        .upsert_room(
            &user_id,
            &device_id,
            &room_dm,
            None,
            Some("main"),
            base + 2000,
            0,
            0,
            true,
            false,
            false,
            false,
            Some("DM Room"),
            None,
            base + 2000,
        )
        .await
        .unwrap();
    storage
        .upsert_room(
            &user_id,
            &device_id,
            &room_group,
            None,
            Some("main"),
            base + 1000,
            0,
            0,
            false,
            false,
            false,
            false,
            Some("Group Room"),
            None,
            base + 1000,
        )
        .await
        .unwrap();

    let (status, body) = post_sliding_sync(
        &app,
        Some(&token),
        json!({
            "lists": {
                "main": {
                    "ranges": [[0, 9]],
                    "sort": ["by_recency"],
                    "filters": {
                        "is_dm": true
                    }
                }
            }
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{:?}", body);
    assert_eq!(body["lists"]["main"]["count"], 1);
    assert_eq!(body["lists"]["main"]["ops"][0]["room_ids"], json!([room_dm]));
    assert!(body["rooms"].get(&room_dm).is_some());
    assert!(body["rooms"].get(&room_group).is_none());
}

#[tokio::test]
async fn test_sliding_sync_room_response_includes_timeline_and_required_state() {
    let Some((app, _pool)) = setup_test_app_with_pool().await else {
        eprintln!("Skipping test: database not available");
        return;
    };

    let token = super::create_test_user(&app).await;
    let room_id = create_room(&app, &token).await;
    send_room_message(&app, &token, &room_id).await;

    let (status, body) = post_sliding_sync(
        &app,
        Some(&token),
        json!({
            "room_subscriptions": {
                room_id.clone(): {
                    "timeline_limit": 1,
                    "required_state": [["m.room.create", ""]]
                }
            }
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{:?}", body);

    let room = &body["rooms"][room_id.clone()];
    assert_eq!(room["room_id"], room_id);
    assert_eq!(room["timeline"].as_array().unwrap().len(), 1);
    assert_eq!(room["timeline"][0]["type"], "m.room.message");
    assert_eq!(room["required_state"].as_array().unwrap().len(), 1);
    assert_eq!(room["required_state"][0]["type"], "m.room.create");
    assert!(room["prev_batch"].as_str().unwrap().starts_with('t'));
}

#[tokio::test]
async fn test_sliding_sync_uses_incremental_ops_for_follow_up_request() {
    let Some((app, pool)) = setup_test_app_with_pool().await else {
        eprintln!("Skipping test: database not available");
        return;
    };

    let token = super::create_test_user(&app).await;
    let (user_id, device_id) = whoami(&app, &token).await;
    let device_id = device_id.unwrap_or_else(|| "default".to_string());
    let conn_id = "conn-incremental";

    let room_a = create_room(&app, &token).await;
    let room_b = create_room(&app, &token).await;
    let base = chrono::Utc::now().timestamp_millis() + 10_000;

    let storage = SlidingSyncStorage::new(pool.clone());
    storage
        .upsert_room(
            &user_id,
            &device_id,
            &room_a,
            Some(conn_id),
            Some("main"),
            base + 2000,
            0,
            0,
            false,
            false,
            false,
            false,
            Some("A"),
            None,
            base + 2000,
        )
        .await
        .unwrap();
    storage
        .upsert_room(
            &user_id,
            &device_id,
            &room_b,
            Some(conn_id),
            Some("main"),
            base + 1000,
            0,
            0,
            false,
            false,
            false,
            false,
            Some("B"),
            None,
            base + 1000,
        )
        .await
        .unwrap();

    let (first_status, first_body) = post_sliding_sync(
        &app,
        Some(&token),
        json!({
            "conn_id": conn_id,
            "lists": {
                "main": {
                    "ranges": [[0, 1]],
                    "sort": ["by_recency"]
                }
            }
        }),
    )
    .await;
    assert_eq!(first_status, StatusCode::OK, "{:?}", first_body);
    assert_eq!(first_body["lists"]["main"]["ops"][0]["op"], "SYNC");

    let room_c = create_room(&app, &token).await;
    storage
        .upsert_room(
            &user_id,
            &device_id,
            &room_c,
            Some(conn_id),
            Some("main"),
            base + 3000,
            0,
            0,
            false,
            false,
            false,
            false,
            Some("C"),
            None,
            base + 3000,
        )
        .await
        .unwrap();

    let (second_status, second_body) = post_sliding_sync(
        &app,
        Some(&token),
        json!({
            "conn_id": conn_id,
            "pos": first_body["pos"].as_str().unwrap(),
            "lists": {
                "main": {
                    "ranges": [[0, 1]],
                    "sort": ["by_recency"]
                }
            }
        }),
    )
    .await;
    assert_eq!(second_status, StatusCode::OK, "{:?}", second_body);

    let ops = second_body["lists"]["main"]["ops"].as_array().unwrap();
    let has_incremental = ops.iter().any(|op| op["op"] == "INSERT") && ops.iter().any(|op| op["op"] == "DELETE");
    let sync_op = ops.iter().find(|op| op["op"] == "SYNC");

    assert!(has_incremental || sync_op.is_some(), "{ops:?}");
    if let Some(sync_op) = sync_op {
        assert_eq!(sync_op["room_ids"], json!([room_c, room_a]));
    }
}

#[tokio::test]
async fn test_sliding_sync_extensions_e2ee_returns_key_counts_and_device_list_deltas() {
    let Some((app, pool)) = setup_isolated_test_app_with_pool().await else {
        eprintln!("Skipping test: database not available");
        return;
    };

    let token_a = super::create_test_user(&app).await;
    let token_b = super::create_test_user(&app).await;
    let (user_a, device_a) = whoami(&app, &token_a).await;
    let (user_b, device_b) = whoami(&app, &token_b).await;
    let device_a = device_a.unwrap_or_else(|| "default".to_string());
    let device_b = device_b.unwrap_or_else(|| "default".to_string());
    let shared_room = create_room(&app, &token_a).await;
    add_join_membership(&pool, &shared_room, &user_b).await;

    upload_device_keys(&app, &token_a, &user_a, &device_a, "alice-otk").await;

    let conn_id = "conn-e2ee";
    let (first_status, first_body) = post_sliding_sync(
        &app,
        Some(&token_a),
        json!({
            "conn_id": conn_id,
            "lists": {},
            "extensions": {
                "e2ee": { "enabled": true }
            }
        }),
    )
    .await;
    assert_eq!(first_status, StatusCode::OK, "{:?}", first_body);
    assert_eq!(first_body["extensions"]["e2ee"]["device_one_time_keys_count"]["signed_curve25519"], 1);
    assert_eq!(first_body["extensions"]["e2ee"]["device_unused_fallback_key_types"], json!([]));

    upload_device_keys(&app, &token_b, &user_b, &device_b, "bob-otk").await;

    let (second_status, second_body) = post_sliding_sync(
        &app,
        Some(&token_a),
        json!({
            "conn_id": conn_id,
            "pos": first_body["pos"].as_str().unwrap(),
            "lists": {},
            "extensions": {
                "e2ee": { "enabled": true }
            }
        }),
    )
    .await;
    assert_eq!(second_status, StatusCode::OK, "{:?}", second_body);
    let changed = second_body["extensions"]["e2ee"]["device_lists"]["changed"].as_array().unwrap();
    assert!(changed.iter().any(|entry| entry == &json!(user_b)));
}

#[tokio::test]
async fn test_sliding_sync_extensions_e2ee_exposes_cross_signing_updates_for_shared_users() {
    let Some((app, state, pool)) = setup_isolated_test_app_with_state_and_pool().await else {
        eprintln!("Skipping test: database not available");
        return;
    };

    let token_a = super::create_test_user(&app).await;
    let token_b = super::create_test_user(&app).await;
    let (_user_a, _device_a) = whoami(&app, &token_a).await;
    let (user_b, _device_b) = whoami(&app, &token_b).await;
    let shared_room = create_room(&app, &token_a).await;
    add_join_membership(&pool, &shared_room, &user_b).await;

    let conn_id = "conn-e2ee-cross-signing-shared";
    let (first_status, first_body) = post_sliding_sync(
        &app,
        Some(&token_a),
        json!({
            "conn_id": conn_id,
            "lists": {},
            "extensions": {
                "e2ee": { "enabled": true }
            }
        }),
    )
    .await;
    assert_eq!(first_status, StatusCode::OK, "{:?}", first_body);

    upload_cross_signing_keys(&state, &user_b, "SLIDINGSHARED").await;

    let (second_status, second_body) = post_sliding_sync(
        &app,
        Some(&token_a),
        json!({
            "conn_id": conn_id,
            "pos": first_body["pos"].as_str().unwrap(),
            "lists": {},
            "extensions": {
                "e2ee": { "enabled": true }
            }
        }),
    )
    .await;
    assert_eq!(second_status, StatusCode::OK, "{:?}", second_body);

    let changed = second_body["extensions"]["e2ee"]["device_lists"]["changed"].as_array().unwrap();
    let left = second_body["extensions"]["e2ee"]["device_lists"]["left"].as_array().unwrap();

    assert!(
        changed.iter().any(|entry| entry == &json!(user_b)),
        "expected sliding-sync e2ee.device_lists.changed to include shared user cross-signing update: {}",
        second_body
    );
    assert!(!left.iter().any(|entry| entry == &json!(user_b)));
    assert_eq!(second_body["extensions"]["e2ee"]["device_unused_fallback_key_types"], json!([]));
}

#[tokio::test]
async fn test_sliding_sync_extensions_e2ee_does_not_leak_cross_signing_updates_without_shared_rooms() {
    let Some((app, state, _pool)) = setup_isolated_test_app_with_state_and_pool().await else {
        eprintln!("Skipping test: database not available");
        return;
    };

    let token_a = super::create_test_user(&app).await;
    let token_b = super::create_test_user(&app).await;
    let (_user_a, _device_a) = whoami(&app, &token_a).await;
    let (user_b, _device_b) = whoami(&app, &token_b).await;

    let conn_id = "conn-e2ee-cross-signing-isolated";
    let (first_status, first_body) = post_sliding_sync(
        &app,
        Some(&token_a),
        json!({
            "conn_id": conn_id,
            "lists": {},
            "extensions": {
                "e2ee": { "enabled": true }
            }
        }),
    )
    .await;
    assert_eq!(first_status, StatusCode::OK, "{:?}", first_body);

    upload_cross_signing_keys(&state, &user_b, "SLIDINGISOLATED").await;

    let (second_status, second_body) = post_sliding_sync(
        &app,
        Some(&token_a),
        json!({
            "conn_id": conn_id,
            "pos": first_body["pos"].as_str().unwrap(),
            "lists": {},
            "extensions": {
                "e2ee": { "enabled": true }
            }
        }),
    )
    .await;
    assert_eq!(second_status, StatusCode::OK, "{:?}", second_body);

    let changed = second_body["extensions"]["e2ee"]["device_lists"]["changed"].as_array().unwrap();
    let left = second_body["extensions"]["e2ee"]["device_lists"]["left"].as_array().unwrap();

    assert!(
        !changed.iter().any(|entry| entry == &json!(user_b)),
        "expected sliding-sync e2ee.device_lists.changed to exclude isolated user cross-signing update: {}",
        second_body
    );
    assert!(!left.iter().any(|entry| entry == &json!(user_b)));
}

#[tokio::test]
async fn test_sliding_sync_extensions_e2ee_left_reports_users_who_stop_sharing_rooms() {
    let Some((app, pool)) = setup_isolated_test_app_with_pool().await else {
        eprintln!("Skipping test: database not available");
        return;
    };

    let token_a = super::create_test_user(&app).await;
    let token_b = super::create_test_user(&app).await;
    let (_user_a, _device_a) = whoami(&app, &token_a).await;
    let (user_b, _device_b) = whoami(&app, &token_b).await;
    let shared_room = create_room(&app, &token_a).await;
    add_join_membership(&pool, &shared_room, &user_b).await;

    let conn_id = "conn-e2ee-left-shared";
    let (first_status, first_body) = post_sliding_sync(
        &app,
        Some(&token_a),
        json!({
            "conn_id": conn_id,
            "lists": {},
            "extensions": {
                "e2ee": { "enabled": true }
            }
        }),
    )
    .await;
    assert_eq!(first_status, StatusCode::OK, "{:?}", first_body);
    let first_left = first_body["extensions"]["e2ee"]["device_lists"]["left"].as_array().unwrap();
    assert!(
        !first_left.iter().any(|entry| entry == &json!(user_b)),
        "initial sliding-sync response should not report left users: {}",
        first_body
    );

    leave_room(&app, &token_b, &shared_room).await;

    let (second_status, second_body) = post_sliding_sync(
        &app,
        Some(&token_a),
        json!({
            "conn_id": conn_id,
            "pos": first_body["pos"].as_str().unwrap(),
            "lists": {},
            "extensions": {
                "e2ee": { "enabled": true }
            }
        }),
    )
    .await;
    assert_eq!(second_status, StatusCode::OK, "{:?}", second_body);

    let changed = second_body["extensions"]["e2ee"]["device_lists"]["changed"].as_array().unwrap();
    let left = second_body["extensions"]["e2ee"]["device_lists"]["left"].as_array().unwrap();

    assert!(
        !changed.iter().any(|entry| entry == &json!(user_b)),
        "membership-only unshare should not masquerade as device change: {}",
        second_body
    );
    assert!(
        left.iter().any(|entry| entry == &json!(user_b)),
        "expected sliding-sync e2ee.device_lists.left to include users who stopped sharing rooms: {}",
        second_body
    );
}

#[tokio::test]
async fn test_sliding_sync_extensions_e2ee_invite_decline_does_not_report_left() {
    let Some((app, _pool)) = setup_isolated_test_app_with_pool().await else {
        eprintln!("Skipping test: database not available");
        return;
    };

    let token_a = super::create_test_user(&app).await;
    let token_b = super::create_test_user(&app).await;
    let (user_b, _device_b) = whoami(&app, &token_b).await;
    let room_id = create_room(&app, &token_a).await;
    invite_user_to_room(&app, &token_a, &room_id, &user_b).await;

    let conn_id = "conn-e2ee-invite-decline";
    let (first_status, first_body) = post_sliding_sync(
        &app,
        Some(&token_a),
        json!({
            "conn_id": conn_id,
            "lists": {},
            "extensions": {
                "e2ee": { "enabled": true }
            }
        }),
    )
    .await;
    assert_eq!(first_status, StatusCode::OK, "{:?}", first_body);

    leave_room(&app, &token_b, &room_id).await;

    let (second_status, second_body) = post_sliding_sync(
        &app,
        Some(&token_a),
        json!({
            "conn_id": conn_id,
            "pos": first_body["pos"].as_str().unwrap(),
            "lists": {},
            "extensions": {
                "e2ee": { "enabled": true }
            }
        }),
    )
    .await;
    assert_eq!(second_status, StatusCode::OK, "{:?}", second_body);

    let changed = second_body["extensions"]["e2ee"]["device_lists"]["changed"].as_array().unwrap();
    let left = second_body["extensions"]["e2ee"]["device_lists"]["left"].as_array().unwrap();

    assert!(
        !changed.iter().any(|entry| entry == &json!(user_b)),
        "invite decline must not masquerade as device change in sliding-sync: {}",
        second_body
    );
    assert!(
        !left.iter().any(|entry| entry == &json!(user_b)),
        "invite decline must not surface as shared-user loss in sliding-sync device_lists.left: {}",
        second_body
    );
}

#[tokio::test]
async fn test_sliding_sync_extensions_e2ee_leave_then_rejoin_does_not_report_left() {
    let Some((app, _pool)) = setup_isolated_test_app_with_pool().await else {
        eprintln!("Skipping test: database not available");
        return;
    };

    let token_a = super::create_test_user(&app).await;
    let token_b = super::create_test_user(&app).await;
    let (user_b, _device_b) = whoami(&app, &token_b).await;
    let room_id = create_room(&app, &token_a).await;
    invite_user_to_room(&app, &token_a, &room_id, &user_b).await;
    join_room(&app, &token_b, &room_id).await;

    let conn_id = "conn-e2ee-leave-rejoin";
    let (first_status, first_body) = post_sliding_sync(
        &app,
        Some(&token_a),
        json!({
            "conn_id": conn_id,
            "lists": {},
            "extensions": {
                "e2ee": { "enabled": true }
            }
        }),
    )
    .await;
    assert_eq!(first_status, StatusCode::OK, "{:?}", first_body);

    leave_room(&app, &token_b, &room_id).await;
    invite_user_to_room(&app, &token_a, &room_id, &user_b).await;
    join_room(&app, &token_b, &room_id).await;

    let (second_status, second_body) = post_sliding_sync(
        &app,
        Some(&token_a),
        json!({
            "conn_id": conn_id,
            "pos": first_body["pos"].as_str().unwrap(),
            "lists": {},
            "extensions": {
                "e2ee": { "enabled": true }
            }
        }),
    )
    .await;
    assert_eq!(second_status, StatusCode::OK, "{:?}", second_body);

    let changed = second_body["extensions"]["e2ee"]["device_lists"]["changed"].as_array().unwrap();
    let left = second_body["extensions"]["e2ee"]["device_lists"]["left"].as_array().unwrap();

    assert!(
        !changed.iter().any(|entry| entry == &json!(user_b)),
        "leave then rejoin should not masquerade as device change in sliding-sync: {}",
        second_body
    );
    assert!(
        !left.iter().any(|entry| entry == &json!(user_b)),
        "user who re-shares before the next sliding-sync request must not remain in device_lists.left: {}",
        second_body
    );
}

#[tokio::test]
async fn test_sliding_sync_extensions_e2ee_kick_reports_left_for_kicked_shared_user() {
    let Some((app, _pool)) = setup_isolated_test_app_with_pool().await else {
        eprintln!("Skipping test: database not available");
        return;
    };

    let token_a = super::create_test_user(&app).await;
    let token_b = super::create_test_user(&app).await;
    let (user_b, _device_b) = whoami(&app, &token_b).await;
    let room_id = create_room(&app, &token_a).await;
    invite_user_to_room(&app, &token_a, &room_id, &user_b).await;
    join_room(&app, &token_b, &room_id).await;

    let conn_id = "conn-e2ee-kick";
    let (first_status, first_body) = post_sliding_sync(
        &app,
        Some(&token_a),
        json!({
            "conn_id": conn_id,
            "lists": {},
            "extensions": {
                "e2ee": { "enabled": true }
            }
        }),
    )
    .await;
    assert_eq!(first_status, StatusCode::OK, "{:?}", first_body);

    kick_user_from_room(&app, &token_a, &room_id, &user_b, "moderation kick").await;

    let (second_status, second_body) = post_sliding_sync(
        &app,
        Some(&token_a),
        json!({
            "conn_id": conn_id,
            "pos": first_body["pos"].as_str().unwrap(),
            "lists": {},
            "extensions": {
                "e2ee": { "enabled": true }
            }
        }),
    )
    .await;
    assert_eq!(second_status, StatusCode::OK, "{:?}", second_body);

    let changed = second_body["extensions"]["e2ee"]["device_lists"]["changed"].as_array().unwrap();
    let left = second_body["extensions"]["e2ee"]["device_lists"]["left"].as_array().unwrap();

    assert!(
        !changed.iter().any(|entry| entry == &json!(user_b)),
        "kick-driven unshare should not masquerade as device change in sliding-sync: {}",
        second_body
    );
    assert!(
        left.iter().any(|entry| entry == &json!(user_b)),
        "kicked shared user should appear in sliding-sync device_lists.left: {}",
        second_body
    );
}

#[tokio::test]
async fn test_sliding_sync_extensions_e2ee_ban_reports_left_for_banned_shared_user() {
    let Some((app, _pool)) = setup_isolated_test_app_with_pool().await else {
        eprintln!("Skipping test: database not available");
        return;
    };

    let token_a = super::create_test_user(&app).await;
    let token_b = super::create_test_user(&app).await;
    let (user_b, _device_b) = whoami(&app, &token_b).await;
    let room_id = create_room(&app, &token_a).await;
    invite_user_to_room(&app, &token_a, &room_id, &user_b).await;
    join_room(&app, &token_b, &room_id).await;

    let conn_id = "conn-e2ee-ban";
    let (first_status, first_body) = post_sliding_sync(
        &app,
        Some(&token_a),
        json!({
            "conn_id": conn_id,
            "lists": {},
            "extensions": {
                "e2ee": { "enabled": true }
            }
        }),
    )
    .await;
    assert_eq!(first_status, StatusCode::OK, "{:?}", first_body);

    ban_user_from_room(&app, &token_a, &room_id, &user_b, "moderation ban").await;

    let (second_status, second_body) = post_sliding_sync(
        &app,
        Some(&token_a),
        json!({
            "conn_id": conn_id,
            "pos": first_body["pos"].as_str().unwrap(),
            "lists": {},
            "extensions": {
                "e2ee": { "enabled": true }
            }
        }),
    )
    .await;
    assert_eq!(second_status, StatusCode::OK, "{:?}", second_body);

    let changed = second_body["extensions"]["e2ee"]["device_lists"]["changed"].as_array().unwrap();
    let left = second_body["extensions"]["e2ee"]["device_lists"]["left"].as_array().unwrap();

    assert!(
        !changed.iter().any(|entry| entry == &json!(user_b)),
        "ban-driven unshare should not masquerade as device change in sliding-sync: {}",
        second_body
    );
    assert!(
        left.iter().any(|entry| entry == &json!(user_b)),
        "banned shared user should appear in sliding-sync device_lists.left: {}",
        second_body
    );
}

#[tokio::test]
async fn test_sliding_sync_extensions_e2ee_unban_does_not_repeat_left_for_already_unshared_user() {
    let Some((app, _pool)) = setup_isolated_test_app_with_pool().await else {
        eprintln!("Skipping test: database not available");
        return;
    };

    let token_a = super::create_test_user(&app).await;
    let token_b = super::create_test_user(&app).await;
    let (user_b, _device_b) = whoami(&app, &token_b).await;
    let room_id = create_room(&app, &token_a).await;
    invite_user_to_room(&app, &token_a, &room_id, &user_b).await;
    join_room(&app, &token_b, &room_id).await;

    let conn_id = "conn-e2ee-unban";
    let (first_status, first_body) = post_sliding_sync(
        &app,
        Some(&token_a),
        json!({
            "conn_id": conn_id,
            "lists": {},
            "extensions": {
                "e2ee": { "enabled": true }
            }
        }),
    )
    .await;
    assert_eq!(first_status, StatusCode::OK, "{:?}", first_body);

    ban_user_from_room(&app, &token_a, &room_id, &user_b, "moderation ban").await;

    let (second_status, second_body) = post_sliding_sync(
        &app,
        Some(&token_a),
        json!({
            "conn_id": conn_id,
            "pos": first_body["pos"].as_str().unwrap(),
            "lists": {},
            "extensions": {
                "e2ee": { "enabled": true }
            }
        }),
    )
    .await;
    assert_eq!(second_status, StatusCode::OK, "{:?}", second_body);
    let ban_left = second_body["extensions"]["e2ee"]["device_lists"]["left"].as_array().unwrap();
    assert!(
        ban_left.iter().any(|entry| entry == &json!(user_b)),
        "ban should first report the shared user in sliding-sync device_lists.left: {}",
        second_body
    );

    unban_user_from_room(&app, &token_a, &room_id, &user_b).await;

    let (third_status, third_body) = post_sliding_sync(
        &app,
        Some(&token_a),
        json!({
            "conn_id": conn_id,
            "pos": second_body["pos"].as_str().unwrap(),
            "lists": {},
            "extensions": {
                "e2ee": { "enabled": true }
            }
        }),
    )
    .await;
    assert_eq!(third_status, StatusCode::OK, "{:?}", third_body);

    let changed = third_body["extensions"]["e2ee"]["device_lists"]["changed"].as_array().unwrap();
    let left = third_body["extensions"]["e2ee"]["device_lists"]["left"].as_array().unwrap();

    assert!(
        !changed.iter().any(|entry| entry == &json!(user_b)),
        "unban should not masquerade as device change in sliding-sync: {}",
        third_body
    );
    assert!(
        !left.iter().any(|entry| entry == &json!(user_b)),
        "unban after a prior ban must not repeat sliding-sync device_lists.left for an already-unshared user: {}",
        third_body
    );
}

#[tokio::test]
async fn test_sliding_sync_extensions_e2ee_forget_does_not_repeat_left_for_already_unshared_user() {
    let Some((app, _pool)) = setup_isolated_test_app_with_pool().await else {
        eprintln!("Skipping test: database not available");
        return;
    };

    let token_a = super::create_test_user(&app).await;
    let token_b = super::create_test_user(&app).await;
    let (user_b, _device_b) = whoami(&app, &token_b).await;
    let room_id = create_room(&app, &token_a).await;
    invite_user_to_room(&app, &token_a, &room_id, &user_b).await;
    join_room(&app, &token_b, &room_id).await;

    let conn_id = "conn-e2ee-forget";
    let (first_status, first_body) = post_sliding_sync(
        &app,
        Some(&token_a),
        json!({
            "conn_id": conn_id,
            "lists": {},
            "extensions": {
                "e2ee": { "enabled": true }
            }
        }),
    )
    .await;
    assert_eq!(first_status, StatusCode::OK, "{:?}", first_body);

    leave_room(&app, &token_a, &room_id).await;

    let (second_status, second_body) = post_sliding_sync(
        &app,
        Some(&token_a),
        json!({
            "conn_id": conn_id,
            "pos": first_body["pos"].as_str().unwrap(),
            "lists": {},
            "extensions": {
                "e2ee": { "enabled": true }
            }
        }),
    )
    .await;
    assert_eq!(second_status, StatusCode::OK, "{:?}", second_body);
    let leave_left = second_body["extensions"]["e2ee"]["device_lists"]["left"].as_array().unwrap();
    assert!(
        leave_left.iter().any(|entry| entry == &json!(user_b)),
        "leaving the last shared room should first report the peer in sliding-sync device_lists.left: {}",
        second_body
    );

    forget_room(&app, &token_a, &room_id).await;

    let (third_status, third_body) = post_sliding_sync(
        &app,
        Some(&token_a),
        json!({
            "conn_id": conn_id,
            "pos": second_body["pos"].as_str().unwrap(),
            "lists": {},
            "extensions": {
                "e2ee": { "enabled": true }
            }
        }),
    )
    .await;
    assert_eq!(third_status, StatusCode::OK, "{:?}", third_body);

    let changed = third_body["extensions"]["e2ee"]["device_lists"]["changed"].as_array().unwrap();
    let left = third_body["extensions"]["e2ee"]["device_lists"]["left"].as_array().unwrap();

    assert!(
        !changed.iter().any(|entry| entry == &json!(user_b)),
        "forget should not masquerade as device change in sliding-sync: {}",
        third_body
    );
    assert!(
        !left.iter().any(|entry| entry == &json!(user_b)),
        "forget after a prior leave must not repeat sliding-sync device_lists.left for an already-unshared user: {}",
        third_body
    );
}

#[tokio::test]
async fn test_sliding_sync_extensions_e2ee_knock_does_not_leak_non_shared_user() {
    let Some((app, _pool)) = setup_isolated_test_app_with_pool().await else {
        eprintln!("Skipping test: database not available");
        return;
    };

    let token_a = super::create_test_user(&app).await;
    let token_b = super::create_test_user(&app).await;
    let (user_b, _device_b) = whoami(&app, &token_b).await;
    let room_id = create_room(&app, &token_a).await;
    set_room_join_rule(&app, &token_a, &room_id, "knock").await;

    let conn_id = "conn-e2ee-knock";
    let (first_status, first_body) = post_sliding_sync(
        &app,
        Some(&token_a),
        json!({
            "conn_id": conn_id,
            "lists": {},
            "extensions": {
                "e2ee": { "enabled": true }
            }
        }),
    )
    .await;
    assert_eq!(first_status, StatusCode::OK, "{:?}", first_body);

    knock_room(&app, &token_b, &room_id, "let me in").await;

    let (second_status, second_body) = post_sliding_sync(
        &app,
        Some(&token_a),
        json!({
            "conn_id": conn_id,
            "pos": first_body["pos"].as_str().unwrap(),
            "lists": {},
            "extensions": {
                "e2ee": { "enabled": true }
            }
        }),
    )
    .await;
    assert_eq!(second_status, StatusCode::OK, "{:?}", second_body);

    let changed = second_body["extensions"]["e2ee"]["device_lists"]["changed"].as_array().unwrap();
    let left = second_body["extensions"]["e2ee"]["device_lists"]["left"].as_array().unwrap();

    assert!(
        !changed.iter().any(|entry| entry == &json!(user_b)),
        "knock by a never-shared user must not appear in sliding-sync device_lists.changed: {}",
        second_body
    );
    assert!(
        !left.iter().any(|entry| entry == &json!(user_b)),
        "knock by a never-shared user must not appear in sliding-sync device_lists.left: {}",
        second_body
    );
}

#[tokio::test]
async fn test_sliding_sync_extensions_e2ee_invite_retract_via_kick_does_not_report_left() {
    let Some((app, _pool)) = setup_isolated_test_app_with_pool().await else {
        eprintln!("Skipping test: database not available");
        return;
    };

    let token_a = super::create_test_user(&app).await;
    let token_b = super::create_test_user(&app).await;
    let (user_b, _device_b) = whoami(&app, &token_b).await;
    let room_id = create_room(&app, &token_a).await;
    invite_user_to_room(&app, &token_a, &room_id, &user_b).await;

    let conn_id = "conn-e2ee-invite-retract";
    let (first_status, first_body) = post_sliding_sync(
        &app,
        Some(&token_a),
        json!({
            "conn_id": conn_id,
            "lists": {},
            "extensions": {
                "e2ee": { "enabled": true }
            }
        }),
    )
    .await;
    assert_eq!(first_status, StatusCode::OK, "{:?}", first_body);

    kick_user_from_room(&app, &token_a, &room_id, &user_b, "invite retracted").await;

    let (second_status, second_body) = post_sliding_sync(
        &app,
        Some(&token_a),
        json!({
            "conn_id": conn_id,
            "pos": first_body["pos"].as_str().unwrap(),
            "lists": {},
            "extensions": {
                "e2ee": { "enabled": true }
            }
        }),
    )
    .await;
    assert_eq!(second_status, StatusCode::OK, "{:?}", second_body);

    let changed = second_body["extensions"]["e2ee"]["device_lists"]["changed"].as_array().unwrap();
    let left = second_body["extensions"]["e2ee"]["device_lists"]["left"].as_array().unwrap();

    assert!(
        !changed.iter().any(|entry| entry == &json!(user_b)),
        "invite retract via kick must not masquerade as device change in sliding-sync: {}",
        second_body
    );
    assert!(
        !left.iter().any(|entry| entry == &json!(user_b)),
        "invite retract via kick must not surface as shared-user loss in sliding-sync device_lists.left: {}",
        second_body
    );
}

#[tokio::test]
async fn test_sliding_sync_extensions_to_device_returns_events_and_next_batch() {
    let Some((app, _pool)) = setup_test_app_with_pool().await else {
        eprintln!("Skipping test: database not available");
        return;
    };

    let token_a = super::create_test_user(&app).await;
    let token_b = super::create_test_user(&app).await;
    let (user_a, device_a) = whoami(&app, &token_a).await;
    let (_user_b, _device_b) = whoami(&app, &token_b).await;
    let device_a = device_a.unwrap_or_else(|| "default".to_string());

    send_to_device_message(
        &app,
        &token_b,
        "org.example.test",
        "txn-to-device-1",
        &user_a,
        &device_a,
        json!({ "body": "hello-to-device" }),
    )
    .await;

    let (first_status, first_body) = post_sliding_sync(
        &app,
        Some(&token_a),
        json!({
            "lists": {},
            "extensions": {
                "to_device": {
                    "enabled": true,
                    "limit": 10
                }
            }
        }),
    )
    .await;
    assert_eq!(first_status, StatusCode::OK, "{:?}", first_body);
    assert_eq!(first_body["extensions"]["to_device"]["events"][0]["type"], "org.example.test");
    assert_eq!(first_body["extensions"]["to_device"]["events"][0]["content"]["body"], "hello-to-device");
    let next_batch = first_body["extensions"]["to_device"]["next_batch"].as_str().unwrap().to_string();

    let (second_status, second_body) = post_sliding_sync(
        &app,
        Some(&token_a),
        json!({
            "lists": {},
            "extensions": {
                "to_device": {
                    "enabled": true,
                    "since": next_batch,
                    "limit": 10
                }
            }
        }),
    )
    .await;
    assert_eq!(second_status, StatusCode::OK, "{:?}", second_body);
    assert_eq!(second_body["extensions"]["to_device"]["events"], json!([]));
    assert!(second_body["extensions"]["to_device"]["next_batch"].is_string());
}

#[tokio::test]
async fn test_sliding_sync_extensions_account_data_returns_global_data() {
    let Some((app, _pool)) = setup_test_app_with_pool().await else {
        eprintln!("Skipping test: database not available");
        return;
    };

    let token = super::create_test_user(&app).await;
    let (user_id, _) = whoami(&app, &token).await;
    put_global_account_data(&app, &token, &user_id, "org.example.test_settings", json!({ "theme": "dark" })).await;

    let (status, body) = post_sliding_sync(
        &app,
        Some(&token),
        json!({
            "lists": {},
            "extensions": {
                "account_data": { "enabled": true }
            }
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{:?}", body);
    assert_eq!(body["extensions"]["account_data"]["global"]["org.example.test_settings"]["theme"], "dark");
}

#[tokio::test]
async fn test_sliding_sync_extensions_receipts_returns_room_receipts() {
    let Some((app, pool)) = setup_test_app_with_pool().await else {
        eprintln!("Skipping test: database not available");
        return;
    };

    let token = super::create_test_user(&app).await;
    let (user_id, device_id) = whoami(&app, &token).await;
    let device_id = device_id.unwrap_or_else(|| "default".to_string());
    let room_id = create_room(&app, &token).await;
    let event_id = send_room_message(&app, &token, &room_id).await;
    let (receipt_status, receipt_body) = send_read_receipt(&app, &token, &room_id, &event_id).await;
    assert_eq!(receipt_status, StatusCode::OK, "{:?}", receipt_body);

    let storage = SlidingSyncStorage::new(pool.clone());
    storage
        .upsert_room(
            &user_id,
            &device_id,
            &room_id,
            None,
            None,
            chrono::Utc::now().timestamp_millis(),
            0,
            0,
            false,
            false,
            false,
            false,
            Some("Room"),
            None,
            chrono::Utc::now().timestamp_millis(),
        )
        .await
        .unwrap();

    let mut room_subscriptions = serde_json::Map::new();
    room_subscriptions.insert(room_id.clone(), json!({}));
    let (status, body) = post_sliding_sync(
        &app,
        Some(&token),
        json!({
            "lists": {},
            "room_subscriptions": room_subscriptions,
            "extensions": {
                "receipts": { "enabled": true }
            }
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{:?}", body);
    assert!(body["extensions"]["receipts"]["rooms"][&room_id]["m.read"][&event_id].get(&user_id).is_some());
}

#[tokio::test]
async fn test_sliding_sync_extensions_typing_returns_room_typing_users() {
    let Some((app, pool)) = setup_test_app_with_pool().await else {
        eprintln!("Skipping test: database not available");
        return;
    };

    let token = super::create_test_user(&app).await;
    let (user_id, device_id) = whoami(&app, &token).await;
    let device_id = device_id.unwrap_or_else(|| "default".to_string());
    let room_id = create_room(&app, &token).await;

    let (typing_status, typing_body) = set_typing(&app, &token, &room_id, &user_id, true).await;
    assert_eq!(typing_status, StatusCode::OK, "{:?}", typing_body);

    let storage = SlidingSyncStorage::new(pool.clone());
    storage
        .upsert_room(
            &user_id,
            &device_id,
            &room_id,
            None,
            None,
            chrono::Utc::now().timestamp_millis(),
            0,
            0,
            false,
            false,
            false,
            false,
            Some("Room"),
            None,
            chrono::Utc::now().timestamp_millis(),
        )
        .await
        .unwrap();

    let mut room_subscriptions = serde_json::Map::new();
    room_subscriptions.insert(room_id.clone(), json!({}));
    let (status, body) = post_sliding_sync(
        &app,
        Some(&token),
        json!({
            "lists": {},
            "room_subscriptions": room_subscriptions,
            "extensions": {
                "typing": { "enabled": true }
            }
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{:?}", body);
    let user_ids = body["extensions"]["typing"]["rooms"][&room_id]["user_ids"].as_array().cloned().unwrap_or_default();
    assert!(user_ids.iter().any(|v| v == &json!(user_id)));
}

#[tokio::test]
async fn test_sliding_sync_beacon_room_subscription_materializes_room_snapshot() {
    let Some((app, _pool)) = setup_test_app_with_pool().await else {
        eprintln!("Skipping test: database not available");
        return;
    };

    let token = super::create_test_user(&app).await;
    let (user_id, _) = whoami(&app, &token).await;
    let room_id = create_room(&app, &token).await;
    let beacon_info_event_id = put_beacon_info(&app, &token, &room_id, &user_id).await;
    let (beacon_status, beacon_body) = send_beacon(&app, &token, &room_id, &beacon_info_event_id).await;
    assert_eq!(beacon_status, StatusCode::OK, "{:?}", beacon_body);

    let mut room_subscriptions = serde_json::Map::new();
    room_subscriptions.insert(room_id.clone(), json!({}));
    let (status, body) = post_sliding_sync(
        &app,
        Some(&token),
        json!({
            "lists": {},
            "room_subscriptions": room_subscriptions
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{:?}", body);
    assert!(body["rooms"].get(&room_id).is_some());
}

#[tokio::test]
async fn test_traditional_get_sync_coexists_with_post_sliding_sync() {
    let Some((app, _pool)) = setup_test_app_with_pool().await else {
        eprintln!("Skipping test: database not available");
        return;
    };

    let token = super::create_test_user(&app).await;

    let (status_post, _post_body) = post_sliding_sync(&app, Some(&token), json!({ "lists": {} })).await;
    assert_eq!(status_post, StatusCode::OK);

    let (status_get, body_get) = get_sync(&app, &token, "/_matrix/client/v3/sync?timeout=1").await;
    assert_eq!(status_get, StatusCode::OK, "{:?}", body_get);
    assert!(body_get.get("next_batch").is_some());
}

#[tokio::test]
async fn test_sliding_sync_pos_is_consistent_across_worker_instances() {
    let Some(((app_a, app_b), _pool)) = setup_two_test_apps_with_shared_pool().await else {
        eprintln!("Skipping test: database not available");
        return;
    };

    let token = super::create_test_user(&app_a).await;

    let (status_1, body_1) = post_sliding_sync(&app_a, Some(&token), json!({ "lists": {} })).await;
    assert_eq!(status_1, StatusCode::OK, "{:?}", body_1);
    let pos_1 = body_1["pos"].as_str().unwrap().to_string();
    assert!(!pos_1.is_empty());

    let (status_2, body_2) = post_sliding_sync(
        &app_b,
        Some(&token),
        json!({
            "lists": {},
            "pos": pos_1.clone()
        }),
    )
    .await;
    assert_eq!(status_2, StatusCode::OK, "{:?}", body_2);
    let pos_2 = body_2["pos"].as_str().unwrap().to_string();
    assert!(!pos_2.is_empty());
    assert_ne!(pos_2, pos_1);

    // Old pos must be rejected even when request lands on a different worker instance.
    let (status_3, body_3) = post_sliding_sync(
        &app_a,
        Some(&token),
        json!({
            "lists": {},
            "pos": pos_1
        }),
    )
    .await;
    assert_eq!(status_3, StatusCode::BAD_REQUEST, "{:?}", body_3);
}

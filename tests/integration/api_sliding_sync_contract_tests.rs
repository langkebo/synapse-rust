use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use std::sync::Arc;
use synapse_rust::cache::CacheManager;
use synapse_rust::services::ServiceContainer;
use synapse_rust::storage::sliding_sync::SlidingSyncStorage;
use synapse_rust::web::routes::state::AppState;
use tower::ServiceExt;

async fn setup_test_app_with_pool() -> Option<(axum::Router, Arc<sqlx::PgPool>)> {
    let pool = super::get_test_pool().await?;
    let container = ServiceContainer::new_test_with_pool(pool.clone());
    let cache = Arc::new(CacheManager::new(Default::default()));
    let state = AppState::new(container, cache);
    Some((synapse_rust::web::create_router(state), pool))
}

async fn setup_two_test_apps_with_shared_pool(
) -> Option<((axum::Router, axum::Router), Arc<sqlx::PgPool>)> {
    let pool = super::get_test_pool().await?;

    let container_a = ServiceContainer::new_test_with_pool(pool.clone());
    let cache_a = Arc::new(CacheManager::new(Default::default()));
    let state_a = AppState::new(container_a, cache_a);
    let app_a = synapse_rust::web::create_router(state_a);

    let container_b = ServiceContainer::new_test_with_pool(pool.clone());
    let cache_b = Arc::new(CacheManager::new(Default::default()));
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

    let response = app
        .clone()
        .oneshot(super::with_local_connect_info(request))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let user_id = json["user_id"].as_str().unwrap().to_string();
    let device_id = json
        .get("device_id")
        .and_then(|v| v.as_str())
        .map(|v| v.to_string());
    (user_id, device_id)
}

async fn create_room(app: &axum::Router, token: &str) -> String {
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/createRoom")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({ "name": "Sliding Sync Room" }).to_string(),
        ))
        .unwrap();

    let response = app
        .clone()
        .oneshot(super::with_local_connect_info(request))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    json["room_id"].as_str().unwrap().to_string()
}

async fn put_global_account_data(
    app: &axum::Router,
    token: &str,
    user_id: &str,
    data_type: &str,
    content: Value,
) {
    let request = Request::builder()
        .method("PUT")
        .uri(format!(
            "/_matrix/client/v3/user/{}/account_data/{}",
            user_id, data_type
        ))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(content.to_string()))
        .unwrap();

    let response = app
        .clone()
        .oneshot(super::with_local_connect_info(request))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

async fn send_room_message(app: &axum::Router, token: &str, room_id: &str) -> String {
    let request = Request::builder()
        .method("PUT")
        .uri(format!(
            "/_matrix/client/v3/rooms/{}/send/m.room.message/{}",
            room_id,
            rand::random::<u32>()
        ))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({ "msgtype": "m.text", "body": "hello" }).to_string(),
        ))
        .unwrap();

    let response = app
        .clone()
        .oneshot(super::with_local_connect_info(request))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    json["event_id"].as_str().unwrap().to_string()
}

async fn upload_device_keys(
    app: &axum::Router,
    token: &str,
    user_id: &str,
    device_id: &str,
    key_suffix: &str,
) {
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
                        format!("curve25519:{}", device_id): format!("curve25519-{}", key_suffix),
                        format!("ed25519:{}", device_id): format!("ed25519-{}", key_suffix)
                    },
                    "signatures": {
                        user_id: {
                            format!("ed25519:{}", device_id): format!("signature-{}", key_suffix)
                        }
                    }
                },
                "one_time_keys": {
                    format!("signed_curve25519:{}", key_suffix): {
                        "key": format!("otk-{}", key_suffix)
                    }
                }
            })
            .to_string(),
        ))
        .unwrap();

    let response = app
        .clone()
        .oneshot(super::with_local_connect_info(request))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
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
        .uri(format!(
            "/_matrix/client/v3/sendToDevice/{}/{}",
            event_type, txn_id
        ))
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

    let response = app
        .clone()
        .oneshot(super::with_local_connect_info(request))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

async fn send_read_receipt(
    app: &axum::Router,
    token: &str,
    room_id: &str,
    event_id: &str,
) -> (StatusCode, Value) {
    let request = Request::builder()
        .method("POST")
        .uri(format!(
            "/_matrix/client/v3/rooms/{}/receipt/m.read/{}",
            room_id, event_id
        ))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({}).to_string()))
        .unwrap();

    let response = app
        .clone()
        .oneshot(super::with_local_connect_info(request))
        .await
        .unwrap();
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .unwrap();
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
        .uri(format!(
            "/_matrix/client/v3/rooms/{}/typing/{}",
            room_id, user_id
        ))
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

    let response = app
        .clone()
        .oneshot(super::with_local_connect_info(request))
        .await
        .unwrap();
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap_or_else(|_| json!({}));
    (status, json)
}

async fn put_beacon_info(
    app: &axum::Router,
    token: &str,
    room_id: &str,
    state_key: &str,
) -> String {
    let request = Request::builder()
        .method("PUT")
        .uri(format!(
            "/_matrix/client/v3/rooms/{}/state/m.beacon_info/{}",
            room_id, state_key
        ))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "m.beacon_info": {
                    "description": "Beacon in sliding sync",
                    "timeout": 60000,
                    "live": true
                },
                "m.ts": chrono::Utc::now().timestamp_millis(),
                "m.asset": { "type": "m.self" }
            })
            .to_string(),
        ))
        .unwrap();

    let response = app
        .clone()
        .oneshot(super::with_local_connect_info(request))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    json["event_id"].as_str().unwrap().to_string()
}

async fn send_beacon(
    app: &axum::Router,
    token: &str,
    room_id: &str,
    beacon_info_id: &str,
) -> (StatusCode, Value) {
    let request = Request::builder()
        .method("PUT")
        .uri(format!(
            "/_matrix/client/v3/rooms/{}/send/m.beacon/{}",
            room_id,
            rand::random::<u32>()
        ))
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

    let response = app
        .clone()
        .oneshot(super::with_local_connect_info(request))
        .await
        .unwrap();
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap_or_else(|_| json!({}));
    (status, json)
}

async fn post_sliding_sync(
    app: &axum::Router,
    token: Option<&str>,
    body: Value,
) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/sync")
        .header("Content-Type", "application/json");

    if let Some(token) = token {
        builder = builder.header("Authorization", format!("Bearer {}", token));
    }

    let request = builder.body(Body::from(body.to_string())).unwrap();

    let response = app
        .clone()
        .oneshot(super::with_local_connect_info(request))
        .await
        .unwrap();

    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();

    let json = if body.is_empty() {
        json!({})
    } else {
        serde_json::from_slice(&body)
            .unwrap_or_else(|_| json!({ "raw": String::from_utf8_lossy(&body) }))
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

    let response = app
        .clone()
        .oneshot(super::with_local_connect_info(request))
        .await
        .unwrap();

    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let json = if body.is_empty() {
        json!({})
    } else {
        serde_json::from_slice(&body)
            .unwrap_or_else(|_| json!({ "raw": String::from_utf8_lossy(&body) }))
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
    let Some((app, _pool)) = setup_test_app_with_pool().await else {
        eprintln!("Skipping test: database not available");
        return;
    };

    let token = super::create_test_user(&app).await;
    let mut limited_body: Option<Value> = None;

    for _ in 0..40 {
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

    let storage = SlidingSyncStorage::new(pool.clone());
    storage
        .upsert_room(
            &user_id,
            &device_id,
            &room_a,
            None,
            Some("main"),
            3000,
            0,
            0,
            false,
            false,
            false,
            false,
            Some("A"),
            None,
            3000,
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
            2000,
            0,
            0,
            false,
            false,
            false,
            false,
            Some("B"),
            None,
            2000,
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
            1000,
            0,
            0,
            false,
            false,
            false,
            false,
            Some("C"),
            None,
            1000,
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
    assert_eq!(
        body["lists"]["main"]["ops"][0]["room_ids"],
        json!([room_a, room_b])
    );

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

    let storage = SlidingSyncStorage::new(pool.clone());
    storage
        .upsert_room(
            &user_id,
            &device_id,
            &room_dm,
            None,
            Some("main"),
            2000,
            0,
            0,
            true,
            false,
            false,
            false,
            Some("DM Room"),
            None,
            2000,
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
            1000,
            0,
            0,
            false,
            false,
            false,
            false,
            Some("Group Room"),
            None,
            1000,
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
    assert_eq!(
        body["lists"]["main"]["ops"][0]["room_ids"],
        json!([room_dm])
    );
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
    let room_c = create_room(&app, &token).await;

    let storage = SlidingSyncStorage::new(pool.clone());
    storage
        .upsert_room(
            &user_id,
            &device_id,
            &room_a,
            Some(conn_id),
            Some("main"),
            2000,
            0,
            0,
            false,
            false,
            false,
            false,
            Some("A"),
            None,
            2000,
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
            1000,
            0,
            0,
            false,
            false,
            false,
            false,
            Some("B"),
            None,
            1000,
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

    storage
        .upsert_room(
            &user_id,
            &device_id,
            &room_c,
            Some(conn_id),
            Some("main"),
            3000,
            0,
            0,
            false,
            false,
            false,
            false,
            Some("C"),
            None,
            3000,
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
    assert!(ops.iter().any(|op| op["op"] == "INSERT"));
    assert!(ops.iter().any(|op| op["op"] == "DELETE"));
}

#[tokio::test]
async fn test_sliding_sync_extensions_e2ee_returns_key_counts_and_device_list_deltas() {
    let Some((app, _pool)) = setup_test_app_with_pool().await else {
        eprintln!("Skipping test: database not available");
        return;
    };

    let token_a = super::create_test_user(&app).await;
    let token_b = super::create_test_user(&app).await;
    let (user_a, device_a) = whoami(&app, &token_a).await;
    let (user_b, device_b) = whoami(&app, &token_b).await;
    let device_a = device_a.unwrap_or_else(|| "default".to_string());
    let device_b = device_b.unwrap_or_else(|| "default".to_string());

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
    assert_eq!(
        first_body["extensions"]["e2ee"]["device_one_time_keys_count"]["signed_curve25519"],
        1
    );
    assert_eq!(
        first_body["extensions"]["e2ee"]["device_unused_fallback_key_types"],
        json!([])
    );

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
    let changed = second_body["extensions"]["e2ee"]["device_lists"]["changed"]
        .as_array()
        .unwrap();
    assert!(changed.iter().any(|entry| entry == &json!(user_b)));
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
    assert_eq!(
        first_body["extensions"]["to_device"]["events"][0]["type"],
        "org.example.test"
    );
    assert_eq!(
        first_body["extensions"]["to_device"]["events"][0]["content"]["body"],
        "hello-to-device"
    );
    let next_batch = first_body["extensions"]["to_device"]["next_batch"]
        .as_str()
        .unwrap()
        .to_string();

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
    put_global_account_data(
        &app,
        &token,
        &user_id,
        "org.example.test_settings",
        json!({ "theme": "dark" }),
    )
    .await;

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
    assert_eq!(
        body["extensions"]["account_data"]["global"]["org.example.test_settings"]["theme"],
        "dark"
    );
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
    assert!(
        body["extensions"]["receipts"]["rooms"][&room_id]["m.read"][&event_id]
            .get(&user_id)
            .is_some()
    );
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
    let user_ids = body["extensions"]["typing"]["rooms"][&room_id]["user_ids"]
        .as_array()
        .cloned()
        .unwrap_or_default();
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
    let (beacon_status, beacon_body) =
        send_beacon(&app, &token, &room_id, &beacon_info_event_id).await;
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

    let (status_post, _post_body) =
        post_sliding_sync(&app, Some(&token), json!({ "lists": {} })).await;
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

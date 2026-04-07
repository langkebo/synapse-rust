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

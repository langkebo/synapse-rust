use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use std::sync::Arc;
use synapse_common::current_timestamp_millis;
use tower::ServiceExt;

async fn setup_test_app_with_pool() -> Option<(axum::Router, Arc<sqlx::PgPool>)> {
    let (app, pool, _) = super::setup_fresh_test_app_with_pool().await?;
    Some((app, pool))
}

async fn whoami(app: &axum::Router, token: &str) -> String {
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
    json["user_id"].as_str().unwrap().to_string()
}

async fn create_room(app: &axum::Router, token: &str) -> String {
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/createRoom")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "name": "Beacon Room" }).to_string()))
        .unwrap();

    let response = app.clone().oneshot(super::with_local_connect_info(request)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    json["room_id"].as_str().unwrap().to_string()
}

async fn invite_and_join_room(
    app: &axum::Router,
    owner_token: &str,
    room_id: &str,
    invitee_token: &str,
    invitee_user_id: &str,
) {
    let invite_req = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/v3/rooms/{}/invite", room_id))
        .header("Authorization", format!("Bearer {}", owner_token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "user_id": invitee_user_id }).to_string()))
        .unwrap();
    let invite_resp = app.clone().oneshot(super::with_local_connect_info(invite_req)).await.unwrap();
    assert_eq!(invite_resp.status(), StatusCode::OK);

    let join_req = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/v3/rooms/{}/join", room_id))
        .header("Authorization", format!("Bearer {}", invitee_token))
        .body(Body::empty())
        .unwrap();
    let join_resp = app.clone().oneshot(super::with_local_connect_info(join_req)).await.unwrap();
    assert_eq!(join_resp.status(), StatusCode::OK);
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
                    "description": "Test beacon",
                    "timeout": 3_600_000,
                    "live": true
                },
                "m.ts": current_timestamp_millis(),
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

async fn put_beacon_info_custom(
    app: &axum::Router,
    token: &str,
    room_id: &str,
    state_key: &str,
    live: bool,
    timeout: i64,
    ts: i64,
) -> String {
    let request = Request::builder()
        .method("PUT")
        .uri(format!("/_matrix/client/v3/rooms/{}/state/m.beacon_info/{}", room_id, state_key))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "m.beacon_info": {
                    "description": "Custom beacon",
                    "timeout": timeout,
                    "live": live
                },
                "m.ts": ts,
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

async fn enable_room_e2ee(app: &axum::Router, token: &str, room_id: &str) {
    let request = Request::builder()
        .method("PUT")
        .uri(format!("/_matrix/client/v3/rooms/{}/state/m.room.encryption", room_id))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "algorithm": "m.megolm.v1.aes-sha2"
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.clone().oneshot(super::with_local_connect_info(request)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

async fn send_room_encrypted(app: &axum::Router, token: &str, room_id: &str) -> (StatusCode, Value) {
    let request = Request::builder()
        .method("PUT")
        .uri(format!("/_matrix/client/v3/rooms/{}/send/m.room.encrypted/{}", room_id, rand::random::<u32>()))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "algorithm": "m.megolm.v1.aes-sha2",
                "ciphertext": "ZmFrZS1jaXBoZXI=",
                "session_id": "test-session",
                "device_id": "TESTDEVICE",
                "sender_key": "curve25519:test"
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

async fn send_beacon(app: &axum::Router, token: &str, room_id: &str, beacon_info_id: &str) -> String {
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
                "m.ts": current_timestamp_millis(),
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

async fn send_beacon_with_ts(
    app: &axum::Router,
    token: &str,
    room_id: &str,
    beacon_info_id: &str,
    ts: i64,
) -> (StatusCode, Value) {
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
                "m.ts": ts
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

#[tokio::test]
async fn test_beacon_info_and_beacon_events_are_indexed() {
    let Some((app, pool)) = setup_test_app_with_pool().await else {
        eprintln!("Skipping test: database not available");
        return;
    };

    let token = super::create_test_user(&app).await;
    let user_id = whoami(&app, &token).await;
    let room_id = create_room(&app, &token).await;

    let beacon_info_event_id = put_beacon_info(&app, &token, &room_id, &user_id).await;
    let beacon_event_id = send_beacon(&app, &token, &room_id, &beacon_info_event_id).await;

    let info_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM beacon_info WHERE room_id = $1 AND event_id = $2 AND sender = $3")
            .bind(&room_id)
            .bind(&beacon_info_event_id)
            .bind(&user_id)
            .fetch_one(&*pool)
            .await
            .unwrap();
    assert_eq!(info_count, 1);

    let row: (i64, String, String) = sqlx::query_as(
        "SELECT COUNT(*), MIN(event_id), MIN(beacon_info_id) FROM beacon_locations WHERE room_id = $1 AND sender = $2",
    )
    .bind(&room_id)
    .bind(&user_id)
    .fetch_one(&*pool)
    .await
    .unwrap();
    assert_eq!(row.0, 1);
    assert_eq!(row.1, beacon_event_id);
    assert_eq!(row.2, beacon_info_event_id);
}

#[tokio::test]
async fn test_beacon_location_is_rate_limited_to_1hz_per_beacon() {
    let Some((app, _pool)) = setup_test_app_with_pool().await else {
        eprintln!("Skipping test: database not available");
        return;
    };

    let token = super::create_test_user(&app).await;
    let user_id = whoami(&app, &token).await;
    let room_id = create_room(&app, &token).await;

    let beacon_info_event_id = put_beacon_info(&app, &token, &room_id, &user_id).await;

    let ts_1 = 100_000;
    let (status_1, body_1) = send_beacon_with_ts(&app, &token, &room_id, &beacon_info_event_id, ts_1).await;
    assert_eq!(status_1, StatusCode::OK, "{:?}", body_1);

    let ts_2 = ts_1 + 500;
    let (status_2, body_2) = send_beacon_with_ts(&app, &token, &room_id, &beacon_info_event_id, ts_2).await;
    assert_eq!(status_2, StatusCode::TOO_MANY_REQUESTS, "{:?}", body_2);
    assert_eq!(body_2["errcode"], "M_LIMIT_EXCEEDED");
    assert!(body_2.get("retry_after_ms").is_some());
}

#[tokio::test]
async fn test_beacon_location_applies_sender_room_quota() {
    let Some((app, _pool)) = setup_test_app_with_pool().await else {
        eprintln!("Skipping test: database not available");
        return;
    };

    let token = super::create_test_user(&app).await;
    let user_id = whoami(&app, &token).await;
    let room_id = create_room(&app, &token).await;

    for i in 0..10 {
        let beacon_info_event_id = put_beacon_info(&app, &token, &room_id, &user_id).await;
        let ts = 200_000 + i * 1_000;
        let (status, body) = send_beacon_with_ts(&app, &token, &room_id, &beacon_info_event_id, ts).await;
        assert_eq!(status, StatusCode::OK, "{:?}", body);
    }

    let beacon_info_event_id = put_beacon_info(&app, &token, &room_id, &user_id).await;
    let (status, body) = send_beacon_with_ts(&app, &token, &room_id, &beacon_info_event_id, 211_000).await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS, "{:?}", body);
    assert_eq!(body["errcode"], "M_LIMIT_EXCEEDED");
    assert!(body.get("retry_after_ms").is_some());
}

#[tokio::test]
async fn test_beacon_location_applies_room_backpressure_token_bucket() {
    let Some((app, _pool)) = setup_test_app_with_pool().await else {
        eprintln!("Skipping test: database not available");
        return;
    };

    // 房间级令牌桶在 `synapse-services/src/beacon_service.rs`：容量 20、每秒回填 5，
    // 且**按墙钟回填**（`CacheManager::rate_limit_token_bucket_take` 取的是
    // `SystemTime::now()`）。所以「顺序发 40 条」能否打出 429 取决于单条请求耗时：
    // 桶耗尽的充要条件是 `n > 19 + 5·t`（t = 整批墙钟秒数），即单条平均耗时必须
    // < 200ms，机器一忙就永远没有 429 —— 这正是本用例历史抖动的真实根因；而改成
    // 40 路并发又会撞 `#[tokio::test]` current_thread runtime 下的 30s 服务端超时
    //（实测 504 M_REQUEST_TIMEOUT）。
    //
    // 确定性做法：利用 `build_beacon_location_params` 的判定顺序 ——
    // **房间令牌桶先于 1Hz 判定**，而被 1Hz 判掉的请求（`m.ts` 不递增 ⇒ 400）
    // **不写位置行、也不计入配额**（配额按已插入行计数）。于是一串 `m.ts` 与最新
    // 位置相同的廉价 400 就成了「消费 1 个房间令牌然后被拒」的抽水机，把桶抽干，
    // 再立刻用一条本应合法的探针观测 429。
    //
    // 抽水请求轮流由 6 个参与者发出、每人 ≤15 次（< 桶容量 20）：若实现退化成
    // 「按发送者」限流（每人 20 个令牌），房间桶永远不会空，探针会拿到 200，
    // 用例必须变红 —— 这样才保住「房间级」语义。
    let owner_token = super::create_test_user(&app).await;
    let owner_user_id = whoami(&app, &owner_token).await;
    let room_id = create_room(&app, &owner_token).await;
    let beacon_info_event_id = put_beacon_info(&app, &owner_token, &room_id, &owner_user_id).await;

    let base_ts = current_timestamp_millis();
    let (status, body) = send_beacon_with_ts(&app, &owner_token, &room_id, &beacon_info_event_id, base_ts).await;
    assert_eq!(status, StatusCode::OK, "{:?}", body);

    // 抽水者与探针都必须是房间成员（`m.beacon` 走发言权校验）。
    let mut drainers = Vec::new();
    for _ in 0..6 {
        let token = super::create_test_user(&app).await;
        let user_id = whoami(&app, &token).await;
        invite_and_join_room(&app, &owner_token, &room_id, &token, &user_id).await;
        drainers.push(token);
    }
    let probe_token = super::create_test_user(&app).await;
    let probe_user_id = whoami(&app, &probe_token).await;
    invite_and_join_room(&app, &owner_token, &room_id, &probe_token, &probe_user_id).await;

    let mut drained = 0usize;
    let mut saw_backpressure = false;
    'drain: for _ in 0..15 {
        for token in &drainers {
            let (status, body) = send_beacon_with_ts(&app, token, &room_id, &beacon_info_event_id, base_ts).await;
            match status {
                StatusCode::BAD_REQUEST => {
                    // 期望路径：`ts <= latest.timestamp` ⇒ 400，且不写位置行。
                    let message = body["error"].as_str().unwrap_or_default();
                    assert!(
                        message.contains("timestamp must be increasing"),
                        "抽水请求应被 1Hz 判定拒绝，实际: {:?}",
                        body
                    );
                    drained += 1;
                }
                StatusCode::TOO_MANY_REQUESTS => {
                    // 桶已被抽干 ⇒ 房间级压回生效。
                    assert_eq!(body["errcode"], "M_LIMIT_EXCEEDED", "{:?}", body);
                    assert!(body.get("retry_after_ms").is_some(), "{:?}", body);
                    saw_backpressure = true;
                    break 'drain;
                }
                other => panic!("抽水请求出现非预期状态码 {other}: {body:?}"),
            }
        }
    }

    // 房主已消费 1 个令牌 ⇒ 桶里只剩 19 个；抽干后必然先出现 429。
    assert!(drained >= 19, "抽水次数不足以抽干容量 20 的房间令牌桶（实际 {drained} 次 400）");
    assert!(saw_backpressure, "房间令牌桶抽干后仍未出现 429");

    // 探针：一条本应完全合法的位置（发送者/房间配额均 0 使用、与最新位置间隔 1000ms
    // 满足 1Hz），在桶刚被抽空的瞬间必须被压回。
    let (status, body) =
        send_beacon_with_ts(&app, &probe_token, &room_id, &beacon_info_event_id, base_ts + 1_000).await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS, "{:?}", body);
    assert_eq!(body["errcode"], "M_LIMIT_EXCEEDED", "{:?}", body);
    assert!(body.get("retry_after_ms").is_some(), "{:?}", body);
}

#[tokio::test]
async fn test_beacon_info_state_key_must_match_sender() {
    let Some((app, _pool)) = setup_test_app_with_pool().await else {
        eprintln!("Skipping test: database not available");
        return;
    };

    let token_a = super::create_test_user(&app).await;
    let token_b = super::create_test_user(&app).await;

    let user_a = whoami(&app, &token_a).await;
    let user_b = whoami(&app, &token_b).await;
    let room_id = create_room(&app, &token_a).await;

    let request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/v3/rooms/{}/invite", room_id))
        .header("Authorization", format!("Bearer {}", token_a))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "user_id": user_b }).to_string()))
        .unwrap();
    let response = app.clone().oneshot(super::with_local_connect_info(request)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/v3/rooms/{}/join", room_id))
        .header("Authorization", format!("Bearer {}", token_b))
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(super::with_local_connect_info(request)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let request = Request::builder()
        .method("PUT")
        .uri(format!("/_matrix/client/v3/rooms/{}/state/m.beacon_info/{}", room_id, user_a))
        .header("Authorization", format!("Bearer {}", token_b))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "m.beacon_info": {
                    "timeout": 60_000,
                    "live": true
                },
                "m.ts": current_timestamp_millis(),
                "m.asset": { "type": "m.self" }
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.clone().oneshot(super::with_local_connect_info(request)).await.unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN, "{:?}", response.status());
}

#[tokio::test]
async fn test_beacon_location_rejected_when_beacon_is_not_live() {
    let Some((app, _pool)) = setup_test_app_with_pool().await else {
        eprintln!("Skipping test: database not available");
        return;
    };

    let token = super::create_test_user(&app).await;
    let user_id = whoami(&app, &token).await;
    let room_id = create_room(&app, &token).await;

    let beacon_info_event_id =
        put_beacon_info_custom(&app, &token, &room_id, &user_id, false, 60_000, current_timestamp_millis()).await;

    let (status, body) =
        send_beacon_with_ts(&app, &token, &room_id, &beacon_info_event_id, current_timestamp_millis()).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{:?}", body);
    assert_eq!(body["errcode"], "M_BAD_JSON");
    assert!(body["error"].as_str().unwrap_or_default().contains("not live"));
}

#[tokio::test]
async fn test_beacon_location_rejected_when_beacon_has_expired() {
    let Some((app, _pool)) = setup_test_app_with_pool().await else {
        eprintln!("Skipping test: database not available");
        return;
    };

    let token = super::create_test_user(&app).await;
    let user_id = whoami(&app, &token).await;
    let room_id = create_room(&app, &token).await;

    let old_ts = current_timestamp_millis() - 10_000;
    let beacon_info_event_id = put_beacon_info_custom(&app, &token, &room_id, &user_id, true, 1_000, old_ts).await;

    let (status, body) =
        send_beacon_with_ts(&app, &token, &room_id, &beacon_info_event_id, current_timestamp_millis()).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{:?}", body);
    assert_eq!(body["errcode"], "M_BAD_JSON");
    assert!(body["error"].as_str().unwrap_or_default().contains("expired"));
}

#[tokio::test]
async fn test_beacon_lifecycle_stop_and_restart_rotates_active_beacon() {
    let Some((app, _pool)) = setup_test_app_with_pool().await else {
        eprintln!("Skipping test: database not available");
        return;
    };

    let token = super::create_test_user(&app).await;
    let user_id = whoami(&app, &token).await;
    let room_id = create_room(&app, &token).await;
    let now = current_timestamp_millis();

    let beacon_v1 = put_beacon_info_custom(&app, &token, &room_id, &user_id, true, 60_000, now).await;
    let (status_v1, body_v1) = send_beacon_with_ts(&app, &token, &room_id, &beacon_v1, now + 2000).await;
    assert_eq!(status_v1, StatusCode::OK, "{:?}", body_v1);

    // Stop: new state event with live=false for same state_key should end previous lifecycle.
    let _stop_event = put_beacon_info_custom(&app, &token, &room_id, &user_id, false, 60_000, now + 3000).await;
    let (old_status, old_body) = send_beacon_with_ts(&app, &token, &room_id, &beacon_v1, now + 4000).await;
    assert_eq!(old_status, StatusCode::BAD_REQUEST, "{:?}", old_body);
    assert!(old_body["error"].as_str().unwrap_or_default().contains("not live"));

    // Restart: another live beacon_info should become new active lifecycle and accept updates.
    let beacon_v2 = put_beacon_info_custom(&app, &token, &room_id, &user_id, true, 60_000, now + 5000).await;
    let (status_v2, body_v2) = send_beacon_with_ts(&app, &token, &room_id, &beacon_v2, now + 7000).await;
    assert_eq!(status_v2, StatusCode::OK, "{:?}", body_v2);
}

#[tokio::test]
async fn test_location_behavior_in_e2ee_room_uses_metadata_only() {
    let Some((app, pool)) = setup_test_app_with_pool().await else {
        eprintln!("Skipping test: database not available");
        return;
    };

    let token = super::create_test_user(&app).await;
    let user_id = whoami(&app, &token).await;
    let room_id = create_room(&app, &token).await;
    enable_room_e2ee(&app, &token, &room_id).await;

    let beacon_info_event_id = put_beacon_info(&app, &token, &room_id, &user_id).await;

    // Encrypted timeline event must not be interpreted/indexed as beacon location.
    let (encrypted_status, encrypted_body) = send_room_encrypted(&app, &token, &room_id).await;
    assert_eq!(encrypted_status, StatusCode::OK, "{:?}", encrypted_body);

    let location_count_after_encrypted: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM beacon_locations WHERE room_id = $1 AND sender = $2")
            .bind(&room_id)
            .bind(&user_id)
            .fetch_one(&*pool)
            .await
            .unwrap();
    assert_eq!(location_count_after_encrypted, 0);

    // Clear m.beacon in the same E2EE room is still processed through metadata checks.
    let ts = current_timestamp_millis();
    let (beacon_status, beacon_body) = send_beacon_with_ts(&app, &token, &room_id, &beacon_info_event_id, ts).await;
    assert_eq!(beacon_status, StatusCode::OK, "{:?}", beacon_body);
}

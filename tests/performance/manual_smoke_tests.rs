use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use futures::future::join_all;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Instant;
use synapse_rust::cache::CacheManager;
use synapse_rust::services::ServiceContainer;
use synapse_rust::web::routes::state::AppState;
use tower::ServiceExt;

fn with_local_connect_info(
    mut request: hyper::Request<axum::body::Body>,
) -> hyper::Request<axum::body::Body> {
    use axum::extract::ConnectInfo;
    use std::net::SocketAddr;
    let local_addr: SocketAddr = "127.0.0.1:65530"
        .parse()
        .expect("valid loopback socket addr");
    request.extensions_mut().insert(ConnectInfo(local_addr));
    request
}

async fn setup_test_app() -> Option<axum::Router> {
    let pool = match synapse_rust::test_utils::prepare_isolated_test_pool().await {
        Ok(pool) => pool,
        Err(error) => {
            eprintln!(
                "Skipping performance manual tests: isolated schema setup failed: {}",
                error
            );
            return None;
        }
    };

    let container = ServiceContainer::new_test_with_pool(pool);
    let cache = Arc::new(CacheManager::new(Default::default()));
    let state = AppState::new(container, cache);
    Some(synapse_rust::web::create_router(state))
}

async fn create_test_user(app: &axum::Router) -> String {
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/register")
        .header("Content-Type", "application/json")
        .body(Body::from(
            serde_json::json!({
                "username": format!("user_{}", rand::random::<u32>()),
                "password": "UserTest@123",
                "device_id": "TESTDEVICE"
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.clone().oneshot(request).await.unwrap();
    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    json["access_token"].as_str().unwrap().to_string()
}

async fn whoami(app: &axum::Router, token: &str) -> String {
    let request = Request::builder()
        .method("GET")
        .uri("/_matrix/client/v3/account/whoami")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let response = app
        .clone()
        .oneshot(with_local_connect_info(request))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    json["user_id"].as_str().unwrap().to_string()
}

async fn create_room(app: &axum::Router, token: &str) -> String {
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/createRoom")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({ "name": "Beacon Performance Room" }).to_string(),
        ))
        .unwrap();

    let response = app
        .clone()
        .oneshot(with_local_connect_info(request))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .unwrap();
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
        .body(Body::from(
            json!({ "user_id": invitee_user_id }).to_string(),
        ))
        .unwrap();
    let invite_resp = app
        .clone()
        .oneshot(with_local_connect_info(invite_req))
        .await
        .unwrap();
    assert_eq!(invite_resp.status(), StatusCode::OK);

    let join_req = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/v3/rooms/{}/join", room_id))
        .header("Authorization", format!("Bearer {}", invitee_token))
        .body(Body::empty())
        .unwrap();
    let join_resp = app
        .clone()
        .oneshot(with_local_connect_info(join_req))
        .await
        .unwrap();
    assert_eq!(join_resp.status(), StatusCode::OK);
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
                    "description": "Beacon performance",
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
        .oneshot(with_local_connect_info(request))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    json["event_id"].as_str().unwrap().to_string()
}

async fn send_beacon_with_ts(
    app: axum::Router,
    token: String,
    room_id: String,
    beacon_info_id: String,
    ts: i64,
) -> (StatusCode, u64) {
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
                "m.ts": ts
            })
            .to_string(),
        ))
        .unwrap();

    let start = Instant::now();
    let response = app.oneshot(with_local_connect_info(request)).await.unwrap();
    let latency_ms = start.elapsed().as_millis() as u64;
    (response.status(), latency_ms)
}

async fn post_sliding_sync_with_latency(app: axum::Router, token: String) -> (StatusCode, u64) {
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/sync")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "lists": {} }).to_string()))
        .unwrap();

    let start = Instant::now();
    let response = app.oneshot(with_local_connect_info(request)).await.unwrap();
    let latency_ms = start.elapsed().as_millis() as u64;
    (response.status(), latency_ms)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore]
async fn sliding_sync_poc_load_smoke() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let token = create_test_user(&app).await;

    let total_requests = 200usize;
    let concurrency = 20usize;
    let mut latencies_ms = Vec::with_capacity(total_requests);
    let mut ok_count = 0usize;
    let mut limited_count = 0usize;
    let mut other_count = 0usize;

    for chunk in (0..total_requests).collect::<Vec<_>>().chunks(concurrency) {
        let futures = chunk.iter().map(|_| {
            let app = app.clone();
            let token = token.clone();
            tokio::spawn(async move { post_sliding_sync_with_latency(app, token).await })
        });

        let results = join_all(futures).await;
        for result in results {
            let (status, latency_ms) = result.unwrap();
            latencies_ms.push(latency_ms);
            match status {
                StatusCode::OK => ok_count += 1,
                StatusCode::TOO_MANY_REQUESTS => limited_count += 1,
                _ => other_count += 1,
            }
        }
    }

    latencies_ms.sort_unstable();
    let p50 = latencies_ms[latencies_ms.len() / 2];
    let p95 = latencies_ms[(latencies_ms.len() as f64 * 0.95) as usize];
    let p99 = latencies_ms[(latencies_ms.len() as f64 * 0.99) as usize];
    let total = total_requests as f64;
    let limited_ratio = (limited_count as f64 / total) * 100.0;

    println!(
        "Sliding Sync PoC load smoke: total={}, ok={}, limited={}, other={}, limited_ratio={:.2}%, concurrency={}, p50={}ms, p95={}ms, p99={}ms",
        total_requests, ok_count, limited_count, other_count, limited_ratio, concurrency, p50, p95, p99
    );
    println!(
        "PERF_SMOKE_JSON={}",
        json!({
            "name": "sliding_sync_poc_load_smoke",
            "total": total_requests,
            "ok": ok_count,
            "limited": limited_count,
            "other": other_count,
            "limited_ratio_percent": limited_ratio,
            "concurrency": concurrency,
            "p50_ms": p50,
            "p95_ms": p95,
            "p99_ms": p99
        })
    );

    assert_eq!(
        other_count, 0,
        "unexpected non-429 failures in sliding sync load smoke"
    );
    assert!(
        ok_count > 0,
        "expected at least one successful sliding sync response"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore]
async fn beacon_hot_room_backpressure_load_smoke() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let owner_token = create_test_user(&app).await;
    let owner_user_id = whoami(&app, &owner_token).await;
    let room_id = create_room(&app, &owner_token).await;
    let beacon_info_id = put_beacon_info(&app, &owner_token, &room_id, &owner_user_id).await;

    let participant_count = 40usize;
    let mut participant_tokens = Vec::with_capacity(participant_count);
    for _ in 0..participant_count {
        let token = create_test_user(&app).await;
        let user_id = whoami(&app, &token).await;
        invite_and_join_room(&app, &owner_token, &room_id, &token, &user_id).await;
        participant_tokens.push(token);
    }

    let mut latencies_ms = Vec::with_capacity(participant_count);
    let mut ok_count = 0usize;
    let mut limited_count = 0usize;
    let mut other_count = 0usize;
    let concurrency = 20usize;
    let base_ts = chrono::Utc::now().timestamp_millis();
    let mut global_idx = 0usize;

    for chunk in participant_tokens.chunks(concurrency) {
        let futures = chunk.iter().map(|token| {
            let app = app.clone();
            let token = token.clone();
            let room_id = room_id.clone();
            let beacon_info_id = beacon_info_id.clone();
            let ts = base_ts + global_idx as i64;
            global_idx += 1;
            tokio::spawn(async move {
                send_beacon_with_ts(app, token, room_id, beacon_info_id, ts).await
            })
        });
        let results = join_all(futures).await;
        for result in results {
            let (status, latency_ms) = result.unwrap();
            latencies_ms.push(latency_ms);
            match status {
                StatusCode::OK => ok_count += 1,
                StatusCode::TOO_MANY_REQUESTS => limited_count += 1,
                _ => other_count += 1,
            }
        }
    }

    latencies_ms.sort_unstable();
    let p50 = latencies_ms[latencies_ms.len() / 2];
    let p95 = latencies_ms[(latencies_ms.len() as f64 * 0.95) as usize];
    let p99 = latencies_ms[(latencies_ms.len() as f64 * 0.99) as usize];
    let total = participant_count as f64;
    let limited_ratio = (limited_count as f64 / total) * 100.0;

    println!(
        "Beacon hotspot backpressure smoke: total={}, ok={}, limited={}, other={}, limited_ratio={:.2}%, p50={}ms, p95={}ms, p99={}ms",
        participant_count, ok_count, limited_count, other_count, limited_ratio, p50, p95, p99
    );
    println!(
        "PERF_SMOKE_JSON={}",
        json!({
            "name": "beacon_hot_room_backpressure_load_smoke",
            "total": participant_count,
            "ok": ok_count,
            "limited": limited_count,
            "other": other_count,
            "limited_ratio_percent": limited_ratio,
            "concurrency": concurrency,
            "p50_ms": p50,
            "p95_ms": p95,
            "p99_ms": p99
        })
    );

    assert_eq!(
        other_count, 0,
        "unexpected non-429 failures in beacon load smoke"
    );
    assert!(
        ok_count > 0,
        "expected at least one successful beacon update"
    );
    assert!(
        limited_count > 0,
        "expected at least one 429 under hotspot room beacon load"
    );
}

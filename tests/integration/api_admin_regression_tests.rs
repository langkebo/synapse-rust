use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use std::sync::{Arc, OnceLock};
use synapse_rust::cache::{CacheConfig, CacheManager};
use synapse_rust::services::ServiceContainer;
use synapse_rust::storage::sliding_sync::SlidingSyncStorage;
use synapse_rust::web::routes::create_router;
use synapse_rust::web::AppState;
use tower::ServiceExt;

static TEST_MUTEX: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();

fn test_mutex() -> &'static tokio::sync::Mutex<()> {
    TEST_MUTEX.get_or_init(|| tokio::sync::Mutex::new(()))
}

fn test_database_url() -> String {
    std::env::var("TEST_DATABASE_URL")
        .or_else(|_| std::env::var("DATABASE_URL"))
        .unwrap_or_else(|_| "postgresql://synapse:synapse@localhost:5432/synapse".to_string())
}

async fn dedicated_pool() -> Arc<sqlx::PgPool> {
    Arc::new(
        sqlx::postgres::PgPoolOptions::new()
            .max_connections(5)
            .acquire_timeout(std::time::Duration::from_secs(10))
            .connect(&test_database_url())
            .await
            .expect("failed to create dedicated test pool"),
    )
}

async fn setup_test_app() -> Option<axum::Router> {
    if !super::init_test_database().await {
        return None;
    }
    let container = ServiceContainer::new_test();
    let cache = Arc::new(CacheManager::new(CacheConfig::default()));
    let state = AppState::new(container, cache);
    Some(create_router(state))
}

async fn register_user(app: &axum::Router, username: &str) -> Option<(String, String)> {
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
        .ok()?;

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .ok()?;

    if response.status() != StatusCode::OK {
        return None;
    }

    let body = axum::body::to_bytes(response.into_body(), 4096)
        .await
        .ok()?;
    let json: Value = serde_json::from_slice(&body).ok()?;

    Some((
        json.get("access_token")?.as_str()?.to_string(),
        json.get("user_id")?.as_str()?.to_string(),
    ))
}

async fn promote_to_admin(user_id: &str) {
    let pool = dedicated_pool().await;

    sqlx::query("UPDATE users SET is_admin = TRUE WHERE user_id = $1")
        .bind(user_id)
        .execute(&*pool)
        .await
        .expect("failed to promote user to admin");
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
        .expect("room creation request failed");

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 4096)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    json["room_id"].as_str().unwrap().to_string()
}

async fn send_message(app: &axum::Router, token: &str, room_id: &str, txn_id: &str) -> String {
    let request = Request::builder()
        .method("PUT")
        .uri(format!(
            "/_matrix/client/v3/rooms/{}/send/m.room.message/{}",
            room_id, txn_id
        ))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "msgtype": "m.text",
                "body": format!("message-{}", txn_id)
            })
            .to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .expect("send message request failed");

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 4096)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    json["event_id"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn test_admin_room_event_reads_from_events_table() {
    let _guard = test_mutex().lock().await;
    let Some(app) = setup_test_app().await else {
        return;
    };

    let (admin_token, admin_user_id) =
        register_user(&app, &format!("admin_room_event_{}", rand::random::<u32>()))
            .await
            .expect("failed to register admin user");
    promote_to_admin(&admin_user_id).await;

    let room_id = create_room(&app, &admin_token, "Admin Room Event").await;
    let first_event_id = send_message(&app, &admin_token, &room_id, "admin-room-event-1").await;
    let second_event_id = send_message(&app, &admin_token, &room_id, "admin-room-event-2").await;

    let request = Request::builder()
        .uri(format!(
            "/_synapse/admin/v1/rooms/{}/event_context/{}",
            room_id, second_event_id
        ))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 8192)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["event"]["event_id"], second_event_id);
    assert_eq!(json["event"]["room_id"], room_id);
    assert_eq!(json["events_before"][0]["event_id"], first_event_id);
}

#[tokio::test]
async fn test_admin_room_reports_follow_current_event_report_schema() {
    let _guard = test_mutex().lock().await;
    let Some(app) = setup_test_app().await else {
        return;
    };

    let (admin_token, admin_user_id) =
        register_user(&app, &format!("admin_room_report_{}", rand::random::<u32>()))
            .await
            .expect("failed to register admin user");
    promote_to_admin(&admin_user_id).await;

    let room_id = create_room(&app, &admin_token, "Admin Room Report").await;
    let event_id = send_message(&app, &admin_token, &room_id, "admin-room-report-1").await;
    let pool = dedicated_pool().await;
    let now = chrono::Utc::now().timestamp_millis();

    sqlx::query(
        r#"
        INSERT INTO event_reports (
            event_id, room_id, reporter_user_id, reported_user_id, event_json,
            reason, description, status, score, received_ts
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
        "#,
    )
    .bind(&event_id)
    .bind(&room_id)
    .bind(&admin_user_id)
    .bind(Option::<String>::None)
    .bind(Some(json!({ "event_id": event_id })))
    .bind(Some("spam"))
    .bind(Some("seeded report"))
    .bind("open")
    .bind(10_i32)
    .bind(now)
    .execute(&*pool)
    .await
    .expect("failed to insert event report");

    let request = Request::builder()
        .uri(format!("/_synapse/admin/v1/rooms/{}/reports", room_id))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 8192)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["total"], 1);
    assert_eq!(json["reports"][0]["event_id"], event_id);
    assert_eq!(json["reports"][0]["user_id"], admin_user_id);
    assert_eq!(json["reports"][0]["content"], "seeded report");
}

#[tokio::test]
async fn test_admin_audit_endpoints_work_without_precreated_table() {
    let _guard = test_mutex().lock().await;
    let Some(app) = setup_test_app().await else {
        return;
    };

    let (admin_token, admin_user_id) =
        register_user(&app, &format!("admin_audit_{}", rand::random::<u32>()))
            .await
            .expect("failed to register admin user");
    promote_to_admin(&admin_user_id).await;

    let (_, target_user_id) = register_user(&app, &format!("audit_target_{}", rand::random::<u32>()))
        .await
        .expect("failed to register target user");

    let pool = dedicated_pool().await;
    sqlx::query("DROP TABLE IF EXISTS audit_events")
        .execute(&*pool)
        .await
        .expect("failed to drop audit_events");

    let request = Request::builder()
        .method("POST")
        .uri(format!(
            "/_synapse/admin/v1/users/{}/shadow_ban",
            target_user_id
        ))
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("x-request-id", "req-admin-audit-regression")
        .body(Body::from("{}"))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let request = Request::builder()
        .uri("/_synapse/admin/v1/audit/events?from=0&limit=10")
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 8192)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let events = json["events"].as_array().unwrap();

    assert!(events.iter().any(|event| {
        event["action"] == "admin.user.shadow_ban" && event["resource_id"] == target_user_id
    }));
}

#[tokio::test]
async fn test_admin_room_token_sync_returns_sliding_sync_state() {
    let _guard = test_mutex().lock().await;
    let Some(app) = setup_test_app().await else {
        return;
    };

    let (admin_token, admin_user_id) =
        register_user(&app, &format!("admin_token_sync_{}", rand::random::<u32>()))
            .await
            .expect("failed to register admin user");
    promote_to_admin(&admin_user_id).await;

    let room_id = create_room(&app, &admin_token, "Admin Room Token Sync").await;
    let pool = dedicated_pool().await;
    let storage = SlidingSyncStorage::new(pool.clone());
    let conn_id = Some("admin-room-token-sync-conn");

    let token = storage
        .create_or_update_token(&admin_user_id, "ADMINSYNCDEVICE", conn_id)
        .await
        .expect("failed to create sliding sync token");

    storage
        .upsert_room(
            &admin_user_id,
            "ADMINSYNCDEVICE",
            &room_id,
            conn_id,
            Some("main"),
            123456789,
            2,
            5,
            false,
            true,
            false,
            false,
            Some("Admin Room Token Sync"),
            None,
            123456789,
        )
        .await
        .expect("failed to upsert sliding sync room");

    let request = Request::builder()
        .uri(format!("/_synapse/admin/v1/rooms/{}/token_sync", room_id))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 8192)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["room_id"], room_id);
    assert_eq!(json["total"], 1);
    assert_eq!(json["results"][0]["user_id"], admin_user_id);
    assert_eq!(json["results"][0]["device_id"], "ADMINSYNCDEVICE");
    assert_eq!(json["results"][0]["conn_id"], "admin-room-token-sync-conn");
    assert_eq!(json["results"][0]["pos"], token.pos);
    assert_eq!(json["summary"]["active_token_count"], 1);
    assert_eq!(json["summary"]["distinct_users"], 1);
    assert_eq!(json["summary"]["distinct_devices"], 1);
}

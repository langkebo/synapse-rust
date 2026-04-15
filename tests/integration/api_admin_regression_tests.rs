use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use serde_json::{json, Value};
use sqlx::Row;
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

async fn setup_test_app() -> Option<(axum::Router, Arc<sqlx::PgPool>)> {
    let pool = synapse_rust::test_utils::prepare_isolated_test_pool()
        .await
        .ok()?;
    let container = ServiceContainer::new_test_with_pool(pool.clone());
    let cache = Arc::new(CacheManager::new(CacheConfig::default()));
    let state = AppState::new(container, cache);
    Some((create_router(state), pool))
}

async fn setup_test_app_with_saml() -> Option<(axum::Router, Arc<sqlx::PgPool>)> {
    let pool = synapse_rust::test_utils::prepare_isolated_test_pool()
        .await
        .ok()?;
    let mut container = ServiceContainer::new_test_with_pool(pool.clone());
    container.config.saml.enabled = true;
    container.config.saml.metadata_url = None;
    container.config.saml.metadata_xml = Some(
        r#"
        <md:EntityDescriptor entityID="https://idp.example.com">
            <md:IDPSSODescriptor>
                <md:SingleSignOnService Location="https://idp.example.com/sso"/>
                <md:SingleLogoutService Location="https://idp.example.com/slo"/>
                <ds:KeyInfo>
                    <ds:X509Data>
                        <ds:X509Certificate>MIIC9jCCAd4CCQD...</ds:X509Certificate>
                    </ds:X509Data>
                </ds:KeyInfo>
            </md:IDPSSODescriptor>
        </md:EntityDescriptor>
        "#
        .to_string(),
    );
    container.saml_service = Arc::new(synapse_rust::services::saml_service::SamlService::new(
        Arc::new(container.config.saml.clone()),
        Arc::new(container.saml_storage.clone()),
        container.server_name.clone(),
    ));

    let cache = Arc::new(CacheManager::new(CacheConfig::default()));
    let state = AppState::new(container, cache);
    Some((create_router(state), pool))
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

async fn promote_to_admin(pool: &sqlx::PgPool, user_id: &str) {
    sqlx::query("UPDATE users SET is_admin = TRUE WHERE user_id = $1")
        .bind(user_id)
        .execute(pool)
        .await
        .expect("failed to promote user to admin");
}

async fn promote_to_admin_with_role(pool: &sqlx::PgPool, user_id: &str, role: &str) {
    sqlx::query("UPDATE users SET is_admin = TRUE, user_type = $2 WHERE user_id = $1")
        .bind(user_id)
        .bind(role)
        .execute(pool)
        .await
        .expect("failed to promote user to admin with role");
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

#[tokio::test]
async fn test_public_register_ignores_admin_flag() {
    let _guard = test_mutex().lock().await;
    let Some((app, pool)) = setup_test_app().await else {
        eprintln!("Skipping test because test database is unavailable");
        return;
    };

    let username = format!("public_admin_flag_{}", rand::random::<u32>());
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/r0/register")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "username": username,
                "password": "Password123!",
                "admin": true,
                "auth": { "type": "m.login.dummy" }
            })
            .to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .expect("registration request failed");
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 4096)
        .await
        .expect("failed to read registration response body");
    let payload: Value = serde_json::from_slice(&body).expect("invalid registration json");
    let user_id = payload["user_id"]
        .as_str()
        .expect("missing user_id in registration response");

    let row = sqlx::query("SELECT is_admin FROM users WHERE user_id = $1")
        .bind(user_id)
        .fetch_one(&*pool)
        .await
        .expect("failed to load created user");
    assert!(
        !row.get::<bool, _>("is_admin"),
        "public registration must not create admin users even if admin=true is supplied"
    );
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
    let Some((app, pool)) = setup_test_app().await else {
        return;
    };

    let (admin_token, admin_user_id) =
        register_user(&app, &format!("admin_room_event_{}", rand::random::<u32>()))
            .await
            .expect("failed to register admin user");
    promote_to_admin(&pool, &admin_user_id).await;

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
    let Some((app, pool)) = setup_test_app().await else {
        return;
    };

    let (admin_token, admin_user_id) = register_user(
        &app,
        &format!("admin_room_report_{}", rand::random::<u32>()),
    )
    .await
    .expect("failed to register admin user");
    promote_to_admin(&pool, &admin_user_id).await;

    let room_id = create_room(&app, &admin_token, "Admin Room Report").await;
    let event_id = send_message(&app, &admin_token, &room_id, "admin-room-report-1").await;
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

    let missing_room_id = format!("!missing-report-room-{}:localhost", rand::random::<u32>());
    let missing_request = Request::builder()
        .uri(format!("/_synapse/admin/v1/rooms/{}/reports", missing_room_id))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let missing_response = ServiceExt::<Request<Body>>::oneshot(app, missing_request)
        .await
        .unwrap();
    assert_eq!(missing_response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_admin_room_collection_queries_require_existing_room() {
    let _guard = test_mutex().lock().await;
    let Some((app, pool)) = setup_test_app().await else {
        return;
    };

    let (admin_token, admin_user_id) = register_user(
        &app,
        &format!("admin_room_queries_{}", rand::random::<u32>()),
    )
    .await
    .expect("failed to register admin user");
    promote_to_admin(&pool, &admin_user_id).await;

    let missing_room_id = format!("missing-room-{}", rand::random::<u32>());

    for uri in [
        format!("/_synapse/admin/v1/rooms/{}/members", missing_room_id),
        format!("/_synapse/admin/v1/rooms/{}/state", missing_room_id),
        format!("/_synapse/admin/v1/rooms/{}/messages", missing_room_id),
        format!("/_synapse/admin/v1/rooms/{}/block", missing_room_id),
        format!("/_synapse/admin/v1/rooms/{}/room_listings", missing_room_id),
        format!("/_synapse/admin/v1/rooms/{}/forward_extremities", missing_room_id),
        format!("/_synapse/admin/v1/rooms/{}/token_sync", missing_room_id),
    ] {
        let request = Request::builder()
            .uri(&uri)
            .header("Authorization", format!("Bearer {}", admin_token))
            .body(Body::empty())
            .unwrap();
        let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "uri: {uri}");
    }

    let event_context_request = Request::builder()
        .uri(format!(
            "/_synapse/admin/v1/rooms/{}/event_context/$missing-event:localhost",
            missing_room_id
        ))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let event_context_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), event_context_request)
            .await
            .unwrap();
    assert_eq!(event_context_response.status(), StatusCode::NOT_FOUND);

    let search_request = Request::builder()
        .method("POST")
        .uri(format!("/_synapse/admin/v1/rooms/{}/search", missing_room_id))
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "search_term": "hello" }).to_string()))
        .unwrap();
    let search_response = ServiceExt::<Request<Body>>::oneshot(app, search_request)
        .await
        .unwrap();
    assert_eq!(search_response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_admin_room_member_delete_requires_existing_targets() {
    let _guard = test_mutex().lock().await;
    let Some((app, pool)) = setup_test_app().await else {
        return;
    };

    let (admin_token, admin_user_id) = register_user(
        &app,
        &format!("admin_room_member_delete_{}", rand::random::<u32>()),
    )
    .await
    .expect("failed to register admin user");
    promote_to_admin(&pool, &admin_user_id).await;

    let (_, managed_user_id) = register_user(&app, &format!("managed_member_{}", rand::random::<u32>()))
        .await
        .expect("failed to register managed user");

    let missing_room_id = format!("missing-room-{}", rand::random::<u32>());
    let missing_room_request = Request::builder()
        .method("DELETE")
        .uri(format!(
            "/_synapse/admin/v1/rooms/{}/members/{}",
            missing_room_id, managed_user_id
        ))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let missing_room_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), missing_room_request)
            .await
            .unwrap();
    assert_eq!(missing_room_response.status(), StatusCode::NOT_FOUND);

    let room_id = create_room(&app, &admin_token, "Admin Member Delete Target").await;
    let missing_user_id = format!("@missing_member_{}:localhost", rand::random::<u32>());
    let missing_user_request = Request::builder()
        .method("DELETE")
        .uri(format!(
            "/_synapse/admin/v1/rooms/{}/members/{}",
            room_id, missing_user_id
        ))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let missing_user_response =
        ServiceExt::<Request<Body>>::oneshot(app, missing_user_request)
            .await
            .unwrap();
    assert_eq!(missing_user_response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_admin_space_collection_queries_require_existing_space() {
    let _guard = test_mutex().lock().await;
    let Some((app, pool)) = setup_test_app().await else {
        return;
    };

    let (admin_token, admin_user_id) = register_user(
        &app,
        &format!("admin_space_queries_{}", rand::random::<u32>()),
    )
    .await
    .expect("failed to register admin user");
    promote_to_admin(&pool, &admin_user_id).await;

    let missing_space_id = format!("missing-space-{}", rand::random::<u32>());

    for uri in [
        format!("/_synapse/admin/v1/spaces/{}/users", missing_space_id),
        format!("/_synapse/admin/v1/spaces/{}/rooms", missing_space_id),
        format!("/_synapse/admin/v1/spaces/{}/stats", missing_space_id),
    ] {
        let request = Request::builder()
            .uri(&uri)
            .header("Authorization", format!("Bearer {}", admin_token))
            .body(Body::empty())
            .unwrap();
        let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "uri: {uri}");
    }

    let existing_space_id = format!("admin-space-{}", rand::random::<u32>());
    let room_id = format!("!{}:localhost", existing_space_id);
    let now = chrono::Utc::now().timestamp_millis();
    sqlx::query(
        r#"
        INSERT INTO spaces (
            space_id, room_id, name, topic, creator, join_rule, visibility, is_public, created_ts
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        "#,
    )
    .bind(&existing_space_id)
    .bind(&room_id)
    .bind(Some("Admin Space"))
    .bind(Some("empty child resources"))
    .bind(&admin_user_id)
    .bind("invite")
    .bind("private")
    .bind(false)
    .bind(now)
    .execute(&*pool)
    .await
    .expect("failed to seed space fixture");

    let users_request = Request::builder()
        .uri(format!("/_synapse/admin/v1/spaces/{}/users", existing_space_id))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let users_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), users_request)
        .await
        .unwrap();
    assert_eq!(users_response.status(), StatusCode::OK);
    let users_body = axum::body::to_bytes(users_response.into_body(), 4096)
        .await
        .unwrap();
    let users_json: Value = serde_json::from_slice(&users_body).unwrap();
    assert_eq!(users_json, json!({ "users": [], "total": 0 }));

    let rooms_request = Request::builder()
        .uri(format!("/_synapse/admin/v1/spaces/{}/rooms", existing_space_id))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let rooms_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), rooms_request)
        .await
        .unwrap();
    assert_eq!(rooms_response.status(), StatusCode::OK);
    let rooms_body = axum::body::to_bytes(rooms_response.into_body(), 4096)
        .await
        .unwrap();
    let rooms_json: Value = serde_json::from_slice(&rooms_body).unwrap();
    assert_eq!(rooms_json, json!({ "rooms": [], "total": 0 }));

    let stats_request = Request::builder()
        .uri(format!("/_synapse/admin/v1/spaces/{}/stats", existing_space_id))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let stats_response = ServiceExt::<Request<Body>>::oneshot(app, stats_request)
        .await
        .unwrap();
    assert_eq!(stats_response.status(), StatusCode::OK);
    let stats_body = axum::body::to_bytes(stats_response.into_body(), 4096)
        .await
        .unwrap();
    let stats_json: Value = serde_json::from_slice(&stats_body).unwrap();
    assert_eq!(
        stats_json,
        json!({
            "space_id": existing_space_id,
            "member_count": 0,
            "child_room_count": 0
        })
    );
}

#[tokio::test]
async fn test_admin_audit_endpoints_remain_available_when_schema_audit_table_missing() {
    let _guard = test_mutex().lock().await;
    let Some((app, pool)) = setup_test_app().await else {
        return;
    };

    let (admin_token, admin_user_id) =
        register_user(&app, &format!("admin_audit_{}", rand::random::<u32>()))
            .await
            .expect("failed to register admin user");
    promote_to_admin(&pool, &admin_user_id).await;

    let (_, target_user_id) =
        register_user(&app, &format!("audit_target_{}", rand::random::<u32>()))
            .await
            .expect("failed to register target user");

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
}

#[tokio::test]
async fn test_admin_room_token_sync_returns_sliding_sync_state() {
    let _guard = test_mutex().lock().await;
    let Some((app, pool)) = setup_test_app().await else {
        return;
    };

    let (admin_token, admin_user_id) =
        register_user(&app, &format!("admin_token_sync_{}", rand::random::<u32>()))
            .await
            .expect("failed to register admin user");
    promote_to_admin(&pool, &admin_user_id).await;

    let room_id = create_room(&app, &admin_token, "Admin Room Token Sync").await;
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

#[tokio::test]
async fn test_module_admin_endpoints_require_admin_role() {
    let _guard = test_mutex().lock().await;
    let Some((app, pool)) = setup_test_app().await else {
        return;
    };

    let (admin_token, admin_user_id) =
        register_user(&app, &format!("module_admin_{}", rand::random::<u32>()))
            .await
            .expect("failed to register admin user");
    promote_to_admin(&pool, &admin_user_id).await;

    let (user_token, user_id) =
        register_user(&app, &format!("module_target_{}", rand::random::<u32>()))
            .await
            .expect("failed to register target user");

    let admin_modules_request = Request::builder()
        .uri("/_synapse/admin/v1/modules")
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), admin_modules_request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let forbidden_modules_request = Request::builder()
        .uri("/_synapse/admin/v1/modules")
        .header("Authorization", format!("Bearer {}", user_token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), forbidden_modules_request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let expiration_ts = chrono::Utc::now().timestamp_millis() + 86_400_000_i64;
    let create_validity_request = Request::builder()
        .method("POST")
        .uri("/_synapse/admin/v1/account_validity")
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "user_id": user_id,
                "expiration_ts": expiration_ts,
                "is_valid": true
            })
            .to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), create_validity_request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let body = axum::body::to_bytes(response.into_body(), 8192)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["user_id"], user_id);
    assert_eq!(json["expiration_ts"], expiration_ts);
    assert_eq!(json["is_valid"], true);

    let forbidden_request = Request::builder()
        .uri(format!("/_synapse/admin/v1/account_validity/{}", user_id))
        .header("Authorization", format!("Bearer {}", user_token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), forbidden_request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let admin_request = Request::builder()
        .uri(format!("/_synapse/admin/v1/account_validity/{}", user_id))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app, admin_request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 8192)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["user_id"], user_id);
    assert_eq!(json["expiration_ts"], expiration_ts);
}

#[tokio::test]
async fn test_cas_admin_endpoints_require_admin_role() {
    let _guard = test_mutex().lock().await;
    let Some((app, pool)) = setup_test_app().await else {
        return;
    };

    let (admin_token, admin_user_id) =
        register_user(&app, &format!("cas_admin_{}", rand::random::<u32>()))
            .await
            .expect("failed to register admin user");
    promote_to_admin(&pool, &admin_user_id).await;

    let (user_token, user_id) = register_user(&app, &format!("cas_user_{}", rand::random::<u32>()))
        .await
        .expect("failed to register regular user");

    let forbidden_services_request = Request::builder()
        .uri("/admin/services")
        .header("Authorization", format!("Bearer {}", user_token))
        .body(Body::empty())
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), forbidden_services_request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let admin_services_request = Request::builder()
        .uri("/admin/services")
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), admin_services_request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let forbidden_attrs_request = Request::builder()
        .uri(format!("/admin/users/{}/attributes", user_id))
        .header("Authorization", format!("Bearer {}", user_token))
        .body(Body::empty())
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), forbidden_attrs_request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let admin_attrs_request = Request::builder()
        .uri(format!("/admin/users/{}/attributes", user_id))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app, admin_attrs_request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_additional_admin_routes_require_admin_role() {
    let _guard = test_mutex().lock().await;
    let Some((app, pool)) = setup_test_app().await else {
        return;
    };

    let (admin_token, admin_user_id) =
        register_user(&app, &format!("extra_admin_{}", rand::random::<u32>()))
            .await
            .expect("failed to register admin user");
    promote_to_admin(&pool, &admin_user_id).await;

    let (user_token, _) = register_user(&app, &format!("extra_user_{}", rand::random::<u32>()))
        .await
        .expect("failed to register regular user");

    let appservice_request = Request::builder()
        .uri("/_synapse/admin/v1/appservices")
        .header("Authorization", format!("Bearer {}", user_token))
        .body(Body::empty())
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), appservice_request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let push_process_request = Request::builder()
        .method("POST")
        .uri("/_synapse/admin/v1/push/process")
        .header("Authorization", format!("Bearer {}", user_token))
        .body(Body::empty())
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), push_process_request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let captcha_cleanup_request = Request::builder()
        .method("POST")
        .uri("/_synapse/admin/v1/captcha/cleanup")
        .header("Authorization", format!("Bearer {}", user_token))
        .body(Body::empty())
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), captcha_cleanup_request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let external_services_request = Request::builder()
        .uri("/_synapse/admin/v1/external_services")
        .header("Authorization", format!("Bearer {}", user_token))
        .body(Body::empty())
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), external_services_request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let admin_appservice_request = Request::builder()
        .uri("/_synapse/admin/v1/appservices")
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), admin_appservice_request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_saml_admin_metadata_refresh_requires_admin_middleware() {
    let _guard = test_mutex().lock().await;
    let Some((app, pool)) = setup_test_app_with_saml().await else {
        return;
    };

    let (admin_token, admin_user_id) =
        register_user(&app, &format!("saml_admin_{}", rand::random::<u32>()))
            .await
            .expect("failed to register admin user");
    promote_to_admin(&pool, &admin_user_id).await;

    let (user_token, _) = register_user(&app, &format!("saml_user_{}", rand::random::<u32>()))
        .await
        .expect("failed to register regular user");

    let unauthenticated_request = Request::builder()
        .method("POST")
        .uri("/_synapse/admin/v1/saml/metadata/refresh")
        .body(Body::empty())
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), unauthenticated_request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let user_request = Request::builder()
        .method("POST")
        .uri("/_synapse/admin/v1/saml/metadata/refresh")
        .header("Authorization", format!("Bearer {}", user_token))
        .body(Body::empty())
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), user_request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let admin_request = Request::builder()
        .method("POST")
        .uri("/_synapse/admin/v1/saml/metadata/refresh")
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app, admin_request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_moderation_report_score_requires_admin() {
    let _guard = test_mutex().lock().await;
    let Some((app, pool)) = setup_test_app().await else {
        return;
    };

    let (admin_token, admin_user_id) =
        register_user(&app, &format!("report_score_admin_{}", rand::random::<u32>()))
            .await
            .expect("failed to register admin user");
    promote_to_admin(&pool, &admin_user_id).await;

    let (user_token, _) = register_user(&app, &format!("report_score_user_{}", rand::random::<u32>()))
        .await
        .expect("failed to register regular user");

    let room_id = create_room(&app, &admin_token, "Moderation Report Score").await;
    let event_id = send_message(&app, &admin_token, &room_id, "moderation-report-score").await;

    let unauthenticated_request = Request::builder()
        .method("PUT")
        .uri(format!(
            "/_matrix/client/v3/rooms/{}/report/{}/score",
            room_id, event_id
        ))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "score": -10 }).to_string()))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), unauthenticated_request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let user_request = Request::builder()
        .method("PUT")
        .uri(format!(
            "/_matrix/client/v3/rooms/{}/report/{}/score",
            room_id, event_id
        ))
        .header("Authorization", format!("Bearer {}", user_token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "score": -10 }).to_string()))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), user_request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let admin_request = Request::builder()
        .method("PUT")
        .uri(format!(
            "/_matrix/client/v3/rooms/{}/report/{}/score",
            room_id, event_id
        ))
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "score": -10 }).to_string()))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app, admin_request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_room_alias_delete_requires_member_or_admin() {
    let _guard = test_mutex().lock().await;
    let Some((app, pool)) = setup_test_app().await else {
        return;
    };

    let (owner_token, _) = register_user(&app, &format!("alias_owner_{}", rand::random::<u32>()))
        .await
        .expect("failed to register owner");
    let (admin_token, admin_user_id) =
        register_user(&app, &format!("alias_admin_{}", rand::random::<u32>()))
            .await
            .expect("failed to register admin");
    promote_to_admin(&pool, &admin_user_id).await;
    let (user_token, _) = register_user(&app, &format!("alias_user_{}", rand::random::<u32>()))
        .await
        .expect("failed to register regular user");

    let room_id = create_room(&app, &owner_token, "Alias Security").await;
    let alias = format!("#alias-security-{}:localhost", rand::random::<u32>());
    let encoded_alias = urlencoding::encode(&alias);

    let create_alias_request = Request::builder()
        .method("PUT")
        .uri(format!("/_matrix/client/v3/directory/room/{}", encoded_alias))
        .header("Authorization", format!("Bearer {}", owner_token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "room_id": room_id }).to_string()))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), create_alias_request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let unauthenticated_request = Request::builder()
        .method("DELETE")
        .uri(format!("/_matrix/client/v3/directory/room/{}", encoded_alias))
        .body(Body::empty())
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), unauthenticated_request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let user_v3_request = Request::builder()
        .method("DELETE")
        .uri(format!("/_matrix/client/v3/directory/room/{}", encoded_alias))
        .header("Authorization", format!("Bearer {}", user_token))
        .body(Body::empty())
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), user_v3_request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let user_r0_request = Request::builder()
        .method("DELETE")
        .uri(format!(
            "/_matrix/client/r0/directory/room/{}/alias/{}",
            room_id, encoded_alias
        ))
        .header("Authorization", format!("Bearer {}", user_token))
        .body(Body::empty())
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), user_r0_request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let admin_request = Request::builder()
        .method("DELETE")
        .uri(format!("/_matrix/client/v3/directory/room/{}", encoded_alias))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), admin_request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let lookup_request = Request::builder()
        .method("GET")
        .uri(format!("/_matrix/client/v3/directory/room/{}", encoded_alias))
        .header("Authorization", format!("Bearer {}", owner_token))
        .body(Body::empty())
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app, lookup_request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_room_summary_internal_write_routes_require_admin() {
    let _guard = test_mutex().lock().await;
    let Some((app, pool)) = setup_test_app().await else {
        return;
    };

    let (owner_token, _) =
        register_user(&app, &format!("summary_owner_{}", rand::random::<u32>()))
            .await
            .expect("failed to register owner");
    let (admin_token, admin_user_id) =
        register_user(&app, &format!("summary_admin_{}", rand::random::<u32>()))
            .await
            .expect("failed to register admin");
    promote_to_admin(&pool, &admin_user_id).await;
    let (user_token, _) = register_user(&app, &format!("summary_user_{}", rand::random::<u32>()))
        .await
        .expect("failed to register regular user");

    let room_id = create_room(&app, &owner_token, "Summary Admin Security").await;
    let create_body = json!({
        "room_id": room_id,
        "name": "locked internal summary"
    });

    let unauthenticated_create = Request::builder()
        .method("POST")
        .uri("/_synapse/room_summary/v1/summaries")
        .header("Content-Type", "application/json")
        .body(Body::from(create_body.to_string()))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), unauthenticated_create)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let user_create = Request::builder()
        .method("POST")
        .uri("/_synapse/room_summary/v1/summaries")
        .header("Authorization", format!("Bearer {}", user_token))
        .header("Content-Type", "application/json")
        .body(Body::from(create_body.to_string()))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), user_create)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let admin_create = Request::builder()
        .method("POST")
        .uri("/_synapse/room_summary/v1/summaries")
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("Content-Type", "application/json")
        .body(Body::from(create_body.to_string()))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), admin_create)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let unauthenticated_process = Request::builder()
        .method("POST")
        .uri("/_synapse/room_summary/v1/updates/process")
        .body(Body::empty())
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), unauthenticated_process)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let user_process = Request::builder()
        .method("POST")
        .uri("/_synapse/room_summary/v1/updates/process")
        .header("Authorization", format!("Bearer {}", user_token))
        .body(Body::empty())
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), user_process)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let admin_process = Request::builder()
        .method("POST")
        .uri("/_synapse/room_summary/v1/updates/process")
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app, admin_process)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_account_validity_post_enforces_unauth_user_admin_states() {
    let _guard = test_mutex().lock().await;
    let Some((app, pool)) = setup_test_app().await else {
        return;
    };

    let (admin_token, admin_user_id) = register_user(
        &app,
        &format!("account_validity_admin_{}", rand::random::<u32>()),
    )
    .await
    .expect("failed to register admin user");
    promote_to_admin(&pool, &admin_user_id).await;

    let (user_token, _) = register_user(
        &app,
        &format!("account_validity_user_{}", rand::random::<u32>()),
    )
    .await
    .expect("failed to register regular user");
    let (_, target_user_id) = register_user(
        &app,
        &format!("account_validity_target_{}", rand::random::<u32>()),
    )
    .await
    .expect("failed to register target user");

    let expiration_ts = chrono::Utc::now().timestamp_millis() + 86_400_000_i64;
    let request_body = json!({
        "user_id": target_user_id,
        "expiration_ts": expiration_ts,
        "is_valid": true
    })
    .to_string();

    let unauthenticated_request = Request::builder()
        .method("POST")
        .uri("/_synapse/admin/v1/account_validity")
        .header("Content-Type", "application/json")
        .body(Body::from(request_body.clone()))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), unauthenticated_request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let regular_user_request = Request::builder()
        .method("POST")
        .uri("/_synapse/admin/v1/account_validity")
        .header("Authorization", format!("Bearer {}", user_token))
        .header("Content-Type", "application/json")
        .body(Body::from(request_body.clone()))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), regular_user_request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let admin_request = Request::builder()
        .method("POST")
        .uri("/_synapse/admin/v1/account_validity")
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("Content-Type", "application/json")
        .body(Body::from(request_body))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app, admin_request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let body = axum::body::to_bytes(response.into_body(), 8192)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["user_id"], target_user_id);
    assert_eq!(json["expiration_ts"], expiration_ts);
    assert_eq!(json["is_valid"], true);
}

#[tokio::test]
async fn test_account_validity_write_endpoints_require_existing_user() {
    let _guard = test_mutex().lock().await;
    let Some((app, pool)) = setup_test_app().await else {
        return;
    };

    let (admin_token, admin_user_id) = register_user(
        &app,
        &format!("account_validity_super_admin_{}", rand::random::<u32>()),
    )
    .await
    .expect("failed to register super admin user");
    promote_to_admin_with_role(&pool, &admin_user_id, "super_admin").await;

    let missing_user_id = format!("@missing_account_validity_{}:localhost", rand::random::<u32>());
    let expiration_ts = chrono::Utc::now().timestamp_millis() + 86_400_000_i64;

    let create_request = Request::builder()
        .method("POST")
        .uri("/_synapse/admin/v1/account_validity")
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "user_id": missing_user_id,
                "expiration_ts": expiration_ts,
                "is_valid": true
            })
            .to_string(),
        ))
        .unwrap();
    let create_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), create_request)
        .await
        .unwrap();
    let create_status = create_response.status();
    let create_body = axum::body::to_bytes(create_response.into_body(), 8192)
        .await
        .unwrap();
    assert_eq!(
        create_status,
        StatusCode::NOT_FOUND,
        "unexpected create response: {}",
        String::from_utf8_lossy(&create_body)
    );

    let validity_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM account_validity WHERE user_id = $1")
        .bind(&missing_user_id)
        .fetch_one(&*pool)
        .await
        .expect("failed to inspect account_validity table");
    assert_eq!(validity_count, 0);

    let renew_request = Request::builder()
        .method("POST")
        .uri(format!(
            "/_synapse/admin/v1/account_validity/{}/renew",
            missing_user_id
        ))
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "renewal_token": "unused",
                "new_expiration_ts": expiration_ts + 86_400_000_i64
            })
            .to_string(),
        ))
        .unwrap();
    let renew_response = ServiceExt::<Request<Body>>::oneshot(app, renew_request)
        .await
        .unwrap();
    let renew_status = renew_response.status();
    let renew_body = axum::body::to_bytes(renew_response.into_body(), 8192)
        .await
        .unwrap();
    assert_eq!(
        renew_status,
        StatusCode::NOT_FOUND,
        "unexpected renew response: {}",
        String::from_utf8_lossy(&renew_body)
    );
}

#[tokio::test]
async fn test_admin_batch_create_users_reports_conflicts_as_failed() {
    let _guard = test_mutex().lock().await;
    let Some((app, pool)) = setup_test_app().await else {
        return;
    };

    let (admin_token, admin_user_id) = register_user(
        &app,
        &format!("batch_create_admin_{}", rand::random::<u32>()),
    )
    .await
    .expect("failed to register admin user");
    promote_to_admin(&pool, &admin_user_id).await;

    let existing_username = format!("batch_existing_{}", rand::random::<u32>());
    register_user(&app, &existing_username)
        .await
        .expect("failed to register existing batch user");

    let new_username = format!("batch_new_{}", rand::random::<u32>());
    let request = Request::builder()
        .method("POST")
        .uri("/_synapse/admin/v1/users/batch")
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "users": [
                    { "username": existing_username },
                    { "username": new_username }
                ]
            })
            .to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app, request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 8192)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["total"], 2);
    assert_eq!(json["created"], json!([new_username]));
    assert_eq!(json["failed"], json!([existing_username]));

    let existing_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE username = $1")
        .bind(json["failed"][0].as_str().unwrap())
        .fetch_one(&*pool)
        .await
        .expect("failed to inspect existing user count");
    assert_eq!(existing_count, 1);

    let new_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE username = $1")
        .bind(json["created"][0].as_str().unwrap())
        .fetch_one(&*pool)
        .await
        .expect("failed to inspect newly created user count");
    assert_eq!(new_count, 1);
}

#[tokio::test]
async fn test_user_admin_cannot_grant_admin_via_set_admin() {
    let _guard = test_mutex().lock().await;
    let Some((app, pool)) = setup_test_app().await else {
        return;
    };

    let (user_admin_token, user_admin_id) =
        register_user(&app, &format!("user_admin_{}", rand::random::<u32>()))
            .await
            .expect("failed to register user admin");
    promote_to_admin_with_role(&pool, &user_admin_id, "user_admin").await;

    let (_, target_user_id) = register_user(&app, &format!("target_user_{}", rand::random::<u32>()))
        .await
        .expect("failed to register target user");

    let request = Request::builder()
        .method("PUT")
        .uri(format!("/_synapse/admin/v1/users/{}/admin", target_user_id))
        .header("Authorization", format!("Bearer {}", user_admin_token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "admin": true }).to_string()))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .expect("set_admin request failed");

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let body = axum::body::to_bytes(response.into_body(), 4096)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["errcode"], "M_FORBIDDEN");

    let target_admin: Option<bool> = sqlx::query_scalar("SELECT is_admin FROM users WHERE user_id = $1")
        .bind(&target_user_id)
        .fetch_optional(&*pool)
        .await
        .expect("failed to query target admin status");
    assert_eq!(target_admin, Some(false));
}

#[tokio::test]
async fn test_user_admin_cannot_assign_admin_fields_via_user_update_v2() {
    let _guard = test_mutex().lock().await;
    let Some((app, pool)) = setup_test_app().await else {
        return;
    };

    let (user_admin_token, user_admin_id) =
        register_user(&app, &format!("user_admin_v2_{}", rand::random::<u32>()))
            .await
            .expect("failed to register user admin");
    promote_to_admin_with_role(&pool, &user_admin_id, "user_admin").await;

    let (_, target_user_id) =
        register_user(&app, &format!("target_v2_{}", rand::random::<u32>()))
            .await
            .expect("failed to register target user");

    let request = Request::builder()
        .method("PUT")
        .uri(format!("/_synapse/admin/v2/users/{}", target_user_id))
        .header("Authorization", format!("Bearer {}", user_admin_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "displayname": "still allowed fields exist",
                "admin": true,
                "user_type": "super_admin"
            })
            .to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .expect("create_or_update_user_v2 request failed");

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let body = axum::body::to_bytes(response.into_body(), 4096)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["errcode"], "M_FORBIDDEN");

    let row = sqlx::query("SELECT is_admin, user_type, displayname FROM users WHERE user_id = $1")
        .bind(&target_user_id)
        .fetch_one(&*pool)
        .await
        .expect("failed to query updated user");
    assert!(!row.get::<bool, _>("is_admin"));
    assert_eq!(row.get::<Option<String>, _>("user_type"), None);
    assert_eq!(row.get::<Option<String>, _>("displayname"), None);
}

#[tokio::test]
async fn test_super_admin_can_grant_admin_via_set_admin() {
    let _guard = test_mutex().lock().await;
    let Some((app, pool)) = setup_test_app().await else {
        return;
    };

    let (super_admin_token, super_admin_id) =
        register_user(&app, &format!("super_admin_{}", rand::random::<u32>()))
            .await
            .expect("failed to register super admin");
    promote_to_admin_with_role(&pool, &super_admin_id, "super_admin").await;

    let (_, target_user_id) =
        register_user(&app, &format!("target_promote_{}", rand::random::<u32>()))
            .await
            .expect("failed to register target user");

    let request = Request::builder()
        .method("PUT")
        .uri(format!("/_synapse/admin/v1/users/{}/admin", target_user_id))
        .header("Authorization", format!("Bearer {}", super_admin_token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "admin": true }).to_string()))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .expect("set_admin request failed");

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 4096)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["success"], true);

    let target_admin: Option<bool> = sqlx::query_scalar("SELECT is_admin FROM users WHERE user_id = $1")
        .bind(&target_user_id)
        .fetch_optional(&*pool)
        .await
        .expect("failed to query target admin status");
    assert_eq!(target_admin, Some(true));
}

#[tokio::test]
async fn test_appservice_post_put_delete_enforce_unauth_user_admin_states() {
    let _guard = test_mutex().lock().await;
    let Some((app, pool)) = setup_test_app().await else {
        return;
    };

    let (admin_token, admin_user_id) =
        register_user(&app, &format!("appservice_admin_{}", rand::random::<u32>()))
            .await
            .expect("failed to register admin user");
    promote_to_admin(&pool, &admin_user_id).await;

    let (user_token, _) = register_user(&app, &format!("appservice_user_{}", rand::random::<u32>()))
        .await
        .expect("failed to register regular user");

    let as_id = format!("admin_regression_as_{}", rand::random::<u32>());
    let create_body = json!({
        "id": as_id,
        "url": "http://localhost:8081",
        "as_token": format!("as_token_{}", rand::random::<u32>()),
        "hs_token": format!("hs_token_{}", rand::random::<u32>()),
        "sender_localpart": format!("bot_{}", rand::random::<u32>()),
        "description": "admin regression service",
        "rate_limited": false,
        "protocols": ["regression"]
    })
    .to_string();

    let unauthenticated_create = Request::builder()
        .method("POST")
        .uri("/_synapse/admin/v1/appservices")
        .header("Content-Type", "application/json")
        .body(Body::from(create_body.clone()))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), unauthenticated_create)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let regular_user_create = Request::builder()
        .method("POST")
        .uri("/_synapse/admin/v1/appservices")
        .header("Authorization", format!("Bearer {}", user_token))
        .header("Content-Type", "application/json")
        .body(Body::from(create_body.clone()))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), regular_user_create)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let admin_create = Request::builder()
        .method("POST")
        .uri("/_synapse/admin/v1/appservices")
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("Content-Type", "application/json")
        .body(Body::from(create_body))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), admin_create)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let update_body = json!({
        "description": "updated regression service",
        "url": "http://localhost:9090",
        "is_enabled": true
    })
    .to_string();

    let unauthenticated_update = Request::builder()
        .method("PUT")
        .uri(format!("/_synapse/admin/v1/appservices/{}", as_id))
        .header("Content-Type", "application/json")
        .body(Body::from(update_body.clone()))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), unauthenticated_update)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let regular_user_update = Request::builder()
        .method("PUT")
        .uri(format!("/_synapse/admin/v1/appservices/{}", as_id))
        .header("Authorization", format!("Bearer {}", user_token))
        .header("Content-Type", "application/json")
        .body(Body::from(update_body.clone()))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), regular_user_update)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let admin_update = Request::builder()
        .method("PUT")
        .uri(format!("/_synapse/admin/v1/appservices/{}", as_id))
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("Content-Type", "application/json")
        .body(Body::from(update_body))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), admin_update)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 8192)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["as_id"], as_id);
    assert_eq!(json["url"], "http://localhost:9090");
    assert_eq!(json["description"], "updated regression service");

    let unauthenticated_delete = Request::builder()
        .method("DELETE")
        .uri(format!("/_synapse/admin/v1/appservices/{}", as_id))
        .body(Body::empty())
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), unauthenticated_delete)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let regular_user_delete = Request::builder()
        .method("DELETE")
        .uri(format!("/_synapse/admin/v1/appservices/{}", as_id))
        .header("Authorization", format!("Bearer {}", user_token))
        .body(Body::empty())
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), regular_user_delete)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let admin_delete = Request::builder()
        .method("DELETE")
        .uri(format!("/_synapse/admin/v1/appservices/{}", as_id))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), admin_delete)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let fetch_deleted = Request::builder()
        .uri(format!("/_synapse/admin/v1/appservices/{}", as_id))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app, fetch_deleted)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_scattered_admin_routes_apply_request_id_middleware() {
    let _guard = test_mutex().lock().await;
    let Some((app, pool)) = setup_test_app().await else {
        return;
    };

    let (admin_token, admin_user_id) = register_user(
        &app,
        &format!("scattered_admin_middleware_{}", rand::random::<u32>()),
    )
    .await
    .expect("failed to register admin user");
    promote_to_admin(&pool, &admin_user_id).await;

    let request_id = "req-scattered-admin-route";
    let request = Request::builder()
        .uri("/_synapse/admin/v1/appservices")
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("x-request-id", request_id)
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app, request)
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get("x-request-id")
            .and_then(|value| value.to_str().ok()),
        Some(request_id)
    );
}

#[tokio::test]
async fn test_external_service_update_enforces_unauth_user_admin_states() {
    let _guard = test_mutex().lock().await;
    let Some((app, pool)) = setup_test_app().await else {
        return;
    };

    let (admin_token, admin_user_id) = register_user(
        &app,
        &format!("external_service_update_admin_{}", rand::random::<u32>()),
    )
    .await
    .expect("failed to register admin user");
    promote_to_admin(&pool, &admin_user_id).await;

    let (user_token, _) = register_user(
        &app,
        &format!("external_service_update_user_{}", rand::random::<u32>()),
    )
    .await
    .expect("failed to register regular user");

    let service_id = format!("external_service_update_{}", rand::random::<u32>());
    let as_id = format!("generic_webhook_{}", service_id);
    let register_body = json!({
        "service_type": "webhook",
        "service_id": service_id,
        "display_name": "Webhook Update Regression",
        "webhook_url": "https://example.com/original",
        "api_key": "initial-api-key",
        "config": {
            "webhook_secret": "initial-secret"
        }
    })
    .to_string();

    let register_request = Request::builder()
        .method("POST")
        .uri("/_synapse/admin/v1/external_services")
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("Content-Type", "application/json")
        .body(Body::from(register_body))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), register_request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let update_body = json!({
        "webhook_url": "https://example.com/updated",
        "api_key": "rotated-api-key",
        "config": {
            "webhook_secret": "rotated-secret"
        },
        "is_enabled": false
    })
    .to_string();

    let unauthenticated_update = Request::builder()
        .method("PUT")
        .uri(format!("/_synapse/admin/v1/external_services/{}", as_id))
        .header("Content-Type", "application/json")
        .body(Body::from(update_body.clone()))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), unauthenticated_update)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let regular_user_update = Request::builder()
        .method("PUT")
        .uri(format!("/_synapse/admin/v1/external_services/{}", as_id))
        .header("Authorization", format!("Bearer {}", user_token))
        .header("Content-Type", "application/json")
        .body(Body::from(update_body.clone()))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), regular_user_update)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let admin_update = Request::builder()
        .method("PUT")
        .uri(format!("/_synapse/admin/v1/external_services/{}", as_id))
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("Content-Type", "application/json")
        .body(Body::from(update_body))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), admin_update)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let row = sqlx::query_as::<_, (String, Option<String>, serde_json::Value, bool)>(
        "SELECT url, api_key, config, is_enabled FROM application_services WHERE as_id = $1",
    )
    .bind(&as_id)
    .fetch_one(&*pool)
    .await
    .expect("failed to load updated external service");

    assert_eq!(row.0, "https://example.com/updated");
    assert_eq!(row.1.as_deref(), Some("rotated-api-key"));
    assert_eq!(row.2["webhook_secret"], "rotated-secret");
    assert!(!row.3);
}

#[tokio::test]
async fn test_external_service_delete_enforces_unauth_user_admin_states() {
    let _guard = test_mutex().lock().await;
    let Some((app, pool)) = setup_test_app().await else {
        return;
    };

    let (admin_token, admin_user_id) = register_user(
        &app,
        &format!("external_service_delete_admin_{}", rand::random::<u32>()),
    )
    .await
    .expect("failed to register admin user");
    promote_to_admin(&pool, &admin_user_id).await;

    let (user_token, _) = register_user(
        &app,
        &format!("external_service_delete_user_{}", rand::random::<u32>()),
    )
    .await
    .expect("failed to register regular user");

    let service_id = format!("external_service_delete_{}", rand::random::<u32>());
    let as_id = format!("generic_webhook_{}", service_id);
    let register_body = json!({
        "service_type": "webhook",
        "service_id": service_id,
        "display_name": "Webhook Delete Regression",
        "webhook_url": "https://example.com/delete",
        "api_key": "delete-api-key"
    })
    .to_string();

    let register_request = Request::builder()
        .method("POST")
        .uri("/_synapse/admin/v1/external_services")
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("Content-Type", "application/json")
        .body(Body::from(register_body))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), register_request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let unauthenticated_delete = Request::builder()
        .method("DELETE")
        .uri(format!("/_synapse/admin/v1/external_services/{}", as_id))
        .body(Body::empty())
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), unauthenticated_delete)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let regular_user_delete = Request::builder()
        .method("DELETE")
        .uri(format!("/_synapse/admin/v1/external_services/{}", as_id))
        .header("Authorization", format!("Bearer {}", user_token))
        .body(Body::empty())
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), regular_user_delete)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let admin_delete = Request::builder()
        .method("DELETE")
        .uri(format!("/_synapse/admin/v1/external_services/{}", as_id))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), admin_delete)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let deleted = sqlx::query_scalar::<_, String>(
        "SELECT as_id FROM application_services WHERE as_id = $1",
    )
    .bind(&as_id)
    .fetch_optional(&*pool)
    .await
    .expect("failed to query deleted external service");
    assert!(deleted.is_none());
}

#[tokio::test]
async fn test_external_webhook_requires_persisted_service_token() {
    let _guard = test_mutex().lock().await;
    let Some((app, pool)) = setup_test_app().await else {
        return;
    };

    let (admin_token, admin_user_id) = register_user(
        &app,
        &format!("external_webhook_admin_{}", rand::random::<u32>()),
    )
    .await
    .expect("failed to register admin user");
    promote_to_admin(&pool, &admin_user_id).await;

    let service_id = format!("webhook_regression_{}", rand::random::<u32>());
    let register_body = json!({
        "service_type": "webhook",
        "service_id": service_id,
        "display_name": "Webhook Regression",
        "webhook_url": "https://example.com/webhook",
        "api_key": "external-webhook-api-key"
    })
    .to_string();

    let register_request = Request::builder()
        .method("POST")
        .uri("/_synapse/admin/v1/external_services")
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("Content-Type", "application/json")
        .body(Body::from(register_body))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), register_request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let webhook_body = json!({
        "event_type": "m.room.message",
        "timestamp": chrono::Utc::now().timestamp_millis(),
        "data": {
            "room_id": format!("!external_webhook_{}:example.com", rand::random::<u32>()),
            "body": "regression"
        }
    })
    .to_string();

    let unauthenticated_request = Request::builder()
        .method("POST")
        .uri(format!("/_synapse/external/webhook/{}", service_id))
        .header("Content-Type", "application/json")
        .body(Body::from(webhook_body.clone()))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), unauthenticated_request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let authenticated_request = Request::builder()
        .method("POST")
        .uri(format!("/_synapse/external/webhook/{}", service_id))
        .header("x-api-key", "external-webhook-api-key")
        .header("Content-Type", "application/json")
        .body(Body::from(webhook_body))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app, authenticated_request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_external_webhook_hmac_uses_updated_persisted_secret() {
    let _guard = test_mutex().lock().await;
    let Some((app, pool)) = setup_test_app().await else {
        return;
    };

    let (admin_token, admin_user_id) = register_user(
        &app,
        &format!("external_webhook_hmac_admin_{}", rand::random::<u32>()),
    )
    .await
    .expect("failed to register admin user");
    promote_to_admin(&pool, &admin_user_id).await;

    let service_id = format!("webhook_hmac_{}", rand::random::<u32>());
    let as_id = format!("generic_webhook_{}", service_id);
    let register_body = json!({
        "service_type": "webhook",
        "service_id": service_id,
        "display_name": "Webhook Hmac",
        "webhook_url": "https://example.com/webhook",
        "config": {
            "webhook_secret": "old-secret"
        }
    })
    .to_string();

    let register_request = Request::builder()
        .method("POST")
        .uri("/_synapse/admin/v1/external_services")
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("Content-Type", "application/json")
        .body(Body::from(register_body))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), register_request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let update_body = json!({
        "config": {
            "webhook_secret": "new-secret"
        }
    })
    .to_string();
    let update_request = Request::builder()
        .method("PUT")
        .uri(format!("/_synapse/admin/v1/external_services/{}", as_id))
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("Content-Type", "application/json")
        .body(Body::from(update_body))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), update_request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let webhook_payload = json!({
        "event_type": "m.room.message",
        "timestamp": chrono::Utc::now().timestamp_millis(),
        "data": {
            "room_id": format!("!external_webhook_hmac_{}:example.com", rand::random::<u32>()),
            "body": "signed-regression"
        }
    });

    let old_signature = format!(
        "sha256={}",
        URL_SAFE_NO_PAD.encode(synapse_rust::common::crypto::hmac_sha256(
            "old-secret",
            &serde_json::to_vec(&webhook_payload).unwrap(),
        ))
    );
    let old_request = Request::builder()
        .method("POST")
        .uri(format!("/_synapse/external/webhook/{}", service_id))
        .header("x-webhook-signature", old_signature)
        .header("Content-Type", "application/json")
        .body(Body::from(webhook_payload.to_string()))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), old_request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let new_signature = format!(
        "sha256={}",
        URL_SAFE_NO_PAD.encode(synapse_rust::common::crypto::hmac_sha256(
            "new-secret",
            &serde_json::to_vec(&webhook_payload).unwrap(),
        ))
    );
    let new_request = Request::builder()
        .method("POST")
        .uri(format!("/_synapse/external/webhook/{}", service_id))
        .header("x-webhook-signature", new_signature)
        .header("Content-Type", "application/json")
        .body(Body::from(webhook_payload.to_string()))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app, new_request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_external_webhook_accepts_payload_embedded_signature() {
    let _guard = test_mutex().lock().await;
    let Some((app, pool)) = setup_test_app().await else {
        return;
    };

    let (admin_token, admin_user_id) = register_user(
        &app,
        &format!("external_webhook_payload_sig_admin_{}", rand::random::<u32>()),
    )
    .await
    .expect("failed to register admin user");
    promote_to_admin(&pool, &admin_user_id).await;

    let service_id = format!("webhook_payload_sig_{}", rand::random::<u32>());
    let register_body = json!({
        "service_type": "webhook",
        "service_id": service_id,
        "display_name": "Webhook Payload Signature",
        "webhook_url": "https://example.com/webhook",
        "config": {
            "webhook_secret": "payload-secret"
        }
    })
    .to_string();

    let register_request = Request::builder()
        .method("POST")
        .uri("/_synapse/admin/v1/external_services")
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("Content-Type", "application/json")
        .body(Body::from(register_body))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), register_request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let unsigned_payload = json!({
        "event_type": "m.room.message",
        "timestamp": chrono::Utc::now().timestamp_millis(),
        "data": {
            "room_id": format!("!external_payload_sig_{}:example.com", rand::random::<u32>()),
            "body": "payload-signature"
        }
    });
    let signature = format!(
        "sha256={}",
        URL_SAFE_NO_PAD.encode(synapse_rust::common::crypto::hmac_sha256(
            "payload-secret",
            &serde_json::to_vec(&unsigned_payload).unwrap(),
        ))
    );
    let signed_payload = json!({
        "event_type": unsigned_payload["event_type"],
        "timestamp": unsigned_payload["timestamp"],
        "data": unsigned_payload["data"],
        "signature": signature
    })
    .to_string();

    let request = Request::builder()
        .method("POST")
        .uri(format!("/_synapse/external/webhook/{}", service_id))
        .header("Content-Type", "application/json")
        .body(Body::from(signed_payload))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app, request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

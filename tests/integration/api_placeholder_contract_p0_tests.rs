use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use std::sync::Arc;
use tower::ServiceExt;

async fn setup_test_app_with_pool() -> (axum::Router, Arc<sqlx::PgPool>) {
    use synapse_rust::cache::CacheManager;
    use synapse_rust::services::ServiceContainer;
    use synapse_rust::web::routes::state::AppState;

    let pool = super::require_test_pool().await;
    let container = ServiceContainer::new_test_with_pool(pool.clone());
    let cache = std::sync::Arc::new(CacheManager::new(Default::default()));
    let state = AppState::new(container, cache);

    (synapse_rust::web::create_router(state), pool)
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
                "auth": { "type": "m.login.dummy" }
            })
            .to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .unwrap();
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

fn encode_room_id(room_id: &str) -> String {
    room_id.replace('!', "%21").replace(':', "%3A")
}

async fn assert_matrix_error(
    app: &axum::Router,
    request: Request<Body>,
    expected_status: StatusCode,
    expected_errcode: &str,
) {
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), expected_status);

    let body = axum::body::to_bytes(response.into_body(), 16 * 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["errcode"], expected_errcode);
}

async fn assert_matrix_error_with_body(
    app: &axum::Router,
    request: Request<Body>,
    expected_status: StatusCode,
    expected_errcode: &str,
) -> Value {
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 16 * 1024)
        .await
        .unwrap();
    let body_text = String::from_utf8_lossy(&body);
    assert_eq!(
        status, expected_status,
        "unexpected status with body: {}",
        body_text
    );

    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["errcode"], expected_errcode);
    json
}

#[tokio::test]
async fn test_push_rules_scope_contract_rejects_non_global_scope() {
    let app = super::setup_test_app()
        .await
        .expect("P0 placeholder contract test requires integration database setup");

    let username = format!("push_scope_{}", rand::random::<u32>());
    let (token, _) = register_user(&app, &username).await;

    for path in [
        "/_matrix/client/r0/pushrules/device",
        "/_matrix/client/v3/pushrules/device",
    ] {
        assert_matrix_error(
            &app,
            Request::builder()
                .method("GET")
                .uri(path)
                .header("Authorization", format!("Bearer {}", token))
                .body(Body::empty())
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "M_INVALID_PARAM",
        )
        .await;
    }
}

#[tokio::test]
async fn test_directory_room_alias_contract_returns_not_found_for_missing_alias() {
    let app = super::setup_test_app()
        .await
        .expect("P0 placeholder contract test requires integration database setup");

    let username = format!("alias_lookup_{}", rand::random::<u32>());
    let (token, _) = register_user(&app, &username).await;
    let alias = format!("#missing-alias-{}:localhost", rand::random::<u32>());
    let encoded_alias = urlencoding::encode(&alias);

    for path in [
        format!("/_matrix/client/r0/directory/room/{}", encoded_alias),
        format!("/_matrix/client/v3/directory/room/{}", encoded_alias),
    ] {
        assert_matrix_error(
            &app,
            Request::builder()
                .method("GET")
                .uri(path)
                .header("Authorization", format!("Bearer {}", token))
                .body(Body::empty())
                .unwrap(),
            StatusCode::NOT_FOUND,
            "M_NOT_FOUND",
        )
        .await;
    }
}

#[tokio::test]
async fn test_account_data_contract_returns_not_found_for_missing_custom_type() {
    let app = super::setup_test_app()
        .await
        .expect("P0 placeholder contract test requires integration database setup");

    let username = format!("account_missing_{}", rand::random::<u32>());
    let (token, user_id) = register_user(&app, &username).await;
    let data_type = format!("com.example.missing.{}", rand::random::<u32>());

    for path in [
        format!(
            "/_matrix/client/r0/user/{}/account_data/{}",
            user_id, data_type
        ),
        format!(
            "/_matrix/client/v3/user/{}/account_data/{}",
            user_id, data_type
        ),
    ] {
        assert_matrix_error(
            &app,
            Request::builder()
                .method("GET")
                .uri(path)
                .header("Authorization", format!("Bearer {}", token))
                .body(Body::empty())
                .unwrap(),
            StatusCode::NOT_FOUND,
            "M_NOT_FOUND",
        )
        .await;
    }
}

#[tokio::test]
async fn test_room_key_distribution_contract_returns_not_found_without_session() {
    let app = super::setup_test_app()
        .await
        .expect("P0 placeholder contract test requires integration database setup");

    let username = format!("key_dist_{}", rand::random::<u32>());
    let (token, _) = register_user(&app, &username).await;
    let room_id = create_room(&app, &token, "Key Distribution Contract").await;
    let encoded_room_id = encode_room_id(&room_id);

    for path in [
        format!(
            "/_matrix/client/r0/rooms/{}/keys/distribution",
            encoded_room_id
        ),
        format!(
            "/_matrix/client/v3/rooms/{}/keys/distribution",
            encoded_room_id
        ),
    ] {
        assert_matrix_error(
            &app,
            Request::builder()
                .method("GET")
                .uri(path)
                .header("Authorization", format!("Bearer {}", token))
                .body(Body::empty())
                .unwrap(),
            StatusCode::NOT_FOUND,
            "M_NOT_FOUND",
        )
        .await;
    }
}

#[tokio::test]
async fn test_report_room_contract_returns_unrecognized() {
    let app = super::setup_test_app()
        .await
        .expect("P0 placeholder contract test requires integration database setup");

    let username = format!("report_room_{}", rand::random::<u32>());
    let (token, _) = register_user(&app, &username).await;
    let room_id = create_room(&app, &token, "Room Report Contract").await;

    assert_matrix_error(
        &app,
        Request::builder()
            .method("POST")
            .uri(format!("/_matrix/client/v3/rooms/{}/report", room_id))
            .header("Authorization", format!("Bearer {}", token))
            .header("Content-Type", "application/json")
            .body(Body::from(
                json!({
                    "reason": "contract check",
                    "description": "should be explicit unsupported"
                })
                .to_string(),
            ))
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "M_UNRECOGNIZED",
    )
    .await;
}

#[tokio::test]
async fn test_sync_events_contract_surfaces_service_errors() {
    let (app, pool) = setup_test_app_with_pool().await;

    let username = format!("sync_events_{}", rand::random::<u32>());
    let (token, user_id) = register_user(&app, &username).await;
    let room_id = create_room(&app, &token, "Sync Events Contract").await;
    let now = chrono::Utc::now().timestamp_millis();

    sqlx::query(
        r#"
        INSERT INTO room_memberships (room_id, user_id, membership, joined_ts)
        VALUES ($1, $2, 'join', $3)
        ON CONFLICT (room_id, user_id) DO UPDATE SET
            membership = EXCLUDED.membership,
            joined_ts = EXCLUDED.joined_ts
        "#,
    )
    .bind(&room_id)
    .bind(&user_id)
    .bind(now)
    .execute(&*pool)
    .await
    .unwrap();

    let joined_rooms: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM room_memberships WHERE room_id = $1 AND user_id = $2 AND membership = 'join'",
    )
    .bind(&room_id)
    .bind(&user_id)
    .fetch_one(&*pool)
    .await
    .unwrap();
    assert!(
        joined_rooms > 0,
        "test setup must ensure joined room exists"
    );

    sqlx::query("DROP TABLE events CASCADE")
        .execute(&*pool)
        .await
        .unwrap();

    let json = assert_matrix_error_with_body(
        &app,
        Request::builder()
            .method("GET")
            .uri("/_matrix/client/v3/events?from=s0&timeout=1")
            .header("Authorization", format!("Bearer {}", token))
            .body(Body::empty())
            .unwrap(),
        StatusCode::INTERNAL_SERVER_ERROR,
        "M_UNKNOWN",
    )
    .await;

    assert!(json.get("chunk").is_none());
}

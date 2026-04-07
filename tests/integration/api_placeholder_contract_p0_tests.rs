use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;

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
                "body": "contract message"
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
    json["event_id"].as_str().unwrap().to_string()
}

async fn create_thread(
    app: &axum::Router,
    token: &str,
    room_id: &str,
    root_event_id: &str,
) -> String {
    let request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/v1/rooms/{}/threads", room_id))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "root_event_id": root_event_id,
                "content": { "body": "thread" }
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
    json["thread_id"].as_str().unwrap().to_string()
}

async fn add_thread_reply(
    app: &axum::Router,
    token: &str,
    room_id: &str,
    thread_id: &str,
    root_event_id: &str,
    event_id: &str,
) {
    let request = Request::builder()
        .method("POST")
        .uri(format!(
            "/_matrix/client/v1/rooms/{}/threads/{}/replies",
            room_id, thread_id
        ))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "event_id": event_id,
                "root_event_id": root_event_id,
                "content": {
                    "msgtype": "m.text",
                    "body": "thread reply"
                }
            })
            .to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
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
    let username = format!("sync_events_{}", rand::random::<u32>());
    let app = super::setup_test_app()
        .await
        .expect("P0 placeholder contract test requires integration database setup");
    let (token, _) = register_user(&app, &username).await;

    let json = assert_matrix_error_with_body(
        &app,
        Request::builder()
            .method("GET")
            .uri("/_matrix/client/v3/events?from=invalid-from-token&timeout=1")
            .header("Authorization", format!("Bearer {}", token))
            .body(Body::empty())
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "M_INVALID_PARAM",
    )
    .await;

    assert!(json.get("chunk").is_none());
}

#[tokio::test]
async fn test_room_event_keys_contract_rejects_invalid_event_id() {
    let app = super::setup_test_app()
        .await
        .expect("P0 placeholder contract test requires integration database setup");

    let username = format!("event_keys_{}", rand::random::<u32>());
    let (token, _) = register_user(&app, &username).await;
    let room_id = create_room(&app, &token, "Event Keys Contract").await;
    let encoded_room_id = encode_room_id(&room_id);

    assert_matrix_error(
        &app,
        Request::builder()
            .method("GET")
            .uri(format!(
                "/_matrix/client/v3/rooms/{}/keys/invalid-event-id",
                encoded_room_id
            ))
            .header("Authorization", format!("Bearer {}", token))
            .body(Body::empty())
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "M_INVALID_PARAM",
    )
    .await;
}

#[tokio::test]
async fn test_room_thread_contract_rejects_invalid_event_id() {
    let app = super::setup_test_app()
        .await
        .expect("P0 placeholder contract test requires integration database setup");

    let username = format!("thread_invalid_{}", rand::random::<u32>());
    let (token, _) = register_user(&app, &username).await;
    let room_id = create_room(&app, &token, "Room Thread Contract").await;
    let encoded_room_id = encode_room_id(&room_id);

    assert_matrix_error(
        &app,
        Request::builder()
            .method("GET")
            .uri(format!(
                "/_matrix/client/v3/rooms/{}/thread/invalid-event-id",
                encoded_room_id
            ))
            .header("Authorization", format!("Bearer {}", token))
            .body(Body::empty())
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "M_INVALID_PARAM",
    )
    .await;
}

#[tokio::test]
async fn test_room_thread_contract_returns_replies_when_thread_exists() {
    let app = super::setup_test_app()
        .await
        .expect("P0 placeholder contract test requires integration database setup");

    let username = format!("thread_real_{}", rand::random::<u32>());
    let (token, _) = register_user(&app, &username).await;
    let room_id = create_room(&app, &token, "Room Thread Replies Contract").await;
    let root_event_id = send_message(&app, &token, &room_id, "thread_root_txn").await;
    let thread_id = create_thread(&app, &token, &room_id, &root_event_id).await;
    let reply_event_id = format!("$thread_reply_{}", rand::random::<u64>());
    add_thread_reply(
        &app,
        &token,
        &room_id,
        &thread_id,
        &root_event_id,
        &reply_event_id,
    )
    .await;

    let encoded_room_id = encode_room_id(&room_id);
    let response = ServiceExt::<Request<Body>>::oneshot(
        app.clone(),
        Request::builder()
            .method("GET")
            .uri(format!(
                "/_matrix/client/v3/rooms/{}/thread/{}",
                encoded_room_id,
                root_event_id.replace('$', "%24")
            ))
            .header("Authorization", format!("Bearer {}", token))
            .body(Body::empty())
            .unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 16 * 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert!(json["reply_count"].as_i64().unwrap_or(0) >= 1);
    assert!(json["replies"].as_array().unwrap().len() >= 1);
}

#[tokio::test]
async fn test_room_initial_sync_contract_is_explicitly_unrecognized() {
    let app = super::setup_test_app()
        .await
        .expect("P0 placeholder contract test requires integration database setup");

    let username = format!("initial_sync_{}", rand::random::<u32>());
    let (token, _) = register_user(&app, &username).await;
    let room_id = create_room(&app, &token, "Initial Sync Contract").await;
    let encoded_room_id = encode_room_id(&room_id);

    assert_matrix_error(
        &app,
        Request::builder()
            .method("GET")
            .uri(format!(
                "/_matrix/client/r0/rooms/{}/initialSync",
                encoded_room_id
            ))
            .header("Authorization", format!("Bearer {}", token))
            .body(Body::empty())
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "M_UNRECOGNIZED",
    )
    .await;
}

#[tokio::test]
async fn test_receipt_contract_rejects_invalid_event_id_and_receipt_type() {
    let app = super::setup_test_app()
        .await
        .expect("P0 placeholder contract test requires integration database setup");

    let username = format!("receipt_contract_{}", rand::random::<u32>());
    let (token, _) = register_user(&app, &username).await;
    let room_id = create_room(&app, &token, "Receipt Contract").await;
    let encoded_room_id = encode_room_id(&room_id);

    assert_matrix_error(
        &app,
        Request::builder()
            .method("POST")
            .uri(format!(
                "/_matrix/client/r0/rooms/{}/receipt/m.read/invalid-event-id",
                encoded_room_id
            ))
            .header("Authorization", format!("Bearer {}", token))
            .header("Content-Type", "application/json")
            .body(Body::from(json!({}).to_string()))
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "M_INVALID_PARAM",
    )
    .await;

    assert_matrix_error(
        &app,
        Request::builder()
            .method("GET")
            .uri(format!(
                "/_matrix/client/r0/rooms/{}/receipts/invalid-receipt/{event_id}",
                encoded_room_id,
                event_id = "$event:localhost".replace('$', "%24")
            ))
            .header("Authorization", format!("Bearer {}", token))
            .body(Body::empty())
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "M_INVALID_PARAM",
    )
    .await;
}

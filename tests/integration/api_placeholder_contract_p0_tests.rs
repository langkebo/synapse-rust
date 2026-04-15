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
    let Some(app) = super::setup_test_app().await else {
        return;
    };

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
    let Some(app) = super::setup_test_app().await else {
        return;
    };

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
    let Some(app) = super::setup_test_app().await else {
        return;
    };

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
    let Some(app) = super::setup_test_app().await else {
        return;
    };

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
async fn test_room_key_distribution_contract_rejects_non_members() {
    let Some(app) = super::setup_test_app().await else {
        return;
    };

    let owner_username = format!("key_dist_owner_{}", rand::random::<u32>());
    let attacker_username = format!("key_dist_attacker_{}", rand::random::<u32>());
    let (owner_token, _) = register_user(&app, &owner_username).await;
    let (attacker_token, _) = register_user(&app, &attacker_username).await;
    let room_id = create_room(&app, &owner_token, "Key Distribution Access Control").await;
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
                .header("Authorization", format!("Bearer {}", attacker_token))
                .body(Body::empty())
                .unwrap(),
            StatusCode::FORBIDDEN,
            "M_FORBIDDEN",
        )
        .await;
    }
}

#[tokio::test]
async fn test_change_password_uia_rejects_dummy_auth() {
    let Some(app) = super::setup_test_app().await else {
        return;
    };

    let username = format!("change_password_dummy_{}", rand::random::<u32>());
    let (token, _) = register_user(&app, &username).await;

    assert_matrix_error(
        &app,
        Request::builder()
            .method("POST")
            .uri("/_matrix/client/v3/account/password")
            .header("Authorization", format!("Bearer {}", token))
            .header("Content-Type", "application/json")
            .body(Body::from(
                json!({
                    "new_password": "NewPassword123!",
                    "auth": { "type": "m.login.dummy" }
                })
                .to_string(),
            ))
            .unwrap(),
        StatusCode::UNAUTHORIZED,
        "M_UNAUTHORIZED",
    )
    .await;
}

#[tokio::test]
async fn test_key_rotation_contract_rejects_non_admin_users() {
    let Some(app) = super::setup_test_app().await else {
        return;
    };

    let username = format!("key_rotation_user_{}", rand::random::<u32>());
    let (token, _) = register_user(&app, &username).await;

    for (method, path) in [
        ("POST", "/_matrix/client/v1/keys/rotation/rotate"),
        ("PUT", "/_matrix/client/v1/keys/rotation/config"),
    ] {
        let body = match method {
            "PUT" => json!({ "enabled": true, "interval_days": 30 }).to_string(),
            _ => "{}".to_string(),
        };

        assert_matrix_error(
            &app,
            Request::builder()
                .method(method)
                .uri(path)
                .header("Authorization", format!("Bearer {}", token))
                .header("Content-Type", "application/json")
                .body(Body::from(body))
                .unwrap(),
            StatusCode::FORBIDDEN,
            "M_FORBIDDEN",
        )
        .await;
    }
}

#[tokio::test]
async fn test_key_rotation_config_contract_returns_unrecognized_for_admin() {
    let Some(app) = super::setup_test_app().await else {
        return;
    };

    let (admin_token, _) = super::get_admin_token(&app).await;

    assert_matrix_error(
        &app,
        Request::builder()
            .method("PUT")
            .uri("/_matrix/client/v1/keys/rotation/config")
            .header("Authorization", format!("Bearer {}", admin_token))
            .header("Content-Type", "application/json")
            .body(Body::from(
                json!({ "enabled": true, "interval_days": 30 }).to_string(),
            ))
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "M_UNRECOGNIZED",
    )
    .await;
}

#[tokio::test]
async fn test_admin_server_placeholder_contract_returns_unrecognized_for_admin() {
    let Some(app) = super::setup_test_app().await else {
        return;
    };

    let (admin_token, _) = super::get_admin_token(&app).await;

    for path in [
        "/_synapse/admin/v1/backups",
        "/_synapse/admin/v1/experimental_features",
    ] {
        assert_matrix_error(
            &app,
            Request::builder()
                .method("GET")
                .uri(path)
                .header("Authorization", format!("Bearer {}", admin_token))
                .body(Body::empty())
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "M_UNRECOGNIZED",
        )
        .await;
    }
}

#[tokio::test]
async fn test_thirdparty_contract_rejects_builtin_irc_placeholders() {
    let Some(app) = super::setup_test_app().await else {
        return;
    };

    let username = format!("thirdparty_contract_{}", rand::random::<u32>());
    let (token, _) = register_user(&app, &username).await;

    for path in [
        "/_matrix/client/v3/thirdparty/protocols",
        "/_matrix/client/r0/thirdparty/protocol/irc",
        "/_matrix/client/v3/thirdparty/location/irc?alias=%23demo:localhost",
        "/_matrix/client/v3/thirdparty/user/irc?userid=%40alice:localhost",
        "/_matrix/client/v3/thirdparty/location?alias=%23demo:localhost",
        "/_matrix/client/v3/thirdparty/user?userid=%40alice:localhost",
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
            "M_UNRECOGNIZED",
        )
        .await;
    }
}

#[tokio::test]
async fn test_report_room_contract_returns_unrecognized() {
    let Some(app) = super::setup_test_app().await else {
        return;
    };

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
    let Some(app) = super::setup_test_app().await else {
        return;
    };
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
    let Some(app) = super::setup_test_app().await else {
        return;
    };

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
    let Some(app) = super::setup_test_app().await else {
        return;
    };

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
    let Some(app) = super::setup_test_app().await else {
        return;
    };

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
    assert!(!json["replies"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn test_room_initial_sync_contract_returns_state_members_and_messages() {
    let Some(app) = super::setup_test_app().await else {
        return;
    };

    let username = format!("initial_sync_{}", rand::random::<u32>());
    let (token, _) = register_user(&app, &username).await;
    let room_id = create_room(&app, &token, "Initial Sync Contract").await;
    let event_id = send_message(&app, &token, &room_id, "initial_sync_txn").await;
    let encoded_room_id = encode_room_id(&room_id);

    let response = ServiceExt::<Request<Body>>::oneshot(
        app.clone(),
        Request::builder()
            .method("GET")
            .uri(format!(
                "/_matrix/client/r0/rooms/{}/initialSync?limit=5",
                encoded_room_id
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
    assert_eq!(json["room_id"], room_id);
    assert_eq!(json["membership"], "join");
    assert!(matches!(
        json["visibility"].as_str(),
        Some("public" | "private")
    ));
    assert!(json["state"]
        .as_array()
        .is_some_and(|events| !events.is_empty()));
    assert!(json["members"]
        .as_array()
        .is_some_and(|events| !events.is_empty()));
    assert!(json["pagination_chunk"].is_array());
    assert!(json["messages"]["chunk"]
        .as_array()
        .is_some_and(|events| events.iter().any(|event| event["event_id"] == event_id)));
    assert!(json["state"]
        .as_array()
        .is_some_and(|events| events.iter().any(|event| event["type"] == "m.room.create")));
}

#[tokio::test]
async fn test_removed_private_room_placeholder_routes_return_404() {
    let Some(app) = super::setup_test_app().await else {
        return;
    };

    let username = format!("room_placeholder_removed_{}", rand::random::<u32>());
    let (token, _) = register_user(&app, &username).await;
    let room_id = create_room(&app, &token, "Removed Room Placeholder Contract").await;
    let encoded_room_id = encode_room_id(&room_id);

    let cases = vec![
        (
            "GET",
            format!(
                "/_matrix/client/v3/rooms/{}/fragments/@user:localhost",
                encoded_room_id
            ),
        ),
        (
            "GET",
            format!("/_matrix/client/v3/rooms/{}/service_types", encoded_room_id),
        ),
        (
            "GET",
            format!(
                "/_matrix/client/v3/rooms/{}/event_perspective",
                encoded_room_id
            ),
        ),
        (
            "GET",
            format!(
                "/_matrix/client/v3/rooms/{}/reduced_events",
                encoded_room_id
            ),
        ),
        (
            "GET",
            format!("/_matrix/client/v3/rooms/{}/rendered/", encoded_room_id),
        ),
        (
            "POST",
            format!(
                "/_matrix/client/v3/rooms/{}/translate/{}",
                encoded_room_id,
                "$event:localhost".replace('$', "%24")
            ),
        ),
        (
            "POST",
            format!(
                "/_matrix/client/v3/rooms/{}/convert/{}",
                encoded_room_id,
                "$event:localhost".replace('$', "%24")
            ),
        ),
        (
            "GET",
            format!("/_matrix/client/v3/rooms/{}/vault_data", encoded_room_id),
        ),
        (
            "PUT",
            format!("/_matrix/client/v3/rooms/{}/vault_data", encoded_room_id),
        ),
        (
            "GET",
            format!("/_matrix/client/v3/rooms/{}/external_ids", encoded_room_id),
        ),
        (
            "GET",
            format!(
                "/_matrix/client/v3/rooms/{}/device/DEVICEID",
                encoded_room_id
            ),
        ),
    ];

    for (method, uri) in cases {
        let request = Request::builder()
            .method(method)
            .uri(uri)
            .header("Authorization", format!("Bearer {}", token))
            .header("Content-Type", "application/json")
            .body(if method == "PUT" {
                Body::from(json!({}).to_string())
            } else {
                Body::empty()
            })
            .unwrap();

        let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}

#[tokio::test]
async fn test_receipt_contract_rejects_invalid_event_id_and_receipt_type() {
    let Some(app) = super::setup_test_app().await else {
        return;
    };

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

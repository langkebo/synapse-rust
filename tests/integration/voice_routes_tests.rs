use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use base64::Engine as _;
use serde_json::{json, Value};
use tower::ServiceExt;

async fn setup_test_app() -> Option<axum::Router> {
    super::setup_test_app().await
}

async fn create_test_user(app: &axum::Router) -> String {
    let username = format!("user_{}", rand::random::<u32>());
    let password = "Password123!";

    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/r0/register")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "username": username,
                "password": password,
                "auth": { "type": "m.login.dummy" }
            })
            .to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();

    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 10240)
        .await
        .unwrap();

    if status != StatusCode::OK {
        panic!(
            "Registration failed with status {}: {:?}",
            status,
            String::from_utf8_lossy(&body)
        );
    }

    let json: Value = serde_json::from_slice(&body).unwrap();
    json["access_token"].as_str().unwrap().to_string()
}

async fn create_room(app: &axum::Router, token: &str) -> String {
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/createRoom")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({"name": "Voice Room"}).to_string()))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();

    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 10240)
        .await
        .unwrap();

    if status != StatusCode::OK {
        panic!(
            "Create room failed with status {}: {:?}",
            status,
            String::from_utf8_lossy(&body)
        );
    }

    let json: Value = serde_json::from_slice(&body).unwrap();
    json["room_id"].as_str().unwrap().to_string()
}

async fn upload_voice_message(
    app: &axum::Router,
    token: &str,
    room_id: Option<&str>,
) -> (StatusCode, Value) {
    let content = base64::engine::general_purpose::STANDARD
        .encode(include_bytes!("../../docker/media/test/message.mp3"));

    let mut payload = json!({
        "content": content,
        "content_type": "audio/mpeg",
        "duration_ms": 1200
    });

    if let Some(room_id) = room_id {
        payload["room_id"] = json!(room_id);
    }

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/_matrix/client/r0/voice/upload")
                .method("POST")
                .header("Authorization", format!("Bearer {}", token))
                .header("Content-Type", "application/json")
                .body(Body::from(payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    (status, json)
}

#[tokio::test]
async fn test_voice_config_endpoint() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/_matrix/client/r0/voice/config")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();

    if status != StatusCode::OK {
        eprintln!("Error response body: {:?}", String::from_utf8_lossy(&body));
    }

    assert_eq!(status, StatusCode::OK);
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert!(json.get("supported_formats").is_some());
    assert_eq!(json["max_size_bytes"], 52428800);
    assert_eq!(json["content_type"], "m.audio");
    assert_eq!(json["voice_extension"], "org.matrix.msc3245.voice");
}

#[tokio::test]
async fn test_voice_upload_returns_content_uri() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let token = create_test_user(&app).await;
    let room_id = create_room(&app, &token).await;

    let (status, json) = upload_voice_message(&app, &token, Some(&room_id)).await;
    if status != StatusCode::OK {
        eprintln!("voice upload error: {}", json);
    }
    assert_eq!(status, StatusCode::OK);

    assert!(json.get("content_uri").is_some());
    assert!(json["content_uri"]
        .as_str()
        .unwrap_or_default()
        .starts_with("mxc://"));
    assert_eq!(json["content_type"], "audio/mpeg");
    assert_eq!(json["duration_ms"], 1200);
    assert!(json["size"].as_u64().unwrap_or(0) > 0);

    let content = json.get("content").expect("should have event content");
    assert_eq!(content["msgtype"], "m.audio");
    assert!(content.get("org.matrix.msc3245.voice").is_some());
    assert!(content.get("url").is_some());
    assert!(content["info"]["duration"].as_i64().unwrap_or(0) > 0);
}

#[tokio::test]
async fn test_voice_upload_forbid_cross_room_upload() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let owner_token = create_test_user(&app).await;
    let attacker_token = create_test_user(&app).await;
    let room_id = create_room(&app, &owner_token).await;

    let (status, json) = upload_voice_message(&app, &attacker_token, Some(&room_id)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(json["errcode"], "M_FORBIDDEN");
}

#[tokio::test]
async fn test_voice_upload_without_room_succeeds() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let token = create_test_user(&app).await;

    let (status, json) = upload_voice_message(&app, &token, None).await;
    if status != StatusCode::OK {
        eprintln!("voice upload error: {}", json);
    }
    assert_eq!(status, StatusCode::OK);
    assert!(json.get("content_uri").is_some());
}

#[tokio::test]
async fn test_voip_routes_work_across_r0_and_v3() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let token = create_test_user(&app).await;

    let r0_config_request = Request::builder()
        .method("GET")
        .uri("/_matrix/client/r0/voip/config")
        .body(Body::empty())
        .unwrap();
    let r0_config_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), r0_config_request)
        .await
        .unwrap();
    assert_eq!(r0_config_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(r0_config_response.into_body(), 2048)
        .await
        .unwrap();
    let r0_config_json: Value = serde_json::from_slice(&body).unwrap();

    let v3_config_request = Request::builder()
        .method("GET")
        .uri("/_matrix/client/v3/voip/config")
        .body(Body::empty())
        .unwrap();
    let v3_config_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), v3_config_request)
        .await
        .unwrap();
    assert_eq!(v3_config_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(v3_config_response.into_body(), 2048)
        .await
        .unwrap();
    let v3_config_json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(r0_config_json, v3_config_json);

    let r0_turn_request = Request::builder()
        .method("GET")
        .uri("/_matrix/client/r0/voip/turnServer")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let r0_turn_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), r0_turn_request)
        .await
        .unwrap();

    let v3_turn_request = Request::builder()
        .method("GET")
        .uri("/_matrix/client/v3/voip/turnServer")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let v3_turn_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), v3_turn_request)
        .await
        .unwrap();
    assert_eq!(r0_turn_response.status(), v3_turn_response.status());
}

async fn create_call_session(
    app: &axum::Router,
    token: &str,
    room_id: &str,
    call_id: &str,
) -> (StatusCode, String) {
    let encoded_room_id = urlencoding::encode(room_id);

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!(
                    "/_matrix/client/v3/rooms/{}/send/m.call.invite/test_txn",
                    encoded_room_id
                ))
                .header("Authorization", format!("Bearer {}", token))
                .header("Content-Type", "application/json")
                .body(Body::from(
                    json!({
                        "call_id": call_id,
                        "version": 1,
                        "offer": {
                            "type": "offer",
                            "sdp": "v=0\r\no=- 1 2 IN IP4 127.0.0.1\r\n"
                        },
                        "lifetime": 60000
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, String::from_utf8_lossy(&body).to_string())
}

#[tokio::test]
async fn test_call_invite_rejects_non_members() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let owner_token = create_test_user(&app).await;
    let outsider_token = create_test_user(&app).await;
    let room_id = create_room(&app, &owner_token).await;
    let call_id = format!("call_{}", rand::random::<u32>());

    let (status, body) = create_call_session(&app, &outsider_token, &room_id, &call_id).await;

    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "unexpected response body: {}",
        body
    );
    let body: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(body["errcode"], "M_FORBIDDEN");
}

#[tokio::test]
async fn test_get_call_session_rejects_non_members() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let owner_token = create_test_user(&app).await;
    let outsider_token = create_test_user(&app).await;
    let room_id = create_room(&app, &owner_token).await;
    let call_id = format!("call_{}", rand::random::<u32>());

    let (invite_status, invite_body) =
        create_call_session(&app, &owner_token, &room_id, &call_id).await;
    assert_eq!(
        invite_status,
        StatusCode::OK,
        "unexpected invite response body: {}",
        invite_body
    );
    let invite_body: Value = serde_json::from_str(&invite_body).unwrap();
    assert_eq!(invite_body["call_id"], call_id);

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!(
                    "/_matrix/client/v3/rooms/{}/call/{}",
                    urlencoding::encode(&room_id),
                    urlencoding::encode(&call_id)
                ))
                .header("Authorization", format!("Bearer {}", outsider_token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "unexpected get_call_session response body: {}",
        String::from_utf8_lossy(&body)
    );
    assert_eq!(json["errcode"], "M_FORBIDDEN");
}

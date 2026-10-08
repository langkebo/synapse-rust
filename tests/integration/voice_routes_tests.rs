use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;

async fn setup_test_app() -> Option<axum::Router> {
    super::setup_fresh_test_app().await
}

async fn create_test_user(app: &axum::Router) -> String {
    create_test_user_with_id(app).await.0
}

/// Registration returns both halves: the RTC call session authorizes the answer
/// against the callee's **user id**, so a test that exercises `m.call.answer`
/// needs it (the access token is not the user id).
async fn create_test_user_with_id(app: &axum::Router) -> (String, String) {
    let username = format!("user_{}", rand::random::<u32>());
    let password = "Password123!";

    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/register")
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

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();

    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 10240).await.unwrap();

    if status != StatusCode::OK {
        panic!("Registration failed with status {}: {:?}", status, String::from_utf8_lossy(&body));
    }

    let json: Value = serde_json::from_slice(&body).unwrap();
    (
        json["access_token"].as_str().unwrap().to_string(),
        json["user_id"].as_str().expect("registration must return the user_id").to_string(),
    )
}

async fn create_room(app: &axum::Router, token: &str) -> String {
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/createRoom")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({"name": "Voice Room"}).to_string()))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();

    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 10240).await.unwrap();

    if status != StatusCode::OK {
        panic!("Create room failed with status {}: {:?}", status, String::from_utf8_lossy(&body));
    }

    let json: Value = serde_json::from_slice(&body).unwrap();
    json["room_id"].as_str().unwrap().to_string()
}

async fn upload_voice_message(app: &axum::Router, token: &str, room_id: Option<&str>) -> (StatusCode, Value) {
    // P0-2: 后端已切换为 multipart/form-data（提交 a80a0071），测试同步改为 multipart。
    //
    // The value declared in `Content-Type: boundary=` must be *exactly* the token
    // that follows the `--` prefix on every delimiter line. The previous form
    // declared `boundary` with its own four leading dashes and then tried to strip
    // them back off for the header only — but `trim_start_matches("--")` removes
    // *every* leading `--` group, not one, so the header advertised `TestBoundary…`
    // while the body carried `------TestBoundary…`. multer never matched the first
    // delimiter and the handler returned 400 before parsing a single field.
    // Define the token once and use it verbatim in both places.
    const BOUNDARY: &str = "----TestBoundary7MaYWYWzKZzvRP5j";

    let audio_bytes: &[u8] = include_bytes!("../../docker/media/test/message.mp3");
    let delimiter = format!("--{BOUNDARY}\r\n");

    let mut body_bytes: Vec<u8> = Vec::new();

    // file field
    body_bytes.extend_from_slice(delimiter.as_bytes());
    body_bytes.extend_from_slice(b"Content-Disposition: form-data; name=\"file\"; filename=\"test.mp3\"\r\n");
    body_bytes.extend_from_slice(b"Content-Type: audio/mpeg\r\n\r\n");
    body_bytes.extend_from_slice(audio_bytes);
    body_bytes.extend_from_slice(b"\r\n");

    // content_type field
    body_bytes.extend_from_slice(delimiter.as_bytes());
    body_bytes.extend_from_slice(b"Content-Disposition: form-data; name=\"content_type\"\r\n\r\naudio/mpeg\r\n");

    // duration_ms field
    body_bytes.extend_from_slice(delimiter.as_bytes());
    body_bytes.extend_from_slice(b"Content-Disposition: form-data; name=\"duration_ms\"\r\n\r\n1200\r\n");

    // room_id field (optional)
    if let Some(rid) = room_id {
        body_bytes.extend_from_slice(delimiter.as_bytes());
        body_bytes.extend_from_slice(b"Content-Disposition: form-data; name=\"room_id\"\r\n\r\n");
        body_bytes.extend_from_slice(rid.as_bytes());
        body_bytes.extend_from_slice(b"\r\n");
    }

    // End boundary
    body_bytes.extend_from_slice(format!("--{BOUNDARY}--\r\n").as_bytes());

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/_matrix/vendor/v1/voice/upload")
                .method("POST")
                .header("Authorization", format!("Bearer {}", token))
                .header("Content-Type", format!("multipart/form-data; boundary={BOUNDARY}"))
                .body(Body::from(body_bytes))
                .unwrap(),
        )
        .await
        .unwrap();

    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    (status, json)
}

#[tokio::test]
async fn test_voice_config_endpoint() {
    // A11: `max_size_bytes` must mirror the authoritative
    // `config.server.max_upload_size`, so read the expected value from the same
    // config the handler sees instead of pinning a hardcoded literal.
    let Some((app, state)) = super::setup_fresh_test_app_with_state().await else {
        return;
    };
    let expected_max_size = state.services.core.config.server.max_upload_size;

    let token = create_test_user(&app).await;

    // ISSUE-13: the private voice surface has exactly one canonical prefix
    // (`/_matrix/vendor/v1`); the `/_matrix/client/v{1,3}` aliases were deleted.
    for uri in ["/_matrix/vendor/v1/voice/config"] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(uri)
                    .header("Authorization", format!("Bearer {}", token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let status = response.status();
        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();

        if status != StatusCode::OK {
            eprintln!("Error response body for {uri}: {:?}", String::from_utf8_lossy(&body));
        }

        assert_eq!(status, StatusCode::OK);
        let json: Value = serde_json::from_slice(&body).unwrap();

        assert!(json.get("supported_formats").is_some());
        assert_eq!(json["max_size_bytes"].as_u64(), Some(expected_max_size));
        assert_eq!(json["content_type"], "m.audio");
        assert_eq!(json["voice_extension"], "org.matrix.msc3245.voice");
        assert_eq!(json["max_duration"], 600);
        assert_eq!(json["auto_transcribe"], false);
        // A3: the convert/optimize/transcription routes are registered but
        // intentionally unsupported (404 M_UNRECOGNIZED per COMPAT-03); the
        // capability declaration must say so.
        assert_eq!(json["server_side_processing"]["convert"], false);
        assert_eq!(json["server_side_processing"]["optimize"], false);
        assert_eq!(json["server_side_processing"]["transcription"], false);
        assert!(json["allowed_formats"].is_array());
    }
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
    assert!(json["content_uri"].as_str().unwrap_or_default().starts_with("mxc://"));
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
        .uri("/_matrix/client/v3/voip/config")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let r0_config_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), r0_config_request).await.unwrap();
    assert_eq!(r0_config_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(r0_config_response.into_body(), 2048).await.unwrap();
    let r0_config_json: Value = serde_json::from_slice(&body).unwrap();

    let v3_config_request = Request::builder()
        .method("GET")
        .uri("/_matrix/client/v3/voip/config")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let v3_config_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), v3_config_request).await.unwrap();
    assert_eq!(v3_config_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(v3_config_response.into_body(), 2048).await.unwrap();
    let v3_config_json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(r0_config_json, v3_config_json);

    let r0_turn_request = Request::builder()
        .method("GET")
        .uri("/_matrix/client/v3/voip/turnServer")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let r0_turn_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), r0_turn_request).await.unwrap();

    let v3_turn_request = Request::builder()
        .method("GET")
        .uri("/_matrix/client/v3/voip/turnServer")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let v3_turn_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), v3_turn_request).await.unwrap();
    assert_eq!(r0_turn_response.status(), v3_turn_response.status());
}

async fn create_call_session(app: &axum::Router, token: &str, room_id: &str, call_id: &str) -> (StatusCode, String) {
    let encoded_room_id = urlencoding::encode(room_id);

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!("/_matrix/client/v3/rooms/{}/send/m.call.invite/test_txn", encoded_room_id))
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
                        "lifetime": 60_000
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
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

    assert_eq!(status, StatusCode::FORBIDDEN, "unexpected response body: {}", body);
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

    let (invite_status, invite_body) = create_call_session(&app, &owner_token, &room_id, &call_id).await;
    assert_eq!(invite_status, StatusCode::OK, "unexpected invite response body: {}", invite_body);
    let invite_body: Value = serde_json::from_str(&invite_body).unwrap();
    assert!(invite_body.get("event_id").is_some(), "expected event_id in invite response: {}", invite_body);

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
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "unexpected get_call_session response body: {}",
        String::from_utf8_lossy(&body)
    );
    assert_eq!(json["errcode"], "M_FORBIDDEN");
}

// ---------------------------------------------------------------------------
// U-13-R10: `call.invite` / `call.answer` must answer with the persisted ID
// ---------------------------------------------------------------------------

async fn create_public_room(app: &axum::Router, token: &str) -> String {
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/createRoom")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "name": "Call Room", "preset": "public_chat" }).to_string()))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 10240).await.unwrap();
    serde_json::from_slice::<Value>(&body).unwrap()["room_id"].as_str().unwrap().to_string()
}

async fn join_room(app: &axum::Router, token: &str, room_id: &str) {
    let request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/v3/rooms/{}/join", urlencoding::encode(room_id)))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from("{}"))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

async fn send_call_event(
    app: &axum::Router,
    token: &str,
    room_id: &str,
    event_type: &str,
    txn_id: &str,
    content: &Value,
) -> (StatusCode, String) {
    let request = Request::builder()
        .method("PUT")
        .uri(format!("/_matrix/client/v3/rooms/{}/send/{event_type}/{txn_id}", urlencoding::encode(room_id)))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(content.to_string()))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (status, String::from_utf8_lossy(&body).to_string())
}

/// Defect B (U-13-R10): pre-fix `call_invite` / `call_answer` minted
/// `format!("${}:{}", Uuid::new_v4(), server_name)` and then
/// `let _ = ….create_event(.., None).await;` — discarding both the returned
/// `RoomEvent` **and** any error — and answered with the placeholder, so the
/// caller was handed an event ID that was never persisted (and a failed write
/// was reported as success).
#[cfg(feature = "voip-tracking")]
#[tokio::test]
async fn test_call_events_return_the_persisted_event_id() {
    let Some((app, pool, _cache)) = super::setup_fresh_test_app_with_pool().await else {
        return;
    };
    let (caller_token, _caller_id) = create_test_user_with_id(&app).await;
    let (callee_token, callee_id) = create_test_user_with_id(&app).await;

    // A public room so both users can be members (the answer must come from the
    // invited callee, and `handle_answer` authorizes only that user).
    let room_id = create_public_room(&app, &caller_token).await;
    join_room(&app, &callee_token, &room_id).await;

    let call_id = format!("call_{}", rand::random::<u32>());

    let (status, body) = send_call_event(
        &app,
        &caller_token,
        &room_id,
        "m.call.invite",
        "invite_txn",
        &json!({
            "call_id": call_id.clone(),
            "version": 1,
            "offer": { "type": "offer", "sdp": "v=0\r\no=- 1 2 IN IP4 127.0.0.1\r\n" },
            "invitee": callee_id,
            "lifetime": 60_000
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "call.invite failed: {body}");
    let invite_id = serde_json::from_str::<Value>(&body).unwrap()["event_id"]
        .as_str()
        .expect("call.invite must return an event_id")
        .to_string();

    let (status, body) = send_call_event(
        &app,
        &callee_token,
        &room_id,
        "m.call.answer",
        "answer_txn",
        &json!({
            "call_id": call_id,
            "version": 1,
            "answer": { "type": "answer", "sdp": "v=0\r\no=- 1 2 IN IP4 127.0.0.1\r\n" }
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "call.answer failed: {body}");
    let answer_id = serde_json::from_str::<Value>(&body).unwrap()["event_id"]
        .as_str()
        .expect("call.answer must return an event_id")
        .to_string();

    for (label, event_id) in [("m.call.invite", &invite_id), ("m.call.answer", &answer_id)] {
        assert!(
            !event_id.contains(":localhost"),
            "{label} must return the v3+ reference hash, not a `:server` placeholder: {event_id}"
        );
        let stored: Option<String> = sqlx::query_scalar("SELECT event_id FROM events WHERE event_id = $1")
            .bind(event_id)
            .fetch_optional(&*pool)
            .await
            .expect("the events table must be queryable");
        assert_eq!(stored.as_deref(), Some(event_id.as_str()), "{label} must return the persisted row's ID");
    }
}

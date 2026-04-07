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
    assert_eq!(json["default_sample_rate"], 48000);
}

#[tokio::test]
async fn test_voice_convert_endpoint() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let token = create_test_user(&app).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/_matrix/client/r0/voice/convert")
                .method("POST")
                .header("Authorization", format!("Bearer {}", token))
                .header("Content-Type", "application/json")
                .body(Body::from(
                    json!({
                        "message_id": "test_message_id",
                        "target_format": "audio/mpeg"
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

    if status != StatusCode::OK {
        eprintln!("Error response body: {:?}", String::from_utf8_lossy(&body));
    }

    assert_eq!(status, StatusCode::OK);

    let json: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["status"], "success");
}

#[tokio::test]
async fn test_voice_optimize_endpoint() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let token = create_test_user(&app).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/_matrix/client/r0/voice/optimize")
                .method("POST")
                .header("Authorization", format!("Bearer {}", token))
                .header("Content-Type", "application/json")
                .body(Body::from(
                    json!({
                        "message_id": "test_message_id"
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

    if status != StatusCode::OK {
        eprintln!("Error response body: {:?}", String::from_utf8_lossy(&body));
    }

    assert_eq!(status, StatusCode::OK);

    let json: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["status"], "success");
}

#[tokio::test]
async fn test_voice_transcription_endpoint_returns_explicit_unsupported_error() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let token = create_test_user(&app).await;
    let content = base64::engine::general_purpose::STANDARD
        .encode(include_bytes!("../../docker/media/test/message.mp3"));

    let upload_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/_matrix/client/r0/voice/upload")
                .method("POST")
                .header("Authorization", format!("Bearer {}", token))
                .header("Content-Type", "application/json")
                .body(Body::from(
                    json!({
                        "content": content,
                        "content_type": "audio/mpeg",
                        "duration_ms": 1200,
                        "room_id": "!voice:localhost"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    let upload_status = upload_response.status();
    let body = axum::body::to_bytes(upload_response.into_body(), usize::MAX)
        .await
        .unwrap();
    if upload_status != StatusCode::OK {
        eprintln!(
            "voice upload error body: {}",
            String::from_utf8_lossy(&body)
        );
    }
    assert_eq!(upload_status, StatusCode::OK);

    let upload_json: Value = serde_json::from_slice(&body).unwrap();
    let event_id = upload_json["event_id"].as_str().unwrap().to_string();

    let transcription_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/_matrix/client/v1/voice/transcription")
                .method("POST")
                .header("Authorization", format!("Bearer {}", token))
                .header("Content-Type", "application/json")
                .body(Body::from(
                    json!({
                        "event_id": event_id
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(transcription_response.status(), StatusCode::BAD_REQUEST);

    let body = axum::body::to_bytes(transcription_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let transcription_json: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(transcription_json["errcode"], "M_UNRECOGNIZED");
    assert!(transcription_json["error"]
        .as_str()
        .unwrap_or_default()
        .contains(upload_json["event_id"].as_str().unwrap()));
}

#[tokio::test]
async fn test_voice_stats_endpoint_reflects_uploaded_message() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let token = create_test_user(&app).await;
    let content = base64::engine::general_purpose::STANDARD
        .encode(include_bytes!("../../docker/media/test/message.mp3"));

    let upload_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/_matrix/client/r0/voice/upload")
                .method("POST")
                .header("Authorization", format!("Bearer {}", token))
                .header("Content-Type", "application/json")
                .body(Body::from(
                    json!({
                        "content": content,
                        "content_type": "audio/mpeg",
                        "duration_ms": 1200,
                        "room_id": "!voice:localhost"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    let upload_status = upload_response.status();
    let body = axum::body::to_bytes(upload_response.into_body(), usize::MAX)
        .await
        .unwrap();
    if upload_status != StatusCode::OK {
        eprintln!(
            "voice upload error body: {}",
            String::from_utf8_lossy(&body)
        );
    }
    assert_eq!(upload_status, StatusCode::OK);

    let stats_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/_matrix/client/r0/voice/stats")
                .method("GET")
                .header("Authorization", format!("Bearer {}", token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(stats_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(stats_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let stats_json: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(stats_json["total_duration_ms"], 1200);
    assert!(stats_json["total_file_size"].as_i64().unwrap_or_default() > 0);
    assert_eq!(stats_json["total_message_count"], 1);
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

    let r0_call_request = Request::builder()
        .method("GET")
        .uri("/_matrix/client/r0/rooms/!room:localhost/call/test-call")
        .body(Body::empty())
        .unwrap();
    let r0_call_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), r0_call_request)
        .await
        .unwrap();

    let v3_call_request = Request::builder()
        .method("GET")
        .uri("/_matrix/client/v3/rooms/!room:localhost/call/test-call")
        .body(Body::empty())
        .unwrap();
    let v3_call_response = ServiceExt::<Request<Body>>::oneshot(app, v3_call_request)
        .await
        .unwrap();
    assert_eq!(r0_call_response.status(), v3_call_response.status());
}

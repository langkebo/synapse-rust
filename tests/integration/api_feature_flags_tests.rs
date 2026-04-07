use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;

async fn setup_test_app() -> Option<axum::Router> {
    super::setup_test_app().await
}

#[tokio::test]
async fn test_feature_flag_create_get_list_and_update() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let (admin_token, _) = super::get_admin_token(&app).await;
    let flag_key = format!("room.summary.realtime_sync.{}", rand::random::<u32>());

    let request = Request::builder()
        .method("POST")
        .uri("/_synapse/admin/v1/feature-flags")
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("Content-Type", "application/json")
        .header("x-request-id", "feature-flags-create")
        .body(Body::from(
            json!({
                "flag_key": flag_key,
                "target_scope": "room",
                "rollout_percent": 25,
                "reason": "enable staged rollout",
                "status": "active",
                "expires_at": chrono::Utc::now().timestamp_millis() + 600000,
                "targets": [
                    { "subject_type": "room", "subject_id": "!room123:localhost" },
                    { "subject_type": "user", "subject_id": "@tester:localhost" }
                ]
            })
            .to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 8192)
        .await
        .unwrap();
    let created: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(created["flag_key"], flag_key);
    assert_eq!(created["rollout_percent"], 25);
    assert_eq!(created["targets"].as_array().unwrap().len(), 2);

    let request = Request::builder()
        .uri(format!("/_synapse/admin/v1/feature-flags/{}", flag_key))
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
    let fetched: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(fetched["status"], "active");

    let request = Request::builder()
        .uri("/_synapse/admin/v1/feature-flags?target_scope=room&status=active")
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
    let listed: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(listed["total"], 1);
    assert_eq!(listed["flags"].as_array().unwrap().len(), 1);

    let request = Request::builder()
        .method("PATCH")
        .uri(format!("/_synapse/admin/v1/feature-flags/{}", flag_key))
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("Content-Type", "application/json")
        .header("x-request-id", "feature-flags-update")
        .body(Body::from(
            json!({
                "rollout_percent": 100,
                "status": "fully_enabled",
                "reason": "guardrails passed"
            })
            .to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 8192)
        .await
        .unwrap();
    let updated: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(updated["rollout_percent"], 100);
    assert_eq!(updated["status"], "fully_enabled");

    let request = Request::builder()
        .uri("/_synapse/admin/v1/audit/events?action=admin.feature_flag.create")
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
    let audit_list: Value = serde_json::from_slice(&body).unwrap();
    let events = audit_list["events"].as_array().unwrap();
    assert!(events.iter().any(|event| event["resource_id"] == flag_key));
}

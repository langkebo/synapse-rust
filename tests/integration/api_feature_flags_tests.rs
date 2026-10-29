use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use synapse_common::current_timestamp_millis;
use tower::ServiceExt;

async fn setup_test_app() -> Option<axum::Router> {
    super::setup_fresh_test_app().await
}

#[tokio::test]
async fn test_feature_flag_create_get_list_and_update() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let (admin_token, _) = super::get_super_admin_token(&app).await;
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
                "expires_at": current_timestamp_millis() + 60_0000,
                "targets": [
                    { "subject_type": "room", "subject_id": "!room123:localhost" },
                    { "subject_type": "user", "subject_id": "@tester:localhost" }
                ]
            })
            .to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 8192).await.unwrap();
    let created: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(created["flag_key"], flag_key);
    assert_eq!(created["rollout_percent"], 25);
    assert_eq!(created["targets"].as_array().unwrap().len(), 2);

    let request = Request::builder()
        .uri(format!("/_synapse/admin/v1/feature-flags/{}", flag_key))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 8192).await.unwrap();
    let fetched: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(fetched["status"], "active");

    let request = Request::builder()
        .uri("/_synapse/admin/v1/feature-flags?target_scope=room&status=active")
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 8192).await.unwrap();
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

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 8192).await.unwrap();
    let updated: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(updated["rollout_percent"], 100);
    assert_eq!(updated["status"], "fully_enabled");

    let request = Request::builder()
        .uri("/_synapse/admin/v1/audit/events?action=admin.feature_flag.create")
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 8192).await.unwrap();
    let audit_list: Value = serde_json::from_slice(&body).unwrap();
    let events = audit_list["events"].as_array().unwrap();
    assert!(events.iter().any(|event| event["resource_id"] == flag_key));
}

#[tokio::test]
async fn test_feature_flag_delete_removes_the_flag_and_then_reads_404() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let (admin_token, _) = super::get_super_admin_token(&app).await;
    let flag_key = format!("room.summary.delete_me.{}", rand::random::<u32>());

    let request = Request::builder()
        .method("POST")
        .uri("/_synapse/admin/v1/feature-flags")
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("Content-Type", "application/json")
        .header("x-request-id", "feature-flags-delete-create")
        .body(Body::from(
            json!({
                "flag_key": flag_key,
                "target_scope": "room",
                "rollout_percent": 10,
                "reason": "will be deleted",
                "status": "active"
            })
            .to_string(),
        ))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let request = Request::builder()
        .method("DELETE")
        .uri(format!("/_synapse/admin/v1/feature-flags/{}", flag_key))
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("x-request-id", "feature-flags-delete")
        .body(Body::empty())
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // Matrix 约定：成功但无返回体时回 `{}`，不是 204 也不是 null。
    let body = axum::body::to_bytes(response.into_body(), 8192).await.unwrap();
    assert_eq!(serde_json::from_slice::<Value>(&body).unwrap(), json!({}));

    let request = Request::builder()
        .uri(format!("/_synapse/admin/v1/feature-flags/{}", flag_key))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND, "删除后读回应当是 404");
}

#[tokio::test]
async fn test_feature_flag_list_rejects_a_malformed_cursor() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let (admin_token, _) = super::get_super_admin_token(&app).await;

    // `from` 存在但解不出 `(updated_ts, flag_key)` ⇒ 400；静默忽略会把
    // 分页游标错误吞成「从头再来」，客户端会无限拉第一页。
    for bad in ["bad-cursor", "123|"] {
        let request = Request::builder()
            .uri(format!("/_synapse/admin/v1/feature-flags?from={bad}"))
            .header("Authorization", format!("Bearer {}", admin_token))
            .body(Body::empty())
            .unwrap();
        let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "from={bad} 应当被拒");

        let body = axum::body::to_bytes(response.into_body(), 8192).await.unwrap();
        let err: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(err["error"], "Invalid from cursor", "from={bad} 实际：{err}");
    }
}

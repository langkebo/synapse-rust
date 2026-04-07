use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use std::sync::Arc;
use synapse_rust::cache::{CacheConfig, CacheManager};
use synapse_rust::services::telemetry_alert_service::TelemetryAlertSeverity;
use synapse_rust::services::ServiceContainer;
use synapse_rust::web::routes::create_router;
use synapse_rust::web::AppState;
use tower::ServiceExt;

async fn setup_test_app() -> Option<(axum::Router, AppState)> {
    let pool = super::get_test_pool().await?;
    let container = ServiceContainer::new_test_with_pool(pool);
    let cache = Arc::new(CacheManager::new(CacheConfig::default()));
    let state = AppState::new(container, cache);
    let app = create_router(state.clone());
    Some((app, state))
}

#[tokio::test]
async fn test_telemetry_metrics_alerts_and_ack() {
    let Some((app, state)) = setup_test_app().await else {
        return;
    };
    let (admin_token, _) = super::get_admin_token(&app).await;

    let counter = state
        .services
        .metrics
        .register_counter("telemetry_test_counter".to_string());
    counter.inc_by(3);
    let gauge = state
        .services
        .metrics
        .register_gauge("telemetry_test_gauge".to_string());
    gauge.set(42.0);
    let histogram = state
        .services
        .metrics
        .register_histogram("telemetry_test_histogram".to_string());
    histogram.observe(100.0);

    let manual_alert = state
        .services
        .telemetry_alert_service
        .raise_alert(
            "telemetry_test_alert",
            "Telemetry test alert",
            TelemetryAlertSeverity::Critical,
            "observability",
            "manual alert for integration test",
            json!({ "source": "integration_test" }),
        )
        .await;

    let request = Request::builder()
        .uri("/_synapse/admin/v1/telemetry/metrics")
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 4096)
        .await
        .unwrap();
    let metrics_summary: Value = serde_json::from_slice(&body).unwrap();
    assert!(metrics_summary["total_counters"].as_u64().unwrap() >= 1);
    assert!(metrics_summary["total_gauges"].as_u64().unwrap() >= 1);
    assert!(metrics_summary["total_histograms"].as_u64().unwrap() >= 1);

    let request = Request::builder()
        .uri("/_synapse/admin/v1/telemetry/alerts?refresh=false")
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
    let alerts_response: Value = serde_json::from_slice(&body).unwrap();
    let alerts = alerts_response["alerts"].as_array().unwrap();
    assert!(alerts
        .iter()
        .any(|alert| alert["alert_id"] == manual_alert.alert_id));

    let request = Request::builder()
        .method("POST")
        .uri(format!(
            "/_synapse/admin/v1/telemetry/alerts/{}/ack",
            manual_alert.alert_id
        ))
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("x-request-id", "telemetry-alert-ack")
        .body(Body::empty())
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 8192)
        .await
        .unwrap();
    let acked: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(acked["status"], "acknowledged");

    let request = Request::builder()
        .uri("/_synapse/admin/v1/telemetry/health")
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 16384)
        .await
        .unwrap();
    let health: Value = serde_json::from_slice(&body).unwrap();
    assert!(health.get("checks").is_some());
    assert!(health.get("database").is_some());

    let request = Request::builder()
        .uri("/_synapse/admin/v1/audit/events?action=admin.telemetry.alert.ack")
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
    assert!(events
        .iter()
        .any(|event| event["resource_id"] == manual_alert.alert_id));
}

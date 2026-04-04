use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use hmac::{Hmac, Mac};
use serde_json::{json, Value};
use std::sync::Arc;
use synapse_rust::cache::{CacheConfig, CacheManager};
use synapse_rust::services::telemetry_alert_service::TelemetryAlertSeverity;
use synapse_rust::services::ServiceContainer;
use synapse_rust::web::routes::create_router;
use synapse_rust::web::AppState;
use tower::ServiceExt;

type HmacSha256 = Hmac<sha2::Sha256>;

async fn setup_test_app() -> Option<(axum::Router, AppState)> {
    if !super::init_test_database().await {
        return None;
    }
    let container = ServiceContainer::new_test();
    let cache = Arc::new(CacheManager::new(CacheConfig::default()));
    let state = AppState::new(container, cache);
    let app = create_router(state.clone());
    Some((app, state))
}

async fn get_admin_token(app: &axum::Router) -> String {
    let request = Request::builder()
        .uri("/_synapse/admin/v1/register/nonce")
        .body(Body::empty())
        .unwrap();
    let response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), super::with_local_connect_info(request))
            .await
            .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let nonce = json["nonce"].as_str().unwrap().to_string();

    let username = format!("telemetry_admin_{}", rand::random::<u32>());
    let password = "password123";
    let shared_secret = "test_shared_secret";

    let mut mac = HmacSha256::new_from_slice(shared_secret.as_bytes()).unwrap();
    mac.update(nonce.as_bytes());
    mac.update(b"\0");
    mac.update(username.as_bytes());
    mac.update(b"\0");
    mac.update(password.as_bytes());
    mac.update(b"\0");
    mac.update(b"admin");

    let mac_hex = mac
        .finalize()
        .into_bytes()
        .iter()
        .map(|byte| format!("{:02x}", byte))
        .collect::<String>();

    let request = Request::builder()
        .method("POST")
        .uri("/_synapse/admin/v1/register")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "nonce": nonce,
                "username": username,
                "password": password,
                "admin": true,
                "mac": mac_hex
            })
            .to_string(),
        ))
        .unwrap();
    let response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), super::with_local_connect_info(request))
            .await
            .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 4096)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    json["access_token"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn test_telemetry_metrics_alerts_and_ack() {
    let Some((app, state)) = setup_test_app().await else {
        return;
    };
    let admin_token = get_admin_token(&app).await;

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

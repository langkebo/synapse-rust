use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use std::sync::Arc;
use synapse_rust::cache::{CacheConfig, CacheManager};
use synapse_storage::application_service::{ApplicationServiceStorage, RegisterApplicationServiceRequest};
use synapse_rust::web::routes::create_router;
use synapse_rust::web::AppState;
use synapse_services::telemetry_service::TelemetryAlertSeverity;
use synapse_services::ServiceContainer;
use tower::ServiceExt;
use wiremock::{matchers::method, Mock, MockServer, ResponseTemplate};

async fn setup_test_app() -> Option<(axum::Router, AppState)> {
    let pool = super::get_test_pool().await?;
    let container = ServiceContainer::new_test_with_pool(pool).await;
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
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

    let counter = state.services.core.metrics.register_counter("telemetry_test_counter".to_string());
    counter.inc_by(3);
    let gauge = state.services.core.metrics.register_gauge("telemetry_test_gauge".to_string());
    gauge.set(42.0);
    let histogram = state.services.core.metrics.register_histogram("telemetry_test_histogram".to_string());
    histogram.observe(100.0);

    let as_id = format!("telemetry_scheduler_as_{}", rand::random::<u32>());
    state
        .services
        .admin
        .modules
        .app_service_manager
        .register(RegisterApplicationServiceRequest {
            as_id: as_id.clone(),
            url: "http://localhost:8080".to_string(),
            as_token: format!("test_as_token_{}", rand::random::<u32>()),
            hs_token: format!("test_hs_token_{}", rand::random::<u32>()),
            sender: "@telemetry_scheduler_bot:localhost".to_string(),
            description: Some("telemetry scheduler bot".to_string()),
            is_rate_limited: Some(false),
            protocols: None,
            namespaces: Some(json!({
                "users": [{"regex": "^@telemetry_scheduler:localhost$", "exclusive": false}],
                "aliases": [],
                "rooms": []
            })),
            api_key: None,
            config: None,
        })
        .await
        .expect("telemetry scheduler appservice registration should succeed");

    for (state_key, state_value) in [
        ("scheduler_last_result", "backoff"),
        ("scheduler_pending_event_count", "3"),
        ("scheduler_pending_transaction_count", "1"),
        ("scheduler_total_backoff_count", "1"),
        ("scheduler_total_failure_count", "2"),
    ] {
        state
            .services
            .admin
            .modules
            .app_service_manager
            .set_state(&as_id, state_key, state_value)
            .await
            .expect("scheduler state should be persisted");
    }

    let manual_alert = state.services.admin.security.telemetry_alert_service.raise_alert(
        "telemetry_test_alert",
        "Telemetry test alert",
        &TelemetryAlertSeverity::Critical,
        "observability",
        "manual alert for integration test",
        json!({ "source": "integration_test" }),
    );

    let request = Request::builder()
        .uri("/_synapse/admin/v1/telemetry/metrics")
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 4096).await.unwrap();
    let metrics_summary: Value = serde_json::from_slice(&body).unwrap();
    assert!(metrics_summary["total_counters"].as_u64().unwrap() >= 1);
    assert!(metrics_summary["total_gauges"].as_u64().unwrap() >= 1);
    assert!(metrics_summary["total_histograms"].as_u64().unwrap() >= 1);
    assert_eq!(metrics_summary["appservice_scheduler"]["total_services"], 1);
    assert_eq!(metrics_summary["appservice_scheduler"]["scheduler_available_services"], 1);
    assert_eq!(metrics_summary["appservice_scheduler"]["services_in_backoff"], 1);
    assert_eq!(metrics_summary["appservice_scheduler"]["services_with_pending_transactions"], 1);
    assert_eq!(metrics_summary["appservice_scheduler"]["total_pending_events"], 3);
    assert_eq!(metrics_summary["appservice_scheduler"]["total_pending_transactions"], 1);
    assert_eq!(metrics_summary["appservice_scheduler"]["total_backoff_count"], 1);
    assert_eq!(metrics_summary["appservice_scheduler"]["total_failure_count"], 2);

    let request = Request::builder()
        .uri("/_synapse/admin/v1/telemetry/alerts?refresh=false")
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 8192).await.unwrap();
    let alerts_response: Value = serde_json::from_slice(&body).unwrap();
    let alerts = alerts_response["alerts"].as_array().unwrap();
    assert!(alerts.iter().any(|alert| alert["alert_id"] == manual_alert.alert_id));

    let request = Request::builder()
        .method("POST")
        .uri(format!("/_synapse/admin/v1/telemetry/alerts/{}/ack", manual_alert.alert_id))
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("x-request-id", "telemetry-alert-ack")
        .body(Body::empty())
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 8192).await.unwrap();
    let acked: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(acked["status"], "acknowledged");

    let request = Request::builder()
        .uri("/_synapse/admin/v1/telemetry/health")
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 16384).await.unwrap();
    let health: Value = serde_json::from_slice(&body).unwrap();
    assert!(health.get("checks").is_some());
    assert!(health.get("database").is_some());

    let request = Request::builder()
        .uri("/_synapse/admin/v1/audit/events?action=admin.telemetry.alert.ack")
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 8192).await.unwrap();
    let audit_list: Value = serde_json::from_slice(&body).unwrap();
    let events = audit_list["events"].as_array().unwrap();
    assert!(events.iter().any(|event| event["resource_id"] == manual_alert.alert_id));
}

#[tokio::test]
async fn test_telemetry_metrics_reflect_real_scheduler_recovery_flow() {
    let Some((app, state)) = setup_test_app().await else {
        return;
    };
    let (admin_token, _) = super::get_admin_token(&app).await;

    let failing_server = MockServer::start().await;
    Mock::given(method("PUT"))
        .respond_with(ResponseTemplate::new(503))
        .up_to_n_times(1)
        .with_priority(1)
        .mount(&failing_server)
        .await;
    Mock::given(method("PUT")).respond_with(ResponseTemplate::new(200)).mount(&failing_server).await;

    let healthy_txn_server = MockServer::start().await;
    Mock::given(method("PUT")).respond_with(ResponseTemplate::new(200)).mount(&healthy_txn_server).await;

    let healthy_event_server = MockServer::start().await;
    Mock::given(method("PUT")).respond_with(ResponseTemplate::new(200)).mount(&healthy_event_server).await;

    let scenario_id = rand::random::<u32>();
    let failing_as_id = format!("telemetry-recovery-failing-{scenario_id}");
    let healthy_txn_as_id = format!("telemetry-recovery-txn-{scenario_id}");
    let healthy_event_as_id = format!("telemetry-recovery-event-{scenario_id}");
    let healthy_event_room_id = format!("!telemetry-recovery-event-{scenario_id}:localhost");

    state
        .services
        .admin
        .modules.app_service_manager
        .register(RegisterApplicationServiceRequest {
            as_id: failing_as_id.clone(),
            url: failing_server.uri(),
            as_token: format!("as_token_{failing_as_id}"),
            hs_token: format!("hs_token_{failing_as_id}"),
            sender: "@bridge:localhost".to_string(),
            description: Some("telemetry transient failing bridge".to_string()),
            is_rate_limited: Some(false),
            protocols: None,
            namespaces: Some(json!({
                "users": [],
                "aliases": [],
                "rooms": [{"exclusive": true, "regex": format!("^!telemetry-recovery-failing-{scenario_id}.*:localhost$")}]
            })),
            api_key: None,
            config: None,
        })
        .await
        .expect("failing appservice registration should succeed");

    state
        .services
        .admin
        .modules
        .app_service_manager
        .register(RegisterApplicationServiceRequest {
            as_id: healthy_txn_as_id.clone(),
            url: healthy_txn_server.uri(),
            as_token: format!("as_token_{healthy_txn_as_id}"),
            hs_token: format!("hs_token_{healthy_txn_as_id}"),
            sender: "@bridge:localhost".to_string(),
            description: Some("telemetry healthy txn bridge".to_string()),
            is_rate_limited: Some(false),
            protocols: None,
            namespaces: Some(json!({
                "users": [],
                "aliases": [],
                "rooms": [{"exclusive": true, "regex": format!("^!telemetry-recovery-txn-{scenario_id}.*:localhost$")}]
            })),
            api_key: None,
            config: None,
        })
        .await
        .expect("healthy transaction appservice registration should succeed");

    state
        .services
        .admin
        .modules.app_service_manager
        .register(RegisterApplicationServiceRequest {
            as_id: healthy_event_as_id.clone(),
            url: healthy_event_server.uri(),
            as_token: format!("as_token_{healthy_event_as_id}"),
            hs_token: format!("hs_token_{healthy_event_as_id}"),
            sender: "@bridge:localhost".to_string(),
            description: Some("telemetry healthy event bridge".to_string()),
            is_rate_limited: Some(false),
            protocols: None,
            namespaces: Some(json!({
                "users": [],
                "aliases": [],
                "rooms": [{"exclusive": true, "regex": format!("^!telemetry-recovery-event-{scenario_id}.*:localhost$")}]
            })),
            api_key: None,
            config: None,
        })
        .await
        .expect("healthy event appservice registration should succeed");

    let pool = state.services.database_pool();
    let storage = ApplicationServiceStorage::new(&pool);

    storage
        .create_transaction(
            &failing_as_id,
            &format!("telemetry-recovery-failing-{scenario_id}"),
            &[json!({"type": "m.room.message", "content": {"body": "fail once"}})],
        )
        .await
        .expect("failing pending transaction should be created");
    storage
        .create_transaction(
            &healthy_txn_as_id,
            &format!("telemetry-recovery-healthy-{scenario_id}"),
            &[json!({"type": "m.room.message", "content": {"body": "healthy"}})],
        )
        .await
        .expect("healthy pending transaction should be created");

    for event_index in 0..60 {
        state
            .services
            .admin
            .modules
            .app_service_manager
            .push_event(
                &healthy_event_as_id,
                &healthy_event_room_id,
                "m.room.message",
                "@bridge:localhost",
                json!({"msgtype": "m.text", "body": format!("telemetry-event-{event_index}")}),
                None,
            )
            .await
            .expect("healthy event enqueue should succeed");
    }

    state
        .services
        .admin
        .modules
        .app_service_scheduler
        .run_once()
        .await
        .expect("telemetry recovery tick one should complete");
    state
        .services
        .admin
        .modules
        .app_service_scheduler
        .run_once()
        .await
        .expect("telemetry recovery tick two should complete");
    tokio::time::sleep(std::time::Duration::from_millis(5_200)).await;
    state
        .services
        .admin
        .modules
        .app_service_scheduler
        .run_once()
        .await
        .expect("telemetry recovery tick three should complete");

    let request = Request::builder()
        .uri("/_synapse/admin/v1/telemetry/metrics")
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 4096).await.unwrap();
    let metrics_summary: Value = serde_json::from_slice(&body).unwrap();

    let failing_last_result = state
        .services
        .admin
        .modules
        .app_service_manager
        .get_state(&failing_as_id, "scheduler_last_result")
        .await
        .expect("failing last result lookup should succeed")
        .expect("failing last result should be persisted");
    assert!(
        matches!(failing_last_result.state_value.as_str(), "success" | "dispatched"),
        "failing service should leave backoff and return to a healthy dispatched/success state after recovery"
    );

    let failing_transaction_state = state
        .services
        .admin
        .modules
        .app_service_manager
        .get_state(&failing_as_id, "scheduler_transaction_state")
        .await
        .expect("failing transaction state lookup should succeed")
        .expect("failing transaction state should be persisted");
    assert_eq!(failing_transaction_state.state_value, "idle");

    assert_eq!(metrics_summary["appservice_scheduler"]["total_services"], 3);
    assert_eq!(metrics_summary["appservice_scheduler"]["scheduler_available_services"], 3);
    assert_eq!(metrics_summary["appservice_scheduler"]["services_in_backoff"], 0);
    assert_eq!(metrics_summary["appservice_scheduler"]["services_with_pending_transactions"], 0);
    assert_eq!(metrics_summary["appservice_scheduler"]["total_pending_events"], 0);
    assert_eq!(metrics_summary["appservice_scheduler"]["total_pending_transactions"], 0);
    assert_eq!(metrics_summary["appservice_scheduler"]["total_success_count"], 3);
}

#[tokio::test]
async fn test_telemetry_metrics_preserve_explainable_mixed_contention_counts() {
    let Some((app, state)) = setup_test_app().await else {
        return;
    };
    let (admin_token, _) = super::get_admin_token(&app).await;

    let failing_server = MockServer::start().await;
    Mock::given(method("PUT")).respond_with(ResponseTemplate::new(503)).mount(&failing_server).await;

    let healthy_txn_server = MockServer::start().await;
    Mock::given(method("PUT")).respond_with(ResponseTemplate::new(200)).mount(&healthy_txn_server).await;

    let event_heavy_a_server = MockServer::start().await;
    Mock::given(method("PUT")).respond_with(ResponseTemplate::new(200)).mount(&event_heavy_a_server).await;

    let event_heavy_b_server = MockServer::start().await;
    Mock::given(method("PUT")).respond_with(ResponseTemplate::new(200)).mount(&event_heavy_b_server).await;

    let scenario_id = rand::random::<u32>();
    let failing_as_id = format!("telemetry-mixed-failing-{scenario_id}");
    let healthy_txn_as_id = format!("telemetry-mixed-txn-{scenario_id}");
    let event_heavy_a_as_id = format!("telemetry-mixed-event-a-{scenario_id}");
    let event_heavy_b_as_id = format!("telemetry-mixed-event-b-{scenario_id}");
    let event_heavy_a_room_id = format!("!telemetry-mixed-event-a-{scenario_id}:localhost");
    let event_heavy_b_room_id = format!("!telemetry-mixed-event-b-{scenario_id}:localhost");

    for (as_id, url, room_regex, description) in [
        (
            failing_as_id.clone(),
            failing_server.uri(),
            format!("^!telemetry-mixed-failing-{scenario_id}.*:localhost$"),
            "telemetry mixed failing bridge".to_string(),
        ),
        (
            healthy_txn_as_id.clone(),
            healthy_txn_server.uri(),
            format!("^!telemetry-mixed-txn-{scenario_id}.*:localhost$"),
            "telemetry mixed healthy txn bridge".to_string(),
        ),
        (
            event_heavy_a_as_id.clone(),
            event_heavy_a_server.uri(),
            format!("^!telemetry-mixed-event-a-{scenario_id}.*:localhost$"),
            "telemetry mixed event-heavy a bridge".to_string(),
        ),
        (
            event_heavy_b_as_id.clone(),
            event_heavy_b_server.uri(),
            format!("^!telemetry-mixed-event-b-{scenario_id}.*:localhost$"),
            "telemetry mixed event-heavy b bridge".to_string(),
        ),
    ] {
        state
            .services
            .admin
            .modules
            .app_service_manager
            .register(RegisterApplicationServiceRequest {
                as_id: as_id.clone(),
                url,
                as_token: format!("as_token_{as_id}"),
                hs_token: format!("hs_token_{as_id}"),
                sender: "@bridge:localhost".to_string(),
                description: Some(description),
                is_rate_limited: Some(false),
                protocols: None,
                namespaces: Some(json!({
                    "users": [],
                    "aliases": [],
                    "rooms": [{"exclusive": true, "regex": room_regex}]
                })),
                api_key: None,
                config: None,
            })
            .await
            .expect("mixed contention appservice registration should succeed");
    }

    let pool = state.services.database_pool();
    let storage = ApplicationServiceStorage::new(&pool);

    storage
        .create_transaction(
            &failing_as_id,
            &format!("telemetry-mixed-failing-{scenario_id}"),
            &[json!({"type": "m.room.message", "content": {"body": "fail continuously"}})],
        )
        .await
        .expect("failing pending transaction should be created");
    storage
        .create_transaction(
            &healthy_txn_as_id,
            &format!("telemetry-mixed-healthy-{scenario_id}"),
            &[json!({"type": "m.room.message", "content": {"body": "healthy"}})],
        )
        .await
        .expect("healthy pending transaction should be created");

    for event_index in 0..60 {
        state
            .services
            .admin
            .modules
            .app_service_manager
            .push_event(
                &event_heavy_a_as_id,
                &event_heavy_a_room_id,
                "m.room.message",
                "@bridge:localhost",
                json!({"msgtype": "m.text", "body": format!("mixed-event-a-{event_index}")}),
                None,
            )
            .await
            .expect("event-heavy a enqueue should succeed");
        state
            .services
            .admin
            .modules
            .app_service_manager
            .push_event(
                &event_heavy_b_as_id,
                &event_heavy_b_room_id,
                "m.room.message",
                "@bridge:localhost",
                json!({"msgtype": "m.text", "body": format!("mixed-event-b-{event_index}")}),
                None,
            )
            .await
            .expect("event-heavy b enqueue should succeed");
    }

    state
        .services
        .admin
        .modules
        .app_service_scheduler
        .run_once()
        .await
        .expect("mixed contention tick one should complete");
    state
        .services
        .admin
        .modules
        .app_service_scheduler
        .run_once()
        .await
        .expect("mixed contention tick two should complete");

    let request = Request::builder()
        .uri("/_synapse/admin/v1/telemetry/metrics")
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 4096).await.unwrap();
    let metrics_summary: Value = serde_json::from_slice(&body).unwrap();
    let scheduler = &metrics_summary["appservice_scheduler"];

    assert_eq!(scheduler["total_services"], 4);
    assert_eq!(scheduler["scheduler_available_services"], 4);
    assert_eq!(scheduler["services_in_backoff"], 1);
    assert_eq!(scheduler["services_with_pending_transactions"], 1);
    assert_eq!(scheduler["total_pending_events"], 0);
    assert_eq!(scheduler["total_pending_transactions"], 1);
    assert!(
        scheduler["total_success_count"].as_i64().unwrap_or_default() >= 3,
        "healthy transaction and both event-heavy services should have completed dispatches"
    );
    assert!(
        scheduler["total_failure_count"].as_i64().unwrap_or_default() >= 1,
        "persistent failing service should contribute at least one scheduler failure"
    );
    assert!(
        scheduler["total_backoff_count"].as_i64().unwrap_or_default() >= 1,
        "persistent failing service should contribute backoff observations"
    );
    assert!(
        scheduler["total_capacity_limited_count"].as_i64().unwrap_or_default() >= 1,
        "event-heavy services should contribute capacity-limited observations under the first tick contention"
    );
}

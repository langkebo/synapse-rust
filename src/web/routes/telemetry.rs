use crate::common::error::ApiError;
use crate::web::routes::{AdminUser, AppState};
use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use synapse_services::telemetry_service::{ExportConfig, TelemetryAlert, TelemetryAlertFilters, TelemetryService};

#[derive(Debug, Serialize)]
pub struct TelemetryStatusResponse {
    pub enabled: bool,
    pub trace_enabled: bool,
    pub metrics_enabled: bool,
    pub service_name: String,
    pub service_version: String,
    pub sampling_ratio: f64,
    pub export_config: ExportConfigResponse,
}

#[derive(Debug, Serialize)]
pub struct ExportConfigResponse {
    pub otlp_endpoint: Option<String>,
    pub prometheus_port: Option<u16>,
    pub prometheus_path: Option<String>,
    pub batch_export: bool,
}

impl From<ExportConfig> for ExportConfigResponse {
    fn from(config: ExportConfig) -> Self {
        Self {
            otlp_endpoint: config.otlp_endpoint,
            prometheus_port: config.prometheus_port,
            prometheus_path: config.prometheus_path,
            batch_export: config.batch_export,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ResourceAttributesResponse {
    pub attributes: std::collections::HashMap<String, String>,
}

#[derive(Debug, Serialize)]
pub struct MetricsSummaryResponse {
    pub total_metrics: usize,
    pub total_counters: usize,
    pub total_gauges: usize,
    pub total_histograms: usize,
    pub rendered_bytes: usize,
    pub snapshot_ts: i64,
    pub appservice_scheduler: AppserviceSchedulerTelemetrySummary,
}

#[derive(Debug, Serialize, Default, PartialEq, Eq)]
pub struct AppserviceSchedulerTelemetrySummary {
    pub total_services: usize,
    pub scheduler_available_services: usize,
    pub services_in_backoff: usize,
    pub services_capacity_limited: usize,
    pub services_with_pending_transactions: usize,
    pub total_pending_events: i64,
    pub total_pending_transactions: i64,
    pub total_success_count: i64,
    pub total_failure_count: i64,
    pub total_backoff_count: i64,
    pub total_capacity_limited_count: i64,
    pub total_in_flight_count: i64,
}

#[derive(Debug, Serialize)]
pub struct TelemetryAlertsResponse {
    pub alerts: Vec<TelemetryAlert>,
}

#[derive(Debug, Deserialize, Default)]
pub struct TelemetryAlertQuery {
    pub status: Option<String>,
    pub severity: Option<String>,
    pub refresh: Option<bool>,
}

pub async fn get_status(State(state): State<AppState>, _admin_user: AdminUser) -> Result<impl IntoResponse, ApiError> {
    let config = &state.services.core.config.telemetry;
    let prometheus = &state.services.core.config.prometheus;

    let telemetry_service = TelemetryService::new(Arc::new(config.clone()), Arc::new(prometheus.clone()));

    let response = TelemetryStatusResponse {
        enabled: telemetry_service.is_enabled(),
        trace_enabled: telemetry_service.is_trace_enabled(),
        metrics_enabled: telemetry_service.is_metrics_enabled(),
        service_name: telemetry_service.get_service_name().to_string(),
        service_version: config.service_version.clone(),
        sampling_ratio: telemetry_service.get_sampling_ratio(),
        export_config: ExportConfigResponse::from(telemetry_service.get_export_config()),
    };

    Ok(Json(response))
}

pub async fn get_resource_attributes(
    State(state): State<AppState>,
    _admin_user: AdminUser,
) -> Result<impl IntoResponse, ApiError> {
    let config = &state.services.core.config.telemetry;
    let prometheus = &state.services.core.config.prometheus;

    let telemetry_service = TelemetryService::new(Arc::new(config.clone()), Arc::new(prometheus.clone()));

    let response = ResourceAttributesResponse { attributes: telemetry_service.get_resource_attributes() };

    Ok(Json(response))
}

pub async fn get_metrics_summary(
    State(state): State<AppState>,
    _admin_user: AdminUser,
) -> Result<impl IntoResponse, ApiError> {
    let inventory = state.services.core.metrics.inventory();
    let metrics = state.services.core.metrics.collect_metrics();
    let rendered = state.services.core.metrics.to_prometheus_format();
    let appservice_statistics = state.services.admin.modules.app_service_manager.get_statistics().await?;

    Ok(Json(MetricsSummaryResponse {
        total_metrics: metrics.len(),
        total_counters: inventory.total_counters,
        total_gauges: inventory.total_gauges,
        total_histograms: inventory.total_histograms,
        rendered_bytes: rendered.len(),
        snapshot_ts: chrono::Utc::now().timestamp_millis(),
        appservice_scheduler: summarize_appservice_scheduler_metrics(&appservice_statistics),
    }))
}

pub(crate) fn summarize_appservice_scheduler_metrics(
    appservice_statistics: &[serde_json::Value],
) -> AppserviceSchedulerTelemetrySummary {
    let mut summary = AppserviceSchedulerTelemetrySummary {
        total_services: appservice_statistics.len(),
        ..AppserviceSchedulerTelemetrySummary::default()
    };

    for entry in appservice_statistics {
        let Some(scheduler) = entry.get("scheduler") else {
            continue;
        };

        if scheduler.get("available").and_then(|value| value.as_bool()).unwrap_or(false) {
            summary.scheduler_available_services += 1;
        }

        match scheduler.get("last_result").and_then(|value| value.as_str()) {
            Some("backoff") => summary.services_in_backoff += 1,
            Some("capacity_limited") => summary.services_capacity_limited += 1,
            _ => {}
        }

        let pending_transactions = scheduler
            .get("pending_transaction_count")
            .and_then(|value| value.as_i64())
            .unwrap_or_else(|| entry.get("pending_transaction_count").and_then(|value| value.as_i64()).unwrap_or(0));
        let pending_events = scheduler
            .get("pending_event_count")
            .and_then(|value| value.as_i64())
            .unwrap_or_else(|| entry.get("pending_event_count").and_then(|value| value.as_i64()).unwrap_or(0));

        if pending_transactions > 0 {
            summary.services_with_pending_transactions += 1;
        }

        summary.total_pending_events += pending_events;
        summary.total_pending_transactions += pending_transactions;
        summary.total_success_count +=
            scheduler.get("total_success_count").and_then(|value| value.as_i64()).unwrap_or_default();
        summary.total_failure_count +=
            scheduler.get("total_failure_count").and_then(|value| value.as_i64()).unwrap_or_default();
        summary.total_backoff_count +=
            scheduler.get("total_backoff_count").and_then(|value| value.as_i64()).unwrap_or_default();
        summary.total_capacity_limited_count +=
            scheduler.get("total_capacity_limited_count").and_then(|value| value.as_i64()).unwrap_or_default();
        summary.total_in_flight_count +=
            scheduler.get("total_in_flight_count").and_then(|value| value.as_i64()).unwrap_or_default();
    }

    summary
}

pub async fn health_check(
    State(state): State<AppState>,
    _admin_user: AdminUser,
) -> Result<impl IntoResponse, ApiError> {
    let config = &state.services.core.config.telemetry;
    let prometheus = &state.services.core.config.prometheus;

    let telemetry_service = TelemetryService::new(Arc::new(config.clone()), Arc::new(prometheus.clone()));

    let readiness = state.health_checker.check_readiness().await;
    let (database_health, alerts) = state.services.admin.security.telemetry_alert_service.sync_with_health().await?;

    Ok(Json(serde_json::json!({
        "status": readiness.status,
        "service": telemetry_service.get_service_name(),
        "trace_enabled": telemetry_service.is_trace_enabled(),
        "metrics_enabled": telemetry_service.is_metrics_enabled(),
        "checks": readiness.checks,
        "database": database_health,
        "alerts": alerts,
    })))
}

pub async fn list_alerts(
    State(state): State<AppState>,
    Query(query): Query<TelemetryAlertQuery>,
    _admin_user: AdminUser,
) -> Result<impl IntoResponse, ApiError> {
    if query.refresh.unwrap_or(true) {
        let _ = state.services.admin.security.telemetry_alert_service.sync_with_health().await?;
    }

    let alerts = state
        .services
        .admin
        .security
        .telemetry_alert_service
        .list_alerts(&TelemetryAlertFilters { status: query.status, severity: query.severity })?;

    Ok(Json(TelemetryAlertsResponse { alerts }))
}

pub async fn acknowledge_alert(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(alert_id): Path<String>,
    admin_user: AdminUser,
) -> Result<impl IntoResponse, ApiError> {
    let alert =
        state.services.admin.security.telemetry_alert_service.acknowledge_alert(&alert_id, &admin_user.user_id)?;

    state
        .services
        .admin
        .security
        .admin_audit_service
        .create_event(synapse_storage::CreateAuditEventRequest {
            actor_id: admin_user.user_id,
            action: "admin.telemetry.alert.ack".to_string(),
            resource_type: "telemetry_alert".to_string(),
            resource_id: alert.alert_id.clone(),
            result: "success".to_string(),
            request_id: request_id(&headers),
            details: Some(serde_json::json!({
                "alert_key": alert.alert_key,
                "status": "acknowledged"
            })),
        })
        .await?;

    Ok(Json(alert))
}

pub fn create_telemetry_router(state: AppState) -> axum::Router<AppState> {
    use axum::routing::*;

    axum::Router::new()
        .route("/_synapse/admin/v1/telemetry/status", get(get_status))
        .route("/_synapse/admin/v1/telemetry/attributes", get(get_resource_attributes))
        .route("/_synapse/admin/v1/telemetry/metrics", get(get_metrics_summary))
        .route("/_synapse/admin/v1/telemetry/alerts", get(list_alerts))
        .route("/_synapse/admin/v1/telemetry/alerts/{alert_id}/ack", post(acknowledge_alert))
        .route("/_synapse/admin/v1/telemetry/health", get(health_check))
        .route_layer(axum::middleware::from_fn_with_state(state.clone(), crate::web::middleware::admin_auth_middleware))
        .with_state(state)
}

pub fn telemetry_route_manifest() -> Vec<crate::web::routes::route_ledger::RouteEntry> {
    use crate::web::routes::route_ledger::RouteEntry;
    use axum::http::Method;
    [
        (Method::GET, "/_synapse/admin/v1/telemetry/status"),
        (Method::GET, "/_synapse/admin/v1/telemetry/attributes"),
        (Method::GET, "/_synapse/admin/v1/telemetry/metrics"),
        (Method::GET, "/_synapse/admin/v1/telemetry/alerts"),
        (Method::POST, "/_synapse/admin/v1/telemetry/alerts/{alert_id}/ack"),
        (Method::GET, "/_synapse/admin/v1/telemetry/health"),
    ]
    .into_iter()
    .map(|(m, p)| RouteEntry::new(m, p, "telemetry"))
    .collect()
}

fn request_id(headers: &HeaderMap) -> String {
    crate::web::utils::auth::resolve_request_id(headers)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summarize_appservice_scheduler_metrics_aggregates_scheduler_state() {
        let statistics = vec![
            serde_json::json!({
                "as_id": "as-1",
                "pending_event_count": 0,
                "pending_transaction_count": 0,
                "scheduler": {
                    "available": true,
                    "last_result": "backoff",
                    "pending_event_count": 3,
                    "pending_transaction_count": 1,
                    "total_success_count": 5,
                    "total_failure_count": 2,
                    "total_backoff_count": 1,
                    "total_capacity_limited_count": 0,
                    "total_in_flight_count": 0
                }
            }),
            serde_json::json!({
                "as_id": "as-2",
                "pending_event_count": 4,
                "pending_transaction_count": 0,
                "scheduler": {
                    "available": true,
                    "last_result": "capacity_limited",
                    "pending_event_count": 4,
                    "pending_transaction_count": 0,
                    "total_success_count": 7,
                    "total_failure_count": 0,
                    "total_backoff_count": 0,
                    "total_capacity_limited_count": 2,
                    "total_in_flight_count": 1
                }
            }),
        ];

        let summary = summarize_appservice_scheduler_metrics(&statistics);

        assert_eq!(
            summary,
            AppserviceSchedulerTelemetrySummary {
                total_services: 2,
                scheduler_available_services: 2,
                services_in_backoff: 1,
                services_capacity_limited: 1,
                services_with_pending_transactions: 1,
                total_pending_events: 7,
                total_pending_transactions: 1,
                total_success_count: 12,
                total_failure_count: 2,
                total_backoff_count: 1,
                total_capacity_limited_count: 2,
                total_in_flight_count: 1,
            }
        );
    }
}

use crate::common::error::ApiError;
use crate::services::telemetry_alert_service::{TelemetryAlert, TelemetryAlertFilters};
use crate::services::telemetry_service::{ExportConfig, TelemetryService};
use crate::web::routes::{AdminUser, AppState};
use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

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

pub async fn get_status(
    State(state): State<AppState>,
    _admin_user: AdminUser,
) -> Result<impl IntoResponse, ApiError> {
    let config = &state.services.config.telemetry;
    let prometheus = &state.services.config.prometheus;

    let telemetry_service =
        TelemetryService::new(Arc::new(config.clone()), Arc::new(prometheus.clone()));

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
    let config = &state.services.config.telemetry;
    let prometheus = &state.services.config.prometheus;

    let telemetry_service =
        TelemetryService::new(Arc::new(config.clone()), Arc::new(prometheus.clone()));

    let response = ResourceAttributesResponse {
        attributes: telemetry_service.get_resource_attributes(),
    };

    Ok(Json(response))
}

pub async fn get_metrics_summary(
    State(state): State<AppState>,
    _admin_user: AdminUser,
) -> Result<impl IntoResponse, ApiError> {
    let inventory = state.services.metrics.inventory();
    let metrics = state.services.metrics.collect_metrics();
    let rendered = state.services.metrics.to_prometheus_format();

    Ok(Json(MetricsSummaryResponse {
        total_metrics: metrics.len(),
        total_counters: inventory.total_counters,
        total_gauges: inventory.total_gauges,
        total_histograms: inventory.total_histograms,
        rendered_bytes: rendered.len(),
        snapshot_ts: chrono::Utc::now().timestamp_millis(),
    }))
}

pub async fn health_check(
    State(state): State<AppState>,
    _admin_user: AdminUser,
) -> Result<impl IntoResponse, ApiError> {
    let config = &state.services.config.telemetry;
    let prometheus = &state.services.config.prometheus;

    let telemetry_service =
        TelemetryService::new(Arc::new(config.clone()), Arc::new(prometheus.clone()));

    let readiness = state.health_checker.check_readiness().await;
    let (database_health, alerts) = state
        .services
        .telemetry_alert_service
        .sync_with_health()
        .await?;

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
        let _ = state
            .services
            .telemetry_alert_service
            .sync_with_health()
            .await?;
    }

    let alerts = state
        .services
        .telemetry_alert_service
        .list_alerts(TelemetryAlertFilters {
            status: query.status,
            severity: query.severity,
        })
        .await?;

    Ok(Json(TelemetryAlertsResponse { alerts }))
}

pub async fn acknowledge_alert(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(alert_id): Path<String>,
    admin_user: AdminUser,
) -> Result<impl IntoResponse, ApiError> {
    let alert = state
        .services
        .telemetry_alert_service
        .acknowledge_alert(&alert_id, &admin_user.user_id)
        .await?;

    state
        .services
        .admin_audit_service
        .create_event(crate::storage::CreateAuditEventRequest {
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
        .route(
            "/_synapse/admin/v1/telemetry/attributes",
            get(get_resource_attributes),
        )
        .route(
            "/_synapse/admin/v1/telemetry/metrics",
            get(get_metrics_summary),
        )
        .route("/_synapse/admin/v1/telemetry/alerts", get(list_alerts))
        .route(
            "/_synapse/admin/v1/telemetry/alerts/{alert_id}/ack",
            post(acknowledge_alert),
        )
        .route("/_synapse/admin/v1/telemetry/health", get(health_check))
        .route_layer(axum::middleware::from_fn_with_state(
            state.clone(),
            crate::web::middleware::admin_auth_middleware,
        ))
        .with_state(state)
}

fn request_id(headers: &HeaderMap) -> String {
    headers
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.trim().is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| format!("telemetry-alert-{}", uuid::Uuid::new_v4()))
}

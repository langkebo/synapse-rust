use crate::common::error::ApiError;
use crate::services::telemetry_service::{ExportConfig, TelemetryService};
use crate::web::routes::AppState;
use crate::web::routes::AuthenticatedUser;
use axum::{
    extract::State,
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
    pub jaeger_agent: Option<String>,
    pub jaeger_collector: Option<String>,
    pub prometheus_port: Option<u16>,
    pub prometheus_path: Option<String>,
    pub batch_export: bool,
}

impl From<ExportConfig> for ExportConfigResponse {
    fn from(config: ExportConfig) -> Self {
        Self {
            otlp_endpoint: config.otlp_endpoint,
            jaeger_agent: config.jaeger_agent,
            jaeger_collector: config.jaeger_collector,
            prometheus_port: config.prometheus_port,
            prometheus_path: config.prometheus_path,
            batch_export: config.batch_export,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct UpdateConfigBody {
    pub enabled: Option<bool>,
    pub trace_enabled: Option<bool>,
    pub metrics_enabled: Option<bool>,
    pub sampling_ratio: Option<f64>,
    pub otlp_endpoint: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ResourceAttributesResponse {
    pub attributes: std::collections::HashMap<String, String>,
}

#[derive(Debug, Serialize)]
pub struct MetricsSummaryResponse {
    pub total_spans: u64,
    pub total_metrics: u64,
    pub active_traces: u64,
    pub export_errors: u64,
    pub last_export: Option<String>,
}

pub async fn get_status(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    let config = &state.services.config.telemetry;
    let jaeger = &state.services.config.jaeger;
    let prometheus = &state.services.config.prometheus;

    let telemetry_service = TelemetryService::new(
        Arc::new(config.clone()),
        Arc::new(jaeger.clone()),
        Arc::new(prometheus.clone()),
    );

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
    _auth_user: AuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    let config = &state.services.config.telemetry;
    let jaeger = &state.services.config.jaeger;
    let prometheus = &state.services.config.prometheus;

    let telemetry_service = TelemetryService::new(
        Arc::new(config.clone()),
        Arc::new(jaeger.clone()),
        Arc::new(prometheus.clone()),
    );

    let response = ResourceAttributesResponse {
        attributes: telemetry_service.get_resource_attributes(),
    };

    Ok(Json(response))
}

pub async fn get_metrics_summary(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    let _config = &state.services.config.telemetry;

    let response = MetricsSummaryResponse {
        total_spans: 0,
        total_metrics: 0,
        active_traces: 0,
        export_errors: 0,
        last_export: None,
    };

    Ok(Json(response))
}

pub async fn health_check(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, ApiError> {
    let config = &state.services.config.telemetry;
    let jaeger = &state.services.config.jaeger;
    let prometheus = &state.services.config.prometheus;

    let telemetry_service = TelemetryService::new(
        Arc::new(config.clone()),
        Arc::new(jaeger.clone()),
        Arc::new(prometheus.clone()),
    );

    let status = if telemetry_service.is_enabled() {
        "healthy"
    } else {
        "disabled"
    };

    Ok(Json(serde_json::json!({
        "status": status,
        "service": telemetry_service.get_service_name(),
        "trace_enabled": telemetry_service.is_trace_enabled(),
        "metrics_enabled": telemetry_service.is_metrics_enabled(),
    })))
}

pub fn create_telemetry_router() -> axum::Router<AppState> {
    use axum::routing::*;

    axum::Router::new()
        .route("/_synapse/admin/v1/telemetry/status", get(get_status))
        .route("/_synapse/admin/v1/telemetry/attributes", get(get_resource_attributes))
        .route("/_synapse/admin/v1/telemetry/metrics", get(get_metrics_summary))
        .route("/_synapse/admin/v1/telemetry/health", get(health_check))
}

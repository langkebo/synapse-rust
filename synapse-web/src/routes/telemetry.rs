use crate::routes::context::AdminContext;
use crate::routes::{AdminUser, AppState};
use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use synapse_common::current_timestamp_millis;
use synapse_common::error::ApiError;
use synapse_services::telemetry_service::{ExportConfig, TelemetryAlert, TelemetryAlertFilters, TelemetryService};

/// The `TelemetryStatusResponse` struct.
#[derive(Debug, Serialize)]
pub struct TelemetryStatusResponse {
    /// The `enabled` field.
    pub enabled: bool,
    /// The `trace_enabled` field.
    pub trace_enabled: bool,
    /// The `metrics_enabled` field.
    pub metrics_enabled: bool,
    /// The `service_name` field.
    pub service_name: String,
    /// The `service_version` field.
    pub service_version: String,
    /// The `sampling_ratio` field.
    pub sampling_ratio: f64,
    /// The `export_config` field.
    pub export_config: ExportConfigResponse,
}

/// The `ExportConfigResponse` struct.
#[derive(Debug, Serialize)]
pub struct ExportConfigResponse {
    /// The `otlp_endpoint` field.
    pub otlp_endpoint: Option<String>,
    /// The `prometheus_port` field.
    pub prometheus_port: Option<u16>,
    /// The `prometheus_path` field.
    pub prometheus_path: Option<String>,
    /// The `batch_export` field.
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

/// The `ResourceAttributesResponse` struct.
#[derive(Debug, Serialize)]
pub struct ResourceAttributesResponse {
    /// The `attributes` field.
    pub attributes: std::collections::HashMap<String, String>,
}

/// Prometheus 抓取目标。
///
/// 本 admin 端点是**给人类看的 JSON 摘要，不能被 Prometheus scrape**——
/// 它只返回统计数（`rendered_bytes` 也只是 Prometheus 文本的字节数，
/// 不含文本本身）。真正的抓取端点跑在**独立端口**上，渲染
/// `MetricsCollector::to_prometheus_format()` 的 text 格式输出。
///
/// 这个结构就是为了让查这个端点的人知道该去哪里抓，避免把 admin JSON
/// 端点错配成 Prometheus 的 scrape target。
#[derive(Debug, Serialize)]
pub struct PrometheusScrapeTarget {
    /// The `port` field.
    pub port: u16,
    /// The `path` field.
    pub path: String,
    /// 提示：抓取端点需要 `telemetry.prometheus.enabled = true` 才会监听。
    pub note: &'static str,
}

/// The `MetricsSummaryResponse` struct.
#[derive(Debug, Serialize)]
pub struct MetricsSummaryResponse {
    /// The `total_metrics` field.
    pub total_metrics: usize,
    /// The `total_counters` field.
    pub total_counters: usize,
    /// The `total_gauges` field.
    pub total_gauges: usize,
    /// The `total_histograms` field.
    pub total_histograms: usize,
    /// The `rendered_bytes` field.
    pub rendered_bytes: usize,
    /// The `snapshot_ts` field.
    pub snapshot_ts: i64,
    /// The `appservice_scheduler` field.
    pub appservice_scheduler: AppserviceSchedulerTelemetrySummary,
    /// Prometheus 抓取端点；仅当 `telemetry.prometheus.enabled` 时非空。
    /// 为 `None` 表示独立端口未监听，此时没有任何可 scrape 的目标。
    pub prometheus_scrape_target: Option<PrometheusScrapeTarget>,
}

/// The `AppserviceSchedulerTelemetrySummary` struct.
#[derive(Debug, Serialize, Default, PartialEq, Eq)]
pub struct AppserviceSchedulerTelemetrySummary {
    /// The `total_services` field.
    pub total_services: usize,
    /// The `scheduler_available_services` field.
    pub scheduler_available_services: usize,
    /// The `services_in_backoff` field.
    pub services_in_backoff: usize,
    /// The `services_capacity_limited` field.
    pub services_capacity_limited: usize,
    /// The `services_with_pending_transactions` field.
    pub services_with_pending_transactions: usize,
    /// The `total_pending_events` field.
    pub total_pending_events: i64,
    /// The `total_pending_transactions` field.
    pub total_pending_transactions: i64,
    /// The `total_success_count` field.
    pub total_success_count: i64,
    /// The `total_failure_count` field.
    pub total_failure_count: i64,
    /// The `total_backoff_count` field.
    pub total_backoff_count: i64,
    /// The `total_capacity_limited_count` field.
    pub total_capacity_limited_count: i64,
    /// The `total_in_flight_count` field.
    pub total_in_flight_count: i64,
}

/// The `TelemetryAlertsResponse` struct.
#[derive(Debug, Serialize)]
pub struct TelemetryAlertsResponse {
    /// The `alerts` field.
    pub alerts: Vec<TelemetryAlert>,
}

/// The `TelemetryAlertQuery` struct.
#[derive(Debug, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct TelemetryAlertQuery {
    /// The `status` field.
    pub status: Option<String>,
    /// The `severity` field.
    pub severity: Option<String>,
    /// The `refresh` field.
    pub refresh: Option<bool>,
}

/// See [`get_status`].
pub async fn get_status(
    State(ctx): State<AdminContext>,
    _admin_user: AdminUser,
) -> Result<impl IntoResponse, ApiError> {
    let config = &ctx.config.telemetry;
    let prometheus = &ctx.config.prometheus;

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

/// See [`get_resource_attributes`].
pub async fn get_resource_attributes(
    State(ctx): State<AdminContext>,
    _admin_user: AdminUser,
) -> Result<impl IntoResponse, ApiError> {
    let config = &ctx.config.telemetry;
    let prometheus = &ctx.config.prometheus;

    let telemetry_service = TelemetryService::new(Arc::new(config.clone()), Arc::new(prometheus.clone()));

    let response = ResourceAttributesResponse { attributes: telemetry_service.get_resource_attributes() };

    Ok(Json(response))
}

/// See [`get_metrics_summary`].
pub async fn get_metrics_summary(
    State(ctx): State<AdminContext>,
    _admin_user: AdminUser,
) -> Result<impl IntoResponse, ApiError> {
    let inventory = ctx.metrics.inventory();
    let metrics = ctx.metrics.collect_metrics();
    let rendered = ctx.metrics.to_prometheus_format();
    let appservice_statistics = ctx.app_service_manager.get_statistics().await?;

    // 真正的 Prometheus 抓取端点是独立端口上的 `render_prometheus_metrics`
    // （见 src/server/mod.rs）。它默认关闭（`PrometheusConfig::enabled = false`），
    // 关闭时这里返回 None —— 明确告诉查询者「当前没有可 scrape 的目标」，
    // 而不是让人误以为本 JSON 端点就是抓取点。
    let prometheus = &ctx.config.prometheus;
    let prometheus_scrape_target = prometheus.enabled.then(|| PrometheusScrapeTarget {
        port: prometheus.port,
        path: prometheus.path.clone(),
        note: "独立端口监听；本 admin 端点是 JSON 摘要，不能被 Prometheus scrape",
    });

    Ok(Json(MetricsSummaryResponse {
        total_metrics: metrics.len(),
        total_counters: inventory.total_counters,
        total_gauges: inventory.total_gauges,
        total_histograms: inventory.total_histograms,
        rendered_bytes: rendered.len(),
        snapshot_ts: current_timestamp_millis(),
        appservice_scheduler: summarize_appservice_scheduler_metrics(&appservice_statistics),
        prometheus_scrape_target,
    }))
}

/// Aggregate the appservice scheduler metrics into the telemetry summary the
/// server's background telemetry loop reports.
pub fn summarize_appservice_scheduler_metrics(
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

/// See [`health_check`].
pub async fn health_check(
    State(ctx): State<AdminContext>,
    _admin_user: AdminUser,
) -> Result<impl IntoResponse, ApiError> {
    let config = &ctx.config.telemetry;
    let prometheus = &ctx.config.prometheus;

    let telemetry_service = TelemetryService::new(Arc::new(config.clone()), Arc::new(prometheus.clone()));

    let readiness = ctx.health_checker.check_readiness().await;
    let (database_health, alerts) = ctx.telemetry_alert_service.sync_with_health().await?;

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

/// See [`list_alerts`].
pub async fn list_alerts(
    State(ctx): State<AdminContext>,
    Query(query): Query<TelemetryAlertQuery>,
    _admin_user: AdminUser,
) -> Result<impl IntoResponse, ApiError> {
    if query.refresh.unwrap_or(true) {
        let _ = ctx.telemetry_alert_service.sync_with_health().await?;
    }

    let alerts = ctx
        .telemetry_alert_service
        .list_alerts(&TelemetryAlertFilters { status: query.status, severity: query.severity })?;

    Ok(Json(TelemetryAlertsResponse { alerts }))
}

/// See [`acknowledge_alert`].
pub async fn acknowledge_alert(
    State(ctx): State<AdminContext>,
    headers: HeaderMap,
    Path(alert_id): Path<String>,
    admin_user: AdminUser,
) -> Result<impl IntoResponse, ApiError> {
    let alert = ctx.telemetry_alert_service.acknowledge_alert(&alert_id, &admin_user.user_id)?;

    ctx.admin_audit_service
        .create_event(synapse_services::admin_audit_service::CreateAuditEventRequest {
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

/// See [`create_telemetry_router`].
pub fn create_telemetry_router(state: AppState) -> axum::Router<AppState> {
    use axum::routing::*;

    axum::Router::new()
        .route("/_synapse/admin/v1/telemetry/status", get(get_status))
        .route("/_synapse/admin/v1/telemetry/attributes", get(get_resource_attributes))
        .route("/_synapse/admin/v1/telemetry/metrics", get(get_metrics_summary))
        .route("/_synapse/admin/v1/telemetry/alerts", get(list_alerts))
        .route("/_synapse/admin/v1/telemetry/alerts/{alert_id}/ack", post(acknowledge_alert))
        .route("/_synapse/admin/v1/telemetry/health", get(health_check))
        .route_layer(axum::middleware::from_fn_with_state(
            <crate::routes::context::AdminContext as axum::extract::FromRef<crate::routes::AppState>>::from_ref(&state),
            crate::middleware::admin_auth_middleware,
        ))
        .with_state(state)
}

fn request_id(headers: &HeaderMap) -> String {
    crate::utils::auth::resolve_request_id(headers)
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

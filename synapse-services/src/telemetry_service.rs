use opentelemetry::{global, KeyValue};
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::{
    trace::{Sampler, SdkTracerProvider},
    Resource,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use synapse_common::error::ApiError;
use synapse_common::telemetry_config::{OpenTelemetryConfig, PrometheusConfig};
use synapse_storage::{DatabaseHealthStatus, DatabaseMonitor};
use tracing::info;

pub struct TelemetryService {
    config: Arc<OpenTelemetryConfig>,
    prometheus_config: Arc<PrometheusConfig>,
}

impl TelemetryService {
    pub fn new(config: Arc<OpenTelemetryConfig>, prometheus_config: Arc<PrometheusConfig>) -> Self {
        Self { config, prometheus_config }
    }

    pub fn is_enabled(&self) -> bool {
        self.config.is_enabled() || self.prometheus_config.enabled
    }

    pub fn is_trace_enabled(&self) -> bool {
        self.config.is_trace_enabled()
    }

    pub fn is_metrics_enabled(&self) -> bool {
        self.config.is_metrics_enabled() || self.prometheus_config.enabled
    }

    pub fn get_service_name(&self) -> &str {
        &self.config.service_name
    }

    pub fn get_sampling_ratio(&self) -> f64 {
        if self.config.is_trace_enabled() {
            self.config.sampling_ratio
        } else {
            0.0
        }
    }

    pub fn get_export_config(&self) -> ExportConfig {
        ExportConfig {
            otlp_endpoint: self.config.otlp_endpoint.clone(),
            prometheus_port: if self.prometheus_config.enabled { Some(self.prometheus_config.port) } else { None },
            prometheus_path: if self.prometheus_config.enabled {
                Some(self.prometheus_config.path.clone())
            } else {
                None
            },
            batch_export: self.config.batch_export,
            export_timeout_seconds: self.config.export_timeout_seconds,
            max_queue_size: self.config.max_queue_size,
            max_export_batch_size: self.config.max_export_batch_size,
            scheduled_delay_millis: self.config.scheduled_delay_millis,
        }
    }

    pub fn get_resource_attributes(&self) -> std::collections::HashMap<String, String> {
        self.config.get_resource_attributes()
    }

    pub fn initialize(&self) -> Result<Option<SdkTracerProvider>, Box<dyn std::error::Error + Send + Sync>> {
        if !self.is_enabled() {
            info!(
                service_name = %self.get_service_name(),
                trace_enabled = self.is_trace_enabled(),
                metrics_enabled = self.is_metrics_enabled(),
                prometheus_enabled = self.prometheus_config.enabled,
                "Telemetry is disabled"
            );
            return Ok(None);
        }

        info!(
            service_name = %self.get_service_name(),
            trace_enabled = self.is_trace_enabled(),
            metrics_enabled = self.is_metrics_enabled(),
            prometheus_enabled = self.prometheus_config.enabled,
            "Initializing telemetry service"
        );

        let provider = if self.config.is_trace_enabled() { Some(self.initialize_tracing()?) } else { None };

        if self.config.is_metrics_enabled() || self.prometheus_config.enabled {
            self.initialize_metrics()?;
        }

        info!(
            service_name = %self.get_service_name(),
            trace_enabled = self.is_trace_enabled(),
            metrics_enabled = self.is_metrics_enabled(),
            prometheus_enabled = self.prometheus_config.enabled,
            "Telemetry service initialized successfully"
        );
        Ok(provider)
    }

    fn initialize_tracing(&self) -> Result<SdkTracerProvider, Box<dyn std::error::Error + Send + Sync>> {
        let Some(endpoint) = self.config.resolve_otlp_endpoint() else {
            tracing::warn!(
                trace_enabled = self.config.is_trace_enabled(),
                metrics_enabled = self.config.is_metrics_enabled(),
                prometheus_enabled = self.prometheus_config.enabled,
                otlp_endpoint_configured = false,
                env_var = %"OTEL_EXPORTER_OTLP_ENDPOINT",
                config_key = %"otlp_endpoint",
                "OTLP endpoint is not configured; tracing will not be initialized. Set the otlp_endpoint config or OTEL_EXPORTER_OTLP_ENDPOINT env var to enable OTLP tracing."
            );
            return Err("OTLP endpoint not configured".into());
        };

        info!(
            service_name = %self.get_service_name(),
            otlp_endpoint_configured = !endpoint.is_empty(),
            sampling_ratio = self.get_sampling_ratio(),
            "Initializing OTLP tracing"
        );

        let resource = Resource::builder()
            .with_attributes(vec![
                KeyValue::new("service.name", self.config.service_name.clone()),
                KeyValue::new("service.version", self.config.service_version.clone()),
                KeyValue::new("service.namespace", self.config.service_namespace.clone()),
            ])
            .build();

        let exporter = opentelemetry_otlp::SpanExporter::builder().with_tonic().with_endpoint(endpoint).build()?;

        let provider = SdkTracerProvider::builder()
            .with_batch_exporter(exporter)
            .with_sampler(Sampler::ParentBased(Box::new(Sampler::TraceIdRatioBased(self.get_sampling_ratio()))))
            .with_resource(resource)
            .build();

        global::set_tracer_provider(provider.clone());

        Ok(provider)
    }

    fn initialize_metrics(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if self.prometheus_config.enabled {
            info!(
                service_name = %self.get_service_name(),
                port = self.prometheus_config.port,
                path = %self.prometheus_config.path,
                "Prometheus metrics export is enabled"
            );
        }
        if self.config.is_metrics_enabled() {
            info!(
                service_name = %self.get_service_name(),
                metrics_enabled = self.config.is_metrics_enabled(),
                "OTLP metrics export is enabled"
            );
        }
        Ok(())
    }

    pub fn shutdown(&self) {
        if self.is_enabled() {
            info!(
                service_name = %self.get_service_name(),
                trace_enabled = self.is_trace_enabled(),
                metrics_enabled = self.is_metrics_enabled(),
                prometheus_enabled = self.prometheus_config.enabled,
                "Shutting down telemetry service"
            );
            // TracerProvider should be explicitly shutdown if kept
        }
    }
}

#[derive(Debug, Clone)]
pub struct ExportConfig {
    pub otlp_endpoint: Option<String>,
    pub prometheus_port: Option<u16>,
    pub prometheus_path: Option<String>,
    pub batch_export: bool,
    pub export_timeout_seconds: u64,
    pub max_queue_size: usize,
    pub max_export_batch_size: usize,
    pub scheduled_delay_millis: u64,
}

pub struct TelemetryBuilder {
    config: OpenTelemetryConfig,
    prometheus_config: PrometheusConfig,
}

impl TelemetryBuilder {
    pub fn new() -> Self {
        Self { config: OpenTelemetryConfig::default(), prometheus_config: PrometheusConfig::default() }
    }

    pub fn with_service_name(mut self, name: impl Into<String>) -> Self {
        self.config.service_name = name.into();
        self
    }

    pub fn with_service_version(mut self, version: impl Into<String>) -> Self {
        self.config.service_version = version.into();
        self
    }

    pub fn with_otlp_endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.config.otlp_endpoint = Some(endpoint.into());
        self.config.enabled = true;
        self
    }

    pub fn with_prometheus(mut self, port: u16, path: impl Into<String>) -> Self {
        self.prometheus_config.enabled = true;
        self.prometheus_config.port = port;
        self.prometheus_config.path = path.into();
        self
    }

    pub fn with_sampling_ratio(mut self, ratio: f64) -> Self {
        self.config.sampling_ratio = ratio;
        self
    }

    pub fn with_trace_enabled(mut self, enabled: bool) -> Self {
        self.config.trace_enabled = enabled;
        self
    }

    pub fn with_metrics_enabled(mut self, enabled: bool) -> Self {
        self.config.metrics_enabled = enabled;
        self
    }

    pub fn build(self) -> TelemetryService {
        TelemetryService::new(Arc::new(self.config), Arc::new(self.prometheus_config))
    }
}

impl Default for TelemetryBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// ——— 告警子系统 ———

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TelemetryAlertSeverity {
    Warning,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TelemetryAlertStatus {
    Warning,
    Critical,
    Acknowledged,
    Recovered,
    Closed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryAlert {
    pub alert_id: String,
    pub alert_key: String,
    pub rule_name: String,
    pub severity: TelemetryAlertSeverity,
    pub status: TelemetryAlertStatus,
    pub owner: String,
    pub message: String,
    pub trigger_count: u64,
    pub triggered_at: i64,
    pub last_seen_ts: i64,
    pub acknowledged_at: Option<i64>,
    pub acknowledged_by: Option<String>,
    pub recovered_at: Option<i64>,
    pub closed_at: Option<i64>,
    pub metrics: serde_json::Value,
}

#[derive(Debug, Clone, Default)]
pub struct TelemetryAlertFilters {
    pub status: Option<String>,
    pub severity: Option<String>,
}

#[derive(Clone)]
pub struct TelemetryAlertService {
    pool: Arc<PgPool>,
    max_connections: u32,
    alerts: Arc<RwLock<HashMap<String, TelemetryAlert>>>,
}

impl TelemetryAlertService {
    pub fn new(pool: Arc<PgPool>, max_connections: u32) -> Self {
        Self { pool, max_connections, alerts: Arc::new(RwLock::new(HashMap::new())) }
    }

    pub async fn sync_with_health(&self) -> Result<(DatabaseHealthStatus, Vec<TelemetryAlert>), ApiError> {
        let monitor = DatabaseMonitor::new((*self.pool).clone(), None, self.max_connections);
        let health = monitor
            .get_full_health_status()
            .await
            .map_err(|error| ApiError::internal_with_log("failed to collect telemetry health", &error))?;

        let now = chrono::Utc::now().timestamp_millis();
        let mut active_rules: HashMap<&str, TelemetryAlert> = HashMap::new();

        if !health.is_healthy {
            active_rules.insert(
                "db_health",
                Self::build_alert(
                    "db_health",
                    "Database health",
                    &TelemetryAlertSeverity::Critical,
                    "database",
                    "database health check returned unhealthy".to_string(),
                    json!({ "is_healthy": false }),
                ),
            );
        }

        let util = health.connection_pool_status.connection_utilization;
        if util >= 90.0 {
            active_rules.insert(
                "db_pool_utilization",
                Self::build_alert(
                    "db_pool_utilization",
                    "Connection pool utilization",
                    &TelemetryAlertSeverity::Critical,
                    "database",
                    format!("connection pool utilization is {util:.1}%"),
                    json!({ "connection_utilization": util }),
                ),
            );
        }

        let mut alerts = match self.alerts.write() {
            Ok(guard) => guard,
            Err(e) => {
                tracing::error!(error = %e, lock = %"alerts", access = %"write", operation = %"evaluate_health", "Failed to acquire alerts write lock");
                return Ok((health, Vec::new()));
            }
        };
        for (key, candidate) in &active_rules {
            match alerts.get_mut(*key) {
                Some(existing) => {
                    existing.severity = candidate.severity.clone();
                    existing.message.clone_from(&candidate.message);
                    existing.metrics = candidate.metrics.clone();
                    existing.last_seen_ts = now;
                    existing.recovered_at = None;
                    existing.trigger_count += 1;
                    if existing.status != TelemetryAlertStatus::Acknowledged {
                        existing.status = Self::status_from_severity(&candidate.severity);
                    }
                }
                None => {
                    alerts.insert(key.to_string(), candidate.clone());
                }
            }
        }
        for (key, alert) in alerts.iter_mut() {
            if !active_rules.contains_key(key.as_str())
                && matches!(
                    alert.status,
                    TelemetryAlertStatus::Warning | TelemetryAlertStatus::Critical | TelemetryAlertStatus::Acknowledged
                )
            {
                alert.status = TelemetryAlertStatus::Recovered;
                alert.recovered_at = Some(now);
                alert.last_seen_ts = now;
            }
        }
        let result = alerts.values().cloned().collect();
        drop(alerts);

        Ok((health, result))
    }

    pub fn raise_alert(
        &self,
        alert_key: &str,
        rule_name: &str,
        severity: &TelemetryAlertSeverity,
        owner: &str,
        message: &str,
        metrics: serde_json::Value,
    ) -> TelemetryAlert {
        let mut alerts = match self.alerts.write() {
            Ok(guard) => guard,
            Err(e) => {
                tracing::error!(
                    error = %e,
                    lock = %"alerts",
                    access = %"write",
                    operation = %"raise_alert",
                    alert_key = %alert_key,
                    rule_name = %rule_name,
                    "Failed to acquire alerts write lock"
                );
                return Self::build_alert(alert_key, rule_name, severity, owner, message.to_string(), metrics);
            }
        };
        let now = chrono::Utc::now().timestamp_millis();
        let entry = alerts.entry(alert_key.to_string()).or_insert_with(|| {
            Self::build_alert(alert_key, rule_name, severity, owner, message.to_string(), metrics.clone())
        });
        entry.severity = severity.clone();
        entry.status = Self::status_from_severity(severity);
        entry.message = message.to_string();
        entry.metrics = metrics;
        entry.last_seen_ts = now;
        entry.recovered_at = None;
        entry.trigger_count += 1;
        entry.clone()
    }

    pub fn list_alerts(&self, filters: &TelemetryAlertFilters) -> Result<Vec<TelemetryAlert>, ApiError> {
        Self::validate_filters(filters)?;
        let alerts = match self.alerts.read() {
            Ok(guard) => guard,
            Err(e) => {
                tracing::error!(error = %e, lock = %"alerts", access = %"read", operation = %"list_alerts", "Failed to acquire alerts read lock");
                return Ok(Vec::new());
            }
        };
        let mut entries: Vec<TelemetryAlert> = alerts
            .values()
            .filter(|a| filters.status.as_deref().is_none_or(|s| Self::matches_status(a, s)))
            .filter(|a| filters.severity.as_deref().is_none_or(|s| Self::matches_severity(a, s)))
            .cloned()
            .collect();
        entries.sort_by(|l, r| r.last_seen_ts.cmp(&l.last_seen_ts));
        Ok(entries)
    }

    pub fn acknowledge_alert(&self, alert_id: &str, acknowledged_by: &str) -> Result<TelemetryAlert, ApiError> {
        let mut alerts = match self.alerts.write() {
            Ok(guard) => guard,
            Err(e) => {
                tracing::error!(
                    error = %e,
                    lock = %"alerts",
                    access = %"write",
                    operation = %"acknowledge_alert",
                    alert_id = %alert_id,
                    acknowledged_by = %acknowledged_by,
                    "Failed to acquire alerts write lock"
                );
                return Err(ApiError::internal("Failed to acquire alerts write lock"));
            }
        };
        let now = chrono::Utc::now().timestamp_millis();
        let alert = alerts
            .values_mut()
            .find(|a| a.alert_id == alert_id)
            .ok_or_else(|| ApiError::not_found(format!("alert not found: {alert_id}")))?;
        if alert.status == TelemetryAlertStatus::Recovered {
            return Err(ApiError::bad_request("ALERT_ACK_FORBIDDEN: recovered alerts cannot be acknowledged"));
        }
        alert.status = TelemetryAlertStatus::Acknowledged;
        alert.acknowledged_at = Some(now);
        alert.acknowledged_by = Some(acknowledged_by.to_string());
        alert.last_seen_ts = now;
        Ok(alert.clone())
    }

    fn build_alert(
        key: &str,
        rule: &str,
        severity: &TelemetryAlertSeverity,
        owner: &str,
        message: String,
        metrics: serde_json::Value,
    ) -> TelemetryAlert {
        let now = chrono::Utc::now().timestamp_millis();
        TelemetryAlert {
            alert_id: uuid::Uuid::new_v4().to_string(),
            alert_key: key.to_string(),
            rule_name: rule.to_string(),
            severity: severity.clone(),
            status: Self::status_from_severity(severity),
            owner: owner.to_string(),
            message,
            trigger_count: 1,
            triggered_at: now,
            last_seen_ts: now,
            acknowledged_at: None,
            acknowledged_by: None,
            recovered_at: None,
            closed_at: None,
            metrics,
        }
    }

    fn status_from_severity(s: &TelemetryAlertSeverity) -> TelemetryAlertStatus {
        match s {
            TelemetryAlertSeverity::Warning => TelemetryAlertStatus::Warning,
            TelemetryAlertSeverity::Critical => TelemetryAlertStatus::Critical,
        }
    }

    fn validate_filters(filters: &TelemetryAlertFilters) -> Result<(), ApiError> {
        if let Some(ref s) = filters.status {
            match s.as_str() {
                "warning" | "critical" | "acknowledged" | "recovered" | "closed" => {}
                _ => return Err(ApiError::bad_request("ALERT_RULE_NOT_FOUND: unsupported alert status filter")),
            }
        }
        if let Some(ref s) = filters.severity {
            match s.as_str() {
                "warning" | "critical" => {}
                _ => return Err(ApiError::bad_request("ALERT_RULE_NOT_FOUND: unsupported alert severity filter")),
            }
        }
        Ok(())
    }

    fn matches_status(alert: &TelemetryAlert, status: &str) -> bool {
        matches!(
            (&alert.status, status),
            (TelemetryAlertStatus::Warning, "warning")
                | (TelemetryAlertStatus::Critical, "critical")
                | (TelemetryAlertStatus::Acknowledged, "acknowledged")
                | (TelemetryAlertStatus::Recovered, "recovered")
                | (TelemetryAlertStatus::Closed, "closed")
        )
    }

    fn matches_severity(alert: &TelemetryAlert, severity: &str) -> bool {
        matches!(
            (&alert.severity, severity),
            (TelemetryAlertSeverity::Warning, "warning") | (TelemetryAlertSeverity::Critical, "critical")
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========== TelemetryAlertSeverity tests ==========

    #[test]
    fn test_telemetry_alert_severity() {
        assert_ne!(TelemetryAlertSeverity::Warning, TelemetryAlertSeverity::Critical);
    }

    #[test]
    fn test_telemetry_alert_severity_clone() {
        let s = TelemetryAlertSeverity::Critical;
        assert_eq!(s.clone(), s);
    }

    // ========== TelemetryAlertStatus tests ==========

    #[test]
    fn test_telemetry_alert_status() {
        assert_ne!(TelemetryAlertStatus::Warning, TelemetryAlertStatus::Critical);
        assert_ne!(TelemetryAlertStatus::Acknowledged, TelemetryAlertStatus::Recovered);
        assert_ne!(TelemetryAlertStatus::Recovered, TelemetryAlertStatus::Closed);
    }

    #[test]
    fn test_telemetry_alert_status_clone() {
        let s = TelemetryAlertStatus::Acknowledged;
        assert_eq!(s.clone(), s);
    }

    // ========== TelemetryAlertFilters tests ==========

    #[test]
    fn test_telemetry_alert_filters_default() {
        let filters = TelemetryAlertFilters::default();
        assert!(filters.status.is_none());
        assert!(filters.severity.is_none());
    }

    #[test]
    fn test_telemetry_alert_filters_with_status() {
        let filters = TelemetryAlertFilters { status: Some("warning".to_string()), severity: None };
        assert_eq!(filters.status, Some("warning".to_string()));
    }

    #[test]
    fn test_telemetry_alert_filters_with_severity() {
        let filters = TelemetryAlertFilters { status: None, severity: Some("critical".to_string()) };
        assert_eq!(filters.severity, Some("critical".to_string()));
    }

    // ========== TelemetryAlert struct tests ==========

    #[test]
    fn test_telemetry_alert() {
        let alert = TelemetryAlert {
            alert_id: "alert-1".to_string(),
            alert_key: "db_health".to_string(),
            rule_name: "Database Health".to_string(),
            severity: TelemetryAlertSeverity::Critical,
            status: TelemetryAlertStatus::Critical,
            owner: "database".to_string(),
            message: "Database is unhealthy".to_string(),
            trigger_count: 3,
            triggered_at: 1700000000000,
            last_seen_ts: 1700000001000,
            acknowledged_at: None,
            acknowledged_by: None,
            recovered_at: None,
            closed_at: None,
            metrics: serde_json::json!({"is_healthy": false}),
        };
        assert_eq!(alert.alert_id, "alert-1");
        assert_eq!(alert.trigger_count, 3);
        assert!(alert.acknowledged_at.is_none());
    }

    // ========== TelemetryAlertService::status_from_severity tests ==========

    #[test]
    fn test_status_from_severity_warning() {
        let status = TelemetryAlertService::status_from_severity(&TelemetryAlertSeverity::Warning);
        assert_eq!(status, TelemetryAlertStatus::Warning);
    }

    #[test]
    fn test_status_from_severity_critical() {
        let status = TelemetryAlertService::status_from_severity(&TelemetryAlertSeverity::Critical);
        assert_eq!(status, TelemetryAlertStatus::Critical);
    }

    // ========== TelemetryAlertService::validate_filters tests ==========

    #[test]
    fn test_validate_filters_empty() {
        let filters = TelemetryAlertFilters::default();
        assert!(TelemetryAlertService::validate_filters(&filters).is_ok());
    }

    #[test]
    fn test_validate_filters_valid_status() {
        let valid_statuses = ["warning", "critical", "acknowledged", "recovered", "closed"];
        for s in &valid_statuses {
            let filters = TelemetryAlertFilters { status: Some(s.to_string()), severity: None };
            assert!(TelemetryAlertService::validate_filters(&filters).is_ok(), "status '{}' should be valid", s);
        }
    }

    #[test]
    fn test_validate_filters_invalid_status() {
        let filters = TelemetryAlertFilters { status: Some("invalid".to_string()), severity: None };
        assert!(TelemetryAlertService::validate_filters(&filters).is_err());
    }

    #[test]
    fn test_validate_filters_valid_severity() {
        let valid_severities = ["warning", "critical"];
        for s in &valid_severities {
            let filters = TelemetryAlertFilters { status: None, severity: Some(s.to_string()) };
            assert!(TelemetryAlertService::validate_filters(&filters).is_ok(), "severity '{}' should be valid", s);
        }
    }

    #[test]
    fn test_validate_filters_invalid_severity() {
        let filters = TelemetryAlertFilters { status: None, severity: Some("invalid".to_string()) };
        assert!(TelemetryAlertService::validate_filters(&filters).is_err());
    }

    // ========== TelemetryAlertService::matches_status tests ==========

    #[test]
    fn test_matches_status_warning() {
        let alert = TelemetryAlert {
            alert_id: "a".to_string(),
            alert_key: "k".to_string(),
            rule_name: "r".to_string(),
            severity: TelemetryAlertSeverity::Warning,
            status: TelemetryAlertStatus::Warning,
            owner: "o".to_string(),
            message: "m".to_string(),
            trigger_count: 1,
            triggered_at: 1,
            last_seen_ts: 1,
            acknowledged_at: None,
            acknowledged_by: None,
            recovered_at: None,
            closed_at: None,
            metrics: serde_json::json!({}),
        };
        assert!(TelemetryAlertService::matches_status(&alert, "warning"));
        assert!(!TelemetryAlertService::matches_status(&alert, "critical"));
        assert!(!TelemetryAlertService::matches_status(&alert, "acknowledged"));
    }

    #[test]
    fn test_matches_status_acknowledged() {
        let alert = TelemetryAlert {
            alert_id: "a".to_string(),
            alert_key: "k".to_string(),
            rule_name: "r".to_string(),
            severity: TelemetryAlertSeverity::Critical,
            status: TelemetryAlertStatus::Acknowledged,
            owner: "o".to_string(),
            message: "m".to_string(),
            trigger_count: 1,
            triggered_at: 1,
            last_seen_ts: 1,
            acknowledged_at: None,
            acknowledged_by: None,
            recovered_at: None,
            closed_at: None,
            metrics: serde_json::json!({}),
        };
        assert!(TelemetryAlertService::matches_status(&alert, "acknowledged"));
        assert!(!TelemetryAlertService::matches_status(&alert, "warning"));
    }

    #[test]
    fn test_matches_status_recovered() {
        let alert = TelemetryAlert {
            alert_id: "a".to_string(),
            alert_key: "k".to_string(),
            rule_name: "r".to_string(),
            severity: TelemetryAlertSeverity::Warning,
            status: TelemetryAlertStatus::Recovered,
            owner: "o".to_string(),
            message: "m".to_string(),
            trigger_count: 1,
            triggered_at: 1,
            last_seen_ts: 1,
            acknowledged_at: None,
            acknowledged_by: None,
            recovered_at: Some(1),
            closed_at: None,
            metrics: serde_json::json!({}),
        };
        assert!(TelemetryAlertService::matches_status(&alert, "recovered"));
        assert!(!TelemetryAlertService::matches_status(&alert, "closed"));
    }

    #[test]
    fn test_matches_status_closed() {
        let alert = TelemetryAlert {
            alert_id: "a".to_string(),
            alert_key: "k".to_string(),
            rule_name: "r".to_string(),
            severity: TelemetryAlertSeverity::Critical,
            status: TelemetryAlertStatus::Closed,
            owner: "o".to_string(),
            message: "m".to_string(),
            trigger_count: 1,
            triggered_at: 1,
            last_seen_ts: 1,
            acknowledged_at: None,
            acknowledged_by: None,
            recovered_at: None,
            closed_at: Some(1),
            metrics: serde_json::json!({}),
        };
        assert!(TelemetryAlertService::matches_status(&alert, "closed"));
    }

    // ========== TelemetryAlertService::matches_severity tests ==========

    #[test]
    fn test_matches_severity_warning() {
        let alert = TelemetryAlert {
            alert_id: "a".to_string(),
            alert_key: "k".to_string(),
            rule_name: "r".to_string(),
            severity: TelemetryAlertSeverity::Warning,
            status: TelemetryAlertStatus::Warning,
            owner: "o".to_string(),
            message: "m".to_string(),
            trigger_count: 1,
            triggered_at: 1,
            last_seen_ts: 1,
            acknowledged_at: None,
            acknowledged_by: None,
            recovered_at: None,
            closed_at: None,
            metrics: serde_json::json!({}),
        };
        assert!(TelemetryAlertService::matches_severity(&alert, "warning"));
        assert!(!TelemetryAlertService::matches_severity(&alert, "critical"));
    }

    #[test]
    fn test_matches_severity_critical() {
        let alert = TelemetryAlert {
            alert_id: "a".to_string(),
            alert_key: "k".to_string(),
            rule_name: "r".to_string(),
            severity: TelemetryAlertSeverity::Critical,
            status: TelemetryAlertStatus::Critical,
            owner: "o".to_string(),
            message: "m".to_string(),
            trigger_count: 1,
            triggered_at: 1,
            last_seen_ts: 1,
            acknowledged_at: None,
            acknowledged_by: None,
            recovered_at: None,
            closed_at: None,
            metrics: serde_json::json!({}),
        };
        assert!(TelemetryAlertService::matches_severity(&alert, "critical"));
        assert!(!TelemetryAlertService::matches_severity(&alert, "warning"));
    }

    // ========== TelemetryBuilder tests ==========

    #[test]
    fn test_telemetry_builder_default() {
        let builder = TelemetryBuilder::default();
        let service = builder.build();
        assert!(!service.is_trace_enabled());
        assert!(!service.is_metrics_enabled());
    }

    #[test]
    fn test_telemetry_builder_with_service_name() {
        let service = TelemetryBuilder::new().with_service_name("my-service").build();
        assert_eq!(service.get_service_name(), "my-service");
    }

    #[test]
    fn test_telemetry_builder_with_trace_enabled() {
        let service =
            TelemetryBuilder::new().with_otlp_endpoint("http://localhost:4317").with_trace_enabled(true).build();
        assert!(service.is_trace_enabled());
    }

    #[test]
    fn test_telemetry_builder_with_sampling_ratio() {
        let service = TelemetryBuilder::new()
            .with_otlp_endpoint("http://localhost:4317")
            .with_trace_enabled(true)
            .with_sampling_ratio(0.5)
            .build();
        assert_eq!(service.get_sampling_ratio(), 0.5);
    }

    #[test]
    fn test_telemetry_builder_with_otlp_endpoint() {
        let service = TelemetryBuilder::new().with_otlp_endpoint("http://localhost:4317").build();
        assert!(service.is_enabled());
    }

    #[test]
    fn test_telemetry_builder_with_prometheus() {
        let service = TelemetryBuilder::new().with_prometheus(9090, "/metrics").build();
        assert!(service.is_metrics_enabled());
    }

    #[test]
    fn test_telemetry_builder_chain() {
        let service = TelemetryBuilder::new()
            .with_service_name("test")
            .with_service_version("1.0")
            .with_otlp_endpoint("http://localhost:4317")
            .with_trace_enabled(true)
            .with_metrics_enabled(true)
            .with_sampling_ratio(0.1)
            .build();
        assert_eq!(service.get_service_name(), "test");
        assert!(service.is_trace_enabled());
        assert!(service.is_metrics_enabled());
        assert_eq!(service.get_sampling_ratio(), 0.1);
    }

    // ========== ExportConfig tests ==========

    #[test]
    fn test_export_config() {
        let config = ExportConfig {
            otlp_endpoint: Some("http://localhost:4317".to_string()),
            prometheus_port: Some(9090),
            prometheus_path: Some("/metrics".to_string()),
            batch_export: true,
            export_timeout_seconds: 30,
            max_queue_size: 2048,
            max_export_batch_size: 512,
            scheduled_delay_millis: 5000,
        };
        assert_eq!(config.otlp_endpoint, Some("http://localhost:4317".to_string()));
        assert_eq!(config.prometheus_port, Some(9090));
        assert!(config.batch_export);
    }
}

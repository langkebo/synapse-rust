//! OpenTelemetry + Prometheus exporter configuration models.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
/// Represents OpenTelemetryConfig.
pub struct OpenTelemetryConfig {
    /// `enabled` field.
    pub enabled: bool,
    /// `service_name` field.
    pub service_name: String,
    /// `service_version` field.
    pub service_version: String,
    /// `service_namespace` field.
    pub service_namespace: String,
    /// `otlp_endpoint` field.
    pub otlp_endpoint: Option<String>,
    /// `otlp_headers` field.
    pub otlp_headers: Option<std::collections::HashMap<String, String>>,
    /// `trace_enabled` field.
    pub trace_enabled: bool,
    /// `metrics_enabled` field.
    pub metrics_enabled: bool,
    /// `logs_enabled` field.
    pub logs_enabled: bool,
    /// `sampling_ratio` field.
    pub sampling_ratio: f64,
    /// `batch_export` field.
    pub batch_export: bool,
    /// `export_timeout_seconds` field.
    pub export_timeout_seconds: u64,
    /// `max_queue_size` field.
    pub max_queue_size: usize,
    /// `max_export_batch_size` field.
    pub max_export_batch_size: usize,
    /// `scheduled_delay_millis` field.
    pub scheduled_delay_millis: u64,
    /// `resource_attributes` field.
    pub resource_attributes: Option<std::collections::HashMap<String, String>>,
}

impl Default for OpenTelemetryConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            service_name: "synapse-rust".to_string(),
            service_version: env!("CARGO_PKG_VERSION").to_string(),
            service_namespace: "matrix".to_string(),
            otlp_endpoint: None,
            otlp_headers: None,
            trace_enabled: true,
            metrics_enabled: true,
            logs_enabled: false,
            sampling_ratio: 1.0,
            batch_export: true,
            export_timeout_seconds: 30,
            max_queue_size: 2048,
            max_export_batch_size: 512,
            scheduled_delay_millis: 5000,
            resource_attributes: None,
        }
    }
}

impl OpenTelemetryConfig {
    /// Returns true if enabled.
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Returns true if trace enabled.
    pub fn is_trace_enabled(&self) -> bool {
        self.enabled && self.trace_enabled
    }

    /// Returns true if metrics enabled.
    pub fn is_metrics_enabled(&self) -> bool {
        self.enabled && self.metrics_enabled
    }

    /// Returns true if logs enabled.
    pub fn is_logs_enabled(&self) -> bool {
        self.enabled && self.logs_enabled
    }

    /// Returns the otlp endpoint.
    pub fn get_otlp_endpoint(&self) -> Option<&str> {
        self.otlp_endpoint.as_deref()
    }

    /// Resolve the OTLP endpoint to use.
    ///
    /// Priority:
    /// 1. Explicit `otlp_endpoint` in config
    /// 2. `OTEL_EXPORTER_OTLP_ENDPOINT` environment variable
    /// 3. In debug builds: default dev endpoint `http://localhost:4317` (gRPC)
    /// 4. In release builds: no default (returns None)
    pub fn resolve_otlp_endpoint(&self) -> Option<String> {
        // 1. Explicit config takes highest priority
        if let Some(ref endpoint) = self.otlp_endpoint {
            return Some(endpoint.clone());
        }

        // 2. Standard OTel environment variable
        if let Ok(endpoint) = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT") {
            if !endpoint.is_empty() {
                return Some(endpoint);
            }
        }

        // 3. Dev default: only in debug builds
        #[cfg(debug_assertions)]
        {
            tracing::info!(
                "OTLP endpoint not explicitly configured; using dev default http://localhost:4317 (gRPC). \
                 Set OTEL_EXPORTER_OTLP_ENDPOINT or config otlp_endpoint to override."
            );
            Some("http://localhost:4317".to_string())
        }

        // 4. Release: no default, must be explicitly configured
        #[cfg(not(debug_assertions))]
        {
            None
        }
    }

    /// Returns the resource attributes.
    pub fn get_resource_attributes(&self) -> std::collections::HashMap<String, String> {
        let mut attrs = self.resource_attributes.clone().unwrap_or_default();
        attrs.insert("service.name".to_string(), self.service_name.clone());
        attrs.insert("service.version".to_string(), self.service_version.clone());
        attrs.insert("service.namespace".to_string(), self.service_namespace.clone());
        attrs
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// Represents PrometheusConfig.
pub struct PrometheusConfig {
    /// `enabled` field.
    pub enabled: bool,
    /// `port` field.
    pub port: u16,
    /// `path` field.
    pub path: String,
    /// `include_namespace` field.
    pub include_namespace: bool,
}

impl Default for PrometheusConfig {
    fn default() -> Self {
        Self { enabled: false, port: 9090, path: "/metrics".to_string(), include_namespace: true }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_otel() -> OpenTelemetryConfig {
        OpenTelemetryConfig::default()
    }

    #[test]
    fn otel_disabled_by_default() {
        let config = make_otel();
        assert!(!config.is_enabled());
        assert!(!config.is_trace_enabled());
        assert!(!config.is_metrics_enabled());
        assert!(!config.is_logs_enabled());
    }

    #[test]
    fn otel_trace_only_enabled_when_both_flags_set() {
        let mut config = make_otel();
        config.enabled = true;
        config.trace_enabled = false;
        assert!(config.is_enabled());
        assert!(!config.is_trace_enabled());
    }

    #[test]
    fn otel_sub_flags_all_false_when_master_false() {
        let mut config = make_otel();
        config.enabled = false;
        config.trace_enabled = true;
        config.metrics_enabled = true;
        config.logs_enabled = true;
        assert!(!config.is_trace_enabled());
        assert!(!config.is_metrics_enabled());
        assert!(!config.is_logs_enabled());
    }

    #[test]
    fn otel_all_enabled_when_all_flags_set() {
        let mut config = make_otel();
        config.enabled = true;
        config.trace_enabled = true;
        config.metrics_enabled = true;
        config.logs_enabled = true;
        assert!(config.is_trace_enabled());
        assert!(config.is_metrics_enabled());
        assert!(config.is_logs_enabled());
    }

    #[test]
    fn otel_get_endpoint_returns_none_by_default() {
        let config = make_otel();
        assert_eq!(config.get_otlp_endpoint(), None);
    }

    #[test]
    fn otel_get_endpoint_returns_custom() {
        let mut config = make_otel();
        config.otlp_endpoint = Some("https://otel.example.com:4317".into());
        assert_eq!(config.get_otlp_endpoint(), Some("https://otel.example.com:4317"));
    }

    #[test]
    fn otel_resolve_endpoint_prefers_explicit_config() {
        let mut config = make_otel();
        config.otlp_endpoint = Some("https://custom.example.com".into());
        let result = config.resolve_otlp_endpoint();
        assert_eq!(result, Some("https://custom.example.com".into()));
    }

    #[test]
    fn otel_resource_attributes_includes_service_metadata() {
        let mut config = make_otel();
        config.service_name = "test-service".into();
        config.service_version = "1.2.3".into();
        config.service_namespace = "testing".into();
        let attrs = config.get_resource_attributes();
        assert_eq!(attrs.get("service.name").map(|s| s.as_str()), Some("test-service"));
        assert_eq!(attrs.get("service.version").map(|s| s.as_str()), Some("1.2.3"));
        assert_eq!(attrs.get("service.namespace").map(|s| s.as_str()), Some("testing"));
    }

    #[test]
    fn otel_resource_attributes_merges_custom() {
        let mut config = make_otel();
        let mut custom = std::collections::HashMap::new();
        custom.insert("custom.key".into(), "custom-value".into());
        config.resource_attributes = Some(custom);
        let attrs = config.get_resource_attributes();
        assert_eq!(attrs.get("custom.key").map(|s| s.as_str()), Some("custom-value"));
        assert!(attrs.contains_key("service.name"));
    }
}

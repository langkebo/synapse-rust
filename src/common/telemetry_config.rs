use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenTelemetryConfig {
    pub enabled: bool,
    pub service_name: String,
    pub service_version: String,
    pub service_namespace: String,
    pub otlp_endpoint: Option<String>,
    pub otlp_headers: Option<std::collections::HashMap<String, String>>,
    pub trace_enabled: bool,
    pub metrics_enabled: bool,
    pub logs_enabled: bool,
    pub sampling_ratio: f64,
    pub batch_export: bool,
    pub export_timeout_seconds: u64,
    pub max_queue_size: usize,
    pub max_export_batch_size: usize,
    pub scheduled_delay_millis: u64,
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
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn is_trace_enabled(&self) -> bool {
        self.enabled && self.trace_enabled
    }

    pub fn is_metrics_enabled(&self) -> bool {
        self.enabled && self.metrics_enabled
    }

    pub fn is_logs_enabled(&self) -> bool {
        self.enabled && self.logs_enabled
    }

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

    pub fn get_resource_attributes(&self) -> std::collections::HashMap<String, String> {
        let mut attrs = self.resource_attributes.clone().unwrap_or_default();
        attrs.insert("service.name".to_string(), self.service_name.clone());
        attrs.insert("service.version".to_string(), self.service_version.clone());
        attrs.insert("service.namespace".to_string(), self.service_namespace.clone());
        attrs
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrometheusConfig {
    pub enabled: bool,
    pub port: u16,
    pub path: String,
    pub include_namespace: bool,
}

impl Default for PrometheusConfig {
    fn default() -> Self {
        Self { enabled: false, port: 9090, path: "/metrics".to_string(), include_namespace: true }
    }
}

#[cfg(test)]
mod tests {
    use super::{OpenTelemetryConfig, PrometheusConfig};
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};

    fn env_lock() -> &'static Mutex<()> {
        static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        ENV_LOCK.get_or_init(|| Mutex::new(()))
    }

    fn with_otlp_env_var<T>(value: Option<&str>, test: impl FnOnce() -> T) -> T {
        let _guard = env_lock().lock().expect("env lock should not be poisoned");
        let previous = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").ok();

        match value {
            Some(endpoint) => {
                // SAFETY: tests serialize environment mutation with a global mutex.
                unsafe { std::env::set_var("OTEL_EXPORTER_OTLP_ENDPOINT", endpoint) }
            }
            None => {
                // SAFETY: tests serialize environment mutation with a global mutex.
                unsafe { std::env::remove_var("OTEL_EXPORTER_OTLP_ENDPOINT") }
            }
        }

        let result = test();

        match previous {
            Some(endpoint) => {
                // SAFETY: tests serialize environment mutation with a global mutex.
                unsafe { std::env::set_var("OTEL_EXPORTER_OTLP_ENDPOINT", endpoint) }
            }
            None => {
                // SAFETY: tests serialize environment mutation with a global mutex.
                unsafe { std::env::remove_var("OTEL_EXPORTER_OTLP_ENDPOINT") }
            }
        }

        result
    }

    #[test]
    fn open_telemetry_config_prefers_explicit_endpoint_over_environment() {
        with_otlp_env_var(Some("http://env:4317"), || {
            let config = OpenTelemetryConfig {
                enabled: true,
                otlp_endpoint: Some("http://config:4317".to_string()),
                ..Default::default()
            };

            assert_eq!(config.resolve_otlp_endpoint().as_deref(), Some("http://config:4317"));
        });
    }

    #[test]
    fn open_telemetry_config_uses_environment_endpoint_when_config_is_absent() {
        with_otlp_env_var(Some("http://env:4317"), || {
            let config = OpenTelemetryConfig::default();

            assert_eq!(config.resolve_otlp_endpoint().as_deref(), Some("http://env:4317"));
        });
    }

    #[test]
    fn open_telemetry_config_uses_debug_default_when_unset() {
        with_otlp_env_var(None, || {
            let config = OpenTelemetryConfig::default();

            #[cfg(debug_assertions)]
            assert_eq!(config.resolve_otlp_endpoint().as_deref(), Some("http://localhost:4317"));

            #[cfg(not(debug_assertions))]
            assert_eq!(config.resolve_otlp_endpoint(), None);
        });
    }

    #[test]
    fn open_telemetry_config_merges_resource_attributes_with_service_metadata() {
        let mut resource_attributes = HashMap::new();
        resource_attributes.insert("deployment.environment".to_string(), "dev".to_string());
        resource_attributes.insert("service.name".to_string(), "should-be-overridden".to_string());

        let config = OpenTelemetryConfig {
            service_name: "synapse-rust-test".to_string(),
            service_version: "9.9.9".to_string(),
            service_namespace: "matrix-test".to_string(),
            resource_attributes: Some(resource_attributes),
            ..Default::default()
        };

        let attrs = config.get_resource_attributes();
        assert_eq!(attrs.get("deployment.environment").map(String::as_str), Some("dev"));
        assert_eq!(attrs.get("service.name").map(String::as_str), Some("synapse-rust-test"));
        assert_eq!(attrs.get("service.version").map(String::as_str), Some("9.9.9"));
        assert_eq!(attrs.get("service.namespace").map(String::as_str), Some("matrix-test"));
    }

    #[test]
    fn prometheus_config_defaults_match_metrics_endpoint_convention() {
        let config = PrometheusConfig::default();

        assert!(!config.enabled);
        assert_eq!(config.port, 9090);
        assert_eq!(config.path, "/metrics");
        assert!(config.include_namespace);
    }
}

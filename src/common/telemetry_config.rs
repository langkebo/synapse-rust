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

    pub fn get_resource_attributes(&self) -> std::collections::HashMap<String, String> {
        let mut attrs = self.resource_attributes.clone().unwrap_or_default();
        attrs.insert("service.name".to_string(), self.service_name.clone());
        attrs.insert("service.version".to_string(), self.service_version.clone());
        attrs.insert(
            "service.namespace".to_string(),
            self.service_namespace.clone(),
        );
        attrs
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JaegerConfig {
    pub enabled: bool,
    pub agent_endpoint: Option<String>,
    pub collector_endpoint: Option<String>,
    pub service_name: String,
    pub sampling_rate: f64,
}

impl Default for JaegerConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            agent_endpoint: Some("127.0.0.1:6831".to_string()),
            collector_endpoint: None,
            service_name: "synapse-rust".to_string(),
            sampling_rate: 1.0,
        }
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
        Self {
            enabled: false,
            port: 9090,
            path: "/metrics".to_string(),
            include_namespace: true,
        }
    }
}

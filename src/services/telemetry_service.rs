use crate::common::telemetry_config::{JaegerConfig, OpenTelemetryConfig, PrometheusConfig};
use std::sync::Arc;
use tracing::info;

pub struct TelemetryService {
    config: Arc<OpenTelemetryConfig>,
    jaeger_config: Arc<JaegerConfig>,
    prometheus_config: Arc<PrometheusConfig>,
}

impl TelemetryService {
    pub fn new(
        config: Arc<OpenTelemetryConfig>,
        jaeger_config: Arc<JaegerConfig>,
        prometheus_config: Arc<PrometheusConfig>,
    ) -> Self {
        Self {
            config,
            jaeger_config,
            prometheus_config,
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.config.is_enabled() || self.jaeger_config.enabled || self.prometheus_config.enabled
    }

    pub fn is_trace_enabled(&self) -> bool {
        self.config.is_trace_enabled() || self.jaeger_config.enabled
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
        } else if self.jaeger_config.enabled {
            self.jaeger_config.sampling_rate
        } else {
            0.0
        }
    }

    pub fn get_export_config(&self) -> ExportConfig {
        ExportConfig {
            otlp_endpoint: self.config.otlp_endpoint.clone(),
            jaeger_agent: self.jaeger_config.agent_endpoint.clone(),
            jaeger_collector: self.jaeger_config.collector_endpoint.clone(),
            prometheus_port: if self.prometheus_config.enabled {
                Some(self.prometheus_config.port)
            } else {
                None
            },
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

    pub fn initialize(&self) -> Result<(), Box<dyn std::error::Error>> {
        if !self.is_enabled() {
            info!("Telemetry is disabled");
            return Ok(());
        }

        info!(
            "Initializing telemetry service: {} (trace: {}, metrics: {})",
            self.get_service_name(),
            self.is_trace_enabled(),
            self.is_metrics_enabled()
        );

        if self.config.is_trace_enabled() {
            self.initialize_tracing()?;
        }

        if self.config.is_metrics_enabled() || self.prometheus_config.enabled {
            self.initialize_metrics()?;
        }

        info!("Telemetry service initialized successfully");
        Ok(())
    }

    fn initialize_tracing(&self) -> Result<(), Box<dyn std::error::Error>> {
        info!(
            "Initializing tracing with sampling ratio: {}",
            self.get_sampling_ratio()
        );

        if let Some(endpoint) = &self.config.otlp_endpoint {
            info!("OTLP tracing endpoint: {}", endpoint);
        }

        if self.jaeger_config.enabled {
            if let Some(agent) = &self.jaeger_config.agent_endpoint {
                info!("Jaeger agent endpoint: {}", agent);
            }
            if let Some(collector) = &self.jaeger_config.collector_endpoint {
                info!("Jaeger collector endpoint: {}", collector);
            }
        }

        Ok(())
    }

    fn initialize_metrics(&self) -> Result<(), Box<dyn std::error::Error>> {
        info!("Initializing metrics collection");

        if self.prometheus_config.enabled {
            info!(
                "Prometheus metrics endpoint: http://localhost:{}{}",
                self.prometheus_config.port, self.prometheus_config.path
            );
        }

        Ok(())
    }

    pub fn shutdown(&self) {
        if self.is_enabled() {
            info!("Shutting down telemetry service");
        }
    }
}

#[derive(Debug, Clone)]
pub struct ExportConfig {
    pub otlp_endpoint: Option<String>,
    pub jaeger_agent: Option<String>,
    pub jaeger_collector: Option<String>,
    pub prometheus_port: Option<u16>,
    pub prometheus_path: Option<String>,
    pub batch_export: bool,
    pub export_timeout_seconds: u64,
    pub max_queue_size: usize,
    pub max_export_batch_size: usize,
    pub scheduled_delay_millis: u64,
}

#[derive(Debug, Clone)]
pub struct SpanContext {
    pub trace_id: String,
    pub span_id: String,
    pub trace_flags: u8,
    pub is_remote: bool,
}

#[derive(Debug, Clone)]
pub struct MetricValue {
    pub name: String,
    pub value: f64,
    pub labels: std::collections::HashMap<String, String>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

pub struct TelemetryBuilder {
    config: OpenTelemetryConfig,
    jaeger_config: JaegerConfig,
    prometheus_config: PrometheusConfig,
}

impl TelemetryBuilder {
    pub fn new() -> Self {
        Self {
            config: OpenTelemetryConfig::default(),
            jaeger_config: JaegerConfig::default(),
            prometheus_config: PrometheusConfig::default(),
        }
    }

    pub fn with_service_name(mut self, name: impl Into<String>) -> Self {
        self.config.service_name = name.into();
        self.jaeger_config.service_name = self.config.service_name.clone();
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

    pub fn with_jaeger_agent(mut self, endpoint: impl Into<String>) -> Self {
        self.jaeger_config.agent_endpoint = Some(endpoint.into());
        self.jaeger_config.enabled = true;
        self
    }

    pub fn with_jaeger_collector(mut self, endpoint: impl Into<String>) -> Self {
        self.jaeger_config.collector_endpoint = Some(endpoint.into());
        self.jaeger_config.enabled = true;
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
        self.jaeger_config.sampling_rate = ratio;
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
        TelemetryService::new(
            Arc::new(self.config),
            Arc::new(self.jaeger_config),
            Arc::new(self.prometheus_config),
        )
    }
}

impl Default for TelemetryBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_telemetry_config_default() {
        let config = OpenTelemetryConfig::default();
        assert!(!config.is_enabled());
        assert!(config.is_trace_enabled() == false);
        assert!(config.service_name == "synapse-rust");
    }

    #[test]
    fn test_telemetry_builder() {
        let service = TelemetryBuilder::new()
            .with_service_name("test-service")
            .with_otlp_endpoint("http://localhost:4317")
            .with_sampling_ratio(0.5)
            .build();

        assert!(service.is_enabled());
        assert!(service.is_trace_enabled());
        assert_eq!(service.get_service_name(), "test-service");
        assert_eq!(service.get_sampling_ratio(), 0.5);
    }

    #[test]
    fn test_prometheus_config() {
        let service = TelemetryBuilder::new()
            .with_prometheus(9091, "/metrics")
            .build();

        assert!(service.is_metrics_enabled());
        let config = service.get_export_config();
        assert_eq!(config.prometheus_port, Some(9091));
        assert_eq!(config.prometheus_path, Some("/metrics".to_string()));
    }

    #[test]
    fn test_resource_attributes() {
        let service = TelemetryBuilder::new()
            .with_service_name("my-service")
            .with_service_version("1.0.0")
            .build();

        let attrs = service.get_resource_attributes();
        assert_eq!(attrs.get("service.name"), Some(&"my-service".to_string()));
        assert_eq!(attrs.get("service.version"), Some(&"1.0.0".to_string()));
        assert_eq!(attrs.get("service.namespace"), Some(&"matrix".to_string()));
    }
}

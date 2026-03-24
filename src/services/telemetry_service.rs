use crate::common::telemetry_config::{OpenTelemetryConfig, PrometheusConfig};
use opentelemetry::{global, KeyValue};
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::{
    trace::{Sampler, SdkTracerProvider},
    Resource,
};
use std::sync::Arc;
use tracing::info;

pub struct TelemetryService {
    config: Arc<OpenTelemetryConfig>,
    prometheus_config: Arc<PrometheusConfig>,
}

impl TelemetryService {
    pub fn new(config: Arc<OpenTelemetryConfig>, prometheus_config: Arc<PrometheusConfig>) -> Self {
        Self {
            config,
            prometheus_config,
        }
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

    pub fn initialize(
        &self,
    ) -> Result<Option<SdkTracerProvider>, Box<dyn std::error::Error + Send + Sync>> {
        if !self.is_enabled() {
            info!("Telemetry is disabled");
            return Ok(None);
        }

        info!(
            "Initializing telemetry service: {} (trace: {}, metrics: {})",
            self.get_service_name(),
            self.is_trace_enabled(),
            self.is_metrics_enabled()
        );

        let provider = if self.config.is_trace_enabled() {
            Some(self.initialize_tracing()?)
        } else {
            None
        };

        if self.config.is_metrics_enabled() || self.prometheus_config.enabled {
            self.initialize_metrics()?;
        }

        info!("Telemetry service initialized successfully");
        Ok(provider)
    }

    fn initialize_tracing(
        &self,
    ) -> Result<SdkTracerProvider, Box<dyn std::error::Error + Send + Sync>> {
        let endpoint = self
            .config
            .otlp_endpoint
            .as_deref()
            .unwrap_or("http://localhost:4317");

        info!(
            "Initializing OTLP tracing with endpoint: {} and sampling ratio: {}",
            endpoint,
            self.get_sampling_ratio()
        );

        let resource = Resource::builder()
            .with_attributes(vec![
                KeyValue::new("service.name", self.config.service_name.clone()),
                KeyValue::new("service.version", self.config.service_version.clone()),
                KeyValue::new("service.namespace", self.config.service_namespace.clone()),
            ])
            .build();

        let exporter = opentelemetry_otlp::SpanExporter::builder()
            .with_tonic()
            .with_endpoint(endpoint)
            .build()?;

        let provider = SdkTracerProvider::builder()
            .with_batch_exporter(exporter)
            .with_sampler(Sampler::ParentBased(Box::new(Sampler::TraceIdRatioBased(
                self.get_sampling_ratio(),
            ))))
            .with_resource(resource)
            .build();

        global::set_tracer_provider(provider.clone());

        Ok(provider)
    }

    fn initialize_metrics(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("Initializing metrics collection (OTLP/Prometheus)");
        // TODO: Implement OTLP metrics initialization if needed
        Ok(())
    }

    pub fn shutdown(&self) {
        if self.is_enabled() {
            info!("Shutting down telemetry service");
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
        Self {
            config: OpenTelemetryConfig::default(),
            prometheus_config: PrometheusConfig::default(),
        }
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

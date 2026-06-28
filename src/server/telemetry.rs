use crate::common::config::Config;
use opentelemetry_sdk::trace::SdkTracerProvider as TracerProvider;
use std::sync::Arc;

/// Guard that shuts down telemetry on drop.
///
/// Holds the `TelemetryService` so that its `shutdown()` method is called
/// when the guard goes out of scope (typically at the end of `main`).
pub struct TracingGuard {
    telemetry_service: Arc<synapse_services::telemetry_service::TelemetryService>,
}

impl Drop for TracingGuard {
    fn drop(&mut self) {
        self.telemetry_service.shutdown();
    }
}

/// Initialize OpenTelemetry tracing/metrics and the global `tracing` subscriber.
///
/// Returns a [`TracingGuard`] that must be kept alive for the lifetime of the
/// process — dropping it triggers graceful telemetry shutdown.
pub fn init_telemetry(config: &Config) -> TracingGuard {
    let telemetry_service = Arc::new(synapse_services::telemetry_service::TelemetryService::new(
        Arc::new(config.telemetry.clone()),
        Arc::new(config.prometheus.clone()),
    ));

    let tracer_provider: Option<TracerProvider> = match telemetry_service.initialize() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Failed to initialize telemetry: {e}");
            None
        }
    };

    if let Err(e) = crate::common::logging::init_logging(&config.logging, tracer_provider) {
        eprintln!("Failed to initialize logging: {e}");
        std::process::exit(1);
    }

    TracingGuard { telemetry_service }
}

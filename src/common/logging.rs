use crate::common::config::LoggingConfig;
use crate::services::telemetry_service::TelemetryService;
use opentelemetry_sdk::trace::SdkTracerProvider as TracerProvider;
use std::sync::Arc;
use tracing_opentelemetry::OpenTelemetryLayer;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{fmt, EnvFilter, Registry};

/// 初始化日志与追踪系统
pub fn init_logging(
    config: &LoggingConfig,
    _telemetry_service: Option<Arc<TelemetryService>>,
    tracer_provider: Option<TracerProvider>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // 1. 设置环境过滤器
    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&config.level));

    // 2. 创建基础 Registry
    let subscriber = Registry::default().with(env_filter);

    // 3. 添加日志层 (JSON 或 Plain)
    let is_json = config.format.to_lowercase() == "json";

    if is_json {
        let fmt_layer = fmt::layer()
            .json()
            .with_target(true)
            .with_thread_ids(true)
            .with_file(true)
            .with_line_number(true)
            .with_timer(fmt::time::uptime());

        if let Some(_provider) = tracer_provider {
            let tracer = opentelemetry::global::tracer("synapse-rust");
            let otel_layer = OpenTelemetryLayer::new(tracer);
            subscriber.with(fmt_layer).with(otel_layer).init();
        } else {
            subscriber.with(fmt_layer).init();
        }
    } else {
        let fmt_layer = fmt::layer()
            .with_target(true)
            .with_thread_ids(false)
            .with_file(true)
            .with_line_number(true)
            .with_timer(fmt::time::uptime());

        if let Some(_provider) = tracer_provider {
            let tracer = opentelemetry::global::tracer("synapse-rust");
            let otel_layer = OpenTelemetryLayer::new(tracer);
            subscriber.with(fmt_layer).with(otel_layer).init();
        } else {
            subscriber.with(fmt_layer).init();
        }
    }

    tracing::info!(
        "Logging initialized: level={}, format={}",
        config.level,
        config.format
    );

    Ok(())
}

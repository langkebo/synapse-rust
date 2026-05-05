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
    // 1. 设置环境过滤器。
    //    优先级:
    //      a. RUST_LOG 显式提供 -> 完全使用其值（运维可全权覆盖）
    //      b. 否则使用 logging.level 作为根级别，并强制把 sqlx 等高频
    //         发声组件单独降到 WARN，否则 debug 模式下每条 SQL 都会被
    //         打印两次（一次 query.summary，一次 db.statement），淹没
    //         真正的业务错误。
    let env_filter = match EnvFilter::try_from_default_env() {
        Ok(filter) => filter,
        Err(_) => {
            let base = config.level.trim();
            let mut directive = base.to_string();
            // Only attach noise-suppression overrides when the operator chose
            // a verbose level globally. At INFO/WARN/ERROR the base directives
            // already keep sqlx quiet enough.
            if matches!(base.to_lowercase().as_str(), "trace" | "debug") {
                directive
                    .push_str(",sqlx::query=warn,sqlx_core=warn,hyper=info,tower_http::trace=info");
            }
            EnvFilter::new(directive)
        }
    };

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

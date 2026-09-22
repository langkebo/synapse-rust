//! Logging initialization (`init_logging`).

use crate::config::LoggingConfig;
use crate::db_query_metrics::{db_query_metrics_enabled, DbQueryMetricsLayer, SQLX_QUERY_TARGET};
use crate::tracing::RequestIdPropagationLayer;
use opentelemetry_sdk::trace::SdkTracerProvider as TracerProvider;
use tracing_opentelemetry::OpenTelemetryLayer;
use tracing_subscriber::filter::{LevelFilter, Targets};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{fmt, EnvFilter, Layer, Registry};

/// 初始化日志与追踪系统
pub fn init_logging(
    config: &LoggingConfig,
    _tracer_provider: Option<TracerProvider>,
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
                directive.push_str(",sqlx::query=warn,sqlx_core=warn,hyper=info,tower_http::trace=info");
            }
            EnvFilter::new(directive)
        }
    };

    // 2. 创建基础 Registry。
    //    过滤器改为「逐层」而非全局 —— 关键原因是 DB 查询指标：
    //
    //      * fmt 层需要把 `sqlx::query` 压到 WARN（否则 verbose 模式下每条 SQL
    //        都被打印两行，淹没真正的业务错误）；
    //      * DB 指标层（`DbQueryMetricsLayer`）需要在 DEBUG 上消费同一条事件，
    //        因为 sqlx 只在事件里给出 `elapsed_secs`，这是唯一可用的逐语句计时。
    //
    //    全局过滤器无法同时满足两者（`tracing::enabled!` 会被全局级别挡掉，
    //    连事件都不会构造）。逐层过滤下 tracing 的判据是「任意一层想要」，
    //    因此事件会被构造、只由指标层接收，fmt 层仍按 WARN 抑制，日志输出不变。
    let subscriber = Registry::default().with(RequestIdPropagationLayer);

    // 仅在启用时声明兴趣；禁用时用空 `Targets`（默认 OFF，匹配不到任何目标），
    // 这样 sqlx 连事件都不会构造，逐查询的开销归零。
    let db_metrics_filter = if db_query_metrics_enabled() {
        Targets::new().with_target(SQLX_QUERY_TARGET, LevelFilter::DEBUG)
    } else {
        Targets::new()
    };
    // Shadowing（而非重新赋值）：每次 `.with()` 都返回新的组合类型。
    let subscriber = subscriber.with(DbQueryMetricsLayer::new().with_filter(db_metrics_filter));

    // 3. 添加日志层 (JSON 或 Plain)
    let is_json = config.format.to_lowercase() == "json";
    let has_tracer = _tracer_provider.is_some();
    let tracer = opentelemetry::global::tracer("synapse-rust");

    if is_json {
        let fmt_layer = fmt::layer()
            .json()
            .with_target(true)
            .with_thread_ids(true)
            .with_file(true)
            .with_line_number(true)
            .with_timer(fmt::time::uptime())
            .with_filter(env_filter.clone());

        if has_tracer {
            subscriber.with(fmt_layer).with(OpenTelemetryLayer::new(tracer).with_filter(env_filter)).init();
        } else {
            subscriber.with(fmt_layer).init();
        }
    } else {
        let fmt_layer = fmt::layer()
            .with_target(true)
            .with_thread_ids(false)
            .with_file(true)
            .with_line_number(true)
            .with_timer(fmt::time::uptime())
            .with_filter(env_filter.clone());

        if has_tracer {
            subscriber.with(fmt_layer).with(OpenTelemetryLayer::new(tracer).with_filter(env_filter)).init();
        } else {
            subscriber.with(fmt_layer).init();
        }
    }

    tracing::info!("Logging initialized: level={}, format={}", config.level, config.format);

    Ok(())
}

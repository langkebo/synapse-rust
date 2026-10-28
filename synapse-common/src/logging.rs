//! Logging initialization (`init_logging`).
//!
//! Split into three pieces so the wiring is testable without touching the
//! process-wide dispatcher: [`fallback_directive`] (pure string policy),
//! [`build_subscriber`] (layer stack, returns a boxed subscriber) and
//! [`init_logging`] (install + one banner line). Before the split the only
//! entry point called `.init()`, whose failure mode is a **panic** on a second
//! install and which can never be exercised from a test — that is why this file
//! sat at 0% coverage while being a production entry point.

use crate::config::LoggingConfig;
use crate::db_query_metrics::{db_query_metrics_enabled, DbQueryMetricsLayer, SQLX_QUERY_TARGET};
use crate::tracing::RequestIdPropagationLayer;
use opentelemetry_sdk::trace::SdkTracerProvider as TracerProvider;
use tracing::Subscriber;
use tracing_opentelemetry::OpenTelemetryLayer;
use tracing_subscriber::filter::{LevelFilter, Targets};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::{fmt, EnvFilter, Layer, Registry};

/// The four branches below compose four *different* `Layered<…>` types; boxing
/// them is what lets `init_logging` hold a single install call.
type BoxedSubscriber = Box<dyn Subscriber + Send + Sync>;

/// Noise-suppression overrides appended to the operator's level.
///
/// Only attached for verbose levels: at INFO/WARN/ERROR the base directive
/// already keeps `sqlx` quiet enough, and forcing `sqlx::query=warn` there
/// would look like a deliberate decision to hide SQL errors when it is really
/// just redundant.
const NOISE_SUPPRESSION: &str = ",sqlx::query=warn,sqlx_core=warn,hyper=info,tower_http::trace=info";

/// Turn the configured level into a `tracing` directives string.
///
/// Kept pure (no env lookup, no `EnvFilter`) so the policy is unit-testable:
/// the caller only reaches this when `RUST_LOG` is absent.
fn fallback_directive(level: &str) -> String {
    let base = level.trim();
    let mut directive = base.to_string();
    if matches!(base.to_lowercase().as_str(), "trace" | "debug") {
        directive.push_str(NOISE_SUPPRESSION);
    }
    directive
}

/// Is `RUST_LOG` carrying an actual directive?
///
/// `RUST_LOG=` (set but empty) is accepted by `try_from_default_env` as a filter
/// with **zero directives**, and a zero-directive filter matches nothing. The
/// process then runs completely silent — including the
/// `Logging initialized: …` banner that would have said so. Measured 2026-10-28
/// on `synapse-common`'s own logging tests: unset `RUST_LOG` printed the
/// configured level, while `RUST_LOG=` printed **no line at all** (not even
/// INFO). A stray `RUST_LOG:` in a compose file / empty ConfigMap entry is
/// enough to trigger it, so empty is treated as absent.
fn rust_log_is_useable(raw: &str) -> bool {
    !raw.trim().is_empty()
}

/// Resolve the filter used by the fmt layer and the OTEL layer.
///
/// Priority:
///   a. `RUST_LOG` carries a directive -> use it verbatim (ops keeps full control);
///   b. otherwise `logging.level` as the root, with [`NOISE_SUPPRESSION`] for
///      verbose levels — else every SQL statement is printed twice
///      (`query.summary` + `db.statement`) and drowns real errors.
///
/// An unparseable `RUST_LOG` also falls back to (b): `try_new` keeps the
/// previous `try_from_default_env` semantics instead of letting
/// `EnvFilter::new`'s lossy parsing drop the bad directive and keep the rest.
fn resolve_env_filter(config: &LoggingConfig) -> EnvFilter {
    match std::env::var("RUST_LOG") {
        Ok(raw) if rust_log_is_useable(&raw) => {
            EnvFilter::try_new(raw).unwrap_or_else(|_| EnvFilter::new(fallback_directive(&config.level)))
        }
        _ => EnvFilter::new(fallback_directive(&config.level)),
    }
}

/// Build the layer stack.
///
/// Filtering is **per layer**, not global, because of the DB query metrics:
///
///   * the fmt layer needs `sqlx::query` pushed to WARN (see above);
///   * `DbQueryMetricsLayer` needs to consume the *same* event at DEBUG,
///     because sqlx only reports `elapsed_secs` in the event — the only usable
///     per-statement timing.
///
/// A global filter cannot satisfy both (`tracing::enabled!` is consulted
/// against the global level, so the event would never even be constructed).
/// Under per-layer filtering tracing asks "does *any* layer want it?", so the
/// event is built and only the metrics layer receives it while fmt still
/// suppresses the line.
fn build_subscriber(config: &LoggingConfig, has_tracer: bool) -> BoxedSubscriber {
    let env_filter = resolve_env_filter(config);

    let subscriber = Registry::default().with(RequestIdPropagationLayer);

    // Only declare interest when enabled; disabled uses an empty `Targets`
    // (everything OFF), so sqlx does not even construct the event and the
    // per-query cost is zero.
    let db_metrics_filter = if db_query_metrics_enabled() {
        Targets::new().with_target(SQLX_QUERY_TARGET, LevelFilter::DEBUG)
    } else {
        Targets::new()
    };
    // Shadowing (not reassignment): every `.with()` yields a new composite type.
    let subscriber = subscriber.with(DbQueryMetricsLayer::new().with_filter(db_metrics_filter));

    // `has_tracer` is a *presence* flag on purpose: the provider itself is
    // already installed globally by `TelemetryService::initialize`, and the
    // OTEL layer must attach to that global tracer. Only the layer's presence
    // changes here.
    let tracer = opentelemetry::global::tracer("synapse-rust");
    let is_json = config.format.to_lowercase() == "json";

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
            Box::new(subscriber.with(fmt_layer).with(OpenTelemetryLayer::new(tracer).with_filter(env_filter)))
        } else {
            Box::new(subscriber.with(fmt_layer))
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
            Box::new(subscriber.with(fmt_layer).with(OpenTelemetryLayer::new(tracer).with_filter(env_filter)))
        } else {
            Box::new(subscriber.with(fmt_layer))
        }
    }
}

/// 初始化日志与追踪系统
///
/// A second install is reported as `Err` rather than panicking: the only caller
/// (`init_telemetry`) already treats `Err` as fatal and exits 1 with the reason
/// in the log, whereas `.init()` aborted with an opaque panic message.
pub fn init_logging(
    config: &LoggingConfig,
    tracer_provider: Option<TracerProvider>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let subscriber = build_subscriber(config, tracer_provider.is_some());
    tracing::subscriber::set_global_default(subscriber)?;

    tracing::info!("Logging initialized: level={}, format={}", config.level, config.format);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(level: &str, format: &str) -> LoggingConfig {
        LoggingConfig { level: level.to_string(), format: format.to_string(), log_file: None }
    }

    /// Emit a couple of events through the real layer stack. `with_default`
    /// installs a *thread-local* dispatcher, so this never fights the global
    /// one and the fmt layer actually formats (stdout is captured by the test
    /// harness).
    ///
    /// Deliberately asserts nothing about *what* comes out: when `RUST_LOG`
    /// carries a directive the operator's value governs and the configured level
    /// is ignored by design — an assertion here would be testing the
    /// environment. The decision itself is pinned by `rust_log_is_useable` and
    /// the two `fallback_directive` tests below, which never read the
    /// environment.
    fn drive(subscriber: BoxedSubscriber) {
        tracing::subscriber::with_default(subscriber, || {
            let span = tracing::info_span!("logging_smoke");
            let _entered = span.enter();
            tracing::info!(answer = 42, "smoke");
            tracing::debug!("second event, in case the level allows it");
        });
    }

    #[test]
    fn verbose_levels_add_noise_suppression() {
        for level in ["trace", "debug", "DEBUG", " Debug ", "TRACE"] {
            let directive = fallback_directive(level);
            assert!(directive.ends_with(NOISE_SUPPRESSION), "{level:?} should suppress sqlx noise, got {directive:?}");
            assert!(
                directive.to_lowercase().starts_with(level.trim().to_lowercase().as_str()),
                "{level:?} should keep the operator's root level, got {directive:?}"
            );
        }
    }

    #[test]
    fn quiet_levels_are_used_verbatim() {
        for level in ["info", "WARN", "error", " Info "] {
            let directive = fallback_directive(level);
            assert_eq!(directive, level.trim());
            assert!(!directive.contains("sqlx"), "no override belongs at {level:?}, got {directive:?}");
        }
    }

    #[test]
    fn empty_rust_log_is_treated_as_absent() {
        // The regression this guards: `RUST_LOG=` yields a zero-directive
        // filter, and a zero-directive filter matches nothing — the process goes
        // silent with no line explaining why.
        for raw in ["", "   ", "\t\n"] {
            assert!(!rust_log_is_useable(raw), "{raw:?} must not be honoured as a directive");
        }
        for raw in ["info", " debug ", "sqlx=debug,synapse=info"] {
            assert!(rust_log_is_useable(raw), "{raw:?} must be honoured");
        }
    }

    #[test]
    fn every_format_and_tracer_combination_can_be_built() {
        // 2 formats x tracer presence = the four branches of `build_subscriber`.
        for format in ["json", "JSON", "plain", "console"] {
            drive(build_subscriber(&config("info", format), false));
            drive(build_subscriber(&config("debug", format), true));
        }
    }

    #[test]
    fn a_second_install_is_reported_instead_of_panicking() {
        // Whoever wins the race, the *second* install must surface as `Err`:
        // either our seed below takes the slot and `init_logging` is the second
        // caller, or the slot was already taken and both fail. No branch panics.
        if let Err(error) = tracing::subscriber::set_global_default(Registry::default()) {
            assert!(
                error.to_string().contains("already been set"),
                "a competing install must fail the documented way, got: {error}"
            );
        }

        let err = init_logging(&config("info", "plain"), None).expect_err("a global subscriber is already installed");

        assert!(err.to_string().contains("already been set"), "unexpected error: {err}");
    }
}

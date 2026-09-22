//! `sqlx::query` tracing layer → `db_query_duration_ms`.
//!
//! sqlx 0.8 emits exactly one `tracing` event per executed statement, from
//! `sqlx_core::logger::QueryLogger::finish`, with a `elapsed_secs: f64` field.
//! That is the **only** per-statement timing hook sqlx exposes — and it is
//! emitted for every query without any call-site change, which matters because
//! this crate's storage layer has ~1700 direct `sqlx::query(..)` call sites and
//! no central query facade to wrap.
//!
//! Two consequences worth knowing:
//!
//! * sqlx reports a **duration but no success flag**, so failures cannot be read
//!   off this event. They are counted instead at the `From<sqlx::Error> for
//!   ApiError` boundary (see `error.rs`), which is the single point every
//!   storage error funnels through.
//! * The event is only *constructed* when some subscriber declares interest in
//!   target `sqlx::query` at the emitted level. Two levels are involved:
//!   `statements_level` (default `Debug`) for normal statements and
//!   `slow_statements_level` (default `Warn`) for statements slower than
//!   `slow_statements_duration` (default 1s). The pool is therefore left at its
//!   defaults and only the subscriber side is configured — see `logging.rs`,
//!   which grants this layer target-scoped interest while keeping the fmt layer's
//!   `sqlx::query=warn` suppression so the SQL text is not printed per query.
//!   Because interest is declared, sqlx does build the event per query
//!   (including a short SQL summary string). That cost is small but non-zero;
//!   set `SYNAPSE_DB_QUERY_METRICS_DISABLED=1` to opt out entirely.

use std::sync::{Arc, OnceLock};

use tracing::field::{Field, Visit};
use tracing::{Event, Subscriber};
use tracing_subscriber::layer::Context as LayerContext;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::Layer;

use crate::server_metrics::{global_server_metrics, ServerMetrics};

/// `tracing` target sqlx uses for per-statement events, on both the normal and
/// the slow-statement path.
pub const SQLX_QUERY_TARGET: &str = "sqlx::query";

/// Field carrying the statement duration in seconds (set by sqlx as
/// `elapsed_secs = elapsed.as_secs_f64()`).
const ELAPSED_SECS_FIELD: &str = "elapsed_secs";

/// Set to `1`/`true` to disable DB query duration collection entirely.
const ENV_DISABLE: &str = "SYNAPSE_DB_QUERY_METRICS_DISABLED";

/// Whether DB query duration collection is enabled (env override, default on).
///
/// Latched once: reading the environment on every query would be a per-statement
/// syscall, and the value must stay stable across the process lifetime so that
/// the subscriber's declared interest cannot change under sqlx's feet.
pub fn db_query_metrics_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| !matches!(std::env::var(ENV_DISABLE).as_deref(), Ok("1") | Ok("true")))
}

/// Feeds `db_query_duration_ms` from sqlx's per-statement events.
///
/// Attach to the subscriber *with* a target-scoped filter (`sqlx::query=debug`)
/// so that the statement events are produced, while the fmt layer keeps its own
/// `sqlx::query=warn` filter and does not print a line per query.
#[derive(Default)]
pub struct DbQueryMetricsLayer {
    /// Explicit handle, injected by tests. Production uses `None` and resolves
    /// the process-wide handle.
    ///
    /// Injectability is not decoration: `install_global_server_metrics` is
    /// first-write-wins, so a test that relies on the global would silently
    /// observe a *different* test's collector and its assertions would be
    /// vacuously true — a test that cannot fail.
    metrics: Option<Arc<ServerMetrics>>,
}

impl DbQueryMetricsLayer {
    /// Constructs a layer that resolves the process-wide [`ServerMetrics`] handle.
    pub fn new() -> Self {
        Self::default()
    }

    /// Constructs a layer bound to an explicit handle (tests).
    #[cfg(test)]
    fn with_metrics(metrics: Arc<ServerMetrics>) -> Self {
        Self { metrics: Some(metrics) }
    }

    fn metrics(&self) -> Option<Arc<ServerMetrics>> {
        match &self.metrics {
            Some(metrics) => Some(metrics.clone()),
            // Optional in production: absent before startup wiring, and in most
            // test binaries.
            None => global_server_metrics().cloned(),
        }
    }
}

impl<S> Layer<S> for DbQueryMetricsLayer
where
    S: Subscriber + for<'lookup> LookupSpan<'lookup>,
{
    fn on_event(&self, event: &Event<'_>, _ctx: LayerContext<'_, S>) {
        if event.metadata().target() != SQLX_QUERY_TARGET {
            return;
        }
        let Some(metrics) = self.metrics() else {
            return;
        };

        let mut visitor = ElapsedSecsVisitor { elapsed_secs: None };
        event.record(&mut visitor);

        let Some(elapsed_secs) = visitor.elapsed_secs else {
            return;
        };
        // Guard the histogram against non-finite input: a NaN would be folded
        // into the `+Inf` bucket by the renderer, silently poisoning quantiles.
        if elapsed_secs.is_finite() && elapsed_secs >= 0.0 {
            metrics.observe_db_query_duration(elapsed_secs * 1000.0);
        }
    }
}

/// Extracts only the numeric `elapsed_secs` field, ignoring the rest of the
/// payload (notably `db.statement`, which we do not want to allocate/copy).
#[derive(Default)]
struct ElapsedSecsVisitor {
    elapsed_secs: Option<f64>,
}

impl Visit for ElapsedSecsVisitor {
    fn record_f64(&mut self, field: &Field, value: f64) {
        if field.name() == ELAPSED_SECS_FIELD {
            self.elapsed_secs = Some(value);
        }
    }

    fn record_debug(&mut self, _field: &Field, _value: &dyn std::fmt::Debug) {
        // `?elapsed` (a `std::time::Duration`) lands here; `elapsed_secs` is the
        // authoritative numeric field, so Debug values are deliberately ignored.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::MetricsCollector;
    use tracing_subscriber::layer::SubscriberExt;

    fn metrics() -> (Arc<ServerMetrics>, DbQueryMetricsLayer) {
        let collected = Arc::new(ServerMetrics::new(Arc::new(MetricsCollector::new())));
        (collected.clone(), DbQueryMetricsLayer::with_metrics(collected))
    }

    /// Drives one synthetic sqlx event through `layer`.
    fn with_layer<F: FnOnce()>(layer: DbQueryMetricsLayer, f: F) {
        let subscriber = tracing_subscriber::registry().with(layer);
        tracing::subscriber::with_default(subscriber, f);
    }

    #[test]
    fn records_duration_from_sqlx_query_event() {
        let (collected, layer) = metrics();
        assert_eq!(collected.db_query_duration.get_count(), 0);

        with_layer(layer, || {
            tracing::debug!(target: SQLX_QUERY_TARGET, elapsed_secs = 0.012, summary = "SELECT 1", "ok");
        });

        assert_eq!(collected.db_query_duration.get_count(), 1, "exactly one observation per statement event");
        let sum_ms = collected.db_query_duration.get_sum();
        assert!((sum_ms - 12.0).abs() < 1e-9, "seconds -> milliseconds (got {sum_ms})");
    }

    #[test]
    fn ignores_events_from_other_targets() {
        let (collected, layer) = metrics();

        with_layer(layer, || {
            tracing::debug!(target: "synapse::other", elapsed_secs = 0.5, "not a db event");
        });

        assert_eq!(collected.db_query_duration.get_count(), 0);
    }

    #[test]
    fn ignores_non_finite_durations() {
        let (collected, layer) = metrics();

        with_layer(layer, || {
            tracing::debug!(target: SQLX_QUERY_TARGET, elapsed_secs = f64::NAN, "bad");
            tracing::debug!(target: SQLX_QUERY_TARGET, elapsed_secs = f64::INFINITY, "bad");
            tracing::debug!(target: SQLX_QUERY_TARGET, elapsed_secs = -1.0, "bad");
        });

        assert_eq!(collected.db_query_duration.get_count(), 0, "non-finite/negative durations must be dropped");
    }

    #[test]
    fn ignores_events_without_numeric_elapsed_secs() {
        let (collected, layer) = metrics();

        with_layer(layer, || {
            // Only the Debug-formatted `elapsed` duration: no numeric field to read.
            tracing::debug!(
                target: SQLX_QUERY_TARGET,
                elapsed = ?std::time::Duration::from_millis(5),
                "duration only in Debug form"
            );
        });

        assert_eq!(collected.db_query_duration.get_count(), 0);
    }
}

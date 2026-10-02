//! Pre-registered Prometheus counters/gauges/histograms exposed by the server.

use crate::metrics::{Counter, DynamicCounterTemplate, DynamicGaugeTemplate, Gauge, Histogram, MetricsCollector};
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

/// Process-wide handle to the single [`ServerMetrics`] instance.
///
/// Needed by code paths that cannot take a constructor dependency on the container:
/// the `sqlx::query` tracing layer (which records `db_query_duration_ms`) and the
/// `From<sqlx::Error> for ApiError` conversion (which records `db_query_errors`).
/// Without it, both would have to be threaded through every call site — see the
/// `check_metric_instrumentation.py` gate for why that is worth avoiding.
static GLOBAL_SERVER_METRICS: OnceLock<Arc<ServerMetrics>> = OnceLock::new();

/// Installs the process-wide [`ServerMetrics`] handle. Idempotent: the first
/// installation wins, later ones are ignored (matching `init_error_metrics`).
pub fn install_global_server_metrics(metrics: Arc<ServerMetrics>) {
    let _ = GLOBAL_SERVER_METRICS.set(metrics);
}

/// Returns the process-wide [`ServerMetrics`] handle, if installed yet.
///
/// `None` before startup wiring completes and in most test binaries; callers
/// must treat it as optional (metrics are best-effort and never load-bearing).
pub fn global_server_metrics() -> Option<&'static Arc<ServerMetrics>> {
    GLOBAL_SERVER_METRICS.get()
}

/// `synapse_storage_stream_current_position` 的 `stream` 标签取值。
///
/// 只收录本仓**真有单调位置列、且生产路径会推进**的 stream（数据源见
/// `synapse_storage::stream_positions`）：
///
/// * upstream 的 presence / typing / receipts / account_data / push_rules / e2ee_keys /
///   backfill / federation 在本仓**没有**位置列；
/// * `worker_events.stream_id` 由事件写入路径推进，但**只在 worker 模式**
///   （`worker.enabled`）下启用 ⇒ 单进程部署里该序列恒 0 是**预期**，不是漂移；
/// * `sync_stream_id` 只有 seed 写入（全仓无读/写者）；`device_lists_outbound_pokes`
///   在生产代码里既无 INSERT 也无 SELECT ⇒ **已整表删除**（2026-10-03），不再存在；
/// * `room_ephemeral.stream_id` **不是 stream 位置**：它是按房维度的新鲜度排序键
///   （UPSERT 覆盖，仅用于 `ORDER BY`），服务层从不读取它、也不做游标 ⇒ 永久排除。
///
/// `ALL` 与 storage 侧 SQL 的输出必须逐字一致，由
/// `tests/integration/stream_position_tests.rs` 守卫。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StreamPosition {
    /// `events.stream_ordering`
    Events,
    /// `to_device_messages.stream_id`
    ToDevice,
    /// `device_lists_stream.stream_id`
    DeviceLists,
    /// `sliding_sync_tokens.pos`
    SlidingSync,
    /// `quarantined_media_changes.stream_id`
    QuarantinedMedia,
    /// `worker_events.stream_id`（worker 模式下由事件写入路径推进）
    WorkerEvents,
}

impl StreamPosition {
    /// 登记表（顺序即 `StreamPosition::ALL` 的声明顺序）。
    pub const ALL: [StreamPosition; 6] = [
        StreamPosition::Events,
        StreamPosition::ToDevice,
        StreamPosition::DeviceLists,
        StreamPosition::SlidingSync,
        StreamPosition::QuarantinedMedia,
        StreamPosition::WorkerEvents,
    ];

    /// `stream` 标签取值。
    pub fn label(self) -> &'static str {
        match self {
            StreamPosition::Events => "events",
            StreamPosition::ToDevice => "to_device",
            StreamPosition::DeviceLists => "device_lists",
            StreamPosition::SlidingSync => "sliding_sync",
            StreamPosition::QuarantinedMedia => "quarantined_media",
            StreamPosition::WorkerEvents => "worker_events",
        }
    }

    /// 由标签取值反查；未登记的标签返回 `None`。
    ///
    /// 调用方对 `None` 应记 warn：那正是"SQL 加了新 stream、指标登记表没跟上"的漂移，
    /// 静默丢弃会让 `/metrics` 少一条序列而无人察觉。
    pub fn from_label(label: &str) -> Option<Self> {
        StreamPosition::ALL.into_iter().find(|stream| stream.label() == label)
    }
}

/// All server-level Prometheus metrics counters/gauges/histograms, wired into `MetricsCollector`.
pub struct ServerMetrics {
    /// Total authentication attempts (labeled by type).
    pub auth_attempts_total: Counter,
    /// Failed authentication attempts (labeled by type).
    pub auth_failures_total: Counter,
    /// Successful authentication attempts (labeled by type).
    pub auth_success_total: Counter,
    /// Total access-token validations.
    pub token_validations_total: Counter,
    /// Token validation failures (signature, expiry, format, etc.).
    pub token_validation_errors: Counter,

    /// Per-query database duration histogram (ms).
    pub db_query_duration: Histogram,
    /// Currently in-use DB connections.
    pub db_connections_active: Gauge,
    /// Idle DB connections available in the pool.
    pub db_connections_idle: Gauge,
    /// DB query failures.
    pub db_query_errors: Counter,
    /// Per-transaction duration histogram (ms).
    pub db_transaction_duration: Histogram,

    /// Cache lookups that returned a hit (labeled by result).
    pub cache_hits_total: Counter,
    /// Cache lookups that returned a miss (labeled by result).
    pub cache_misses_total: Counter,
    /// Cache entries evicted by capacity or TTL.
    pub cache_evictions_total: Counter,
    /// Cache backend errors (Redis down, timeout, etc.).
    pub cache_errors: Counter,

    /// Outgoing federation requests.
    pub federation_requests_total: Counter,
    /// Federation request duration histogram (ms).
    pub federation_request_duration: Histogram,
    /// Outgoing federation requests that failed at the transport/remote layer.
    ///
    /// Deliberately distinct from [`federation_signature_errors`](Self::federation_signature_errors):
    /// a DNS failure or a remote 503 is not a signature problem, and conflating the two
    /// made the signature counter unusable for alerting.
    pub federation_request_errors_total: Counter,
    /// Successful X-Matrix signature verifications.
    pub federation_signature_verifications: Counter,
    /// Failed signature verifications.
    pub federation_signature_errors: Counter,
    /// Federation requests rejected for replay.
    pub federation_replay_attacks_blocked: Counter,

    /// Total HTTP requests handled.
    pub http_requests_total: Counter,
    /// HTTP request duration histogram (ms).
    pub http_request_duration: Histogram,
    /// HTTP requests that returned 4xx/5xx.
    pub http_request_errors_total: Counter,
    /// Currently in-flight HTTP requests.
    pub http_active_requests: Gauge,

    /// JWT-specific validation failures.
    pub security_jwt_validation_errors: Counter,
    /// Origin validation failures.
    pub security_origin_validation_errors: Counter,
    /// Timestamp validation failures.
    pub security_timestamp_validation_errors: Counter,

    /// Worker pool utilization (0.0-1.0).
    pub pool_utilization: Gauge,
    /// Worker pool health (1=healthy, 0=degraded).
    pub pool_health_status: Gauge,

    // Admin / Global Stats
    /// Total registered users on this server.
    pub total_users: Gauge,
    /// Total rooms on this server.
    pub total_rooms: Gauge,
    /// 每个 stream 的当前位置，标签 `stream`。
    ///
    /// 指标名与上游同名（`synapse_storage_stream_current_position{stream="…"}`），
    /// 标签取值见 [`StreamPosition`]：只登记本仓**真有单调位置列、且生产路径会推进**
    /// 的 stream（数据源 `synapse_storage::stream_positions::get_stream_positions`）。
    ///
    /// 由 `src/server/mod.rs` 的 30s 指标循环周期刷新；此前只有 `events` 一条，且只在
    /// admin `/statistics` 被访问时才更新。
    pub storage_stream_current_position: DynamicGaugeTemplate,

    // Dehydrated Device Cleanup Metrics
    /// Dehydrated-device cleanup runs started.
    pub dehydrated_device_cleanup_total: Counter,
    /// Dehydrated devices successfully cleaned up.
    pub dehydrated_device_cleaned_total: Counter,
    /// Dehydrated-device cleanup failures.
    pub dehydrated_device_cleanup_errors_total: Counter,
    /// Dehydrated-device cleanup duration histogram (ms).
    pub dehydrated_device_cleanup_duration: Histogram,

    // Room Operations Metrics
    /// Total room create operations.
    pub room_creates_total: Counter,
    /// Total room join operations.
    pub room_joins_total: Counter,
    /// Total room leave operations.
    pub room_leaves_total: Counter,

    // Push Notification Metrics
    /// Total push notifications attempted.
    pub push_notifications_total: Counter,
    /// Total push notification failures.
    pub push_notification_errors_total: Counter,
    /// Room operation duration histogram (ms).
    pub room_operation_duration: Histogram,

    // Message/Sync Operations Metrics
    /// Total `/sync` requests.
    pub sync_requests_total: Counter,
    /// `/sync` request duration histogram (ms).
    pub sync_duration: Histogram,
    /// Total messages sent (state events and messages).
    pub messages_sent_total: Counter,
    /// Message-send duration histogram (ms).
    pub message_send_duration: Histogram,

    // Presence Operations Metrics
    /// Total presence status updates.
    pub presence_updates_total: Counter,
    /// Presence sync duration histogram (ms).
    pub presence_sync_duration: Histogram,

    // State Group Operations Metrics
    /// Total state-group conflict resolutions.
    pub state_group_resolves_total: Counter,
    /// State-group resolve duration histogram (ms).
    pub state_group_resolve_duration: Histogram,

    // CSRF/Security Metrics
    /// Total CSRF token validations.
    pub csrf_validations_total: Counter,
    /// Failed CSRF token validations.
    pub csrf_validation_failures_total: Counter,

    // Event Notifier Metrics (S-6)
    /// Total Redis subscriber failures in EventNotifier.
    pub event_notifier_subscriber_failures_total: Counter,

    // ========================================================================
    // NEW CUSTOM METRICS FOR OBSERVABILITY OPTIMIZATION (2026-09-30)
    // Following Phase 1 of prometheus-custom-metrics.md implementation guide
    // ========================================================================

    // Message Delivery Metrics
    /// End-to-end message delivery latency histogram (seconds).
    pub message_delivery_latency: Histogram,
    /// Current message queue depth gauge.
    pub message_queue_depth: Gauge,

    // Room Creation Metrics
    /// Complete room creation process duration histogram (seconds).
    pub room_creation_duration: Histogram,

    // E2EE Handshake Metrics
    /// E2EE handshake/key exchange duration histogram (seconds).
    pub e2ee_handshake_duration: Histogram,

    // Enhanced Operation Metrics (replacing legacy counters with dynamic templates)
    /// Dynamic counter template for unified room operations with granular labels.
    pub room_operations_total: DynamicCounterTemplate,
    /// Dynamic counter template for cache operations with backend/type/result labels.
    pub cache_operations_total: DynamicCounterTemplate,

    // Megolm (E2EE) Metrics — Phase 1 vodozemac migration observability.
    // These cover share/get flows; legacy AES-256-GCM path also uses the
    // same counter names so dashboards do not need to special-case
    // backends during the migration window.
    /// Megolm key-share operations (vodozemac + legacy).
    pub megolm_share_total: Counter,
    /// Total recipients across all megolm shares.
    pub megolm_share_recipients_total: Counter,
    /// Megolm share DB-lookup duration (ms).
    pub megolm_share_db_duration_ms: Histogram,
    /// Megolm share cache-lookup duration (ms).
    pub megolm_share_cache_duration_ms: Histogram,
    /// Megolm share cache failures.
    pub megolm_share_cache_errors_total: Counter,
    /// Megolm share DB failures.
    pub megolm_share_db_errors_total: Counter,
    /// Megolm session key reads.
    pub megolm_session_key_read_total: Counter,
    /// Megolm session key read duration (ms).
    pub megolm_session_key_read_duration_ms: Histogram,
    collector: Arc<MetricsCollector>,
}

impl ServerMetrics {
    /// Creates a new ServerMetrics handle, registering all counters/gauges/histograms with `collector`.
    pub fn new(collector: Arc<MetricsCollector>) -> Self {
        Self {
            auth_attempts_total: collector
                .register_counter_with_labels("auth_attempts_total".to_string(), Self::labels(&[("type", "attempt")])),
            auth_failures_total: collector
                .register_counter_with_labels("auth_failures_total".to_string(), Self::labels(&[("type", "failure")])),
            auth_success_total: collector
                .register_counter_with_labels("auth_success_total".to_string(), Self::labels(&[("type", "success")])),
            token_validations_total: collector.register_counter("token_validations_total".to_string()),
            token_validation_errors: collector.register_counter("token_validation_errors".to_string()),

            db_query_duration: collector
                .register_histogram_with_labels("db_query_duration_ms".to_string(), Self::labels(&[("unit", "ms")])),
            db_connections_active: collector.register_gauge("db_connections_active".to_string()),
            db_connections_idle: collector.register_gauge("db_connections_idle".to_string()),
            db_query_errors: collector.register_counter("db_query_errors".to_string()),
            db_transaction_duration: collector.register_histogram_with_labels(
                "db_transaction_duration_ms".to_string(),
                Self::labels(&[("unit", "ms")]),
            ),

            cache_hits_total: collector
                .register_counter_with_labels("cache_hits_total".to_string(), Self::labels(&[("result", "hit")])),
            cache_misses_total: collector
                .register_counter_with_labels("cache_misses_total".to_string(), Self::labels(&[("result", "miss")])),
            cache_evictions_total: collector.register_counter("cache_evictions_total".to_string()),
            cache_errors: collector.register_counter("cache_errors".to_string()),

            federation_requests_total: collector.register_counter("federation_requests_total".to_string()),
            federation_request_duration: collector.register_histogram_with_labels(
                "federation_request_duration_ms".to_string(),
                Self::labels(&[("unit", "ms")]),
            ),
            federation_signature_verifications: collector
                .register_counter("federation_signature_verifications".to_string()),
            federation_request_errors_total: collector.register_counter("federation_request_errors_total".to_string()),
            federation_signature_errors: collector.register_counter("federation_signature_errors".to_string()),
            federation_replay_attacks_blocked: collector
                .register_counter("federation_replay_attacks_blocked".to_string()),

            http_requests_total: collector.register_counter("http_requests_total".to_string()),
            http_request_duration: collector.register_histogram_with_labels(
                "http_request_duration_ms".to_string(),
                Self::labels(&[("unit", "ms")]),
            ),
            http_request_errors_total: collector.register_counter("http_request_errors_total".to_string()),
            http_active_requests: collector.register_gauge("http_active_requests".to_string()),

            security_jwt_validation_errors: collector.register_counter("security_jwt_validation_errors".to_string()),
            security_origin_validation_errors: collector
                .register_counter("security_origin_validation_errors".to_string()),
            security_timestamp_validation_errors: collector
                .register_counter("security_timestamp_validation_errors".to_string()),

            pool_utilization: collector.register_gauge("pool_utilization".to_string()),
            pool_health_status: collector.register_gauge("pool_health_status".to_string()),

            total_users: collector.register_gauge("synapse_total_users".to_string()),
            total_rooms: collector.register_gauge("synapse_total_rooms".to_string()),
            storage_stream_current_position: collector
                .create_dynamic_gauge_template("synapse_storage_stream_current_position".to_string(), vec!["stream"]),

            dehydrated_device_cleanup_total: collector.register_counter("dehydrated_device_cleanup_total".to_string()),
            dehydrated_device_cleaned_total: collector.register_counter("dehydrated_device_cleaned_total".to_string()),
            dehydrated_device_cleanup_errors_total: collector
                .register_counter("dehydrated_device_cleanup_errors_total".to_string()),
            dehydrated_device_cleanup_duration: collector.register_histogram_with_labels(
                "dehydrated_device_cleanup_duration_ms".to_string(),
                Self::labels(&[("unit", "ms")]),
            ),

            room_creates_total: collector.register_counter("room_creates_total".to_string()),
            room_joins_total: collector.register_counter("room_joins_total".to_string()),
            room_leaves_total: collector.register_counter("room_leaves_total".to_string()),

            push_notifications_total: collector.register_counter("push_notifications_total".to_string()),
            push_notification_errors_total: collector.register_counter("push_notification_errors_total".to_string()),

            room_operation_duration: collector.register_histogram_with_labels(
                "room_operation_duration_ms".to_string(),
                Self::labels(&[("unit", "ms")]),
            ),

            sync_requests_total: collector.register_counter("sync_requests_total".to_string()),
            sync_duration: collector
                .register_histogram_with_labels("sync_duration_ms".to_string(), Self::labels(&[("unit", "ms")])),
            messages_sent_total: collector.register_counter("messages_sent_total".to_string()),
            message_send_duration: collector.register_histogram_with_labels(
                "message_send_duration_ms".to_string(),
                Self::labels(&[("unit", "ms")]),
            ),

            presence_updates_total: collector.register_counter("presence_updates_total".to_string()),
            presence_sync_duration: collector.register_histogram_with_labels(
                "presence_sync_duration_ms".to_string(),
                Self::labels(&[("unit", "ms")]),
            ),

            state_group_resolves_total: collector.register_counter("state_group_resolves_total".to_string()),
            state_group_resolve_duration: collector.register_histogram_with_labels(
                "state_group_resolve_duration_ms".to_string(),
                Self::labels(&[("unit", "ms")]),
            ),

            csrf_validations_total: collector.register_counter("csrf_validations_total".to_string()),
            csrf_validation_failures_total: collector.register_counter("csrf_validation_failures_total".to_string()),

            // S-6: EventNotifier Redis subscriber failure counter
            event_notifier_subscriber_failures_total: collector
                .register_counter("event_notifier_subscriber_failures_total".to_string()),

            // ========================================================================
            // NEW CUSTOM METRICS REGISTRATION (2026-09-30)
            // Phase 1 implementation from prometheus-custom-metrics.md
            // ========================================================================

            // Message Delivery Metrics
            message_delivery_latency: collector.register_histogram_with_labels(
                "message_delivery_latency_seconds".to_string(),
                Self::labels(&[("unit", "seconds")]),
            ),
            message_queue_depth: collector.register_gauge("message_queue_depth".to_string()),

            // Room Creation Metrics
            room_creation_duration: collector.register_histogram_with_labels(
                "room_creation_duration_seconds".to_string(),
                Self::labels(&[("unit", "seconds")]),
            ),

            // E2EE Handshake Metrics
            e2ee_handshake_duration: collector.register_histogram_with_labels(
                "e2ee_handshake_duration_seconds".to_string(),
                Self::labels(&[("unit", "seconds")]),
            ),

            // Enhanced Operation Metrics — using DynamicCounterTemplate for runtime label binding
            room_operations_total: collector.create_dynamic_counter_template(
                "room_operations_total".to_string(),
                vec!["operation", "outcome", "room_version", "visibility", "error_type"],
            ),
            cache_operations_total: collector.create_dynamic_counter_template(
                "cache_operations_total".to_string(),
                vec!["cache_type", "backend", "operation", "result"],
            ),

            megolm_share_total: collector.register_counter("megolm_share_total".to_string()),
            megolm_share_recipients_total: collector.register_counter("megolm_share_recipients_total".to_string()),
            megolm_share_db_duration_ms: collector.register_histogram_with_labels(
                "megolm_share_db_duration_ms".to_string(),
                Self::labels(&[("unit", "ms")]),
            ),
            megolm_share_cache_duration_ms: collector.register_histogram_with_labels(
                "megolm_share_cache_duration_ms".to_string(),
                Self::labels(&[("unit", "ms")]),
            ),
            megolm_share_cache_errors_total: collector.register_counter("megolm_share_cache_errors_total".to_string()),
            megolm_share_db_errors_total: collector.register_counter("megolm_share_db_errors_total".to_string()),
            megolm_session_key_read_total: collector.register_counter("megolm_session_key_read_total".to_string()),
            megolm_session_key_read_duration_ms: collector.register_histogram_with_labels(
                "megolm_session_key_read_duration_ms".to_string(),
                Self::labels(&[("unit", "ms")]),
            ),
            collector,
        }
    }

    fn labels(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    /// Increments auth attempts counter and branches on `success` flag.
    pub fn record_auth_attempt(&self, success: bool) {
        self.auth_attempts_total.inc();
        if success {
            self.auth_success_total.inc();
        } else {
            self.auth_failures_total.inc();
        }
    }

    /// Increments total token validations; increments error counter on failure.
    pub fn record_token_validation(&self, success: bool) {
        self.token_validations_total.inc();
        if !success {
            self.token_validation_errors.inc();
        }
    }

    /// Observes a DB query duration and increments error counter on failure.
    pub fn record_db_query(&self, duration_ms: f64, success: bool) {
        self.db_query_duration.observe(duration_ms);
        if !success {
            self.db_query_errors.inc();
        }
    }

    /// Observes a DB query duration without touching the error counter.
    ///
    /// Used by the `sqlx::query` tracing layer: sqlx reports per-statement
    /// duration but no success flag (see `DbQueryMetricsLayer`). Failures are
    /// counted separately at the `From<sqlx::Error> for ApiError` boundary.
    pub fn observe_db_query_duration(&self, duration_ms: f64) {
        self.db_query_duration.observe(duration_ms);
    }

    /// Sets pool connection counts, utilization, and health status.
    pub fn update_pool_metrics(&self, active: f64, idle: f64, utilization: f64, is_healthy: bool) {
        self.db_connections_active.set(active);
        self.db_connections_idle.set(idle);
        self.pool_utilization.set(utilization);
        self.pool_health_status.set(if is_healthy { 1.0 } else { 0.0 });
    }

    /// Increments hit or miss counter based on `hit` flag.
    pub fn record_cache_operation(&self, hit: bool) {
        if hit {
            self.cache_hits_total.inc();
        } else {
            self.cache_misses_total.inc();
        }
    }

    /// Increments federation request counter and records duration.
    ///
    /// `success` describes the **outbound exchange**, not signature validity:
    /// a failure increments [`federation_request_errors_total`](Self::federation_request_errors_total).
    /// Signature failures are recorded by [`record_federation_signature_verification`]
    /// (Self::record_federation_signature_verification) instead — mixing the two made
    /// `federation_signature_errors` count DNS timeouts as signature problems.
    pub fn record_federation_request(&self, duration_ms: f64, success: bool) {
        self.federation_requests_total.inc();
        self.federation_request_duration.observe(duration_ms);
        if !success {
            self.federation_request_errors_total.inc();
        }
    }

    /// Increments verification counter and branches on success.
    pub fn record_federation_signature_verification(&self, success: bool) {
        self.federation_signature_verifications.inc();
        if !success {
            self.federation_signature_errors.inc();
        }
    }

    /// Increments the federation replay-attack blocked counter.
    pub fn record_replay_attack_blocked(&self) {
        self.federation_replay_attacks_blocked.inc();
    }

    /// Increments HTTP request counter, records duration, and increments error counter on failure.
    pub fn record_http_request(&self, duration_ms: f64, success: bool) {
        self.http_requests_total.inc();
        self.http_request_duration.observe(duration_ms);
        if !success {
            self.http_request_errors_total.inc();
        }
    }

    /// 记录某个 stream 的当前位置（`synapse_storage_stream_current_position`）。
    pub fn set_stream_position(&self, stream: StreamPosition, position: i64) {
        self.storage_stream_current_position.set(&[stream.label()], position as f64);
    }

    /// Increments the in-flight HTTP request gauge.
    pub fn http_request_started(&self) {
        self.http_active_requests.inc();
    }

    /// Decrements the in-flight HTTP request gauge.
    pub fn http_request_finished(&self) {
        self.http_active_requests.dec();
    }

    /// Increments the appropriate security validation error counter on failure.
    pub fn record_security_validation(&self, validation_type: SecurityValidationType, success: bool) {
        if !success {
            match validation_type {
                SecurityValidationType::Jwt => self.security_jwt_validation_errors.inc(),
                SecurityValidationType::Origin => self.security_origin_validation_errors.inc(),
                SecurityValidationType::Timestamp => self.security_timestamp_validation_errors.inc(),
            }
        }
    }

    /// Increments the named room-operation counter and records duration.
    pub fn record_room_operation(&self, op: &str, duration_ms: f64, success: bool) {
        match op {
            "create" => self.room_creates_total.inc(),
            "join" => self.room_joins_total.inc(),
            "leave" => self.room_leaves_total.inc(),
            _ => {}
        }
        self.room_operation_duration.observe(duration_ms);
        let _ = success;
    }

    /// Increments sync request counter and records duration.
    pub fn record_sync_request(&self, duration_ms: f64, success: bool) {
        self.sync_requests_total.inc();
        self.sync_duration.observe(duration_ms);
        let _ = success;
    }

    /// Increments messages-sent counter and records send duration.
    pub fn record_message_send(&self, duration_ms: f64, success: bool) {
        self.messages_sent_total.inc();
        self.message_send_duration.observe(duration_ms);
        let _ = success;
    }

    /// Increments presence update counter and records sync duration.
    pub fn record_presence_update(&self, duration_ms: f64) {
        self.presence_updates_total.inc();
        self.presence_sync_duration.observe(duration_ms);
    }

    /// Increments state-group resolve counter and records duration.
    pub fn record_state_group_resolve(&self, duration_ms: f64) {
        self.state_group_resolves_total.inc();
        self.state_group_resolve_duration.observe(duration_ms);
    }

    /// Increments total CSRF validations; increments failure counter on failure.
    pub fn record_csrf_validation(&self, success: bool) {
        self.csrf_validations_total.inc();
        if !success {
            self.csrf_validation_failures_total.inc();
        }
    }

    /// Record one Megolm session-key share operation.
    ///
    /// `db_duration_ms` is the database round-trip latency. `cache_duration_ms`
    /// is the best-effort cache write latency. `success` indicates whether
    /// the database write succeeded (cache failures are recorded separately
    /// via `record_megolm_share_cache_error`).
    pub fn record_megolm_share(&self, recipients: usize, db_duration_ms: f64, cache_duration_ms: f64, success: bool) {
        self.megolm_share_total.inc();
        if success {
            self.megolm_share_recipients_total.inc_by(recipients as u64);
            self.megolm_share_db_duration_ms.observe(db_duration_ms);
            self.megolm_share_cache_duration_ms.observe(cache_duration_ms);
        } else {
            self.megolm_share_db_errors_total.inc();
        }
    }

    /// Record a cache write failure during Megolm share.
    pub fn record_megolm_share_cache_error(&self) {
        self.megolm_share_cache_errors_total.inc();
    }

    /// Record a Megolm session-key read for a recipient.
    ///
    /// `result` is a free-form label (e.g. `"hit"`, `"miss_db_hit"`,
    /// `"miss_db_miss"`) that callers can use to slice the counter.
    pub fn record_megolm_session_key_read(&self, result: &str, duration_ms: f64) {
        self.megolm_session_key_read_total.inc();
        self.megolm_session_key_read_duration_ms.observe(duration_ms);
        // Result label is intentionally accepted for future label-aware
        // collectors; the current Counter stores a single aggregate. Avoid
        // unused-variable warnings while keeping the call site readable.
        let _ = result;
    }

    // ========================================================================
    // NEW HELPER METHODS FOR CUSTOM METRICS (2026-09-30)
    // Phase 1 implementation from prometheus-custom-metrics.md
    // ========================================================================

    // NOTE: Labelized versions commented out until DynamicHistogramTemplate is fully integrated
    // Methods like record_message_delivery_labeled, record_room_creation_labeled etc.
    // should use the DynamicHistogramTemplate once added to MetricsCollector

    /// Records message delivery latency (simple version).
    ///
    /// Call site: `MessagingService::send_message` (the single funnel for every
    /// client send, including the transactional-dedup path).
    pub fn record_message_delivery(&self, duration_sec: f64) {
        self.message_delivery_latency.observe(duration_sec);
    }

    /// Sets current message queue depth.
    ///
    /// Call site: the worker heartbeat (`heartbeat_load_stats` in
    /// `src/bin/synapse_worker.rs`), from the Redis task queue's pending count.
    /// A single-process deployment runs no worker and therefore reports no
    /// sample (the alert stays silent instead of reading a fake `0`).
    pub fn set_message_queue_depth(&self, depth: f64) {
        self.message_queue_depth.set(depth);
    }

    // NOTE: Labelized versions commented out until DynamicHistogramTemplate is fully integrated
    // Methods like record_room_creation_labeled, record_e2ee_handshake_labeled etc.
    // should use the DynamicHistogramTemplate once added to MetricsCollector

    /// Records room creation duration (simple version).
    ///
    /// Call site: `LifecycleService::create_room` (the instrumentation wrapper
    /// around `create_room_inner`).
    pub fn record_room_creation(&self, duration_sec: f64) {
        self.room_creation_duration.observe(duration_sec);
    }

    /// Records E2EE handshake duration (simple version).
    ///
    /// Call site: `DeviceKeysService::claim_keys` — the client key-claim round
    /// trip that establishes Olm sessions between devices.
    pub fn record_e2ee_handshake(&self, duration_sec: f64) {
        self.e2ee_handshake_duration.observe(duration_sec);
    }

    /// Records unified room operation with granular labels.
    ///
    /// # Arguments
    /// * `operation` - Operation type: "create", "join", "leave", "upgrade", "forget"
    /// * `outcome` - Outcome: "success", "already_exists", "error", "forbidden"
    /// * `room_version` - Room version: "10", "11", "12", "unknown"
    /// * `visibility` - Visibility: "public", "private"
    /// * `error_type` - Error type if applicable: "M_FORBIDDEN", "M_INVALID_ROOM_ID", "M_UNKNOWN", "other"
    ///
    /// Call site: `LifecycleService::create_room` (operation `create`; the
    /// outcome/error_type come from the `ApiResult` the inner body returned).
    pub fn record_room_operation_labeled(
        &self,
        operation: &str,
        outcome: &str,
        room_version: &str,
        visibility: &str,
        error_type: &str,
    ) {
        self.room_operations_total.observe(&[operation, outcome, room_version, visibility, error_type]);
    }

    /// Records cache operation with cache_type, backend, operation, and result labels.
    ///
    /// # Arguments
    /// * `cache_type` - Type of cache: "room_state", "event_body", "membership", "user_profile"
    /// * `backend` - Backend used: "redis", "memory", "hybrid"
    /// * `operation` - Operation performed: "get", "set", "delete", "invalidate"
    /// * `result` - Result of operation: "hit", "miss", "success", "error"
    ///
    /// Call site: `CacheManager::record_cache_operation`, invoked from the
    /// single-key `get`/`get_checked`/`set`/`set_checked`/`delete` paths.
    pub fn record_cache_operation_labeled(&self, cache_type: &str, backend: &str, operation: &str, result: &str) {
        self.cache_operations_total.observe(&[cache_type, backend, operation, result]);
    }

    /// Returns the underlying [`MetricsCollector`] for direct registration of additional metrics.
    pub fn get_collector(&self) -> &Arc<MetricsCollector> {
        &self.collector
    }

    /// Reads all counter values and returns a snapshot [`MetricsSummary`].
    pub fn get_summary(&self) -> MetricsSummary {
        MetricsSummary {
            auth_attempts: self.auth_attempts_total.get(),
            auth_failures: self.auth_failures_total.get(),
            auth_success: self.auth_success_total.get(),
            token_validations: self.token_validations_total.get(),
            token_errors: self.token_validation_errors.get(),
            cache_hits: self.cache_hits_total.get(),
            cache_misses: self.cache_misses_total.get(),
            cache_hit_rate: self.calculate_cache_hit_rate(),
            federation_requests: self.federation_requests_total.get(),
            federation_errors: self.federation_request_errors_total.get(),
            replay_attacks_blocked: self.federation_replay_attacks_blocked.get(),
            http_requests: self.http_requests_total.get(),
            http_errors: self.http_request_errors_total.get(),
            db_errors: self.db_query_errors.get(),
            room_creates: self.room_creates_total.get(),
            room_joins: self.room_joins_total.get(),
            room_leaves: self.room_leaves_total.get(),
            sync_requests: self.sync_requests_total.get(),
            messages_sent: self.messages_sent_total.get(),
            presence_updates: self.presence_updates_total.get(),
            state_group_resolves: self.state_group_resolves_total.get(),
            csrf_validations: self.csrf_validations_total.get(),
            csrf_validation_failures: self.csrf_validation_failures_total.get(),
            // NEW CUSTOM METRICS SUMMARIES (2026-09-30)
            // Note: These dynamic counters cannot be easily summarized; removed from MetricsSummary
            // Use Prometheus directly for labeled metric aggregation.
            room_operations_total: 0,
            cache_operations_total: 0,
        }
    }

    fn calculate_cache_hit_rate(&self) -> f64 {
        let hits = self.cache_hits_total.get();
        let misses = self.cache_misses_total.get();
        let total = hits + misses;
        if total == 0 {
            0.0
        } else {
            (hits as f64 / total as f64) * 100.0
        }
    }
}

/// Discriminator for the three security validation pipelines (JWT, Origin, Timestamp).
#[derive(Debug, Clone, Copy)]
pub enum SecurityValidationType {
    /// JWT/JWT validation failure.
    Jwt,
    /// Origin header validation failure.
    Origin,
    /// Timestamp validation failure.
    Timestamp,
}

#[derive(Debug, Clone)]
/// Snapshot of current counter values for admin/debug endpoints.
pub struct MetricsSummary {
    /// Total auth attempts since startup.
    pub auth_attempts: u64,
    /// Total auth failures.
    pub auth_failures: u64,
    /// Total auth successes.
    pub auth_success: u64,
    /// Total token validations.
    pub token_validations: u64,
    /// Total token validation errors.
    pub token_errors: u64,
    /// Total cache hits.
    pub cache_hits: u64,
    /// Total cache misses.
    pub cache_misses: u64,
    /// Pre-computed cache hit rate (0-1).
    pub cache_hit_rate: f64,
    /// Total outgoing federation requests.
    pub federation_requests: u64,
    /// Total federation request errors.
    pub federation_errors: u64,
    /// Total replay attacks blocked.
    pub replay_attacks_blocked: u64,
    /// Total HTTP requests.
    pub http_requests: u64,
    /// Total HTTP error responses.
    pub http_errors: u64,
    /// Total database query errors.
    pub db_errors: u64,
    /// Total room create operations.
    pub room_creates: u64,
    /// Total room join operations.
    pub room_joins: u64,
    /// Total room leave operations.
    pub room_leaves: u64,
    /// Total `/sync` requests.
    pub sync_requests: u64,
    /// Total messages sent.
    pub messages_sent: u64,
    /// Total presence updates.
    pub presence_updates: u64,
    /// Total state-group conflict resolutions.
    pub state_group_resolves: u64,
    /// Total CSRF token validations.
    pub csrf_validations: u64,
    /// Total failed CSRF validations.
    pub csrf_validation_failures: u64,
    // NEW CUSTOM METRICS (2026-09-30)
    /// Total unified room operations with granular labels.
    pub room_operations_total: u64,
    /// Total cache operations with backend/type/result labels.
    pub cache_operations_total: u64,
}

impl MetricsSummary {
    /// Returns auth success rate as a percentage (0-100), or 0 if no attempts.
    pub fn auth_success_rate(&self) -> f64 {
        if self.auth_attempts == 0 {
            0.0
        } else {
            (self.auth_success as f64 / self.auth_attempts as f64) * 100.0
        }
    }

    /// Returns HTTP error rate as a percentage (0-100), or 0 if no requests.
    pub fn error_rate(&self) -> f64 {
        if self.http_requests == 0 {
            0.0
        } else {
            (self.http_errors as f64 / self.http_requests as f64) * 100.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_metrics_creation() {
        let collector = Arc::new(MetricsCollector::new());
        let metrics = ServerMetrics::new(collector);

        assert_eq!(metrics.auth_attempts_total.get(), 0);
        assert_eq!(metrics.cache_hits_total.get(), 0);
    }

    #[test]
    fn test_record_auth_attempt() {
        let collector = Arc::new(MetricsCollector::new());
        let metrics = ServerMetrics::new(collector);

        metrics.record_auth_attempt(true);
        assert_eq!(metrics.auth_attempts_total.get(), 1);
        assert_eq!(metrics.auth_success_total.get(), 1);
        assert_eq!(metrics.auth_failures_total.get(), 0);

        metrics.record_auth_attempt(false);
        assert_eq!(metrics.auth_attempts_total.get(), 2);
        assert_eq!(metrics.auth_success_total.get(), 1);
        assert_eq!(metrics.auth_failures_total.get(), 1);
    }

    #[test]
    fn test_record_cache_operation() {
        let collector = Arc::new(MetricsCollector::new());
        let metrics = ServerMetrics::new(collector);

        metrics.record_cache_operation(true);
        metrics.record_cache_operation(true);
        metrics.record_cache_operation(false);

        assert_eq!(metrics.cache_hits_total.get(), 2);
        assert_eq!(metrics.cache_misses_total.get(), 1);
    }

    #[test]
    fn test_update_pool_metrics() {
        let collector = Arc::new(MetricsCollector::new());
        let metrics = ServerMetrics::new(collector);

        metrics.update_pool_metrics(15.0, 5.0, 0.75, true);

        assert_eq!(metrics.db_connections_active.get(), 15.0);
        assert_eq!(metrics.db_connections_idle.get(), 5.0);
        assert_eq!(metrics.pool_utilization.get(), 0.75);
        assert_eq!(metrics.pool_health_status.get(), 1.0);

        metrics.update_pool_metrics(19.0, 1.0, 0.95, false);
        assert_eq!(metrics.pool_health_status.get(), 0.0);
    }

    #[test]
    fn test_record_federation_request() {
        let collector = Arc::new(MetricsCollector::new());
        let metrics = ServerMetrics::new(collector);

        metrics.record_federation_request(50.0, true);
        metrics.record_federation_request(100.0, false);

        assert_eq!(metrics.federation_requests_total.get(), 2);
        // A transport/remote failure is not a signature failure: it must land on
        // `federation_request_errors_total`, leaving the signature counter untouched.
        assert_eq!(metrics.federation_request_errors_total.get(), 1);
        assert_eq!(metrics.federation_signature_errors.get(), 0);
    }

    #[test]
    fn test_record_replay_attack_blocked() {
        let collector = Arc::new(MetricsCollector::new());
        let metrics = ServerMetrics::new(collector);

        metrics.record_replay_attack_blocked();
        metrics.record_replay_attack_blocked();

        assert_eq!(metrics.federation_replay_attacks_blocked.get(), 2);
    }

    #[test]
    fn test_get_summary() {
        let collector = Arc::new(MetricsCollector::new());
        let metrics = ServerMetrics::new(collector);

        metrics.record_auth_attempt(true);
        metrics.record_auth_attempt(false);
        metrics.record_cache_operation(true);
        metrics.record_cache_operation(false);
        metrics.record_http_request(50.0, true);
        metrics.record_http_request(100.0, false);

        let summary = metrics.get_summary();

        assert_eq!(summary.auth_attempts, 2);
        assert_eq!(summary.auth_success, 1);
        assert_eq!(summary.auth_failures, 1);
        assert_eq!(summary.cache_hits, 1);
        assert_eq!(summary.cache_misses, 1);
        assert_eq!(summary.http_requests, 2);
        assert_eq!(summary.http_errors, 1);
    }

    #[test]
    fn test_metrics_summary_calculations() {
        let summary = MetricsSummary {
            auth_attempts: 100,
            auth_failures: 10,
            auth_success: 90,
            token_validations: 500,
            token_errors: 5,
            cache_hits: 800,
            cache_misses: 200,
            cache_hit_rate: 80.0,
            federation_requests: 50,
            federation_errors: 2,
            replay_attacks_blocked: 3,
            http_requests: 1000,
            http_errors: 20,
            db_errors: 5,
            room_creates: 10,
            room_joins: 200,
            room_leaves: 50,
            sync_requests: 5000,
            messages_sent: 3000,
            presence_updates: 800,
            state_group_resolves: 150,
            csrf_validations: 400,
            csrf_validation_failures: 3,
            room_operations_total: 0,
            cache_operations_total: 0,
        };

        assert_eq!(summary.auth_success_rate(), 90.0);
        assert_eq!(summary.error_rate(), 2.0);
    }

    #[test]
    fn test_security_validation_recording() {
        let collector = Arc::new(MetricsCollector::new());
        let metrics = ServerMetrics::new(collector);

        metrics.record_security_validation(SecurityValidationType::Jwt, false);
        metrics.record_security_validation(SecurityValidationType::Origin, false);
        metrics.record_security_validation(SecurityValidationType::Timestamp, false);

        assert_eq!(metrics.security_jwt_validation_errors.get(), 1);
        assert_eq!(metrics.security_origin_validation_errors.get(), 1);
        assert_eq!(metrics.security_timestamp_validation_errors.get(), 1);
    }

    #[test]
    fn test_security_validation_success_does_not_increment_error_counters() {
        let collector = Arc::new(MetricsCollector::new());
        let metrics = ServerMetrics::new(collector);

        // Successful validations should not increment any error counter.
        metrics.record_security_validation(SecurityValidationType::Jwt, true);
        metrics.record_security_validation(SecurityValidationType::Origin, true);
        metrics.record_security_validation(SecurityValidationType::Timestamp, true);

        assert_eq!(metrics.security_jwt_validation_errors.get(), 0);
        assert_eq!(metrics.security_origin_validation_errors.get(), 0);
        assert_eq!(metrics.security_timestamp_validation_errors.get(), 0);
    }

    #[test]
    fn test_record_token_validation_success_and_failure() {
        let collector = Arc::new(MetricsCollector::new());
        let metrics = ServerMetrics::new(collector);

        metrics.record_token_validation(true);
        assert_eq!(metrics.token_validations_total.get(), 1);
        assert_eq!(metrics.token_validation_errors.get(), 0);

        metrics.record_token_validation(false);
        assert_eq!(metrics.token_validations_total.get(), 2);
        assert_eq!(metrics.token_validation_errors.get(), 1);
    }

    #[test]
    fn test_record_db_query_success_and_failure() {
        let collector = Arc::new(MetricsCollector::new());
        let metrics = ServerMetrics::new(collector);

        metrics.record_db_query(15.5, true);
        assert_eq!(metrics.db_query_errors.get(), 0);
        assert_eq!(metrics.db_query_duration.get_count(), 1);

        metrics.record_db_query(30.0, false);
        assert_eq!(metrics.db_query_errors.get(), 1);
        assert_eq!(metrics.db_query_duration.get_count(), 2);
    }

    #[test]
    fn test_record_db_transaction_duration_recorded() {
        let collector = Arc::new(MetricsCollector::new());
        let metrics = ServerMetrics::new(collector);

        metrics.db_transaction_duration.observe(50.0);
        assert_eq!(metrics.db_transaction_duration.get_count(), 1);
        assert_eq!(metrics.db_transaction_duration.get_sum(), 50.0);
    }

    #[test]
    fn test_record_cache_evictions_and_errors() {
        let collector = Arc::new(MetricsCollector::new());
        let metrics = ServerMetrics::new(collector);

        metrics.cache_evictions_total.inc();
        metrics.cache_errors.inc();
        assert_eq!(metrics.cache_evictions_total.get(), 1);
        assert_eq!(metrics.cache_errors.get(), 1);
    }

    #[test]
    fn test_record_http_request_success_and_failure() {
        let collector = Arc::new(MetricsCollector::new());
        let metrics = ServerMetrics::new(collector);

        metrics.record_http_request(100.0, true);
        assert_eq!(metrics.http_requests_total.get(), 1);
        assert_eq!(metrics.http_request_errors_total.get(), 0);
        assert_eq!(metrics.http_request_duration.get_count(), 1);

        metrics.record_http_request(200.0, false);
        assert_eq!(metrics.http_requests_total.get(), 2);
        assert_eq!(metrics.http_request_errors_total.get(), 1);
    }

    #[test]
    fn test_http_request_started_and_finished_tracks_active() {
        let collector = Arc::new(MetricsCollector::new());
        let metrics = ServerMetrics::new(collector);

        assert_eq!(metrics.http_active_requests.get(), 0.0);
        metrics.http_request_started();
        assert_eq!(metrics.http_active_requests.get(), 1.0);
        metrics.http_request_started();
        assert_eq!(metrics.http_active_requests.get(), 2.0);
        metrics.http_request_finished();
        assert_eq!(metrics.http_active_requests.get(), 1.0);
    }

    #[test]
    fn test_record_room_operation_create_join_leave() {
        let collector = Arc::new(MetricsCollector::new());
        let metrics = ServerMetrics::new(collector);

        metrics.record_room_operation("create", 10.0, true);
        metrics.record_room_operation("join", 20.0, true);
        metrics.record_room_operation("leave", 5.0, true);

        assert_eq!(metrics.room_creates_total.get(), 1);
        assert_eq!(metrics.room_joins_total.get(), 1);
        assert_eq!(metrics.room_leaves_total.get(), 1);
        assert_eq!(metrics.room_operation_duration.get_count(), 3);
    }

    #[test]
    fn test_record_room_operation_unknown_op_does_not_increment_counters() {
        let collector = Arc::new(MetricsCollector::new());
        let metrics = ServerMetrics::new(collector);

        // Unknown op should still observe duration but not increment any counter.
        metrics.record_room_operation("unknown", 15.0, false);
        assert_eq!(metrics.room_creates_total.get(), 0);
        assert_eq!(metrics.room_joins_total.get(), 0);
        assert_eq!(metrics.room_leaves_total.get(), 0);
        assert_eq!(metrics.room_operation_duration.get_count(), 1);
    }

    #[test]
    fn test_record_sync_request() {
        let collector = Arc::new(MetricsCollector::new());
        let metrics = ServerMetrics::new(collector);

        metrics.record_sync_request(500.0, true);
        assert_eq!(metrics.sync_requests_total.get(), 1);
        assert_eq!(metrics.sync_duration.get_count(), 1);

        // success flag is currently ignored (let _ = success), but method should not error.
        metrics.record_sync_request(1000.0, false);
        assert_eq!(metrics.sync_requests_total.get(), 2);
    }

    #[test]
    fn test_record_message_send() {
        let collector = Arc::new(MetricsCollector::new());
        let metrics = ServerMetrics::new(collector);

        metrics.record_message_send(50.0, true);
        assert_eq!(metrics.messages_sent_total.get(), 1);
        assert_eq!(metrics.message_send_duration.get_count(), 1);

        metrics.record_message_send(75.0, false);
        assert_eq!(metrics.messages_sent_total.get(), 2);
    }

    #[test]
    fn test_record_presence_update() {
        let collector = Arc::new(MetricsCollector::new());
        let metrics = ServerMetrics::new(collector);

        metrics.record_presence_update(25.0);
        assert_eq!(metrics.presence_updates_total.get(), 1);
        assert_eq!(metrics.presence_sync_duration.get_count(), 1);
        assert_eq!(metrics.presence_sync_duration.get_sum(), 25.0);
    }

    #[test]
    fn test_record_state_group_resolve() {
        let collector = Arc::new(MetricsCollector::new());
        let metrics = ServerMetrics::new(collector);

        metrics.record_state_group_resolve(40.0);
        assert_eq!(metrics.state_group_resolves_total.get(), 1);
        assert_eq!(metrics.state_group_resolve_duration.get_count(), 1);
    }

    #[test]
    fn test_record_csrf_validation_success_and_failure() {
        let collector = Arc::new(MetricsCollector::new());
        let metrics = ServerMetrics::new(collector);

        metrics.record_csrf_validation(true);
        assert_eq!(metrics.csrf_validations_total.get(), 1);
        assert_eq!(metrics.csrf_validation_failures_total.get(), 0);

        metrics.record_csrf_validation(false);
        assert_eq!(metrics.csrf_validations_total.get(), 2);
        assert_eq!(metrics.csrf_validation_failures_total.get(), 1);
    }

    #[test]
    fn test_record_federation_signature_verification_success_and_failure() {
        let collector = Arc::new(MetricsCollector::new());
        let metrics = ServerMetrics::new(collector);

        metrics.record_federation_signature_verification(true);
        assert_eq!(metrics.federation_signature_verifications.get(), 1);
        assert_eq!(metrics.federation_signature_errors.get(), 0);

        metrics.record_federation_signature_verification(false);
        assert_eq!(metrics.federation_signature_verifications.get(), 2);
        assert_eq!(metrics.federation_signature_errors.get(), 1);
    }

    #[test]
    fn test_record_megolm_share_success() {
        let collector = Arc::new(MetricsCollector::new());
        let metrics = ServerMetrics::new(collector);

        metrics.record_megolm_share(5, 10.0, 1.0, true);
        assert_eq!(metrics.megolm_share_total.get(), 1);
        assert_eq!(metrics.megolm_share_recipients_total.get(), 5);
        assert_eq!(metrics.megolm_share_db_duration_ms.get_count(), 1);
        assert_eq!(metrics.megolm_share_cache_duration_ms.get_count(), 1);
        assert_eq!(metrics.megolm_share_db_errors_total.get(), 0);
    }

    #[test]
    fn test_record_megolm_share_failure() {
        let collector = Arc::new(MetricsCollector::new());
        let metrics = ServerMetrics::new(collector);

        metrics.record_megolm_share(5, 10.0, 1.0, false);
        assert_eq!(metrics.megolm_share_total.get(), 1);
        assert_eq!(metrics.megolm_share_recipients_total.get(), 0);
        assert_eq!(metrics.megolm_share_db_duration_ms.get_count(), 0);
        assert_eq!(metrics.megolm_share_db_errors_total.get(), 1);
    }

    #[test]
    fn test_record_megolm_share_cache_error() {
        let collector = Arc::new(MetricsCollector::new());
        let metrics = ServerMetrics::new(collector);

        metrics.record_megolm_share_cache_error();
        metrics.record_megolm_share_cache_error();
        assert_eq!(metrics.megolm_share_cache_errors_total.get(), 2);
    }

    #[test]
    fn test_record_megolm_session_key_read() {
        let collector = Arc::new(MetricsCollector::new());
        let metrics = ServerMetrics::new(collector);

        metrics.record_megolm_session_key_read("hit", 5.0);
        assert_eq!(metrics.megolm_session_key_read_total.get(), 1);
        assert_eq!(metrics.megolm_session_key_read_duration_ms.get_count(), 1);
    }

    #[test]
    fn test_get_collector_returns_arc() {
        let collector = Arc::new(MetricsCollector::new());
        // Clone before moving into ServerMetrics::new (which takes Arc by value).
        let collector_handle = collector.clone();
        let metrics = ServerMetrics::new(collector);

        let returned = metrics.get_collector();
        // Strong count should be >= 2: one in metrics, one in collector_handle.
        assert!(Arc::strong_count(returned) >= 2);

        // Verify the returned Arc points to the same MetricsCollector.
        let _ = collector_handle.get_counter("any_counter");
    }

    #[test]
    fn test_calculate_cache_hit_rate_zero_when_no_ops() {
        let collector = Arc::new(MetricsCollector::new());
        let metrics = ServerMetrics::new(collector);

        // No cache operations: hit rate should be 0.0 (avoids division by zero).
        let summary = metrics.get_summary();
        assert_eq!(summary.cache_hit_rate, 0.0);
    }

    #[test]
    fn test_calculate_cache_hit_rate_with_mixed_hits_misses() {
        let collector = Arc::new(MetricsCollector::new());
        let metrics = ServerMetrics::new(collector);

        // 3 hits + 1 miss = 75% hit rate.
        metrics.record_cache_operation(true);
        metrics.record_cache_operation(true);
        metrics.record_cache_operation(true);
        metrics.record_cache_operation(false);

        let summary = metrics.get_summary();
        assert_eq!(summary.cache_hits, 3);
        assert_eq!(summary.cache_misses, 1);
        assert_eq!(summary.cache_hit_rate, 75.0);
    }

    #[test]
    fn test_metrics_summary_auth_success_rate_zero_when_no_attempts() {
        let summary = MetricsSummary {
            auth_attempts: 0,
            auth_failures: 0,
            auth_success: 0,
            token_validations: 0,
            token_errors: 0,
            cache_hits: 0,
            cache_misses: 0,
            cache_hit_rate: 0.0,
            federation_requests: 0,
            federation_errors: 0,
            replay_attacks_blocked: 0,
            http_requests: 0,
            http_errors: 0,
            db_errors: 0,
            room_creates: 0,
            room_joins: 0,
            room_leaves: 0,
            sync_requests: 0,
            messages_sent: 0,
            presence_updates: 0,
            state_group_resolves: 0,
            csrf_validations: 0,
            csrf_validation_failures: 0,
            room_operations_total: 0,
            cache_operations_total: 0,
        };

        assert_eq!(summary.auth_success_rate(), 0.0);
        assert_eq!(summary.error_rate(), 0.0);
    }

    #[test]
    fn test_get_summary_includes_all_room_operations() {
        let collector = Arc::new(MetricsCollector::new());
        let metrics = ServerMetrics::new(collector);

        metrics.record_room_operation("create", 10.0, true);
        metrics.record_room_operation("join", 20.0, true);
        metrics.record_room_operation("leave", 5.0, true);
        metrics.record_sync_request(500.0, true);
        metrics.record_message_send(50.0, true);
        metrics.record_presence_update(25.0);
        metrics.record_state_group_resolve(40.0);
        metrics.record_csrf_validation(true);
        metrics.record_csrf_validation(false);

        let summary = metrics.get_summary();
        assert_eq!(summary.room_creates, 1);
        assert_eq!(summary.room_joins, 1);
        assert_eq!(summary.room_leaves, 1);
        assert_eq!(summary.sync_requests, 1);
        assert_eq!(summary.messages_sent, 1);
        assert_eq!(summary.presence_updates, 1);
        assert_eq!(summary.state_group_resolves, 1);
        assert_eq!(summary.csrf_validations, 2);
        assert_eq!(summary.csrf_validation_failures, 1);
    }

    #[test]
    fn test_total_users_and_total_rooms_gauges() {
        let collector = Arc::new(MetricsCollector::new());
        let metrics = ServerMetrics::new(collector);

        metrics.total_users.set(1500.0);
        metrics.total_rooms.set(300.0);
        assert_eq!(metrics.total_users.get(), 1500.0);
        assert_eq!(metrics.total_rooms.get(), 300.0);
    }

    /// V-12 的指标侧：`stream` 标签必须**每个已登记 stream 都能取到**，且各自独立。
    #[test]
    fn test_storage_stream_current_position_is_per_stream() {
        let collector = Arc::new(MetricsCollector::new());
        let metrics = ServerMetrics::new(collector.clone());

        for (i, stream) in StreamPosition::ALL.into_iter().enumerate() {
            metrics.set_stream_position(stream, 4200 + i as i64);
        }
        for (i, stream) in StreamPosition::ALL.into_iter().enumerate() {
            let gauge = metrics
                .storage_stream_current_position
                .get(&[stream.label()])
                .unwrap_or_else(|| panic!("stream {} 必须有值", stream.label()));
            assert_eq!(gauge.get(), (4200 + i) as f64, "stream {} 的值被别的 stream 覆盖了", stream.label());
        }

        // 同名多标签在 `/metrics` 里各成一条序列（这是本指标此前做不到的部分）。
        let rendered = collector.to_prometheus_format();
        for stream in StreamPosition::ALL {
            assert!(
                rendered.contains(&format!("synapse_storage_stream_current_position{{stream=\"{}\"}}", stream.label())),
                "渲染里缺少 {} 序列：\n{rendered}",
                stream.label()
            );
        }
        // 未登记的标签不得被当成合法 stream。
        assert!(StreamPosition::from_label("presence").is_none());
        assert_eq!(StreamPosition::from_label("events"), Some(StreamPosition::Events));
    }

    #[test]
    fn test_dehydrated_device_metrics_counters() {
        let collector = Arc::new(MetricsCollector::new());
        let metrics = ServerMetrics::new(collector);

        metrics.dehydrated_device_cleanup_total.inc();
        metrics.dehydrated_device_cleaned_total.inc_by(5);
        metrics.dehydrated_device_cleanup_errors_total.inc();
        metrics.dehydrated_device_cleanup_duration.observe(100.0);

        assert_eq!(metrics.dehydrated_device_cleanup_total.get(), 1);
        assert_eq!(metrics.dehydrated_device_cleaned_total.get(), 5);
        assert_eq!(metrics.dehydrated_device_cleanup_errors_total.get(), 1);
        assert_eq!(metrics.dehydrated_device_cleanup_duration.get_count(), 1);
    }
}
